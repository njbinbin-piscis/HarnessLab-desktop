import { useEffect, useState } from "react";
import { AgentSpec, ComponentSpec, Ref, api } from "../api";
import { useT } from "../i18n/context";
import { useDesigner } from "./store";
import { Select } from "../components/Select";

const LIFECYCLE_SLOTS = [
  "credentials", "capabilities", "skills", "prompt",
  "approval", "budget", "hooks", "session", "tracing",
] as const;

function EmbeddingControl({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const t = useT();
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
      <Select
        block
        value={value}
        onChange={onChange}
        options={[
          { value: "bow", label: t("inspector.embeddingBow") },
          { value: "hashing", label: t("inspector.embeddingHash") },
          { value: "openai", label: t("inspector.embeddingOpenai") },
        ]}
      />
      {status ? (
        <span className={"insp-embed-dot " + (status.ok ? "ok" : "warn")} title={status.model}>
          {status.ok ? t("inspector.embedLive", { model: status.model ?? "" }) : t("inspector.embedFallback")}
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
        <Select
          block
          value={ref?.name ?? ""}
          onChange={(name) =>
            name
              ? applyOps([{ op: "set_slot", agent: agentName, kind: slot, ref: name }])
              : applyOps([{ op: "clear_slot", agent: agentName, kind: slot }])
          }
          placeholder="—"
          options={[
            { value: "", label: "—" },
            ...options(catalog, slot).map((n) => ({ value: n, label: n })),
          ]}
        />
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
  const t = useT();
  const { catalog, applyOps } = useDesigner();
  const [name, setName] = useState(agent.name);

  const setParam = (key: string, value: unknown) => applyOps([{ op: "set_param", agent: agent.name, key, value }]);

  return (
    <div className="insp-body">
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
        {t("inspector.deleteAgent")}
      </button>
    </div>
  );
}

function TopologyInspector() {
  const tr = useT();
  const { project, catalog, applyOps } = useDesigner();
  if (!project) return null;
  const topo = project.topology;
  return (
    <div className="insp-body">
      <label className="insp-field">
        <span>mode</span>
        <Select
          block
          value={topo.mode}
          onChange={(mode) =>
            mode === "coordinated"
              ? applyOps([{ op: "set_topology", mode: "coordinated", coordinator: options(catalog, "coordinator")[0] ?? "router" }])
              : applyOps([{ op: "set_topology", mode: "single" }])
          }
          options={[
            { value: "single", label: tr("projects.modeSingle") },
            { value: "coordinated", label: tr("projects.modeCoordinated") },
          ]}
        />
      </label>
      {topo.mode === "coordinated" ? (
        <label className="insp-field">
          <span>coordinator</span>
          <Select
            block
            value={topo.coordinator?.name ?? ""}
            onChange={(name) => applyOps([{ op: "set_topology", mode: "coordinated", coordinator: name }])}
            options={options(catalog, "coordinator").map((n) => ({ value: n, label: n }))}
          />
        </label>
      ) : null}
      <div className="insp-section">{tr("inspector.agentOrder")}</div>
      <ol className="insp-order">
        {topo.agent_order.map((n) => (
          <li key={n}>{n}</li>
        ))}
      </ol>
    </div>
  );
}

function ScorerInspector() {
  const t = useT();
  const { project, catalog, applyOps } = useDesigner();
  if (!project) return null;
  return (
    <div className="insp-body">
      <label className="insp-field">
        <span>scorer</span>
        <Select
          block
          value={project.scorer?.name ?? ""}
          onChange={(name) => (name ? applyOps([{ op: "set_scorer", ref: name }]) : applyOps([{ op: "clear_scorer" }]))}
          placeholder="—"
          options={[
            { value: "", label: "—" },
            ...options(catalog, "scorer").map((n) => ({ value: n, label: n })),
          ]}
        />
      </label>
      <p className="muted" style={{ fontSize: 12 }}>{t("inspector.scorerHint")}</p>
    </div>
  );
}

export function Inspector() {
  const t = useT();
  const { selection, project } = useDesigner();
  if (!project) return <aside className="inspector"><div className="insp-empty">{t("inspector.noProject")}</div></aside>;
  let title = t("inspector.title");
  let body = <div className="insp-empty">{t("inspector.selectNode")}</div>;
  if (selection?.type === "agent") {
    const agent = project.agents.find((a) => a.name === selection.id);
    if (agent) {
      title = `${t("inspector.agent")} · ${agent.name}`;
      body = <AgentInspector key={agent.name} agent={agent} />;
    }
  } else if (selection?.type === "topology") {
    title = t("inspector.topology");
    body = <TopologyInspector />;
  } else if (selection?.type === "scorer") {
    title = t("inspector.scorer");
    body = <ScorerInspector />;
  }
  return (
    <aside className="inspector">
      <div className="insp-head">{title}</div>
      {body}
    </aside>
  );
}
