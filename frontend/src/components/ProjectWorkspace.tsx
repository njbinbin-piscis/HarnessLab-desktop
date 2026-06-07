import { useEffect, useState } from "react";
import { api, ComponentSpec, LibraryAgentSummary, Project, ToolSpec } from "../api";
import { useT } from "../i18n/context";
import { Select } from "./Select";

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

export function ProjectWorkspace({ activeProject, setActiveProject }: Props) {
  const t = useT();
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [library, setLibrary] = useState<LibraryAgentSummary[]>([]);
  const [allTools, setAllTools] = useState<ToolSpec[]>([]);
  // Per-agent tool visibility override (library agent id → visible tool names).
  // `undefined` means "use the library agent's own tool set" (all by default).
  const [toolVis, setToolVis] = useState<Record<string, string[] | undefined>>({});
  const [catalog, setCatalog] = useState<Record<string, ComponentSpec[]>>({});
  const [name, setName] = useState("New Project");
  const [description, setDescription] = useState("");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [mode, setMode] = useState("single");
  const [coordinator, setCoordinator] = useState("");
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);

  const refresh = () => api.listProjects().then(setProjects);
  useEffect(() => {
    refresh();
    api.listLibraryAgents().then(setLibrary);
    api.catalog().then(setCatalog);
    api.tools().then((r) => setAllTools(r.tools));
  }, []);

  const coordNames = (catalog.coordinator ?? []).map((c) => c.name);
  const toolNames = allTools.map((tl) => tl.name);

  const toggleAgent = (id: string) => {
    setSelectedIds((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
  };

  // The effective visible tool set for an agent (defaults to all tools).
  const visibleFor = (id: string): string[] => toolVis[id] ?? toolNames;

  const toggleTool = (id: string, tool: string) => {
    setToolVis((prev) => {
      const current = prev[id] ?? toolNames;
      const next = current.includes(tool)
        ? current.filter((t) => t !== tool)
        : [...current, tool];
      return { ...prev, [id]: next };
    });
  };

  const newDraft = () => {
    setEditingId(null);
    setActiveProject(null);
    setName("New Project");
    setDescription("");
    setSelectedIds([]);
    setMode("single");
    setCoordinator("");
    setErr("");
    setMsg("");
  };

  const loadProject = async (id: string) => {
    const p = await api.getProject(id);
    setEditingId(id);
    setActiveProject(id);
    setName(p.name);
    setDescription(p.description ?? "");
    setMode(p.topology.mode);
    setCoordinator(p.topology.coordinator?.name ?? "");
    const ids: string[] = [];
    const vis: Record<string, string[] | undefined> = {};
    for (const a of p.agents) {
      const hit = library.find((l) => l.name === a.name);
      if (hit) {
        ids.push(hit.id);
        if (a.enabled_tools) vis[hit.id] = a.enabled_tools;
      }
    }
    setSelectedIds(ids);
    setToolVis(vis);
  };

  useEffect(() => {
    if (activeProject && activeProject !== editingId && library.length > 0) {
      loadProject(activeProject);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProject, library.length]);

  const save = async () => {
    setErr("");
    setMsg("");
    if (selectedIds.length === 0) {
      setErr(t("agentLibrary.pickOne"));
      return;
    }
    try {
      const agents = await Promise.all(
        selectedIds.map(async (id) => {
          const lib = await api.getLibraryAgent(id);
          if (!lib) throw new Error(`Agent ${id} not found`);
          const spec = lib.spec;
          // Apply the per-agent tool-visibility override chosen for this project.
          const override = toolVis[id];
          if (override !== undefined) {
            return { ...spec, enabled_tools: override };
          }
          return spec;
        }),
      );
      const agentOrder = agents.map((a) => a.name);
      const project: Project = {
        name,
        description,
        agents,
        topology: {
          mode,
          agent_order: agentOrder,
          coordinator: mode === "coordinated" && coordinator
            ? { name: coordinator, params: {} }
            : undefined,
        },
      };
      if (editingId) {
        const saved = await api.updateProject(editingId, { ...project, id: editingId });
        setMsg(t("projects.saved", { id: saved.id! }));
        setActiveProject(saved.id ?? null);
      } else {
        const saved = await api.createProject(project);
        setEditingId(saved.id ?? null);
        setMsg(t("projects.saved", { id: saved.id! }));
        setActiveProject(saved.id ?? null);
      }
      refresh();
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <div className="page-split">
      <div className="panel col panel-stack" style={{ maxWidth: 280, flex: "0 0 280px" }}>
        <h2>{t("projects.listTitle")}</h2>
        <div className="panel-body-scroll">
          {projects.map((p) => (
            <div
              key={p.id}
              className="card"
              style={{ cursor: "pointer", borderColor: editingId === p.id ? "var(--accent)" : undefined }}
              onClick={() => loadProject(p.id)}
            >
              <strong>{p.name}</strong>
              <div className="muted">{p.id}</div>
            </div>
          ))}
          <button type="button" className="btn" onClick={newDraft}>{t("projects.newDraft")}</button>
        </div>
      </div>

      <div className="panel col panel-stack">
        <h2>{t("projects.editorTitle")}</h2>
        <div className="panel-body-scroll">
          <label>{t("common.name")}</label>
          <input value={name} onChange={(e) => setName(e.target.value)} />
          <label>{t("common.description")}</label>
          <input value={description} onChange={(e) => setDescription(e.target.value)} />

          <h3>{t("agentLibrary.pickForProject")}</h3>
          <p className="muted" style={{ fontSize: 12 }}>{t("agentLibrary.pickHint")}</p>
          {library.length === 0 ? (
            <div className="muted">{t("agentLibrary.emptyLibrary")}</div>
          ) : (
            <div className="agent-pick-list">
              {library.map((a) => {
                const selected = selectedIds.includes(a.id);
                const visible = visibleFor(a.id);
                return (
                  <div key={a.id} className="agent-pick-wrap">
                    <label className={"agent-pick" + (selected ? " on" : "")}>
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={() => toggleAgent(a.id)}
                      />
                      <span className="agent-pick-name">{a.name}</span>
                      <span className="muted">{a.model}</span>
                    </label>
                    {selected && allTools.length > 0 && (
                      <div className="agent-tool-vis">
                        <div className="muted" style={{ fontSize: 12 }}>
                          {t("agentLibrary.visibleTools")} ·{" "}
                          {visible.length === toolNames.length
                            ? t("agentLibrary.allTools")
                            : `${visible.length}/${toolNames.length}`}
                        </div>
                        <div className="tool-vis-chips">
                          {allTools.map((tl) => (
                            <span
                              key={tl.name}
                              className={"chip" + (visible.includes(tl.name) ? " active" : "")}
                              onClick={() => toggleTool(a.id, tl.name)}
                              title={tl.description}
                            >
                              {tl.name}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}

          <h3>{t("projects.topology")}</h3>
          <div className="row">
            <div className="col">
              <label>{t("common.mode")}</label>
              <Select
                block
                value={mode}
                onChange={setMode}
                options={[
                  { value: "single", label: t("projects.modeSingle") },
                  { value: "coordinated", label: t("projects.modeCoordinated") },
                ]}
              />
            </div>
            {mode === "coordinated" && (
              <div className="col">
                <label>{t("projects.coordinator")}</label>
                <Select
                  block
                  value={coordinator}
                  onChange={setCoordinator}
                  placeholder={t("projects.choose")}
                  options={coordNames.map((n) => ({ value: n, label: n }))}
                />
              </div>
            )}
          </div>

          <div className="panel-actions" style={{ marginTop: 12 }}>
            <button type="button" className="btn btn-primary" onClick={save}>{t("projects.saveProject")}</button>
            {msg && <span className="muted" style={{ marginLeft: 8 }}>{msg}</span>}
            {err && <div className="err">{err}</div>}
          </div>
        </div>
      </div>
    </div>
  );
}
