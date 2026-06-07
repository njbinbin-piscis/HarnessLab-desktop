import { create } from "zustand";
import {
  AgentEvent, ApiError, api, CanvasRun, CollabState, ComponentSpec, EditOp, EditProposal,
  isDesktop, Layout, NodeMetrics, Project, RunMsg, RuntimeRun, SessionNode,
} from "../api";
import { t } from "../i18n/index";

function clientIdentity(): { id: string; name: string } {
  let id = localStorage.getItem("hl_client_id");
  if (!id) {
    id = "c_" + Math.random().toString(36).slice(2, 10);
    localStorage.setItem("hl_client_id", id);
  }
  let name = localStorage.getItem("hl_client_name") || "";
  if (!name) {
    name = "User-" + id.slice(2, 6);
    localStorage.setItem("hl_client_name", name);
  }
  return { id, name };
}

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
  // collaboration
  clientId: string;
  clientName: string;
  presence: CollabState["presence"];
  locks: CollabState["locks"];
  lease: CollabState["lease"];
  handoff: CollabState["handoff"];

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
  startCollab: (projectId: string) => void;
  stopCollab: () => void;
  lockTarget: (target: string) => Promise<boolean>;
  unlockTarget: (target: string) => void;
  acquireLease: () => Promise<void>;
  releaseLease: () => void;
  requestLease: () => Promise<void>;
  loadRuns: () => Promise<void>;
  showRunOnCanvas: (runId: string | null) => void;
}

let runtimeSource: EventSource | null = null;

let collabTimer: ReturnType<typeof setInterval> | null = null;
let collabSource: EventSource | null = null;
let collabSocket: WebSocket | null = null;

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
  clientId: clientIdentity().id,
  clientName: clientIdentity().name,
  presence: [],
  locks: {},
  lease: null,
  handoff: null,

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
    // Don't even attempt an edit while someone else holds the exclusive lease.
    const lease = get().lease;
    if (lease && lease.client_id !== get().clientId) {
      set({ error: t("errors.leaseReadonly", { name: lease.name }) });
      return false;
    }
    set({ status: "applying", error: "" });
    const current = get().project;
    try {
      const res = await api.applyOps(id, ops, false, get().project?.rev, get().clientId);
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
        // a collaborator changed the project under us — refresh and warn
        await get().reload();
        set({ error: t("errors.projectStale"), status: "" });
        return false;
      }
      if (e instanceof ApiError && e.status === 423) {
        const holder = (e.detail as { holder?: string })?.holder ?? "…";
        set({ error: t("errors.leaseHolderReadonly", { holder }), status: "" });
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
    // Live runtime monitoring lands in P3; skip on the desktop for now.
    if (isDesktop()) return;
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
    api.runtimeRuns(projectId).then(({ runs }) => {
      apply(runs, runs.filter((r) => r.status === "running").length);
    }).catch(() => {});
    if (typeof EventSource === "undefined") return;
    try {
      const src = new EventSource(api.runtimeStreamUrl(projectId));
      runtimeSource = src;
      src.addEventListener("state", (e) => {
        try {
          const snap = JSON.parse((e as MessageEvent).data);
          apply(snap.runs ?? [], snap.active ?? 0);
        } catch {
          /* ignore */
        }
      });
      src.onerror = () => {
        /* browser reconnects */
      };
    } catch {
      /* ignore */
    }
  },

  stopRuntimeMonitor: () => {
    if (runtimeSource) {
      runtimeSource.close();
      runtimeSource = null;
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

  startCollab: (projectId: string) => {
    get().stopCollab();
    // Multi-user collaboration is dropped for the single-user desktop.
    if (isDesktop()) return;
    const { clientId, clientName } = get();
    const apply = (st: CollabState) =>
      set({ presence: st.presence, locks: st.locks, lease: st.lease, handoff: st.handoff ?? null });

    // Prefer a single SSE connection (the open stream *is* our presence — no
    // client heartbeat polling). Fall back to polling if SSE errors out.
    const startPolling = () => {
      if (collabTimer) return;
      api.collabJoin(projectId, clientId, clientName).then(apply).catch(() => {});
      collabTimer = setInterval(() => {
        const id = get().projectId;
        if (!id) return;
        api.collabHeartbeat(id, clientId).then(apply).catch(() => {});
      }, 4000);
    };

    // Opt-in bidirectional WebSocket transport (alternative to SSE + REST).
    // Enable with `localStorage.setItem("hl_collab_ws", "1")`.
    const wsEnabled = (() => {
      try {
        return localStorage.getItem("hl_collab_ws") === "1";
      } catch {
        return false;
      }
    })();
    if (wsEnabled && typeof WebSocket !== "undefined") {
      try {
        const sock = new WebSocket(api.collabWsUrl(projectId, clientId, clientName));
        collabSocket = sock;
        sock.onmessage = (e) => {
          try {
            const msg = JSON.parse(e.data);
            if (msg.type === "state") apply(msg.state as CollabState);
          } catch {
            /* ignore */
          }
        };
        sock.onclose = () => {
          if (collabSocket === sock) {
            collabSocket = null;
            startPolling();
          }
        };
        return;
      } catch {
        /* fall through to SSE */
      }
    }

    if (typeof EventSource !== "undefined") {
      try {
        const src = new EventSource(api.collabStreamUrl(projectId, clientId, clientName));
        collabSource = src;
        src.addEventListener("state", (e) => {
          try {
            apply(JSON.parse((e as MessageEvent).data));
          } catch {
            /* ignore */
          }
        });
        src.onerror = () => {
          // browser auto-reconnects; if it never opens, polling fallback covers us
          if (src.readyState === EventSource.CLOSED) {
            collabSource = null;
            startPolling();
          }
        };
        return;
      } catch {
        /* fall through to polling */
      }
    }
    startPolling();
  },

  stopCollab: () => {
    if (collabSocket) {
      collabSocket.close();
      collabSocket = null;
    }
    if (collabSource) {
      collabSource.close();
      collabSource = null;
    }
    if (collabTimer) {
      clearInterval(collabTimer);
      collabTimer = null;
    }
    const { projectId, clientId } = get();
    if (projectId) api.collabLeave(projectId, clientId).catch(() => {});
    set({ presence: [], locks: {}, lease: null, handoff: null });
  },

  lockTarget: async (target: string) => {
    const { projectId, clientId } = get();
    if (!projectId) return false;
    try {
      const r = await api.collabLock(projectId, clientId, target);
      set({ presence: r.state.presence, locks: r.state.locks, lease: r.state.lease, handoff: r.state.handoff ?? null });
      return r.ok;
    } catch {
      return false;
    }
  },

  unlockTarget: (target: string) => {
    const { projectId, clientId } = get();
    if (!projectId) return;
    api.collabUnlock(projectId, clientId, target)
      .then((st) => set({ presence: st.presence, locks: st.locks, lease: st.lease, handoff: st.handoff ?? null }))
      .catch(() => {});
  },

  acquireLease: async () => {
    const { projectId, clientId, clientName } = get();
    if (!projectId) return;
    try {
      const r = await api.leaseAcquire(projectId, clientId, clientName);
      set({ presence: r.state.presence, locks: r.state.locks, lease: r.state.lease, handoff: r.state.handoff ?? null });
      if (!r.ok) {
        const holder = r.holder || "…";
        set({ error: t("errors.leaseCannotAcquire", { holder }) });
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  releaseLease: () => {
    const { projectId, clientId } = get();
    if (!projectId) return;
    api.leaseRelease(projectId, clientId)
      .then((st) => set({ presence: st.presence, locks: st.locks, lease: st.lease, handoff: st.handoff ?? null }))
      .catch(() => {});
  },

  requestLease: async () => {
    const { projectId, clientId, clientName } = get();
    if (!projectId) return;
    try {
      const r = await api.leaseRequest(projectId, clientId, clientName);
      set({ presence: r.state.presence, locks: r.state.locks, lease: r.state.lease, handoff: r.state.handoff ?? null });
    } catch (e) {
      set({ error: String(e) });
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
