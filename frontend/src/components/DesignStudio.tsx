import React, { useEffect, useState } from 'react';
import axios from 'axios';

interface DatasetSpec {
  id: string;
  name: string;
  scorer: string;
  source: string;
  num_tasks: number;
  description: string;
}

interface LineageEntry {
  round: number;
  source: string;
  run_id: string | null;
  accuracy: number;
  parent_run_id: string | null;
  config_yaml: string;
}

interface AutoLoopResult {
  requirement: string;
  dataset_id: string;
  rounds_run: number;
  best_round: number | null;
  best_accuracy: number;
  best_run_id: string | null;
  best_config_yaml: string | null;
  stopped_reason: string;
  lineage: LineageEntry[];
}

const box: React.CSSProperties = {
  border: '1px solid #2c3142',
  borderRadius: 8,
  padding: 12,
  background: '#1a1d29',
};

const mono: React.CSSProperties = {
  width: '100%',
  fontFamily: 'monospace',
  fontSize: 12,
};

interface Props {
  onUseConfig?: (yaml: string) => void;
}

/** Parse an SSE byte stream from a POST endpoint (EventSource only supports GET). */
async function readSse(
  resp: Response,
  onEvent: (e: Record<string, unknown>) => void,
): Promise<void> {
  const reader = resp.body?.getReader();
  if (!reader) return;
  const decoder = new TextDecoder();
  let buffer = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const frames = buffer.split('\n\n');
    buffer = frames.pop() ?? '';
    for (const frame of frames) {
      for (const line of frame.split('\n')) {
        const trimmed = line.replace(/^data:\s?/, '');
        if (line.startsWith('data:') && trimmed) {
          try {
            onEvent(JSON.parse(trimmed));
          } catch {
            /* ignore malformed frame */
          }
        }
      }
    }
  }
}

const DesignStudio: React.FC<Props> = ({ onUseConfig }) => {
  const [datasets, setDatasets] = useState<DatasetSpec[]>([]);

  // --- Design assistant (Architect) ---
  const [goal, setGoal] = useState('Solve grade-school math word problems and return only the final number.');
  const [strategy, setStrategy] = useState('react');
  const [model, setModel] = useState('deepseek-v4-flash');
  const [designing, setDesigning] = useState(false);
  const [designYaml, setDesignYaml] = useState('');
  const [designMeta, setDesignMeta] = useState<{ source?: string; valid?: boolean; errors?: string[] } | null>(null);

  // --- Autoloop ---
  const [loopReq, setLoopReq] = useState('Solve grade-school math word problems and return only the final number.');
  const [datasetId, setDatasetId] = useState('smoke_gsm8k');
  const [rounds, setRounds] = useState(3);
  const [target, setTarget] = useState(1.0);
  const [patience, setPatience] = useState(2);
  const [limit, setLimit] = useState<number | ''>(3);
  const [looping, setLooping] = useState(false);
  const [loopLog, setLoopLog] = useState<string[]>([]);
  const [loopResult, setLoopResult] = useState<AutoLoopResult | null>(null);

  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const d = await axios.get('/api/v1/datasets');
        if (!active) return;
        const list: DatasetSpec[] = d.data.datasets || [];
        setDatasets(list);
        if (list.length && !list.some((x) => x.id === 'smoke_gsm8k')) {
          setDatasetId(list[0].id);
        }
      } catch {
        /* backend may be offline */
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const generate = async () => {
    setDesigning(true);
    setDesignMeta(null);
    try {
      const r = await axios.post('/api/v1/design/generate', {
        goal_description: goal,
        strategy_type: strategy,
        model_preference: model,
      });
      setDesignYaml(r.data.config_yaml || '');
      setDesignMeta({
        source: r.data.source,
        valid: r.data.validation?.valid,
        errors: r.data.validation?.errors,
      });
    } catch (err) {
      const msg = axios.isAxiosError(err) ? err.response?.data?.detail || err.message : String(err);
      setDesignMeta({ source: 'error', valid: false, errors: [String(msg)] });
    } finally {
      setDesigning(false);
    }
  };

  const startLoop = async () => {
    setLooping(true);
    setLoopLog([`▶ 启动自举闭环 · ${loopReq.slice(0, 40)}…`]);
    setLoopResult(null);
    const append = (line: string) => setLoopLog((p) => [...p, line]);
    try {
      const resp = await fetch('/api/v1/autoloop', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
        body: JSON.stringify({
          requirement: loopReq,
          dataset_id: datasetId,
          rounds,
          target_accuracy: target,
          patience,
          strategy_type: strategy,
          model,
          limit: limit === '' ? null : limit,
        }),
      });
      if (!resp.ok) {
        const detail = await resp.text();
        append(`✗ 启动失败 (${resp.status}): ${detail}`);
        setLooping(false);
        return;
      }
      await readSse(resp, (e) => {
        switch (e.type) {
          case 'autoloop_start':
            append(`  目标准确率 ${(Number(e.target_accuracy) * 100).toFixed(0)}% · 最多 ${e.rounds} 轮`);
            break;
          case 'round_proposed':
            append(`  ⟳ 第 ${e.round} 轮：提出候选配置…`);
            break;
          case 'round_evaluated':
            append(`  ✔ 第 ${e.round} 轮：准确率 ${(Number(e.accuracy) * 100).toFixed(1)}% (run ${String(e.run_id).slice(0, 8)})`);
            break;
          case 'autoloop_result':
            setLoopResult(e as unknown as AutoLoopResult);
            append(`■ 完成 — 最佳第 ${e.best_round} 轮 · ${(Number(e.best_accuracy) * 100).toFixed(1)}% · ${e.stopped_reason}`);
            break;
          case 'error':
            append(`✗ 错误: ${e.message}`);
            break;
          default:
            break;
        }
      });
    } catch (err) {
      append(`✗ ${String(err)}`);
    } finally {
      setLooping(false);
    }
  };

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
      {/* Design assistant */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style={box}>
          <h3 style={{ marginTop: 0 }}>设计助手（Architect）</h3>
          <p style={{ fontSize: 12, color: '#8b90a0', marginTop: 0 }}>
            用自然语言描述目标，Architect 生成可插拔 agent 配置（无引擎时降级为模板）。
          </p>
          <label style={{ fontSize: 13 }}>目标描述</label>
          <textarea value={goal} onChange={(e) => setGoal(e.target.value)} rows={4} spellCheck={false} style={{ ...mono, marginBottom: 8 }} />
          <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 13 }}>策略</label>
              <select value={strategy} onChange={(e) => setStrategy(e.target.value)} style={{ width: '100%' }}>
                <option value="react">react</option>
                <option value="plan_and_solve">plan_and_solve</option>
                <option value="reflexion">reflexion</option>
              </select>
            </div>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 13 }}>模型</label>
              <input value={model} onChange={(e) => setModel(e.target.value)} style={{ width: '100%' }} />
            </div>
          </div>
          <button onClick={generate} disabled={designing} style={{ width: '100%' }}>
            {designing ? '生成中…' : '生成配置'}
          </button>
          {designMeta && (
            <div style={{ fontSize: 12, marginTop: 8 }}>
              来源:{' '}
              <strong style={{ color: designMeta.source === 'agent' ? '#6cffb0' : designMeta.source === 'error' ? '#ff7a7a' : '#ffcf6c' }}>
                {designMeta.source}
              </strong>{' '}
              · 校验: {designMeta.valid ? '✅ 通过' : '❌ 失败'}
              {designMeta.errors && designMeta.errors.length > 0 && (
                <pre style={{ color: '#ff7a7a', whiteSpace: 'pre-wrap' }}>{designMeta.errors.join('\n')}</pre>
              )}
            </div>
          )}
        </div>

        {designYaml && (
          <div style={box}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h3 style={{ marginTop: 0 }}>生成的配置</h3>
              {onUseConfig && (
                <button onClick={() => onUseConfig(designYaml)}>用于运行/评测 →</button>
              )}
            </div>
            <textarea value={designYaml} onChange={(e) => setDesignYaml(e.target.value)} rows={16} spellCheck={false} style={mono} />
          </div>
        )}
      </div>

      {/* Autoloop */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style={box}>
          <h3 style={{ marginTop: 0 }}>自举闭环（Autoloop）</h3>
          <p style={{ fontSize: 12, color: '#8b90a0', marginTop: 0 }}>
            提出 → 在验证集上评测 → 读分 → 修订，自动迭代并记录血缘。
          </p>
          <label style={{ fontSize: 13 }}>需求</label>
          <textarea value={loopReq} onChange={(e) => setLoopReq(e.target.value)} rows={3} spellCheck={false} style={{ ...mono, marginBottom: 8 }} />
          <label style={{ fontSize: 13 }}>验证集</label>
          <select value={datasetId} onChange={(e) => setDatasetId(e.target.value)} style={{ width: '100%', marginBottom: 8 }}>
            {datasets.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name} ({d.num_tasks} 任务 · {d.scorer})
              </option>
            ))}
          </select>
          <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 12 }}>轮数</label>
              <input type="number" min={1} max={20} value={rounds} onChange={(e) => setRounds(Number(e.target.value))} style={{ width: '100%' }} />
            </div>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 12 }}>目标准确率</label>
              <input type="number" min={0} max={1} step={0.05} value={target} onChange={(e) => setTarget(Number(e.target.value))} style={{ width: '100%' }} />
            </div>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 12 }}>耐心</label>
              <input type="number" min={1} max={10} value={patience} onChange={(e) => setPatience(Number(e.target.value))} style={{ width: '100%' }} />
            </div>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 12 }}>limit</label>
              <input type="number" min={1} value={limit} onChange={(e) => setLimit(e.target.value === '' ? '' : Number(e.target.value))} style={{ width: '100%' }} />
            </div>
          </div>
          <button onClick={startLoop} disabled={looping} style={{ width: '100%' }}>
            {looping ? '迭代中…' : '启动自举'}
          </button>
          {loopLog.length > 0 && (
            <pre style={{ marginTop: 8, maxHeight: 200, overflow: 'auto', fontSize: 12, background: '#11131c', padding: 8, borderRadius: 6, whiteSpace: 'pre-wrap' }}>
              {loopLog.join('\n')}
            </pre>
          )}
        </div>

        {loopResult && (
          <div style={box}>
            <h3 style={{ marginTop: 0 }}>血缘（lineage）</h3>
            <table style={{ width: '100%', fontSize: 13, borderCollapse: 'collapse' }}>
              <thead>
                <tr style={{ textAlign: 'left', color: '#8b90a0' }}>
                  <th>轮</th><th>来源</th><th>准确率</th><th>run</th><th>parent</th>
                </tr>
              </thead>
              <tbody>
                {loopResult.lineage.map((l) => (
                  <tr
                    key={l.round}
                    style={{
                      borderTop: '1px solid #2c3142',
                      background: l.round === loopResult.best_round ? '#1e3326' : undefined,
                    }}
                  >
                    <td>{l.round}{l.round === loopResult.best_round ? ' ★' : ''}</td>
                    <td>{l.source}</td>
                    <td>{(l.accuracy * 100).toFixed(1)}%</td>
                    <td>{l.run_id ? l.run_id.slice(0, 8) : '—'}</td>
                    <td>{l.parent_run_id ? l.parent_run_id.slice(0, 8) : '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {loopResult.best_config_yaml && (
              <details style={{ marginTop: 8 }}>
                <summary style={{ cursor: 'pointer' }}>
                  最佳配置（第 {loopResult.best_round} 轮 · {(loopResult.best_accuracy * 100).toFixed(1)}%）
                </summary>
                <textarea value={loopResult.best_config_yaml} readOnly rows={14} style={{ ...mono, marginTop: 8 }} />
                {onUseConfig && (
                  <button onClick={() => onUseConfig(loopResult.best_config_yaml as string)}>用于运行/评测 →</button>
                )}
              </details>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default DesignStudio;
