// Desktop data layer. In HarnessLab (web) this module spoke HTTP REST/SSE to a
// FastAPI backend; in HarnessLab Desktop it speaks **Tauri IPC** to the Rust
// core. Implemented commands go over `invoke`; endpoints not yet ported in the
// current phase resolve to safe empty defaults (so the UI degrades gracefully)
// or reject with a clear "coming in P<n>" message.

import { invoke, Channel } from "@tauri-apps/api/core";

/** True when running inside the Tauri webview (vs. a plain browser). */
export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export interface ComponentSpec {
  kind: string;
  name: string;
  description: string;
  params_schema: Record<string, unknown>;
  source?: string;
  reference_langs?: string[];
}

export interface ComponentReference {
  kind: string;
  name: string;
  languages: string[];
  snippets: Record<string, string>;
}

export type Ref = { name: string; params?: Record<string, unknown> } | null;

export interface Project {
  id?: string;
  name: string;
  description?: string;
  agents: AgentSpec[];
  topology: Topology;
  datasets?: string[];
  scorer?: Ref;
  rev?: number;
}

export interface AgentSpec {
  name: string;
  role?: string;
  model?: string;
  system_prompt?: string;
  max_turns?: number;
  max_tokens?: number;
  max_total_tokens?: number;
  enabled_tools?: string[] | null;
  loop?: Ref;
  compaction?: Ref;
  memory?: Ref;
  context?: Ref;
  // harness lifecycle stages (empty by default)
  credentials?: Ref;
  capabilities?: Ref;
  skills?: Ref;
  prompt?: Ref;
  approval?: Ref;
  budget?: Ref;
  hooks?: Ref;
  session?: Ref;
  tracing?: Ref;
}

export type EditOp = Record<string, unknown>;

export interface OpsResult {
  project: Project;
  diff: Record<string, { before: unknown; after: unknown }>;
  applied: number;
  dry_run: boolean;
  rev: number;
}

export interface CanvasRun {
  id: string;
  project_id: string;
  project_rev?: number;
  task: string;
  result: string;
  created_at: number;
  duration_ms: number;
  total_tokens: number;
  metrics: Record<string, NodeMetrics>;
}

export interface SessionNode {
  id: string;
  type?: "run" | "turn";
  parent: string | null;
  run_id?: string;
  turn?: number;
  task?: string;
  result?: string;
  assistant?: string;
  created_at?: number;
  total_tokens?: number;
}

export interface CollabState {
  version: number;
  presence: { client_id: string; name: string; color: string }[];
  locks: Record<string, { client_id: string; name: string }>;
  lease: { client_id: string; name: string } | null;
  handoff?: { client_id: string; name: string } | null;
}

export interface LockResult {
  ok: boolean;
  holder?: string;
  state: CollabState;
}

export class ApiError extends Error {
  status: number;
  detail: unknown;
  constructor(status: number, detail: unknown) {
    super(typeof detail === "string" ? detail : JSON.stringify(detail));
    this.status = status;
    this.detail = detail;
  }
}

export interface SessionUI {
  open?: boolean;
  expanded?: string[];
  position?: { x: number; y: number };
}

export interface Layout {
  nodes: Record<string, { x: number; y: number }>;
  viewport?: { x: number; y: number; zoom: number };
  collapsed?: string[];
  session?: SessionUI;
}

export interface AgentRunState {
  agent: string;
  phase: string;
  turn: number;
  tools: number;
  tokens: number;
  last_text: string;
}

export interface RuntimeRun {
  run_id: string;
  project_id: string;
  task: string;
  status: string;
  started_at: number;
  updated_at: number;
  agents: Record<string, AgentRunState>;
  result?: string;
  error?: string;
}

export interface RuntimeSnapshot {
  version: number;
  active: number;
  runs: RuntimeRun[];
}

export interface AgentEvent {
  kind: string;
  agent: string;
  text: string;
  data: Record<string, unknown>;
  ts: number;
}

/** A streaming message from a live run (Tauri IPC channel envelope). */
export type RunMsg =
  | { type: "event"; event: AgentEvent }
  | { type: "result"; result: string; run: CanvasRun }
  | { type: "error"; message: string };

export interface EditProposal {
  ops: EditOp[];
  explanation: string;
  source: string;
  preview: {
    valid: boolean;
    problems: string[];
    diff: Record<string, { before: unknown; after: unknown }>;
  };
}

export interface NodeMetrics {
  turns: number;
  tools: number;
  tokens: number;
  ms: number;
}

export interface Topology {
  mode: string;
  coordinator?: { name: string; params?: Record<string, unknown> } | null;
  agent_order: string[];
}

// Reject helper for endpoints not yet wired in the current phase.
function comingSoon<T>(phase: string, feature: string): Promise<T> {
  return Promise.reject(new ApiError(501, `${feature} 将在 ${phase} 提供`));
}

const EMPTY_COLLAB: CollabState = {
  version: 0,
  presence: [],
  locks: {},
  lease: null,
  handoff: null,
};

export const api = {
  // ── Registry / catalog (P1) ───────────────────────────────────────────────
  catalog: () => invoke<Record<string, ComponentSpec[]>>("registry_catalog"),
  tools: () => invoke<{ tools: { name: string; description: string }[] }>("registry_tools"),
  componentReference: (kind: string, name: string) =>
    invoke<ComponentReference>("component_reference", { kind, name }),
  reloadLibrary: () => Promise.resolve({}),

  // ── Projects (P1) ─────────────────────────────────────────────────────────
  listProjects: () => invoke<{ id: string; name: string }[]>("list_projects"),
  getProject: (id: string) => invoke<Project>("get_project", { id }),
  createProject: (p: Project) => invoke<Project>("create_project", { project: p }),
  updateProject: (id: string, p: Project) => invoke<Project>("update_project", { id, project: p }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),

  // ── Layout / canvas view-state (P1) ───────────────────────────────────────
  getLayout: (id: string) => invoke<Layout>("get_layout", { id }),
  saveLayout: (id: string, layout: Layout) => invoke<Layout>("save_layout", { id, layout }),

  // ── Edit Ops (P2) ─────────────────────────────────────────────────────────
  applyOps: async (
    id: string,
    ops: EditOp[],
    dry_run = false,
    base_rev?: number,
    _clientId?: string,
  ): Promise<OpsResult> => {
    try {
      // Tauri converts camelCase JS args → snake_case Rust params.
      return await invoke<OpsResult>("apply_ops", { id, ops, dryRun: dry_run, baseRev: base_rev });
    } catch (e) {
      const err = e as { status?: number; message?: string; problems?: string[]; project?: Project };
      const status = typeof err?.status === "number" ? err.status : 500;
      if (status === 409) throw new ApiError(409, { reason: "stale", project: err.project });
      const detail = err?.problems ?? err?.message ?? err;
      throw new ApiError(status, detail);
    }
  },

  // ── Run console / observability (P3) ──────────────────────────────────────
  listCanvasRuns: (id: string, limit = 25) =>
    invoke<{ runs: CanvasRun[] }>("list_canvas_runs", { id, limit }),
  sessionTree: (id: string) =>
    invoke<{ nodes: SessionNode[]; roots: string[] }>("session_tree", { id }),
  /**
   * Run a task live, streaming events back over a Tauri IPC channel. The
   * returned promise resolves when the run finishes (success or handled error;
   * terminal state is delivered as a `result`/`error` message via `onMsg`).
   */
  runProjectChannel: (id: string, task: string, onMsg: (msg: RunMsg) => void) => {
    const channel = new Channel<RunMsg>();
    channel.onmessage = onMsg;
    // Tauri converts camelCase JS args → snake_case Rust params.
    return invoke<void>("run_project", { id, task, onEvent: channel });
  },
  runtimeRuns: (_projectId?: string, _activeOnly = false) =>
    Promise.resolve({ runs: [] as RuntimeRun[] }),
  runtimeStreamUrl: (_projectId?: string) => "",

  // ── Collaboration (dropped for the single-user desktop) ───────────────────
  collabJoin: (_id: string, _clientId: string, _name: string) => Promise.resolve(EMPTY_COLLAB),
  collabHeartbeat: (_id: string, _clientId: string) => Promise.resolve(EMPTY_COLLAB),
  collabLeave: (_id: string, _clientId: string) => Promise.resolve(EMPTY_COLLAB),
  collabLock: (_id: string, _clientId: string, _target: string) =>
    Promise.resolve({ ok: true, state: EMPTY_COLLAB } as LockResult),
  collabUnlock: (_id: string, _clientId: string, _target: string) => Promise.resolve(EMPTY_COLLAB),
  leaseAcquire: (_id: string, _clientId: string, _name: string) =>
    Promise.resolve({ ok: true, state: EMPTY_COLLAB } as LockResult),
  leaseRelease: (_id: string, _clientId: string) => Promise.resolve(EMPTY_COLLAB),
  leaseRequest: (_id: string, _clientId: string, _name: string) =>
    Promise.resolve({ ok: true, state: EMPTY_COLLAB } as LockResult),
  collabStreamUrl: (_id: string, _clientId: string, _name: string) => "",
  collabWsUrl: (_id: string, _clientId: string, _name: string) => "",

  // ── Design / Copilot (P5) ─────────────────────────────────────────────────
  designEdit: (_projectId: string, _instruction: string) =>
    comingSoon<EditProposal>("P5", "AI Copilot"),
  design: (_request: string) => comingSoon<{ source: string; project: Project }>("P5", "Design Assistant"),
  authorComponent: (_kind: string, _name: string, _description: string) =>
    comingSoon("P5", "组件创作"),

  // ── Datasets / experiments (P3+) ──────────────────────────────────────────
  listDatasets: () =>
    Promise.resolve([] as { id: string; scorer: string; num_tasks: number; source: string }[]),
  evalRun: (_projectId: string, _datasetId: string) => comingSoon<any>("P3", "评测运行"),
  listRuns: (_projectId?: string) => Promise.resolve([] as any[]),
  getRun: (id: string) => comingSoon<any>("P3", `运行 ${id}`),
  leaderboard: (_datasetId?: string, _limit = 20) => Promise.resolve([] as any[]),

  // ── Factory codegen (P4) ──────────────────────────────────────────────────
  languages: () => Promise.resolve({ languages: ["python", "typescript", "rust"] }),
  generate: (_projectId: string, _language: string, _options: Record<string, unknown> = {}) =>
    comingSoon<{ language: string; files: Record<string, string> }>("P4", "代码生成"),

  // ── Embeddings / market (later) ───────────────────────────────────────────
  embeddingStatus: (_probe = false) =>
    Promise.resolve<{
      backends: string[];
      api_configured: boolean;
      base_url: string;
      model: string;
      probe?: { ok: boolean; dim?: number; note?: string; reason?: string };
    }>({ backends: ["bow", "hashing"], api_configured: false, base_url: "", model: "" }),
  marketInstalled: () => Promise.resolve([] as any[]),
  marketInspect: (_source: string) => comingSoon<any>("P6", "市场检视"),
  marketInstall: (_source: string, _consent = false) => comingSoon<any>("P6", "市场安装"),
  marketUninstall: (_name: string) => comingSoon("P6", "市场卸载"),
  downloadUrl: "",
};
