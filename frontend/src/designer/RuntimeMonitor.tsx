import { RuntimeRun } from "../api";
import { useT } from "../i18n/context";
import { useDesigner } from "./store";

function AgentRow({ ag }: { ag: RuntimeRun["agents"][string] }) {
  const t = useT();
  const phaseLabel = (phase: string) => {
    const key = `runtime.phase.${phase}` as const;
    const label = t(key);
    return label === key ? phase : label;
  };
  return (
    <div className={"rt-agent phase-" + ag.phase}>
      <span className="rt-agent-name">{ag.agent}</span>
      <span className="rt-phase">{phaseLabel(ag.phase)}</span>
      <span className="rt-meta">T{ag.turn} · {ag.tools} tools · {ag.tokens} tok</span>
      {ag.last_text ? <span className="rt-snippet" title={ag.last_text}>{ag.last_text}</span> : null}
    </div>
  );
}

export function RuntimeMonitorPanel() {
  const t = useT();
  const { runtimeRuns, runtimeActive, projectId } = useDesigner();

  const runs = runtimeRuns.filter((r) => !projectId || r.project_id === projectId);
  const statusText = runtimeActive > 0 ? t("runtime.active", { count: runtimeActive }) : t("runtime.idle");

  return (
    <div className="dock-pane">
      <div className="dock-row">
        <span className="muted" style={{ fontSize: 12 }}>
          {t("runtime.title", { status: statusText })}
        </span>
      </div>
      {runs.length === 0 ? (
        <span className="muted" style={{ fontSize: 12 }}>{t("runtime.noRuns")}</span>
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
              {r.result ? <div className="rt-result">{r.result}</div> : null}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
