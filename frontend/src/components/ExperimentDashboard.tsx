import { useEffect, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n/context";
import { Select } from "./Select";

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

export function ExperimentDashboard({ activeProject, setActiveProject }: Props) {
  const t = useT();
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
    <div className="panel panel-stack page-fill">
      <h2>{t("experiments.title")}</h2>
      <div className="panel-body-scroll">
        <div className="row nowrap">
          <div className="col">
            <label>{t("experiments.project")}</label>
            <Select
              block
              value={activeProject ?? ""}
              onChange={(id) => setActiveProject(id || null)}
              placeholder={t("common.selectPlaceholder")}
              options={projects.map((p) => ({ value: p.id, label: p.name }))}
            />
          </div>
          <div className="col">
            <label>{t("experiments.dataset")}</label>
            <Select
              block
              value={dataset}
              onChange={setDataset}
              options={datasets.map((d) => ({
                value: d.id,
                label: `${d.id} (${d.num_tasks}, ${d.scorer})`,
              }))}
            />
          </div>
          <div className="col" style={{ display: "flex", flexDirection: "column", justifyContent: "flex-end", flex: "0 0 auto" }}>
            <button type="button" className="btn btn-primary" disabled={!activeProject || busy} onClick={run}>
              {busy ? t("experiments.running") : t("experiments.runEval")}
            </button>
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
            <div className="table-scroll" style={{ maxHeight: 240 }}>
              <table>
                <thead><tr><th>{t("experiments.task")}</th><th>{t("experiments.score")}</th><th>{t("experiments.prediction")}</th></tr></thead>
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
          </div>
        )}

        <h3>{t("experiments.leaderboard")}</h3>
        <div className="table-scroll" style={{ maxHeight: 200 }}>
          <table>
            <thead><tr><th>#</th><th>{t("common.project")}</th><th>{t("experiments.dataset")}</th><th>{t("experiments.accuracy")}</th><th>{t("experiments.avgLatency")}</th><th>{t("experiments.tokens")}</th></tr></thead>
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
              {board.length === 0 && <tr><td colSpan={6} className="muted">{t("experiments.noRuns")}</td></tr>}
            </tbody>
          </table>
        </div>

        <h3>{t("experiments.pastRuns")}</h3>
        <div className="table-scroll" style={{ maxHeight: 200 }}>
          <table>
            <thead><tr><th>{t("experiments.runCol")}</th><th>{t("experiments.dataset")}</th><th>{t("experiments.accuracy")}</th></tr></thead>
            <tbody>
              {runs.map((r) => (
                <tr key={r.id} style={{ cursor: "pointer" }} onClick={() => api.getRun(r.id).then(setCurrent)}>
                  <td>{r.id}</td><td>{r.dataset}</td><td>{r.accuracy != null ? (r.accuracy * 100).toFixed(0) + "%" : "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
