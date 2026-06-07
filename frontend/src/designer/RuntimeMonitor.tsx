import { RuntimeRun } from "../api";
import { useDesigner } from "./store";

const PHASE_LABEL: Record<string, string> = {
  running: "启动",
  turn: "推理",
  tool: "工具",
  done: "完成",
  error: "错误",
  idle: "空闲",
};

function AgentRow({ ag }: { ag: RuntimeRun["agents"][string] }) {
  return (
    <div className={"rt-agent phase-" + ag.phase}>
      <span className="rt-agent-name">{ag.agent}</span>
      <span className="rt-phase">{PHASE_LABEL[ag.phase] ?? ag.phase}</span>
      <span className="rt-meta">T{ag.turn} · {ag.tools} tools · {ag.tokens} tok</span>
      {ag.last_text ? <span className="rt-snippet" title={ag.last_text}>{ag.last_text}</span> : null}
    </div>
  );
}

export function RuntimeMonitorPanel() {
  const { runtimeRuns, runtimeActive, projectId } = useDesigner();

  const runs = runtimeRuns.filter((r) => !projectId || r.project_id === projectId);

  return (
    <div className="dock-pane">
      <div className="dock-row">
        <span className="muted" style={{ fontSize: 12 }}>
          实时 agent 运行状态 · {runtimeActive > 0 ? `${runtimeActive} 个运行中` : "空闲"}
        </span>
      </div>
      {runs.length === 0 ? (
        <span className="muted" style={{ fontSize: 12 }}>暂无运行 — 在 Run 里启动任务后此处会实时更新。</span>
      ) : (
        <div className="rt-list">
          {runs.map((r) => (
            <div key={r.run_id} className={"rt-run status-" + r.status}>
              <div className="rt-run-head">
                <span className="rt-id">{r.run_id.slice(0, 8)}</span>
                <span className={"rt-status " + r.status}>{r.status}</span>
                <span className="rt-task" title={r.task}>{r.task}</span>
              </div>
              <div className="rt-agents">
                {Object.values(r.agents).map((ag) => (
                  <AgentRow key={ag.agent} ag={ag} />
                ))}
              </div>
              {r.error ? <div className="rt-error">{r.error}</div> : null}
              {r.result && r.status === "completed" ? (
                <div className="rt-result" title={r.result}>{r.result.slice(0, 120)}</div>
              ) : null}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
