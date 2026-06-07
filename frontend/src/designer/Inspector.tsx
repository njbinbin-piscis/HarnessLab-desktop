import { useEffect, useState } from "react";
import { AgentSpec, ComponentSpec, Ref, api } from "../api";
import { useDesigner } from "./store";

const LIFECYCLE_SLOTS = [
  "credentials", "capabilities", "skills", "prompt",
  "approval", "budget", "hooks", "session", "tracing",
] as const;

function EmbeddingControl({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [status, setStatus] = useState<{ ok?: boolean; model?: string } | null>(null);
  useEffect(() => {
    if (value === "openai") {
      api.embeddingStatus(true).then((s) => setStatus({ ok: s.probe?.ok, model: s.model })).catch(() => {});
    } else {
      setStatus(null);
    }
  }, [value]);
  return (
    <div className="insp-embed">
      <span className="insp-embed-label">embedding</span>
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="bow">bow (local)</option>
        <option value="hashing">hashing (local)</option>
        <option value="openai">openai / BGE-M3 (model)</option>
      </select>
      {status ? (
        <span className={"insp-embed-dot " + (status.ok ? "ok" : "warn")} title={status.model}>
          {status.ok ? `● live (${status.model})` : "● endpoint unreachable — using fallback"}
        </span>
      ) : null}
    </div>
  );
}

const SLOTS: ("loop" | "compaction" | "memory" | "context")[] = ["loop", "compaction", "memory", "context"];

function SlotEditor({ agentName, slot, value }: { agentName: string; slot: string; value?: Ref }) {
  const { catalog, applyOps } = useDesigner();
  const ref = value;
  return (
    <div className="insp-slot">
      <div className="insp-field">
        <span>{slot}</span>
        <select
          value={ref?.name ?? ""}
          onChange={(e) =>
            e.target.value
              ? applyOps([{ op: "set_slot", agent: agentName, kind: slot, ref: e.target.value }])
              : applyOps([{ op: "clear_slot", agent: agentName, kind: slot }])
          }
        >
          <option value="">—</option>
          {options(catalog, slot).map((n) => (
            <option key={n} value={n}>{n}</option>
          ))}
        </select>
      </div>
      {ref && specOf(catalog, slot, ref.name)?.params_schema?.embedding ? (
        <EmbeddingControl
          value={String((ref.params as Record<string, unknown> | undefined)?.embedding ?? "bow")}
          onChange={(backend) =>
            applyOps([{ op: "set_slot", agent: agentName, kind: slot, ref: ref.name,
              params: { ...(ref.params ?? {}), embedding: backend } }])
          }
        />
      ) : null}
      {ref ? (
        <ParamsEditor
          value={ref.params ?? {}}
          onCommit={(v) => applyOps([{ op: "set_slot", agent: agentName, kind: slot, ref: ref.name, params: v }])}
        />
      ) : null}
    </div>
  );
}

function options(catalog: Record<string, ComponentSpec[]>, kind: string): string[] {
  return (catalog[kind] ?? []).map((c) => c.name);
}

function specOf(catalog: Record<string, ComponentSpec[]>, kind: string, name: string): ComponentSpec | undefined {
  return (catalog[kind] ?? []).find((c) => c.name === name);
}

function ParamsEditor({ value, onCommit }: { value: Record<string, unknown>; onCommit: (v: Record<string, unknown>) => void }) {
  const [text, setText] = useState(JSON.stringify(value ?? {}, null, 2));
  const [err, setErr] = useState("");
  return (
    <div className="insp-params">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          try {
            onCommit(text.trim() ? JSON.parse(text) : {});
            setErr("");
          } catch {
            setErr("invalid JSON");
          }
        }}
        rows={3}
      />
      {err ? <span className="insp-err">{err}</span> : null}
    </div>
  );
}

function AgentInspector({ agent }: { agent: AgentSpec }) {
  const { catalog, applyOps, locks, clientId } = useDesigner();
  const [name, setName] = useState(agent.name);

  const setParam = (key: string, value: unknown) => applyOps([{ op: "set_param", agent: agent.name, key, value }]);

  const lock = locks[`agent:${agent.name}`];
  const lockedByOther = lock && lock.client_id !== clientId;

  return (
    <div className="insp-body">
      {lockedByOther ? (
        <div className="insp-lock-banner">🔒 {lock.name} 正在编辑该 agent,你的修改可能与之冲突。</div>
      ) : null}
      <label className="insp-field">
        <span>name</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name && name !== agent.name && applyOps([{ op: "rename_agent", name: agent.name, to: name }])}
        />
      </label>
      <label className="insp-field">
        <span>model</span>
        <input defaultValue={agent.model} onBlur={(e) => e.target.value !== agent.model && setParam("model", e.target.value)} />
      </label>
      <label className="insp-field">
        <span>system_prompt</span>
        <textarea defaultValue={agent.system_prompt} rows={3} onBlur={(e) => e.target.value !== agent.system_prompt && setParam("system_prompt", e.target.value)} />
      </label>
      <div className="insp-grid">
        {(["max_turns", "max_tokens", "max_total_tokens"] as const).map((k) => (
          <label className="insp-field" key={k}>
            <span>{k}</span>
            <input type="number" defaultValue={agent[k] ?? 0} onBlur={(e) => Number(e.target.value) !== (agent[k] ?? 0) && setParam(k, Number(e.target.value))} />
          </label>
        ))}
      </div>

      <div className="insp-section">Slots</div>
      {SLOTS.map((slot) => (
        <SlotEditor key={slot} agentName={agent.name} slot={slot} value={agent[slot]} />
      ))}

      <details className="insp-stages">
        <summary>Lifecycle stages <span className="muted">(empty slots — fill to harden)</span></summary>
        {LIFECYCLE_SLOTS.map((slot) => (
          <SlotEditor key={slot} agentName={agent.name} slot={slot} value={agent[slot]} />
        ))}
      </details>

      <div className="insp-section">Tools</div>
      <div className="insp-tools">
        {agent.enabled_tools === null || agent.enabled_tools === undefined ? (
          <span className="muted" style={{ fontSize: 12 }}>all built-ins (drop a tool to restrict)</span>
        ) : (
          agent.enabled_tools.map((t) => (
            <span key={t} className="chip">
              {t}
              <button onClick={() => applyOps([{ op: "detach_tool", agent: agent.name, tool: t }])}>×</button>
            </span>
          ))
        )}
      </div>

      <button className="btn danger insp-del" onClick={() => applyOps([{ op: "remove_agent", name: agent.name }])}>
        Delete agent
      </button>
    </div>
  );
}

function TopologyInspector() {
  const { project, catalog, applyOps } = useDesigner();
  if (!project) return null;
  const t = project.topology;
  return (
    <div className="insp-body">
      <label className="insp-field">
        <span>mode</span>
        <select
          value={t.mode}
          onChange={(e) =>
            e.target.value === "coordinated"
              ? applyOps([{ op: "set_topology", mode: "coordinated", coordinator: options(catalog, "coordinator")[0] ?? "router" }])
              : applyOps([{ op: "set_topology", mode: "single" }])
          }
        >
          <option value="single">single</option>
          <option value="coordinated">coordinated</option>
        </select>
      </label>
      {t.mode === "coordinated" ? (
        <label className="insp-field">
          <span>coordinator</span>
          <select value={t.coordinator?.name ?? ""} onChange={(e) => applyOps([{ op: "set_topology", mode: "coordinated", coordinator: e.target.value }])}>
            {options(catalog, "coordinator").map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
        </label>
      ) : null}
      <div className="insp-section">Agent order</div>
      <ol className="insp-order">
        {t.agent_order.map((n) => (
          <li key={n}>{n}</li>
        ))}
      </ol>
    </div>
  );
}

function ScorerInspector() {
  const { project, catalog, applyOps } = useDesigner();
  if (!project) return null;
  return (
    <div className="insp-body">
      <label className="insp-field">
        <span>scorer</span>
        <select
          value={project.scorer?.name ?? ""}
          onChange={(e) => (e.target.value ? applyOps([{ op: "set_scorer", ref: e.target.value }]) : applyOps([{ op: "clear_scorer" }]))}
        >
          <option value="">—</option>
          {options(catalog, "scorer").map((n) => (
            <option key={n} value={n}>{n}</option>
          ))}
        </select>
      </label>
      <p className="muted" style={{ fontSize: 12 }}>Default scorer used by evaluation runs when none is chosen explicitly.</p>
    </div>
  );
}

export function Inspector() {
  const { selection, project } = useDesigner();
  if (!project) return <aside className="inspector"><div className="insp-empty">No project loaded.</div></aside>;
  let title = "Inspector";
  let body = <div className="insp-empty">Select a node to edit its properties.</div>;
  if (selection?.type === "agent") {
    const agent = project.agents.find((a) => a.name === selection.id);
    if (agent) {
      title = `Agent · ${agent.name}`;
      body = <AgentInspector key={agent.name} agent={agent} />;
    }
  } else if (selection?.type === "topology") {
    title = "Topology";
    body = <TopologyInspector />;
  } else if (selection?.type === "scorer") {
    title = "Scorer";
    body = <ScorerInspector />;
  }
  return (
    <aside className="inspector">
      <div className="insp-head">{title}</div>
      {body}
    </aside>
  );
}
