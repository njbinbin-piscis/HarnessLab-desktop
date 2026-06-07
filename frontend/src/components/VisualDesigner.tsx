import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  Connection,
  Controls,
  Edge,
  Handle,
  MiniMap,
  Node,
  NodeTypes,
  Position,
  ReactFlow,
  ReactFlowProvider,
  Viewport,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import dagre from "dagre";
import { api, Project } from "../api";
import { AgentNode } from "../designer/AgentNode";
import { Palette } from "../designer/Palette";
import { Inspector } from "../designer/Inspector";
import { Dock } from "../designer/Dock";
import { SessionTreePanel } from "../designer/SessionTreePanel";
import { useDesigner } from "../designer/store";

const COORD_ID = "__coordinator__";
const SCORER_ID = "__scorer__";

function CoordinatorNode({ data }: { data: { label: string } }) {
  return (
    <div className="coord-node">
      <Handle type="target" position={Position.Left} />
      <span className="cn-kicker">coordinator</span>
      <span className="cn-name">{data.label}</span>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function ScorerNode({ data }: { data: { label: string } }) {
  return (
    <div className="scorer-node">
      <Handle type="target" position={Position.Left} />
      <span className="cn-kicker">scorer</span>
      <span className="cn-name">{data.label || "—"}</span>
    </div>
  );
}

const nodeTypes: NodeTypes = { agent: AgentNode, coordinator: CoordinatorNode, scorer: ScorerNode };

function autoLayout(ids: { id: string; w: number; h: number }[], edges: Edge[]): Record<string, { x: number; y: number }> {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 90 });
  ids.forEach((n) => g.setNode(n.id, { width: n.w, height: n.h }));
  edges.forEach((e) => g.setEdge(e.source, e.target));
  dagre.layout(g);
  const out: Record<string, { x: number; y: number }> = {};
  ids.forEach((n) => {
    const p = g.node(n.id);
    out[n.id] = { x: p.x - n.w / 2, y: p.y - n.h / 2 };
  });
  return out;
}

function buildGraph(project: Project, saved: Record<string, { x: number; y: number }>) {
  const edges: Edge[] = [];
  const t = project.topology;
  const hasCoord = t.mode === "coordinated";
  if (hasCoord) {
    for (const name of t.agent_order.length ? t.agent_order : project.agents.map((a) => a.name)) {
      edges.push({ id: `c-${name}`, source: COORD_ID, target: name, animated: true });
    }
  }
  const order = t.agent_order.length ? t.agent_order : project.agents.map((a) => a.name);
  // Faint sequence edges encode agent_order; drag-to-connect reorders them.
  for (let i = 0; i < order.length - 1; i++) {
    edges.push({ id: `ord-${order[i]}-${order[i + 1]}`, source: order[i], target: order[i + 1],
                 className: "order-edge", style: { strokeDasharray: "4 4" } });
  }
  const flowSource = hasCoord ? COORD_ID : order[0];
  if (flowSource) edges.push({ id: "to-scorer", source: flowSource, target: SCORER_ID });

  const sizes = [
    ...project.agents.map((a) => ({ id: a.name, w: 220, h: 150 })),
    ...(hasCoord ? [{ id: COORD_ID, w: 150, h: 70 }] : []),
    { id: SCORER_ID, w: 140, h: 64 },
  ];
  const auto = autoLayout(sizes, edges);
  const pos = (id: string) => saved[id] ?? auto[id] ?? { x: 0, y: 0 };

  const nodes: Node[] = [];
  if (hasCoord) {
    nodes.push({ id: COORD_ID, type: "coordinator", position: pos(COORD_ID), data: { label: t.coordinator?.name ?? "?" } });
  }
  for (const a of project.agents) {
    nodes.push({ id: a.name, type: "agent", position: pos(a.name), data: { agent: a } });
  }
  nodes.push({ id: SCORER_ID, type: "scorer", position: pos(SCORER_ID), data: { label: project.scorer?.name ?? "" } });
  return { nodes, edges };
}

function Canvas() {
  const { project, layout, selection, select, applyOps, setNodePos, setViewport } = useDesigner();
  const wrapper = useRef<HTMLDivElement>(null);

  const { nodes, edges } = useMemo(
    () => (project ? buildGraph(project, layout?.nodes ?? {}) : { nodes: [], edges: [] }),
    [project, layout?.nodes],
  );

  const styledNodes = nodes.map((n) => ({
    ...n,
    selected:
      (selection?.type === "agent" && selection.id === n.id) ||
      (selection?.type === "topology" && n.id === COORD_ID) ||
      (selection?.type === "scorer" && n.id === SCORER_ID),
  }));

  const onNodeClick = useCallback(
    (_: unknown, node: Node) => {
      if (node.id === COORD_ID) select({ type: "topology" });
      else if (node.id === SCORER_ID) select({ type: "scorer" });
      else select({ type: "agent", id: node.id });
    },
    [select],
  );

  const onNodeDragStop = useCallback(
    (_: unknown, node: Node) => setNodePos(node.id, Math.round(node.position.x), Math.round(node.position.y)),
    [setNodePos],
  );

  const onConnect = useCallback(
    (c: Connection) => {
      if (!project) return;
      const special = new Set([COORD_ID, SCORER_ID]);
      if (!c.source || !c.target || special.has(c.source) || special.has(c.target) || c.source === c.target) return;
      const order = (project.topology.agent_order.length ? project.topology.agent_order : project.agents.map((a) => a.name)).slice();
      const next = order.filter((n) => n !== c.target);
      next.splice(next.indexOf(c.source) + 1, 0, c.target);
      const op: Record<string, unknown> = { op: "set_topology", mode: project.topology.mode, order: next };
      if (project.topology.mode === "coordinated") op.coordinator = project.topology.coordinator?.name;
      applyOps([op]);
    },
    [project, applyOps],
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      let payload: { kind: string; name: string } | null = null;
      try {
        payload = JSON.parse(e.dataTransfer.getData("application/harnesslab"));
      } catch {
        return;
      }
      if (!payload) return;
      if (payload.kind === "coordinator") applyOps([{ op: "set_topology", mode: "coordinated", coordinator: payload.name }]);
      else if (payload.kind === "scorer") applyOps([{ op: "set_scorer", ref: payload.name }]);
    },
    [applyOps],
  );

  const onMoveEnd = useCallback(
    (_: unknown, vp: Viewport) => setViewport({ x: vp.x, y: vp.y, zoom: vp.zoom }),
    [setViewport],
  );

  return (
    <div className="canvas-wrap" ref={wrapper} onDragOver={(e) => e.preventDefault()} onDrop={onDrop}>
      <ReactFlow
        nodes={styledNodes}
        edges={edges}
        nodeTypes={nodeTypes}
        defaultViewport={layout.viewport}
        fitView={!layout.viewport}
        onNodeClick={onNodeClick}
        onNodeDragStop={onNodeDragStop}
        onConnect={onConnect}
        onMoveEnd={onMoveEnd}
        onPaneClick={() => select(null)}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={18} />
        <Controls showInteractive={false} />
        <MiniMap pannable zoomable />
      </ReactFlow>
    </div>
  );
}

interface Props {
  activeProject: string | null;
  setActiveProject: (id: string | null) => void;
}

function Toast() {
  const { error, clearError } = useDesigner();
  useEffect(() => {
    if (!error) return;
    const t = setTimeout(clearError, 5000);
    return () => clearTimeout(t);
  }, [error, clearError]);
  if (!error) return null;
  return (
    <div className="toast" role="alert" onClick={clearError}>
      <span className="toast-x">!</span>
      <span className="toast-msg">{error}</span>
      <button className="toast-close">×</button>
    </div>
  );
}

function OverlayBadge() {
  const { overlayRunId, runs, showRunOnCanvas } = useDesigner();
  if (!overlayRunId) return null;
  const run = runs.find((r) => r.id === overlayRunId);
  return (
    <div className="canvas-busy overlay" onClick={() => showRunOnCanvas(null)} title="click to clear" style={{ pointerEvents: "auto", cursor: "pointer" }}>
      viewing past run{run ? ` · ${run.total_tokens.toLocaleString()} tok` : ""} ✕
    </div>
  );
}

function LeaseControl() {
  const { lease, handoff, clientId, acquireLease, releaseLease, requestLease } = useDesigner();
  // Someone else holds the lease → show read-only badge + a "request access" nudge.
  if (lease && lease.client_id !== clientId) {
    const asked = !!handoff && handoff.client_id === clientId;
    return (
      <span className="lease-group">
        <span className="lease-badge other" title="project is locked for editing">🔒 {lease.name} 编辑中 (只读)</span>
        <button
          className="btn lease-request"
          onClick={requestLease}
          disabled={asked}
          title="ask the holder to release the edit lease"
        >
          {asked ? "已请求…" : "请求编辑权"}
        </button>
      </span>
    );
  }
  // I hold the lease → release button, with a notice if someone is waiting.
  if (lease && lease.client_id === clientId) {
    const waiting = handoff && handoff.client_id !== clientId;
    return (
      <button
        className={"btn lease-mine" + (waiting ? " lease-wanted" : "")}
        onClick={releaseLease}
        title={waiting ? `${handoff!.name} 想要编辑权,点击释放` : "release the exclusive edit lease"}
      >
        {waiting ? `释放编辑租约 (${handoff!.name} 等待中)` : "释放编辑租约"}
      </button>
    );
  }
  return <button className="btn" onClick={acquireLease} title="take an exclusive edit lease">获取编辑租约</button>;
}

function PresenceBar() {
  const { presence, clientId } = useDesigner();
  if (presence.length === 0) return null;
  return (
    <div className="presence" title={`${presence.length} viewing`}>
      {presence.map((p) => (
        <span
          key={p.client_id}
          className={"presence-chip" + (p.client_id === clientId ? " me" : "")}
          style={{ background: p.color }}
          title={p.name + (p.client_id === clientId ? " (you)" : "")}
        >
          {p.name.slice(0, 2).toUpperCase()}
        </span>
      ))}
    </div>
  );
}

export function VisualDesigner({ activeProject, setActiveProject }: Props) {
  const { projectId, project, status, selection, lease, clientId, load, applyOps, startCollab, stopCollab, lockTarget, unlockTarget, sessionPanelOpen, setSessionPanelOpen, runtimeActive, startRuntimeMonitor, stopRuntimeMonitor } =
    useDesigner();
  const readOnly = !!lease && lease.client_id !== clientId;
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [dockOpen, setDockOpen] = useState(true);
  const lockedRef = useRef<string | null>(null);

  useEffect(() => {
    api.listProjects().then(setProjects).catch(() => {});
  }, []);

  useEffect(() => {
    if (activeProject && activeProject !== projectId) load(activeProject);
  }, [activeProject, projectId, load]);

  useEffect(() => {
    if (!projectId) return;
    startCollab(projectId);
    startRuntimeMonitor(projectId);
    return () => {
      stopCollab();
      stopRuntimeMonitor();
    };
  }, [projectId, startCollab, stopCollab, startRuntimeMonitor, stopRuntimeMonitor]);

  // Soft-lock the selected agent so collaborators see who's editing it.
  useEffect(() => {
    const want = selection?.type === "agent" ? `agent:${selection.id}` : null;
    if (lockedRef.current && lockedRef.current !== want) {
      unlockTarget(lockedRef.current);
      lockedRef.current = null;
    }
    if (want) {
      lockTarget(want);
      lockedRef.current = want;
    }
  }, [selection, lockTarget, unlockTarget]);

  const addAgent = () => {
    if (!project) return;
    let i = project.agents.length + 1;
    let name = `agent${i}`;
    const taken = new Set(project.agents.map((a) => a.name));
    while (taken.has(name)) name = `agent${++i}`;
    applyOps([{ op: "add_agent", name, loop: "react" }]);
  };

  return (
    <div className="designer">
      <div className="designer-toolbar">
        <select
          value={activeProject ?? ""}
          onChange={(e) => setActiveProject(e.target.value || null)}
        >
          <option value="">Select a project…</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
        <button className="btn" onClick={addAgent} disabled={!project || readOnly}>+ Agent</button>
        <span className="spacer" />
        <LeaseControl />
        <PresenceBar />
        {status ? <span className="muted">{status}…</span> : null}
        {runtimeActive > 0 ? (
          <span className="runtime-badge" title="agent run in progress">{runtimeActive} running</span>
        ) : null}
        <button className={"btn" + (sessionPanelOpen ? " primary" : "")} onClick={() => setSessionPanelOpen(!sessionPanelOpen)} disabled={!project} title="Session tree on canvas">
          Session
        </button>
        <button className={"btn" + (dockOpen ? " primary" : "")} onClick={() => setDockOpen((s) => !s)} disabled={!project}>
          {dockOpen ? "Hide console" : "Run · Copilot · Code"}
        </button>
      </div>

      {!project ? (
        <div className="designer-empty">Select a project to open it on the visual canvas.</div>
      ) : (
        <div className="designer-grid">
          <Palette />
          <div className={"designer-center" + (dockOpen ? " with-dock" : "")}>
            <div className="canvas-host">
              <ReactFlowProvider>
                <Canvas />
              </ReactFlowProvider>
              {status === "applying" ? <div className="canvas-busy">applying…</div> : null}
              <OverlayBadge />
              {sessionPanelOpen ? <SessionTreePanel onClose={() => setSessionPanelOpen(false)} /> : null}
            </div>
            {dockOpen ? <Dock onClose={() => setDockOpen(false)} /> : null}
          </div>
          <Inspector />
        </div>
      )}
      <Toast />
    </div>
  );
}
