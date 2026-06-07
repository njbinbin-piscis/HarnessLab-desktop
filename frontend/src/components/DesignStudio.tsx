import { useState } from "react";
import { api, Project } from "../api";

export function DesignStudio({ onSaved }: { onSaved: (id: string) => void }) {
  const [request, setRequest] = useState("Build a team that debates to solve hard math problems step by step.");
  const [result, setResult] = useState<{ source: string; project: Project } | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  // component authoring
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
      setAuthored(`Authored ${kind}:${cname} and hot-loaded it into the library.`);
    } catch (e) { setErr(String(e)); }
  };

  return (
    <div className="row">
      <div className="panel col">
        <h2>Architect — design a project</h2>
        <label>Brief</label>
        <textarea rows={4} value={request} onChange={(e) => setRequest(e.target.value)} />
        <div style={{ marginTop: 8 }}>
          <button className="primary" disabled={busy} onClick={design}>{busy ? "Designing…" : "Generate design"}</button>
        </div>
        {err && <div className="err">{err}</div>}
        {result && (
          <div style={{ marginTop: 12 }}>
            <div className="muted">source: {result.source}</div>
            <div className="code">{JSON.stringify(result.project, null, 2)}</div>
            <button className="primary" style={{ marginTop: 8 }} onClick={saveDesigned}>Save & experiment</button>
          </div>
        )}
      </div>

      <div className="panel col" style={{ maxWidth: 360 }}>
        <h2>Author a new algorithm</h2>
        <p className="muted">Python components are hot-loaded — no recompilation.</p>
        <label>Kind</label>
        <select value={kind} onChange={(e) => setKind(e.target.value)}>
          {["loop", "compaction", "memory", "context", "coordinator", "scorer"].map((k) => <option key={k}>{k}</option>)}
        </select>
        <label>Name</label>
        <input value={cname} onChange={(e) => setCname(e.target.value)} />
        <label>Description</label>
        <input value={cdesc} onChange={(e) => setCdesc(e.target.value)} />
        <button className="ghost" style={{ marginTop: 8 }} onClick={author}>Author & hot-load</button>
        {authored && <div className="muted" style={{ marginTop: 8 }}>{authored}</div>}
      </div>
    </div>
  );
}
