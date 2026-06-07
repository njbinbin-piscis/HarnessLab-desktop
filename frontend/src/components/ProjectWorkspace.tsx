import { useEffect, useState } from "react";
import { api, ComponentSpec, Project, AgentSpec } from "../api";

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

function emptyAgent(name: string): AgentSpec {
  return {
    name,
    role: "",
    model: "mock",
    system_prompt: "You are a helpful agent.",
    loop: { name: "react", params: {} },
    compaction: { name: "sliding_window", params: {} },
    memory: { name: "noop", params: {} },
    context: { name: "none", params: {} },
  };
}

export function ProjectWorkspace({ activeProject, setActiveProject }: Props) {
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [catalog, setCatalog] = useState<Record<string, ComponentSpec[]>>({});
  const [draft, setDraft] = useState<Project>({
    name: "New Project",
    description: "",
    agents: [emptyAgent("agent1")],
    topology: { mode: "single", agent_order: ["agent1"] },
  });
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");

  const refresh = () => api.listProjects().then(setProjects);
  useEffect(() => { refresh(); api.catalog().then(setCatalog); }, []);

  const names = (kind: string) => (catalog[kind] || []).map((c) => c.name);

  const updateAgent = (i: number, patch: Partial<AgentSpec>) => {
    const agents = draft.agents.slice();
    agents[i] = { ...agents[i], ...patch };
    setDraft({ ...draft, agents });
  };

  const addAgent = () => {
    const name = `agent${draft.agents.length + 1}`;
    setDraft({
      ...draft,
      agents: [...draft.agents, emptyAgent(name)],
      topology: { ...draft.topology, agent_order: [...draft.topology.agent_order, name] },
    });
  };

  const save = async () => {
    setErr(""); setMsg("");
    try {
      const ordered = { ...draft, topology: { ...draft.topology, agent_order: draft.agents.map((a) => a.name) } };
      const saved = await api.createProject(ordered);
      setMsg(`Saved project ${saved.id}`);
      setActiveProject(saved.id ?? null);
      refresh();
    } catch (e) { setErr(String(e)); }
  };

  const loadProject = async (id: string) => {
    const p = await api.getProject(id);
    setDraft(p);
    setActiveProject(id);
  };

  return (
    <div className="row">
      <div className="panel col" style={{ maxWidth: 280 }}>
        <h2>Projects</h2>
        {projects.map((p) => (
          <div key={p.id} className="card" style={{ cursor: "pointer", borderColor: activeProject === p.id ? "var(--accent)" : undefined }}
            onClick={() => loadProject(p.id)}>
            <strong>{p.name}</strong>
            <div className="muted">{p.id}</div>
          </div>
        ))}
        <button className="ghost" onClick={() => setDraft({ name: "New Project", description: "", agents: [emptyAgent("agent1")], topology: { mode: "single", agent_order: ["agent1"] } })}>
          + New draft
        </button>
      </div>

      <div className="panel col">
        <h2>Project editor</h2>
        <label>Name</label>
        <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
        <label>Description</label>
        <input value={draft.description ?? ""} onChange={(e) => setDraft({ ...draft, description: e.target.value })} />

        <h3>Agents</h3>
        {draft.agents.map((a, i) => (
          <div key={i} className="card">
            <div className="row">
              <div className="col"><label>Name</label>
                <input value={a.name} onChange={(e) => updateAgent(i, { name: e.target.value })} /></div>
              <div className="col"><label>Model</label>
                <input value={a.model} onChange={(e) => updateAgent(i, { model: e.target.value })} /></div>
            </div>
            <label>System prompt</label>
            <textarea rows={2} value={a.system_prompt} onChange={(e) => updateAgent(i, { system_prompt: e.target.value })} />
            <div className="row">
              {(["loop", "compaction", "memory", "context"] as const).map((kind) => (
                <div className="col" key={kind}>
                  <label>{kind}</label>
                  <select value={a[kind]?.name ?? ""} onChange={(e) => updateAgent(i, { [kind]: { name: e.target.value, params: {} } } as Partial<AgentSpec>)}>
                    {names(kind).map((n) => <option key={n} value={n}>{n}</option>)}
                  </select>
                </div>
              ))}
            </div>
          </div>
        ))}
        <button className="ghost" onClick={addAgent}>+ Add agent</button>

        <h3>Topology</h3>
        <div className="row">
          <div className="col">
            <label>Mode</label>
            <select value={draft.topology.mode} onChange={(e) => setDraft({ ...draft, topology: { ...draft.topology, mode: e.target.value } })}>
              <option value="single">single</option>
              <option value="coordinated">coordinated</option>
            </select>
          </div>
          {draft.topology.mode === "coordinated" && (
            <div className="col">
              <label>Coordinator</label>
              <select value={draft.topology.coordinator?.name ?? ""} onChange={(e) => setDraft({ ...draft, topology: { ...draft.topology, coordinator: { name: e.target.value, params: {} } } })}>
                <option value="">(choose)</option>
                {names("coordinator").map((n) => <option key={n} value={n}>{n}</option>)}
              </select>
            </div>
          )}
        </div>

        <div style={{ marginTop: 12 }}>
          <button className="primary" onClick={save}>Save project</button>
          {msg && <span className="muted" style={{ marginLeft: 8 }}>{msg}</span>}
          {err && <div className="err">{err}</div>}
        </div>
      </div>
    </div>
  );
}
