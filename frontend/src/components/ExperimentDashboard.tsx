import { useEffect, useState } from "react";
import { api } from "../api";

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

export function ExperimentDashboard({ activeProject, setActiveProject }: Props) {
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [datasets, setDatasets] = useState<{ id: string; num_tasks: number; scorer: string }[]>([]);
  const [dataset, setDataset] = useState("smoke_qa");
  const [runs, setRuns] = useState<any[]>([]);
  const [board, setBoard] = useState<any[]>([]);
  const [current, setCurrent] = useState<any | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const refreshBoard = () => api.leaderboard().then(setBoard).catch(() => {});

  useEffect(() => {
    api.listProjects().then(setProjects);
    api.listDatasets().then(setDatasets);
    refreshBoard();
  }, []);
  useEffect(() => { if (activeProject) api.listRuns(activeProject).then(setRuns); }, [activeProject]);

  const run = async () => {
    if (!activeProject) return;
    setBusy(true); setErr("");
    try {
      const summary = await api.evalRun(activeProject, dataset);
      const full = await api.getRun(summary.id);
      setCurrent(full);
      api.listRuns(activeProject).then(setRuns);
      refreshBoard();
    } catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="panel">
      <h2>Experiments</h2>
      <div className="row">
        <div className="col">
          <label>Project</label>
          <select value={activeProject ?? ""} onChange={(e) => setActiveProject(e.target.value || null)}>
            <option value="">(select)</option>
            {projects.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
        <div className="col">
          <label>Dataset</label>
          <select value={dataset} onChange={(e) => setDataset(e.target.value)}>
            {datasets.map((d) => <option key={d.id} value={d.id}>{d.id} ({d.num_tasks}, {d.scorer})</option>)}
          </select>
        </div>
        <div className="col" style={{ display: "flex", alignItems: "flex-end" }}>
          <button className="primary" disabled={!activeProject || busy} onClick={run}>{busy ? "Running…" : "Run evaluation"}</button>
        </div>
      </div>
      {err && <div className="err">{err}</div>}

      {current && (
        <div style={{ marginTop: 16 }}>
          <div className="metric">{(current.accuracy * 100).toFixed(0)}%</div>
          <div className="muted">
            accuracy on {current.dataset} · scorer {current.scorer} · {current.task_results.length} tasks
            {current.metrics && ` · avg ${current.metrics.avg_latency}s · ~${current.metrics.total_est_tokens} tok`}
          </div>
          <table>
            <thead><tr><th>task</th><th>score</th><th>prediction</th></tr></thead>
            <tbody>
              {current.task_results.map((t: any) => (
                <tr key={t.task_id}>
                  <td>{t.task_id}</td>
                  <td>{t.score.toFixed(2)}</td>
                  <td className="muted">{String(t.prediction).slice(0, 80)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3>Leaderboard</h3>
      <table>
        <thead><tr><th>#</th><th>project</th><th>dataset</th><th>accuracy</th><th>avg latency</th><th>~tokens</th></tr></thead>
        <tbody>
          {board.map((e, i) => (
            <tr key={e.id} style={{ cursor: "pointer" }} onClick={() => api.getRun(e.id).then(setCurrent)}>
              <td>{i + 1}</td>
              <td>{e.project_name ?? "-"}</td>
              <td>{e.dataset}</td>
              <td>{e.accuracy != null ? (e.accuracy * 100).toFixed(0) + "%" : "-"}</td>
              <td className="muted">{e.avg_latency != null ? e.avg_latency + "s" : "-"}</td>
              <td className="muted">{e.total_est_tokens ?? 0}</td>
            </tr>
          ))}
          {board.length === 0 && <tr><td colSpan={6} className="muted">No runs yet.</td></tr>}
        </tbody>
      </table>

      <h3>Past runs</h3>
      <table>
        <thead><tr><th>run</th><th>dataset</th><th>accuracy</th></tr></thead>
        <tbody>
          {runs.map((r) => (
            <tr key={r.id} style={{ cursor: "pointer" }} onClick={() => api.getRun(r.id).then(setCurrent)}>
              <td>{r.id}</td><td>{r.dataset}</td><td>{r.accuracy != null ? (r.accuracy * 100).toFixed(0) + "%" : "-"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
