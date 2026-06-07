import { useEffect, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n/context";
import { Select } from "./Select";

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

export function FactoryPanel({ activeProject, setActiveProject }: Props) {
  const t = useT();
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [languages, setLanguages] = useState<string[]>([]);
  const [language, setLanguage] = useState("python");
  const [rustClient, setRustClient] = useState("std");
  const [tsClient, setTsClient] = useState("fetch");
  const [files, setFiles] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<string>("");
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");

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
    setMsg("");
    try {
      const res = await api.generate(activeProject, language, codeOptions());
      setFiles(res.files);
      setSelected(Object.keys(res.files)[0] ?? "");
    } catch (e) { setErr(String(e)); }
  };

  const download = async () => {
    if (!activeProject) return;
    setErr("");
    setMsg("");
    try {
      const saved = await api.downloadFactoryZip(activeProject, language, codeOptions());
      if (saved) setMsg(t("factory.savedTo", { path: saved }));
      else setMsg(t("factory.downloadCancelled"));
    } catch (e) { setErr(String(e)); }
  };

  const fileKeys = Object.keys(files);

  return (
    <div className="panel panel-stack page-fill">
      <h2>{t("factory.title")}</h2>
      <div className="row nowrap panel-intro">
        <div className="col">
          <label>{t("factory.project")}</label>
          <Select
            block
            value={activeProject ?? ""}
            onChange={(id) => setActiveProject(id || null)}
            placeholder={t("common.selectPlaceholder")}
            options={projects.map((p) => ({ value: p.id, label: p.name }))}
          />
        </div>
        <div className="col">
          <label>{t("factory.targetLanguage")}</label>
          <Select
            block
            value={language}
            onChange={setLanguage}
            options={languages.map((l) => ({ value: l, label: l }))}
          />
        </div>
        {language === "rust" && (
          <div className="col">
            <label>{t("factory.rustClient")}</label>
            <Select
              block
              value={rustClient}
              onChange={setRustClient}
              options={[
                { value: "std", label: t("factory.rustStd") },
                { value: "reqwest", label: t("factory.rustReqwest") },
              ]}
            />
          </div>
        )}
        {language === "typescript" && (
          <div className="col">
            <label>{t("factory.tsClient")}</label>
            <Select
              block
              value={tsClient}
              onChange={setTsClient}
              options={[
                { value: "fetch", label: t("factory.tsFetch") },
                { value: "openai", label: t("factory.tsOpenai") },
              ]}
            />
          </div>
        )}
        <div className="col" style={{ display: "flex", flexDirection: "column", justifyContent: "flex-end", flex: "0 0 auto" }}>
          <div className="panel-actions">
            <button type="button" className="btn btn-primary" disabled={!activeProject} onClick={generate}>{t("common.generate")}</button>
            <button type="button" className="btn" disabled={!activeProject} onClick={download}>{t("factory.downloadZip")}</button>
          </div>
        </div>
      </div>
      {msg && <div className="muted panel-intro">{msg}</div>}
      {err && <div className="err panel-intro">{err}</div>}

      {fileKeys.length > 0 && (
        <div className="factory-output">
          <div className="factory-files">
            {fileKeys.map((f) => (
              <div key={f} className="card" style={{ cursor: "pointer", borderColor: selected === f ? "var(--accent)" : undefined }} onClick={() => setSelected(f)}>
                {f}
              </div>
            ))}
          </div>
          <div className="factory-code-col">
            <div className="code">{files[selected]}</div>
          </div>
        </div>
      )}
    </div>
  );
}
