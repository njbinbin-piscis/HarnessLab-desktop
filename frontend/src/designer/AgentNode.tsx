import { Handle, Position } from "@xyflow/react";
import { AgentSpec } from "../api";
import { useDesigner } from "./store";

const SLOTS: { key: "loop" | "compaction" | "memory" | "context"; label: string }[] = [
  { key: "loop", label: "loop" },
  { key: "compaction", label: "compact" },
  { key: "memory", label: "memory" },
  { key: "context", label: "context" },
];

type DragPayload = { kind: string; name: string };

function readPayload(e: React.DragEvent): DragPayload | null {
  try {
    return JSON.parse(e.dataTransfer.getData("application/harnesslab"));
  } catch {
    return null;
  }
}

export function AgentNode({ data, selected }: { data: { agent: AgentSpec }; selected?: boolean }) {
  const agent = data.agent;
  const applyOps = useDesigner((s) => s.applyOps);
  const metrics = useDesigner((s) => s.metrics[agent.name]);
  const livePhase = useDesigner((s) => s.runtimeAgentPhase[agent.name]);

  const onDropSlot = (slot: string) => (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const p = readPayload(e);
    if (!p) return;
    if (p.kind === slot) applyOps([{ op: "set_slot", agent: agent.name, kind: slot, ref: p.name }]);
  };

  const onDropTool = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const p = readPayload(e);
    if (p && p.kind === "tool") applyOps([{ op: "attach_tool", agent: agent.name, tool: p.name }]);
  };

  const allow = (e: React.DragEvent) => e.preventDefault();
  const tools = agent.enabled_tools ?? null;

  return (
    <div className={"agent-node" + (selected ? " selected" : "")}>
      <Handle type="target" position={Position.Left} />
      <div className="an-head">
        <span className="an-name">{agent.name}</span>
        {livePhase && livePhase !== "done" ? (
          <span className={"an-live phase-" + livePhase} title={"running: " + livePhase}>{livePhase}</span>
        ) : null}
        <span className="badge lang">{agent.model || "mock"}</span>
      </div>
      <div className="an-slots">
        {SLOTS.map((s) => {
          const ref = agent[s.key];
          return (
            <div
              key={s.key}
              className={"an-slot" + (ref ? " filled" : "")}
              onDragOver={allow}
              onDrop={onDropSlot(s.key)}
              title={`Drop a ${s.key} component here`}
            >
              <span className="an-slot-k">{s.label}</span>
              <span className="an-slot-v">{ref ? ref.name : "—"}</span>
            </div>
          );
        })}
      </div>
      <div className="an-tools" onDragOver={allow} onDrop={onDropTool} title="Drop a tool here">
        <span className="an-tools-label">tools</span>
        {tools === null ? (
          <span className="badge">all</span>
        ) : tools.length === 0 ? (
          <span className="muted" style={{ fontSize: 11 }}>none</span>
        ) : (
          tools.map((t) => <span key={t} className="badge">{t}</span>)
        )}
      </div>
      {metrics ? (
        <div className="an-metrics" title="last run">
          <span>{metrics.turns} turns</span>
          <span>{metrics.tools} tools</span>
          <span>{metrics.tokens.toLocaleString()} tok</span>
          <span>{metrics.ms.toLocaleString()} ms</span>
        </div>
      ) : null}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
