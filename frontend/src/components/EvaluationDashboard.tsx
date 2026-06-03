import React, { useCallback, useEffect, useMemo, useState } from 'react';
import axios from 'axios';

interface DatasetSpec {
  id: string;
  name: string;
  scorer: string;
  source: string;
  num_tasks: number;
  description: string;
}

interface RunRow {
  id: string;
  label: string | null;
  agent_name: string | null;
  dataset_id: string | null;
  scorer: string | null;
  status: string;
  num_tasks: number;
  completed_tasks: number;
  accuracy: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_latency_s: number;
}

interface TaskResult {
  task_id: string;
  success: number;
  score: number;
  events: number;
  input_tokens: number;
  output_tokens: number;
  latency_s: number;
  final_text: string;
  error: string | null;
}

interface TrajectoryEvent {
  seq: number;
  type: string;
  content: string;
  metadata?: Record<string, unknown>;
}

const DEFAULT_YAML = `agent:
  name: "EvalAgent"
  model: "claude-sonnet-4.5"
  context_window: 200000
  max_tokens: 4096
plugins:
  memory: "InMemoryVectorStore"
  context: "ProjectContextManager"
strategy:
  type: "react"
`;

const box: React.CSSProperties = {
  border: '1px solid #2c3142',
  borderRadius: 8,
  padding: 12,
  background: '#1a1d29',
};

interface Props {
  initialYaml?: string;
}

const EvaluationDashboard: React.FC<Props> = ({ initialYaml }) => {
  const [datasets, setDatasets] = useState<DatasetSpec[]>([]);
  const [runs, setRuns] = useState<RunRow[]>([]);
  const [datasetId, setDatasetId] = useState('smoke_gaia');
  const [yamlConfig, setYamlConfig] = useState(initialYaml || DEFAULT_YAML);
  const [label, setLabel] = useState('');
  const [seeds, setSeeds] = useState(1);
  const [limit, setLimit] = useState<number | ''>('');
  const [progress, setProgress] = useState<string[]>([]);
  const [starting, setStarting] = useState(false);

  const [selectedRun, setSelectedRun] = useState<RunRow | null>(null);
  const [taskResults, setTaskResults] = useState<TaskResult[]>([]);
  const [trajectory, setTrajectory] = useState<{ taskId: string; events: TrajectoryEvent[] } | null>(null);

  const [compareIds, setCompareIds] = useState<Set<string>>(new Set());
  const [comparison, setComparison] = useState<RunRow[] | null>(null);

  const refreshRuns = useCallback(async () => {
    const r = await axios.get('/api/v1/eval-runs');
    setRuns(r.data.runs || []);
  }, []);

  useEffect(() => {
    let active = true;
    (async () => {
      const [d, r] = await Promise.all([
        axios.get('/api/v1/datasets'),
        axios.get('/api/v1/eval-runs'),
      ]);
      if (!active) return;
      setDatasets(d.data.datasets || []);
      setRuns(r.data.runs || []);
    })();
    return () => {
      active = false;
    };
  }, []);

  const startRun = async () => {
    setStarting(true);
    setProgress([]);
    try {
      const body: Record<string, unknown> = {
        config_yaml: yamlConfig,
        dataset_id: datasetId,
        seeds,
        label: label || null,
      };
      if (limit !== '') body.limit = limit;
      const r = await axios.post('/api/v1/eval-runs', body);
      const runId = r.data.run_id as string;
      setProgress((p) => [...p, `▶ started ${runId}`]);

      const es = new EventSource(`/api/v1/eval-runs/${runId}/stream`);
      es.onmessage = (ev) => {
        try {
          const e = JSON.parse(ev.data);
          if (e.type === 'task_complete') {
            setProgress((p) => [...p, `  task ${e.task_id}: ${e.success ? '✅' : '❌'} (score ${e.score?.toFixed?.(2) ?? e.score})`]);
          } else if (e.type === 'eval_complete') {
            setProgress((p) => [...p, `■ done — accuracy ${(e.accuracy * 100).toFixed(1)}%`]);
            es.close();
            refreshRuns();
            setStarting(false);
          } else if (e.type === 'error') {
            setProgress((p) => [...p, `✗ error: ${e.message}`]);
            es.close();
            setStarting(false);
          }
        } catch {
          /* ignore malformed frame */
        }
      };
      es.onerror = () => {
        es.close();
        setStarting(false);
        refreshRuns();
      };
    } catch (err) {
      const msg = axios.isAxiosError(err) ? err.response?.data?.detail || err.message : String(err);
      setProgress((p) => [...p, `✗ failed to start: ${msg}`]);
      setStarting(false);
    }
  };

  const openRun = async (run: RunRow) => {
    setSelectedRun(run);
    setTrajectory(null);
    const r = await axios.get(`/api/v1/eval-runs/${run.id}`);
    setTaskResults(r.data.task_results || []);
  };

  const openTrajectory = async (runId: string, taskId: string) => {
    const r = await axios.get(`/api/v1/eval-runs/${runId}/tasks/${taskId}/trajectory`);
    setTrajectory({ taskId, events: r.data.events || [] });
  };

  const toggleCompare = (id: string) => {
    setCompareIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const runComparison = async () => {
    if (compareIds.size === 0) return;
    const ids = Array.from(compareIds).join(',');
    const r = await axios.get('/api/v1/eval-runs/compare', { params: { ids } });
    setComparison(r.data.comparison || []);
  };

  const selectedDataset = useMemo(
    () => datasets.find((d) => d.id === datasetId),
    [datasets, datasetId],
  );

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '360px 1fr', gap: 16 }}>
      {/* Left column: start a run + dataset list */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style={box}>
          <h3 style={{ marginTop: 0 }}>启动评测运行</h3>
          <label style={{ fontSize: 13 }}>验证集</label>
          <select
            value={datasetId}
            onChange={(e) => setDatasetId(e.target.value)}
            style={{ width: '100%', marginBottom: 8 }}
          >
            {datasets.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name} ({d.num_tasks} 任务 · {d.scorer})
              </option>
            ))}
          </select>
          {selectedDataset && (
            <p style={{ fontSize: 12, color: '#8b90a0', margin: '0 0 8px' }}>{selectedDataset.description}</p>
          )}
          <label style={{ fontSize: 13 }}>Agent YAML 配置</label>
          <textarea
            value={yamlConfig}
            onChange={(e) => setYamlConfig(e.target.value)}
            rows={10}
            spellCheck={false}
            style={{ width: '100%', fontFamily: 'monospace', fontSize: 12, marginBottom: 8 }}
          />
          <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
            <input
              placeholder="标签 (对比用)"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              style={{ flex: 2 }}
            />
            <input
              type="number"
              min={1}
              max={16}
              value={seeds}
              onChange={(e) => setSeeds(Number(e.target.value))}
              title="seeds (pass^k)"
              style={{ flex: 1 }}
            />
            <input
              type="number"
              min={1}
              placeholder="limit"
              value={limit}
              onChange={(e) => setLimit(e.target.value === '' ? '' : Number(e.target.value))}
              style={{ flex: 1 }}
            />
          </div>
          <button onClick={startRun} disabled={starting} style={{ width: '100%' }}>
            {starting ? '运行中…' : '启动评测'}
          </button>
          {progress.length > 0 && (
            <pre style={{ marginTop: 8, maxHeight: 160, overflow: 'auto', fontSize: 12, background: '#11131c', padding: 8, borderRadius: 6 }}>
              {progress.join('\n')}
            </pre>
          )}
        </div>

        <div style={box}>
          <h3 style={{ marginTop: 0 }}>验证集 ({datasets.length})</h3>
          {datasets.map((d) => (
            <div key={d.id} style={{ fontSize: 13, padding: '4px 0', borderBottom: '1px solid #23273500' }}>
              <strong>{d.name}</strong>{' '}
              <span style={{ color: '#8b90a0' }}>· {d.source} · {d.num_tasks} 任务 · {d.scorer}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Right column: runs list, comparison, run detail */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style={box}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <h3 style={{ marginTop: 0 }}>评测运行 ({runs.length})</h3>
            <div>
              <button onClick={refreshRuns} style={{ marginRight: 8 }}>刷新</button>
              <button onClick={runComparison} disabled={compareIds.size === 0}>
                对比所选 ({compareIds.size})
              </button>
            </div>
          </div>
          <table style={{ width: '100%', fontSize: 13, borderCollapse: 'collapse' }}>
            <thead>
              <tr style={{ textAlign: 'left', color: '#8b90a0' }}>
                <th></th><th>标签 / ID</th><th>验证集</th><th>状态</th><th>准确率</th><th>进度</th><th>tokens(in/out)</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((r) => (
                <tr key={r.id} style={{ borderTop: '1px solid #2c3142', cursor: 'pointer' }}>
                  <td>
                    <input type="checkbox" checked={compareIds.has(r.id)} onChange={() => toggleCompare(r.id)} />
                  </td>
                  <td onClick={() => openRun(r)}>{r.label || r.id}</td>
                  <td onClick={() => openRun(r)}>{r.dataset_id}</td>
                  <td onClick={() => openRun(r)}>{r.status}</td>
                  <td onClick={() => openRun(r)}>{(r.accuracy * 100).toFixed(1)}%</td>
                  <td onClick={() => openRun(r)}>{r.completed_tasks}/{r.num_tasks}</td>
                  <td onClick={() => openRun(r)}>{r.total_input_tokens}/{r.total_output_tokens}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {comparison && (
          <div style={box}>
            <h3 style={{ marginTop: 0 }}>对比</h3>
            <table style={{ width: '100%', fontSize: 13, borderCollapse: 'collapse' }}>
              <thead>
                <tr style={{ textAlign: 'left', color: '#8b90a0' }}>
                  <th>标签</th><th>验证集</th><th>准确率</th><th>tokens(in/out)</th><th>时延(s)</th>
                </tr>
              </thead>
              <tbody>
                {comparison.map((c) => (
                  <tr key={c.id} style={{ borderTop: '1px solid #2c3142' }}>
                    <td>{c.label || c.id}</td>
                    <td>{c.dataset_id}</td>
                    <td>{(c.accuracy * 100).toFixed(1)}%</td>
                    <td>{c.total_input_tokens}/{c.total_output_tokens}</td>
                    <td>{c.total_latency_s?.toFixed?.(2)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {selectedRun && (
          <div style={box}>
            <h3 style={{ marginTop: 0 }}>
              运行详情 · {selectedRun.label || selectedRun.id}
            </h3>
            <table style={{ width: '100%', fontSize: 13, borderCollapse: 'collapse' }}>
              <thead>
                <tr style={{ textAlign: 'left', color: '#8b90a0' }}>
                  <th>任务</th><th>结果</th><th>得分</th><th>events</th><th>final</th><th></th>
                </tr>
              </thead>
              <tbody>
                {taskResults.map((t) => (
                  <tr key={t.task_id} style={{ borderTop: '1px solid #2c3142' }}>
                    <td>{t.task_id}</td>
                    <td>{t.success ? '✅' : '❌'}</td>
                    <td>{t.score?.toFixed?.(2)}</td>
                    <td>{t.events}</td>
                    <td style={{ maxWidth: 280, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {t.final_text}
                    </td>
                    <td>
                      <button onClick={() => openTrajectory(selectedRun.id, t.task_id)}>轨迹</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            {trajectory && (
              <div style={{ marginTop: 12 }}>
                <h4>轨迹回放 · {trajectory.taskId}</h4>
                <div style={{ maxHeight: 280, overflow: 'auto', background: '#11131c', padding: 8, borderRadius: 6 }}>
                  {trajectory.events.map((e) => (
                    <div key={e.seq} style={{ fontSize: 12, padding: '2px 0' }}>
                      <span style={{ color: '#6cc5ff' }}>[{e.type}]</span> {e.content}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default EvaluationDashboard;
