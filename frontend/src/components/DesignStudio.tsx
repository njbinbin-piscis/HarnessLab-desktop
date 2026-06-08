import { useState } from "react";
import { api, Project } from "../api";
import { useT } from "../i18n/context";
import { CodeViewer } from "./CodeViewer";

type Dest = "designer" | "experiments";

export function DesignStudio({ onOpen }: { onOpen: (id: string, dest: Dest) => void }) {
  const t = useT();
  const [request, setRequest] = useState(
    "Build a team that debates to solve hard math problems step by step.",
  );
  const [result, setResult] = useState<{ source: string; explanation: string; project: Project } | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState("");

  const design = async () => {
    setBusy(true);
    setErr("");
    setResult(null);
    try {
      setResult(await api.design(request));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const open = async (dest: Dest) => {
    if (!result) return;
    setSaving(true);
    setErr("");
    try {
      const saved = await api.createProject({ ...result.project, id: undefined });
      onOpen(saved.id!, dest);
    } catch (e) {
      setErr(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page-fill panel-stack">
      <div className="panel col panel-stack">
        <h2>{t("designStudio.architectTitle")}</h2>
        <div className="design-studio-scroll">
          <p className="muted">{t("designStudio.architectHint")}</p>
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
              <div className="muted">
                {t("designStudio.source")}: {result.source}
              </div>
              {result.explanation && (
                <div className="muted" style={{ marginTop: 4 }}>{result.explanation}</div>
              )}
              <div className="design-preview"><CodeViewer code={JSON.stringify(result.project, null, 2)} path="project.json" /></div>
              <div className="panel-actions" style={{ marginTop: 8 }}>
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={saving}
                  onClick={() => open("designer")}
                >
                  {saving ? t("common.loading") : t("designStudio.editInDesigner")}
                </button>
                <button type="button" className="btn" disabled={saving} onClick={() => open("experiments")}>
                  {t("designStudio.saveExperiment")}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
