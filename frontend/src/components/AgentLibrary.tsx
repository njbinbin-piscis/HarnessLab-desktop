import { useEffect, useState } from "react";
import { api, ComponentSpec, LibraryAgent } from "../api";
import { useT } from "../i18n/context";
import { AgentEditorForm, defaultLibraryAgentSpec } from "./AgentEditorForm";
import { AgentTryDialog } from "./AgentTryDialog";

function newDraft(): LibraryAgent {
  const name = "New Agent";
  return {
    id: "",
    name,
    description: "",
    spec: defaultLibraryAgentSpec(name),
  };
}

export function AgentLibrary() {
  const t = useT();
  const [list, setList] = useState<Awaited<ReturnType<typeof api.listLibraryAgents>>>([]);
  const [catalog, setCatalog] = useState<Record<string, ComponentSpec[]>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<LibraryAgent | null>(null);
  const [tryAgent, setTryAgent] = useState<(typeof list)[0] | null>(null);
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");

  const refresh = () => api.listLibraryAgents().then(setList);

  useEffect(() => {
    refresh();
    api.catalog().then(setCatalog);
  }, []);

  const load = async (id: string) => {
    const a = await api.getLibraryAgent(id);
    if (a) {
      setSelectedId(id);
      setDraft(a);
    }
  };

  const startNew = () => {
    setSelectedId(null);
    setDraft(newDraft());
  };

  const save = async () => {
    if (!draft) return;
    setErr("");
    setMsg("");
    const payload = { ...draft, spec: { ...draft.spec, name: draft.name } };
    try {
      if (selectedId) {
        const saved = await api.updateLibraryAgent(selectedId, payload);
        if (saved) {
          setDraft(saved);
          setMsg(t("agentLibrary.saved"));
        }
      } else {
        const saved = await api.createLibraryAgent(payload);
        setDraft(saved);
        setSelectedId(saved.id);
        setMsg(t("agentLibrary.created"));
      }
      refresh();
    } catch (e) {
      setErr(String(e));
    }
  };

  const remove = async () => {
    if (!selectedId) return;
    if (!confirm(t("agentLibrary.deleteConfirm"))) return;
    await api.deleteLibraryAgent(selectedId);
    setSelectedId(null);
    setDraft(null);
    refresh();
  };

  return (
    <div className="page-split agent-library">
      <div className="panel col panel-stack" style={{ maxWidth: 280, flex: "0 0 280px" }}>
        <h2>{t("agentLibrary.listTitle")}</h2>
        <div className="panel-body-scroll">
          {list.map((a) => (
            <div
              key={a.id}
              className={"card lib-agent-item" + (selectedId === a.id ? " active" : "")}
              style={{ cursor: "pointer", borderColor: selectedId === a.id ? "var(--accent)" : undefined }}
              onClick={() => load(a.id)}
            >
              <strong>{a.name}</strong>
              <div className="muted">{a.model}</div>
              {a.description ? <div className="muted" style={{ fontSize: 12 }}>{a.description}</div> : null}
            </div>
          ))}
          <button type="button" className="btn" onClick={startNew}>{t("agentLibrary.newAgent")}</button>
        </div>
      </div>

      <div className="panel col panel-stack">
        {!draft ? (
          <div className="empty">{t("agentLibrary.emptySelect")}</div>
        ) : (
          <>
            <div className="panel-actions panel-intro">
              <button type="button" className="btn btn-primary" onClick={save}>{t("common.save")}</button>
              {selectedId ? (
                <>
                  <button type="button" className="btn" onClick={() => setTryAgent(list.find((x) => x.id === selectedId) ?? null)}>
                    {t("agentLibrary.tryRun")}
                  </button>
                  <button type="button" className="btn" onClick={remove}>{t("common.remove")}</button>
                </>
              ) : null}
              {msg && <span className="muted">{msg}</span>}
            </div>
            {err && <div className="err panel-intro">{err}</div>}
            <div className="panel-body-scroll">
              <AgentEditorForm
                name={draft.name}
                description={draft.description ?? ""}
                spec={draft.spec}
                catalog={catalog}
                onName={(name) => setDraft({ ...draft, name, spec: { ...draft.spec, name } })}
                onDescription={(description) => setDraft({ ...draft, description })}
                onSpec={(spec) => setDraft({ ...draft, spec })}
              />
            </div>
          </>
        )}
      </div>

      {tryAgent ? <AgentTryDialog agent={tryAgent} onClose={() => setTryAgent(null)} /> : null}
    </div>
  );
}
