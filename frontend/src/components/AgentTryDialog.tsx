import { useEffect, useRef, useState } from "react";
import { api, LibraryAgentSummary, LibraryRunMsg } from "../api";
import { useT } from "../i18n/context";

interface Props {
  agent: LibraryAgentSummary;
  onClose: () => void;
}

export function AgentTryDialog({ agent, onClose }: Props) {
  const t = useT();
  const [task, setTask] = useState("What is 2 + 2? Reply with FINAL ANSWER:");
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<LibraryRunMsg[]>([]);
  const [result, setResult] = useState("");
  const [err, setErr] = useState("");
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: "smooth" });
  }, [log, result]);

  const run = async () => {
    setRunning(true);
    setLog([]);
    setResult("");
    setErr("");
    try {
      await api.runLibraryAgentChannel(agent.id, task, (msg) => {
        if (msg.type === "event") setLog((s) => [...s, msg]);
        else if (msg.type === "result") setResult(msg.result);
        else if (msg.type === "error") setErr(msg.message);
      });
    } catch (e) {
      setErr(String(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal try-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="try-head">
          <h3>{t("agentLibrary.tryTitle", { name: agent.name })}</h3>
          <button type="button" className="btn btn-sm" onClick={onClose}>×</button>
        </div>
        <div className="try-body">
          <div className="dock-row">
            <input
              value={task}
              onChange={(e) => setTask(e.target.value)}
              placeholder={t("common.taskPlaceholder")}
              onKeyDown={(e) => e.key === "Enter" && !running && run()}
            />
            <button type="button" className="btn btn-primary" disabled={running} onClick={run}>
              {running ? t("experiments.running") : t("common.run")}
            </button>
          </div>
          {err && <div className="err">{err}</div>}
          {result && <div className="run-result"><b>{t("agentLibrary.result")}:</b> {result}</div>}
          <div className="run-log try-log">
            {log.map((entry, i) =>
              entry.type === "event" ? (
                <div key={i} className={"log-line k-" + entry.event.kind}>
                  <span className="log-agent">{entry.event.agent}</span>
                  <span className="log-kind">{entry.event.kind}</span>
                  <span className="log-text">{entry.event.text}</span>
                </div>
              ) : null,
            )}
            <div ref={bottom} />
          </div>
        </div>
      </div>
    </div>
  );
}
