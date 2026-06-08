// Desktop data layer. In HarnessLab (web) this module spoke HTTP REST/SSE to a
// FastAPI backend; in HarnessLab Desktop it speaks **Tauri IPC** to the Rust
// core. Implemented commands go over `invoke`; endpoints not yet ported in the
// current phase resolve to safe empty defaults (so the UI degrades gracefully)
// or reject with a clear "coming in P<n>" message.

import { invoke, Channel } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

export interface ToolSpec {
  name: string;
  description: string;
  platform: string;
  category: string;
}

export interface ClawHubSkill {
  slug: string;
  name: string;
  description: string;
  version: string;
  stars: number;
  tags: string[];
}

export interface InstalledSkill {
  slug: string;
  name: string;
  description: string;
  version: string;
  content: string;
  enabled: boolean;
  installed_at: number;
}

export interface EvalTaskResult {
  task_id: string;
  score: number;
  prediction: string;
  expected: string;
  latency_s?: number;
  est_tokens?: number;
  error?: string | null;
}

export interface EvalRun {
  id: string;
  project_id: string;
  project_name: string;
  dataset: string;
  scorer: string;
  accuracy: number;
  task_results: EvalTaskResult[];
  metrics: { avg_latency: number; total_est_tokens: number };
  created_at: number;
}

export interface DeclarativeAlgorithm {
  kind: string;
  name: string;
  description?: string;
  addendum: string;
  reflective?: boolean;
  max_iterations?: number | null;
  params?: unknown;
}

/** Declarative algorithm kinds the runtime can dispatch end-to-end. */
export const RUNTIME_ALGO_KINDS = ["loop", "compaction", "context", "scorer"] as const;

export interface EvalRunSummary {
  id: string;
  project_id: string;
  project_name: string;
  dataset: string;
  scorer: string;
  accuracy: number;
  avg_latency: number;
  total_est_tokens: number;
  created_at: number;
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

export interface LibraryAgent {
  id: string;
  name: string;
  description?: string;
  spec: AgentSpec;
  created_at?: number;
  updated_at?: number;
}

export interface LibraryAgentSummary {
  id: string;
  name: string;
  description: string;
  model: string;
}

export type LibraryRunMsg =
  | { type: "event"; event: { kind: string; agent: string; text: string; data?: unknown; ts: number } }
  | { type: "result"; result: string; total_tokens: number }
  | { type: "error"; message: string };

export type AlgorithmTrialMsg =
  | { type: "event"; event: { kind: string; agent: string; text: string; data?: unknown; ts: number } }
  | { type: "result"; result: string; total_tokens: number; success: boolean }
  | { type: "error"; message: string };

export interface AutoloopRound {
  round: number;
  accuracy: number;
  baseline: boolean;
  instruction: string;
  ops: unknown[];
  kept: boolean;
  note: string;
}

export type AutoloopMsg =
  | { type: "round"; round: AutoloopRound }
  | {
      type: "result";
      best_round: number;
      best_accuracy: number;
      saved_project_id?: string | null;
      saved_project_name?: string | null;
    }
  | { type: "error"; message: string };

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

export const api = {
  // ── Registry / catalog (P1) ───────────────────────────────────────────────
  catalog: () => invoke<Record<string, ComponentSpec[]>>("registry_catalog"),
  tools: () => invoke<{ tools: ToolSpec[] }>("registry_tools"),
  componentReference: (kind: string, name: string) =>
    invoke<ComponentReference>("component_reference", { kind, name }),
  reloadLibrary: () => invoke<Record<string, ComponentSpec[]>>("reload_library"),

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
  runtimeRuns: () => invoke<RuntimeSnapshot>("runtime_runs"),
  onRuntimeUpdate: (cb: (snap: RuntimeSnapshot) => void): Promise<UnlistenFn> =>
    listen<RuntimeSnapshot>("runtime_update", (e) => cb(e.payload)),

  // ── Design / Copilot (P5) ─────────────────────────────────────────────────
  designEdit: (projectId: string, instruction: string) =>
    invoke<EditProposal>("design_edit", { id: projectId, instruction }),
  design: (brief: string) =>
    invoke<{ source: string; explanation: string; project: Project }>("design_project", { brief }),
  runAlgorithmTrialChannel: (
    algorithm: DeclarativeAlgorithm,
    task: string,
    onMsg: (msg: AlgorithmTrialMsg) => void,
  ) => {
    const channel = new Channel<AlgorithmTrialMsg>();
    channel.onmessage = onMsg;
    return invoke<void>("run_algorithm_trial", { algorithm, task, onEvent: channel });
  },

  // ── Datasets / experiments ────────────────────────────────────────────────
  listDatasets: () =>
    invoke<{ datasets: { id: string; scorer: string; num_tasks: number; source: string }[] }>(
      "list_datasets",
    ).then((r) => r.datasets),
  evalRun: (projectId: string, datasetId: string) =>
    invoke<EvalRunSummary>("run_eval", { projectId, datasetId }),
  runAutoloopChannel: (
    projectId: string,
    datasetId: string,
    rounds: number,
    onMsg: (msg: AutoloopMsg) => void,
  ) => {
    const channel = new Channel<AutoloopMsg>();
    channel.onmessage = onMsg;
    return invoke<void>("run_autoloop", { projectId, datasetId, rounds, onEvent: channel });
  },
  listRuns: (projectId?: string) =>
    invoke<{ runs: EvalRunSummary[] }>("list_eval_runs", { projectId: projectId ?? null }).then(
      (r) => r.runs,
    ),
  getRun: (id: string) => invoke<EvalRun>("get_eval_run", { id }),
  leaderboard: (_datasetId?: string, limit = 20) =>
    invoke<{ entries: EvalRunSummary[] }>("eval_leaderboard", { limit }).then((r) => r.entries),

  // ── Factory codegen (P4) ──────────────────────────────────────────────────
  languages: () => invoke<{ languages: string[] }>("languages"),
  generate: (projectId: string, language: string, options: Record<string, unknown> = {}) =>
    invoke<{ language: string; files: Record<string, string> }>("generate", {
      id: projectId,
      language,
      options,
    }),
  downloadFactoryZip: (
    projectId: string,
    language: string,
    options: Record<string, unknown> = {},
  ) =>
    invoke<string | null>("download_factory_zip", {
      id: projectId,
      language,
      options,
    }),

  // ── Templates (P6) ────────────────────────────────────────────────────────
  listTemplates: () => invoke<{ templates: { id: string; name: string; description: string }[] }>("list_templates"),
  createFromTemplate: (templateId: string, name?: string) =>
    invoke<Project>("create_from_template", { templateId, name: name ?? null }),

  // ── Embeddings / market (P6) ──────────────────────────────────────────────
  embeddingStatus: (probe = false) =>
    invoke<{
      backends: string[];
      api_configured: boolean;
      base_url: string;
      model: string;
      probe?: { ok: boolean; dim?: number; note?: string; reason?: string };
    }>("embedding_status", { probe }),
  marketInstalled: () => invoke<{ packages: { name: string; version: string; type: string; category: string; source: string; installed_at: number }[] }>("market_installed").then((r) => r.packages),
  marketSampleSource: () => invoke<string>("market_sample_source"),
  marketInspect: (source: string) =>
    invoke<{ manifest: Record<string, unknown>; problems: string[] }>("market_inspect", { source }),
  marketInstall: (source: string, consent = false, verifySignature = false) =>
    invoke<{ name: string; version: string; type: string; category: string; wired: string; signature: string; capabilities: string[] }>(
      "market_install",
      { source, consentCapabilities: consent, verifySignature },
    ),
  marketUninstall: (name: string) => invoke<void>("market_uninstall", { name }),
  marketGetRegistries: () =>
    invoke<{ registries: string[] }>("market_get_registries").then((r) => r.registries),
  marketSetRegistries: (registries: string[]) =>
    invoke<void>("market_set_registries", { registries }),
  marketCatalog: (query?: string) =>
    invoke<{
      entries: {
        registry: string;
        registry_url: string;
        name: string;
        latest?: string;
        description?: string;
        type?: string;
        category?: string;
        keywords: string[];
        versions: string[];
      }[];
    }>("market_catalog", { query: query ?? null }).then((r) => r.entries),

  // ── Agent library ─────────────────────────────────────────────────────────
  listLibraryAgents: () =>
    invoke<{ agents: LibraryAgentSummary[] }>("list_library_agents").then((r) => r.agents),
  getLibraryAgent: (id: string) => invoke<LibraryAgent | null>("get_library_agent", { id }),
  createLibraryAgent: (agent: LibraryAgent) =>
    invoke<LibraryAgent>("create_library_agent", { agent }),
  updateLibraryAgent: (id: string, agent: LibraryAgent) =>
    invoke<LibraryAgent | null>("update_library_agent", { id, agent }),
  deleteLibraryAgent: (id: string) => invoke<boolean>("delete_library_agent", { id }),
  runLibraryAgentChannel: (id: string, task: string, onMsg: (msg: LibraryRunMsg) => void) => {
    const channel = new Channel<LibraryRunMsg>();
    channel.onmessage = onMsg;
    return invoke<void>("run_library_agent", { id, task, onEvent: channel });
  },

  // ── Skill library (ClawHub) ───────────────────────────────────────────────
  clawhubSearch: (query: string, limit = 20) =>
    invoke<{ items: ClawHubSkill[]; total: number; query: string }>("clawhub_search", {
      query,
      limit,
    }),
  clawhubInstall: (slug: string, version?: string) =>
    invoke<InstalledSkill>("clawhub_install", { slug, version: version ?? null }),
  listSkills: () =>
    invoke<{ skills: InstalledSkill[] }>("list_skills").then((r) => r.skills),
  uninstallSkill: (slug: string) => invoke<boolean>("uninstall_skill", { slug }),
  setSkillEnabled: (slug: string, enabled: boolean) =>
    invoke<InstalledSkill | null>("set_skill_enabled", { slug, enabled }),

  // ── Declarative algorithm registry (meta-agent) ───────────────────────────
  registerAlgorithm: (algorithm: DeclarativeAlgorithm) =>
    invoke<DeclarativeAlgorithm>("register_algorithm", { algorithm }),
  authorAlgorithm: (instruction: string, current?: DeclarativeAlgorithm | null, kind?: string) =>
    invoke<{ algorithm: DeclarativeAlgorithm; explanation: string; source: string }>(
      "author_algorithm",
      { instruction, current: current ?? null, kind: kind ?? null },
    ),
  listAlgorithms: () =>
    invoke<{ algorithms: DeclarativeAlgorithm[] }>("list_algorithms").then((r) => r.algorithms),
  deleteAlgorithm: (name: string) => invoke<boolean>("delete_algorithm", { name }),
};
