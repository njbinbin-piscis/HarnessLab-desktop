import { create } from "zustand";
import {
  AgentEvent, ApiError, api, CanvasRun, ComponentSpec, EditOp, EditProposal,
  isDesktop, Layout, NodeMetrics, Project, RunMsg, RuntimeRun, SessionNode,
} from "../api";
import { t } from "../i18n/index";

export type Selection =
  | { type: "agent"; id: string }
  | { type: "topology" }
  | { type: "scorer" }
  | null;

interface DesignerState {
  projectId: string | null;
  project: Project | null;
  layout: Layout;
  catalog: Record<string, ComponentSpec[]>;
  selection: Selection;
  status: string;
  error: string;

  // P2 — run / observability
  running: boolean;
  runLog: AgentEvent[];
  runResult: string;
  metrics: Record<string, NodeMetrics>;
  // run history & comparison
  runs: CanvasRun[];
  overlayRunId: string | null; // which historical run's metrics are shown on the canvas
  // session (fork) tree
  session: SessionNode[];
  sessionRoots: string[];
  forkParent: string | null; // next run branches under this run node
  forkTurn: number | null; // when set, resume from this turn snapshot
  sessionPanelOpen: boolean;
  sessionExpanded: string[];
  sessionPanelPosition: { x: number; y: number };
  // runtime monitor (live agent state)
  runtimeRuns: RuntimeRun[];
  runtimeActive: number;
  runtimeAgentPhase: Record<string, string>; // agent name -> phase (current project)
  // P3 — copilot
  proposing: boolean;
  proposal: EditProposal | null;
  // P6 — undo / redo (project snapshots)
  undoStack: Project[];
  redoStack: Project[];

  load: (projectId: string) => Promise<void>;
  reload: () => Promise<void>;
  select: (sel: Selection) => void;
  clearError: () => void;
  applyOps: (ops: EditOp[]) => Promise<boolean>;
  setNodePos: (id: string, x: number, y: number) => void;
  persistLayout: () => void;
  runTask: (task: string) => Promise<void>;
  loadSession: () => Promise<void>;
  setForkParent: (runId: string | null, turn?: number | null) => void;
  setSessionPanelOpen: (open: boolean) => void;
  setSessionExpanded: (ids: string[]) => void;
  toggleSessionExpanded: (runId: string) => void;
  setViewport: (viewport: { x: number; y: number; zoom: number }) => void;
  setSessionPanelPosition: (x: number, y: number) => void;
  startRuntimeMonitor: (projectId: string) => void;
  stopRuntimeMonitor: () => void;
  propose: (instruction: string) => Promise<void>;
  clearProposal: () => void;
  applyProposal: () => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  loadRuns: () => Promise<void>;
  showRunOnCanvas: (runId: string | null) => void;
}

let runtimeUnlisten: (() => void) | null = null;

function computeMetrics(events: AgentEvent[]): Record<string, NodeMetrics> {
  const m: Record<string, NodeMetrics> = {};
  const span: Record<string, { lo: number; hi: number }> = {};
  for (const e of events) {
    const a = e.agent;
    if (!m[a]) m[a] = { turns: 0, tools: 0, tokens: 0, ms: 0 };
    if (e.kind === "turn_start") m[a].turns += 1;
    if (e.kind === "tool_call") m[a].tools += 1;
    if (e.kind === "run_end") m[a].tokens += Number(e.data?.total_tokens ?? 0);
    const s = (span[a] ??= { lo: e.ts, hi: e.ts });
    s.lo = Math.min(s.lo, e.ts);
    s.hi = Math.max(s.hi, e.ts);
  }
  for (const a of Object.keys(m)) m[a].ms = Math.round((span[a].hi - span[a].lo) * 1000);
  return m;
}

let layoutTimer: ReturnType<typeof setTimeout> | null = null;

export const useDesigner = create<DesignerState>((set, get) => ({
  projectId: null,
  project: null,
  layout: { nodes: {} },
  catalog: {},
  selection: null,
  status: "",
  error: "",
  running: false,
  runLog: [],
  runResult: "",
  metrics: {},
  runs: [],
  overlayRunId: null,
  session: [],
  sessionRoots: [],
  forkParent: null,
  forkTurn: null,
  sessionPanelOpen: false,
  sessionExpanded: [],
  sessionPanelPosition: { x: 12, y: 12 },
  runtimeRuns: [],
  runtimeActive: 0,
  runtimeAgentPhase: {},
  proposing: false,
  proposal: null,
  undoStack: [],
  redoStack: [],

  load: async (projectId: string) => {
    set({ projectId, status: "loading", error: "" });
    try {
      const [project, layout, catalog] = await Promise.all([
        api.getProject(projectId),
        api.getLayout(projectId).catch(() => ({ nodes: {} } as Layout)),
        api.catalog(),
      ]);
      const safeLayout: Layout = { ...layout, nodes: layout?.nodes ?? {} };
      set({
        project, layout: safeLayout, catalog, status: "", selection: null,
        runs: [], metrics: {}, overlayRunId: null,
        undoStack: [], redoStack: [],
        sessionPanelOpen: safeLayout.session?.open ?? false,
        sessionExpanded: safeLayout.session?.expanded ?? [],
        sessionPanelPosition: safeLayout.session?.position ?? { x: 12, y: 12 },
      });
      get().loadRuns();
      get().loadSession();
    } catch (e) {
      set({ error: String(e), status: "" });
    }
  },

  reload: async () => {
    const id = get().projectId;
    if (id) await get().load(id);
  },

  select: (sel) => set({ selection: sel }),

  clearError: () => set({ error: "" }),

  applyOps: async (ops) => {
    const id = get().projectId;
    if (!id) return false;
    set({ status: "applying", error: "" });
    const current = get().project;
    try {
      const res = await api.applyOps(id, ops, false, get().project?.rev);
      set({
        project: res.project,
        status: "",
        undoStack: current
          ? [...get().undoStack.slice(-49), structuredClone(current)]
          : get().undoStack,
        redoStack: [],
      });
      return true;
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        // the project changed under us (e.g. another window) — refresh and warn
        await get().reload();
        set({ error: t("errors.projectStale"), status: "" });
        return false;
      }
      set({ error: String(e), status: "" });
      return false;
    }
  },

  setNodePos: (id, x, y) => {
    const layout = get().layout;
    const next: Layout = { ...layout, nodes: { ...layout.nodes, [id]: { x, y } } };
    set({ layout: next });
    get().persistLayout();
  },

  persistLayout: () => {
    const id = get().projectId;
    if (!id) return;
    if (layoutTimer) clearTimeout(layoutTimer);
    layoutTimer = setTimeout(() => {
      api.saveLayout(id, get().layout).catch(() => {});
    }, 500);
  },

  runTask: async (task: string) => {
    const id = get().projectId;
    if (!id || !task.trim()) return;
    set({ running: true, error: "", runLog: [], runResult: "", metrics: {}, overlayRunId: null });
    const events: AgentEvent[] = [];
    // Light up nodes live by deriving a coarse phase from streamed events.
    const phaseFor = (e: AgentEvent): string | null => {
      if (e.kind === "tool_call") return "tool";
      if (e.kind === "turn_start" || e.kind === "text") return "think";
      if (e.kind === "run_end" || e.kind === "cancelled" || e.kind === "error") return "done";
      return null;
    };
    const onMsg = (msg: RunMsg) => {
      if (msg.type === "event") {
        const e = msg.event;
        events.push(e);
        const phase = phaseFor(e);
        const patch: Partial<DesignerState> = { runLog: [...events], metrics: computeMetrics(events) };
        if (phase) patch.runtimeAgentPhase = { ...get().runtimeAgentPhase, [e.agent]: phase };
        set(patch);
      } else if (msg.type === "result") {
        set({ runResult: msg.result ?? "" });
        if (msg.run) set({ runs: [msg.run, ...get().runs] });
      } else if (msg.type === "error") {
        set({ error: msg.message ?? "run error" });
      }
    };
    try {
      await api.runProjectChannel(id, task, onMsg);
      set({ running: false, forkParent: null, forkTurn: null, runtimeAgentPhase: {} });
      get().loadSession();
    } catch (e) {
      set({ error: String(e), running: false, runtimeAgentPhase: {} });
    }
  },

  loadSession: async () => {
    const id = get().projectId;
    if (!id) return;
    try {
      const { nodes, roots } = await api.sessionTree(id);
      set({ session: nodes, sessionRoots: roots });
    } catch {
      /* ignore */
    }
  },

  setForkParent: (runId: string | null, turn: number | null = null) =>
    set({ forkParent: runId, forkTurn: turn }),

  setSessionPanelOpen: (open: boolean) => {
    const pos = get().sessionPanelPosition;
    const layout: Layout = {
      ...get().layout,
      session: { ...get().layout.session, open, expanded: get().sessionExpanded, position: pos },
    };
    set({ sessionPanelOpen: open, layout });
    get().persistLayout();
  },

  setSessionExpanded: (ids: string[]) => {
    const pos = get().sessionPanelPosition;
    const layout: Layout = {
      ...get().layout,
      session: { ...get().layout.session, open: get().sessionPanelOpen, expanded: ids, position: pos },
    };
    set({ sessionExpanded: ids, layout });
    get().persistLayout();
  },

  toggleSessionExpanded: (runId: string) => {
    const cur = new Set(get().sessionExpanded);
    cur.has(runId) ? cur.delete(runId) : cur.add(runId);
    get().setSessionExpanded([...cur]);
  },

  setViewport: (viewport: { x: number; y: number; zoom: number }) => {
    const layout: Layout = { ...get().layout, viewport };
    set({ layout });
    get().persistLayout();
  },

  setSessionPanelPosition: (x: number, y: number) => {
    const layout: Layout = {
      ...get().layout,
      session: {
        ...get().layout.session,
        open: get().sessionPanelOpen,
        expanded: get().sessionExpanded,
        position: { x: Math.round(x), y: Math.round(y) },
      },
    };
    set({ sessionPanelPosition: { x: Math.round(x), y: Math.round(y) }, layout });
    get().persistLayout();
  },

  startRuntimeMonitor: (projectId: string) => {
    get().stopRuntimeMonitor();
    if (!isDesktop()) return;
    const apply = (runs: RuntimeRun[], active: number) => {
      const phases: Record<string, string> = {};
      for (const r of runs) {
        if (r.project_id !== projectId || r.status !== "running") continue;
        for (const [name, ag] of Object.entries(r.agents)) {
          phases[name] = ag.phase;
        }
      }
      set({ runtimeRuns: runs, runtimeActive: active, runtimeAgentPhase: phases });
    };
    // Seed from the current snapshot, then subscribe to live updates.
    api.runtimeRuns().then((snap) => apply(snap.runs, snap.active)).catch(() => {});
    api
      .onRuntimeUpdate((snap) => apply(snap.runs, snap.active))
      .then((un) => { runtimeUnlisten = un; })
      .catch(() => {});
  },

  stopRuntimeMonitor: () => {
    if (runtimeUnlisten) {
      runtimeUnlisten();
      runtimeUnlisten = null;
    }
    set({ runtimeRuns: [], runtimeActive: 0, runtimeAgentPhase: {} });
  },

  propose: async (instruction: string) => {
    const id = get().projectId;
    if (!id || !instruction.trim()) return;
    set({ proposing: true, error: "", proposal: null });
    try {
      const proposal = await api.designEdit(id, instruction);
      set({ proposal, proposing: false });
    } catch (e) {
      set({ error: String(e), proposing: false });
    }
  },

  clearProposal: () => set({ proposal: null }),

  applyProposal: async () => {
    const p = get().proposal;
    if (!p || !p.preview.valid) return;
    const ok = await get().applyOps(p.ops);
    if (ok) set({ proposal: null });
  },

  undo: async () => {
    const { projectId, project, undoStack, redoStack } = get();
    if (!projectId || undoStack.length === 0) return;
    const prev = undoStack[undoStack.length - 1];
    const nextUndo = undoStack.slice(0, -1);
    const nextRedo = project ? [...redoStack, structuredClone(project)] : redoStack;
    set({ status: "applying", error: "" });
    try {
      const saved = await api.updateProject(projectId, { ...prev, id: projectId });
      set({ project: saved, undoStack: nextUndo, redoStack: nextRedo, status: "" });
    } catch (e) {
      set({ error: String(e), status: "" });
    }
  },

  redo: async () => {
    const { projectId, project, undoStack, redoStack } = get();
    if (!projectId || redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1];
    const nextRedo = redoStack.slice(0, -1);
    const nextUndo = project ? [...undoStack, structuredClone(project)] : undoStack;
    set({ status: "applying", error: "" });
    try {
      const saved = await api.updateProject(projectId, { ...next, id: projectId });
      set({ project: saved, undoStack: nextUndo, redoStack: nextRedo, status: "" });
    } catch (e) {
      set({ error: String(e), status: "" });
    }
  },

  loadRuns: async () => {
    const id = get().projectId;
    if (!id) return;
    try {
      const { runs } = await api.listCanvasRuns(id);
      set({ runs });
    } catch {
      /* ignore */
    }
  },

  showRunOnCanvas: (runId: string | null) => {
    if (runId === null) {
      set({ overlayRunId: null, metrics: {} });
      return;
    }
    const run = get().runs.find((r) => r.id === runId);
    if (run) set({ overlayRunId: runId, metrics: run.metrics });
  },
}));
