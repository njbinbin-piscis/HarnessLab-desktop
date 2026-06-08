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
  type Node as FlowNode,
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
import { useT } from "../i18n/context";
import { Select } from "./Select";

const COORD_ID = "__coordinator__";
const SCORER_ID = "__scorer__";

function CoordinatorNode({ data }: { data: { label: string } }) {
  const t = useT();
  return (
    <div className="coord-node">
      <Handle type="target" position={Position.Left} />
      <span className="cn-kicker">{t("designer.coordinator")}</span>
      <span className="cn-name">{data.label}</span>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function ScorerNode({ data }: { data: { label: string } }) {
  const t = useT();
  return (
    <div className="scorer-node">
      <Handle type="target" position={Position.Left} />
      <span className="cn-kicker">{t("designer.scorer")}</span>
      <span className="cn-name">{data.label || t("common.dash")}</span>
    </div>
  );
}

const nodeTypes: NodeTypes = { agent: AgentNode, coordinator: CoordinatorNode, scorer: ScorerNode };

function minimapNodeColor(node: FlowNode): string {
  switch (node.type) {
    case "coordinator":
      return "#ff6b6b";
    case "scorer":
      return "#4dd0e1";
    default:
      return "#5b8cff";
  }
}

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
        colorMode="dark"
        defaultViewport={layout.viewport}
        fitView={!layout.viewport}
        onNodeClick={onNodeClick}
        onNodeDragStop={onNodeDragStop}
        onConnect={onConnect}
        onMoveEnd={onMoveEnd}
        onPaneClick={() => select(null)}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={18} color="var(--border)" />
        <Controls showInteractive={false} position="bottom-left" className="designer-flow-controls" />
        <MiniMap
          className="designer-flow-minimap"
          position="bottom-right"
          pannable
          zoomable={false}
          nodeColor={minimapNodeColor}
          nodeStrokeWidth={0}
          bgColor="var(--panel)"
          maskColor="rgba(11, 13, 18, 0.72)"
          maskStrokeColor="var(--accent)"
          maskStrokeWidth={1}
          style={{ width: 108, height: 68 }}
          offsetScale={3}
        />
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
  const t = useT();
  const { overlayRunId, runs, showRunOnCanvas } = useDesigner();
  if (!overlayRunId) return null;
  const run = runs.find((r) => r.id === overlayRunId);
  return (
    <div className="canvas-busy overlay" onClick={() => showRunOnCanvas(null)} title={t("designer.clickToClear")} style={{ pointerEvents: "auto", cursor: "pointer" }}>
      {t("designer.viewingRun")}{run ? ` · ${run.total_tokens.toLocaleString()} tok` : ""} ✕
    </div>
  );
}

export function VisualDesigner({ activeProject, setActiveProject }: Props) {
  const t = useT();
  const { projectId, project, status, load, applyOps, setNodePos, undo, redo, undoStack, redoStack, sessionPanelOpen, setSessionPanelOpen, runtimeActive, startRuntimeMonitor, stopRuntimeMonitor } =
    useDesigner();
  const [projects, setProjects] = useState<{ id: string; name: string }[]>([]);
  const [templates, setTemplates] = useState<{ id: string; name: string }[]>([]);
  const [templatePick, setTemplatePick] = useState("");
  const [libraryAgents, setLibraryAgents] = useState<{ id: string; name: string }[]>([]);
  const [agentPick, setAgentPick] = useState("");
  const [dockOpen, setDockOpen] = useState(true);

  useEffect(() => {
    api.listProjects().then(setProjects).catch(() => {});
    api.listLibraryAgents().then((a) => setLibraryAgents(a.map((x) => ({ id: x.id, name: x.name })))).catch(() => {});
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      } else if (e.key === "z" && e.shiftKey || e.key === "y") {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  useEffect(() => {
    if (activeProject && activeProject !== projectId) load(activeProject);
  }, [activeProject, projectId, load]);

  useEffect(() => {
    if (!projectId) return;
    startRuntimeMonitor(projectId);
    return () => {
      stopRuntimeMonitor();
    };
  }, [projectId, startRuntimeMonitor, stopRuntimeMonitor]);

  const importFromLibrary = async (libraryId: string) => {
    if (!libraryId || !project) return;
    const lib = await api.getLibraryAgent(libraryId);
    if (!lib) return;
    const taken = new Set(project.agents.map((a) => a.name));
    if (taken.has(lib.name)) return;
    await applyOps([{ op: "import_agent", spec: lib.spec }]);
  };

  const autoLayout = () => {
    if (!project) return;
    const { nodes } = buildGraph(project, {});
    for (const n of nodes) {
      setNodePos(n.id, Math.round(n.position.x), Math.round(n.position.y));
    }
  };

  const createFromTemplate = async (templateId: string) => {
    if (!templateId) return;
    try {
      const p = await api.createFromTemplate(templateId);
      setActiveProject(p.id ?? null);
      setProjects(await api.listProjects());
    } catch {
      /* ignore */
    }
  };

  return (
    <div className="designer page-fill">
      <div className="designer-toolbar control-row">
        <Select
          value={activeProject ?? ""}
          onChange={(id) => setActiveProject(id || null)}
          placeholder={t("designer.selectProject")}
          options={projects.map((p) => ({ value: p.id, label: p.name }))}
        />
        <Select
          value={agentPick}
          onChange={(id) => {
            setAgentPick("");
            if (id) importFromLibrary(id);
          }}
          disabled={!project}
          placeholder={t("agentLibrary.addToProject")}
          options={libraryAgents
            .filter((a) => !project?.agents.some((ag) => ag.name === a.name))
            .map((a) => ({ value: a.id, label: a.name }))}
        />
        <button type="button" className="btn" onClick={autoLayout} disabled={!project} title="Dagre auto-layout">{t("designer.autoLayout")}</button>
        <button type="button" className="btn" onClick={undo} disabled={!project || undoStack.length === 0} title="Ctrl+Z">{t("designer.undo")}</button>
        <button type="button" className="btn" onClick={redo} disabled={!project || redoStack.length === 0} title="Ctrl+Shift+Z">{t("designer.redo")}</button>
        <Select
          value={templatePick}
          onChange={(id) => {
            setTemplatePick("");
            if (id) createFromTemplate(id);
          }}
          placeholder={t("designer.template")}
          options={templates.map((tpl) => ({ value: tpl.id, label: tpl.name }))}
          aria-label="New project from template"
        />
        <span className="spacer" />
        {status ? <span className="muted">{status}…</span> : null}
        {runtimeActive > 0 ? (
          <span className="runtime-badge" title="agent run in progress">{t("designer.runningBadge", { count: runtimeActive })}</span>
        ) : null}
        <button type="button" className={"btn" + (sessionPanelOpen ? " btn-primary" : "")} onClick={() => setSessionPanelOpen(!sessionPanelOpen)} disabled={!project} title="Session tree on canvas">
          {t("designer.session")}
        </button>
        <button type="button" className={"btn" + (dockOpen ? " btn-primary" : "")} onClick={() => setDockOpen((s) => !s)} disabled={!project}>
          {dockOpen ? t("designer.hideConsole") : t("designer.showConsole")}
        </button>
      </div>

      {!project ? (
        <div className="designer-empty">{t("designer.empty")}</div>
      ) : (
        <div className="designer-grid">
          <Palette />
          <div className={"designer-center" + (dockOpen ? " with-dock" : "")}>
            <div className="canvas-host">
              <ReactFlowProvider>
                <Canvas />
              </ReactFlowProvider>
              {status === "applying" ? <div className="canvas-busy">{t("designer.applying")}</div> : null}
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
