import { useEffect, useState } from "react";
import { api } from "../api";

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

export function FactoryPanel({ activeProject, setActiveProject }: Props) {
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [languages, setLanguages] = useState<string[]>([]);
  const [language, setLanguage] = useState("python");
  const [rustClient, setRustClient] = useState("std");
  const [tsClient, setTsClient] = useState("fetch");
  const [files, setFiles] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<string>("");
  const [err, setErr] = useState("");

  const codeOptions = () =>
    language === "rust" ? { rust_client: rustClient }
    : language === "typescript" ? { ts_client: tsClient }
    : {};

  useEffect(() => {
    api.listProjects().then(setProjects);
    api.languages().then((l) => { setLanguages(l.languages); });
  }, []);

  const generate = async () => {
    if (!activeProject) return;
    setErr("");
    try {
      const res = await api.generate(activeProject, language, codeOptions());
      setFiles(res.files);
      setSelected(Object.keys(res.files)[0] ?? "");
    } catch (e) { setErr(String(e)); }
  };

  const download = () => {
    if (!activeProject) return;
    const form = document.createElement("form");
    form.method = "POST";
    form.action = api.downloadUrl;
    // Use fetch instead to send JSON body; trigger blob download.
    fetch(api.downloadUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ project_id: activeProject, language, options: codeOptions() }),
    }).then(async (r) => {
      const blob = await r.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url; a.download = `project_${language}.zip`; a.click();
      URL.revokeObjectURL(url);
    });
  };

  return (
    <div className="panel">
      <h2>Factory — generate production multi-agent code</h2>
      <div className="row">
        <div className="col">
          <label>Project</label>
          <select value={activeProject ?? ""} onChange={(e) => setActiveProject(e.target.value || null)}>
            <option value="">(select)</option>
            {projects.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
        <div className="col">
          <label>Target language</label>
          <select value={language} onChange={(e) => setLanguage(e.target.value)}>
            {languages.map((l) => <option key={l} value={l}>{l}</option>)}
          </select>
        </div>
        {language === "rust" && (
          <div className="col">
            <label>Rust LLM client</label>
            <select value={rustClient} onChange={(e) => setRustClient(e.target.value)}>
              <option value="std">std-only (offline mock)</option>
              <option value="reqwest">reqwest (real OpenAI-compatible)</option>
            </select>
          </div>
        )}
        {language === "typescript" && (
          <div className="col">
            <label>TypeScript LLM client</label>
            <select value={tsClient} onChange={(e) => setTsClient(e.target.value)}>
              <option value="fetch">fetch (built-in)</option>
              <option value="openai">openai (npm SDK)</option>
            </select>
          </div>
        )}
        <div className="col" style={{ display: "flex", alignItems: "flex-end", gap: 8 }}>
          <button className="primary" disabled={!activeProject} onClick={generate}>Generate</button>
          <button className="ghost" disabled={!activeProject} onClick={download}>Download .zip</button>
        </div>
      </div>
      {err && <div className="err">{err}</div>}

      {Object.keys(files).length > 0 && (
        <div className="row" style={{ marginTop: 12 }}>
          <div className="col" style={{ maxWidth: 280 }}>
            {Object.keys(files).map((f) => (
              <div key={f} className="card" style={{ cursor: "pointer", borderColor: selected === f ? "var(--accent)" : undefined }} onClick={() => setSelected(f)}>
                {f}
              </div>
            ))}
          </div>
          <div className="col">
            <div className="code">{files[selected]}</div>
          </div>
        </div>
      )}
    </div>
  );
}
