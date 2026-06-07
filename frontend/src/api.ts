const BASE = "/api/v1";

async function req<T>(path: string, opts: RequestInit = {}): Promise<T> {
  const res = await fetch(BASE + path, {
    headers: { "Content-Type": "application/json" },
    ...opts,
  });
  if (!res.ok) {
    let detail: unknown = res.statusText;
    try {
      detail = (await res.json()).detail ?? detail;
    } catch {
      /* ignore */
    }
    throw new ApiError(res.status, detail);
  }
  return res.json() as Promise<T>;
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
  catalog: () => req<Record<string, ComponentSpec[]>>("/registry"),
  tools: () => req<{ tools: { name: string; description: string }[] }>("/registry/tools/list"),
  componentReference: (kind: string, name: string) =>
    req<ComponentReference>(`/registry/${kind}/${name}/reference`),
  reloadLibrary: () => req("/registry/reload", { method: "POST" }),
  listProjects: () => req<{ id: string; name: string }[]>("/projects"),
  getProject: (id: string) => req<Project>(`/projects/${id}`),
  createProject: (p: Project) => req<Project>("/projects", { method: "POST", body: JSON.stringify(p) }),
  updateProject: (id: string, p: Project) =>
    req<Project>(`/projects/${id}`, { method: "PUT", body: JSON.stringify(p) }),
  deleteProject: (id: string) => req(`/projects/${id}`, { method: "DELETE" }),
  applyOps: (id: string, ops: EditOp[], dry_run = false, base_rev?: number, client_id?: string) =>
    req<OpsResult>(`/projects/${id}/ops`, { method: "POST", body: JSON.stringify({ ops, dry_run, base_rev, client_id }) }),
  listCanvasRuns: (id: string, limit = 25) =>
    req<{ runs: CanvasRun[] }>(`/projects/${id}/runs?limit=${limit}`),
  collabJoin: (id: string, client_id: string, name: string) =>
    req<CollabState>(`/collab/${id}/join`, { method: "POST", body: JSON.stringify({ client_id, name }) }),
  collabHeartbeat: (id: string, client_id: string) =>
    req<CollabState>(`/collab/${id}/heartbeat`, { method: "POST", body: JSON.stringify({ client_id }) }),
  collabLeave: (id: string, client_id: string) =>
    req<CollabState>(`/collab/${id}/leave`, { method: "POST", body: JSON.stringify({ client_id }) }),
  collabLock: (id: string, client_id: string, target: string) =>
    req<LockResult>(`/collab/${id}/lock`, { method: "POST", body: JSON.stringify({ client_id, target }) }),
  collabUnlock: (id: string, client_id: string, target: string) =>
    req<CollabState>(`/collab/${id}/unlock`, { method: "POST", body: JSON.stringify({ client_id, target }) }),
  leaseAcquire: (id: string, client_id: string, name: string) =>
    req<LockResult>(`/collab/${id}/lease/acquire`, { method: "POST", body: JSON.stringify({ client_id, name }) }),
  leaseRelease: (id: string, client_id: string) =>
    req<CollabState>(`/collab/${id}/lease/release`, { method: "POST", body: JSON.stringify({ client_id }) }),
  leaseRequest: (id: string, client_id: string, name: string) =>
    req<LockResult>(`/collab/${id}/lease/request`, { method: "POST", body: JSON.stringify({ client_id, name }) }),
  collabStreamUrl: (id: string, client_id: string, name: string) =>
    `${BASE}/collab/${id}/stream?client_id=${encodeURIComponent(client_id)}&name=${encodeURIComponent(name)}`,
  collabWsUrl: (id: string, client_id: string, name: string) => {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const q = `client_id=${encodeURIComponent(client_id)}&name=${encodeURIComponent(name)}`;
    return `${proto}//${location.host}${BASE}/collab/${id}/ws?${q}`;
  },
  getLayout: (id: string) => req<Layout>(`/projects/${id}/layout`),
  saveLayout: (id: string, layout: Layout) =>
    req<Layout>(`/projects/${id}/layout`, { method: "PUT", body: JSON.stringify(layout) }),
  runProject: (id: string, task: string) =>
    req<{ result: string; events: AgentEvent[] }>(`/projects/${id}/run`, {
      method: "POST",
      body: JSON.stringify({ task }),
    }),
  runProjectStream: (id: string, task: string, parent_run?: string, parent_turn?: number) =>
    fetch(BASE + `/projects/${id}/run/stream`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ task, parent_run, parent_turn }),
    }),
  sessionTree: (id: string) =>
    req<{ nodes: SessionNode[]; roots: string[] }>(`/projects/${id}/session`),
  runtimeRuns: (project_id?: string, active_only = false) =>
    req<{ runs: RuntimeRun[] }>(
      `/runtime/runs?${project_id ? `project_id=${encodeURIComponent(project_id)}&` : ""}active_only=${active_only}`,
    ),
  runtimeStreamUrl: (project_id?: string) =>
    `${BASE}/runtime/stream${project_id ? `?project_id=${encodeURIComponent(project_id)}` : ""}`,
  designEdit: (project_id: string, instruction: string) =>
    req<EditProposal>("/design/edit", {
      method: "POST",
      body: JSON.stringify({ project_id, instruction }),
    }),
  listDatasets: () => req<{ id: string; scorer: string; num_tasks: number; source: string }[]>("/datasets"),
  evalRun: (project_id: string, dataset_id: string) =>
    req<any>("/eval-runs", { method: "POST", body: JSON.stringify({ project_id, dataset_id }) }),
  listRuns: (project_id?: string) =>
    req<any[]>("/eval-runs" + (project_id ? `?project_id=${project_id}` : "")),
  getRun: (id: string) => req<any>(`/eval-runs/${id}`),
  leaderboard: (dataset_id?: string, limit = 20) =>
    req<any[]>("/leaderboard?limit=" + limit + (dataset_id ? `&dataset_id=${dataset_id}` : "")),
  design: (request: string) =>
    req<{ source: string; project: Project }>("/design/generate", {
      method: "POST",
      body: JSON.stringify({ request }),
    }),
  authorComponent: (kind: string, name: string, description: string) =>
    req("/design/components", { method: "POST", body: JSON.stringify({ kind, name, description }) }),
  languages: () => req<{ languages: string[] }>("/factory/languages"),
  generate: (project_id: string, language: string, options: Record<string, unknown> = {}) =>
    req<{ language: string; files: Record<string, string> }>("/factory/generate", {
      method: "POST",
      body: JSON.stringify({ project_id, language, options }),
    }),
  embeddingStatus: (probe = false) =>
    req<{ backends: string[]; api_configured: boolean; base_url: string; model: string;
      probe?: { ok: boolean; dim?: number; note?: string; reason?: string } }>(
      "/registry/embeddings/status" + (probe ? "?probe=true" : ""),
    ),
  marketInstalled: () => req<any[]>("/market/installed"),
  marketInspect: (source: string) =>
    req<any>("/market/inspect", { method: "POST", body: JSON.stringify({ source }) }),
  marketInstall: (source: string, consent_capabilities = false) =>
    req<any>("/market/install", {
      method: "POST",
      body: JSON.stringify({ source, consent_capabilities }),
    }),
  marketUninstall: (name: string) => req(`/market/${name}`, { method: "DELETE" }),
  downloadUrl: BASE + "/factory/download",
};
