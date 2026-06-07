import { useState } from "react";
import { api, Project } from "../api";
import { useT } from "../i18n/context";
import { Select, selectOptions } from "./Select";

export function DesignStudio({ onSaved }: { onSaved: (id: string) => void }) {
  const t = useT();
  const [request, setRequest] = useState("Build a team that debates to solve hard math problems step by step.");
  const [result, setResult] = useState<{ source: string; project: Project } | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const [kind, setKind] = useState("loop");
  const [cname, setCname] = useState("my_strategy");
  const [cdesc, setCdesc] = useState("A community-authored strategy.");
  const [authored, setAuthored] = useState("");

  const design = async () => {
    setBusy(true); setErr(""); setResult(null);
    try { setResult(await api.design(request)); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  const saveDesigned = async () => {
    if (!result) return;
    const saved = await api.createProject(result.project);
    onSaved(saved.id!);
  };

  const author = async () => {
    setAuthored("");
    try {
      await api.authorComponent(kind, cname, cdesc);
      setAuthored(t("designStudio.authored", { kind, name: cname }));
    } catch (e) { setErr(String(e)); }
  };

  return (
    <div className="design-studio-row page-fill">
      <div className="panel col panel-stack">
        <h2>{t("designStudio.architectTitle")}</h2>
        <div className="design-studio-scroll">
          <label>{t("designStudio.brief")}</label>
          <textarea rows={4} value={request} onChange={(e) => setRequest(e.target.value)} />
          <div className="panel-actions" style={{ marginTop: 8 }}>
            <button type="button" className="btn btn-primary" disabled={busy} onClick={design}>
              {busy ? t("designStudio.designing") : t("designStudio.generateDesign")}
            </button>
          </div>
          {err && <div className="err">{err}</div>}
          {result && (
            <div style={{ marginTop: 12 }}>
              <div className="muted">source: {result.source}</div>
              <div className="code code-in-panel">{JSON.stringify(result.project, null, 2)}</div>
              <div className="panel-actions" style={{ marginTop: 8 }}>
                <button type="button" className="btn btn-primary" onClick={saveDesigned}>{t("designStudio.saveExperiment")}</button>
              </div>
            </div>
          )}
        </div>
      </div>

      <div className="panel col panel-stack" style={{ maxWidth: 360, flex: "0 0 360px" }}>
        <h2>{t("designStudio.authorTitle")}</h2>
        <div className="design-studio-scroll">
          <p className="muted">{t("designStudio.authorHint")}</p>
          <label>{t("designStudio.kind")}</label>
          <Select
            block
            value={kind}
            onChange={setKind}
            options={selectOptions(["loop", "compaction", "memory", "context", "coordinator", "scorer"])}
          />
          <label>{t("common.name")}</label>
          <input value={cname} onChange={(e) => setCname(e.target.value)} />
          <label>{t("common.description")}</label>
          <input value={cdesc} onChange={(e) => setCdesc(e.target.value)} />
          <div className="panel-actions" style={{ marginTop: 8 }}>
            <button type="button" className="btn" onClick={author}>{t("designStudio.authorHotload")}</button>
          </div>
          {authored && <div className="muted" style={{ marginTop: 8 }}>{authored}</div>}
        </div>
      </div>
    </div>
  );
}
