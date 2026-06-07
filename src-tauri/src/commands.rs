//! Tauri commands — the desktop IPC surface that replaces HarnessLab's FastAPI
//! REST endpoints. P1 covers the read-only-canvas surface: the component
//! catalog, the project CRUD, and the canvas layout. Run/eval/design/market
//! land in later phases.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use harnesslab_core::agent_library::{LibraryAgent, LibraryAgentSummary};
use harnesslab_core::copilot;
use harnesslab_core::eval;
use harnesslab_core::factory::{self, GenerateOutput};
use harnesslab_core::market::{InspectResult, InstallSummary, InstalledPackage, MarketStore};
use harnesslab_core::ops;
use harnesslab_core::project::{new_id, now_secs, Project};
use harnesslab_core::registry::{self, ComponentReference, ComponentSpec, ToolSpec};
use harnesslab_core::runner::{self, NodeMetrics, UiEvent};
use harnesslab_core::skills::InstalledSkill;
use harnesslab_core::store::{
    CanvasRun, Layout, OpsError, OpsResult, ProjectSummary, SessionTree, Store,
};
use harnesslab_core::templates::{self, ProjectTemplate};
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::mpsc;

/// Shared application state.
pub struct AppState {
    pub store: Mutex<Store>,
    pub market: Mutex<MarketStore>,
}

impl AppState {
    pub fn new(store: Store, market: MarketStore) -> Self {
        AppState {
            store: Mutex::new(store),
            market: Mutex::new(market),
        }
    }

    fn market_name_extras(state: &State<'_, AppState>) -> Vec<(String, String)> {
        let mut extras: Vec<(String, String)> = state
            .market
            .lock()
            .unwrap()
            .market_components()
            .into_iter()
            .map(|(kind, name, _)| (kind, name))
            .collect();
        // Include meta-agent hot-registered declarative algorithms so Edit-Ops
        // validation accepts selecting them.
        for (kind, name, _) in
            harnesslab_core::algorithms::catalog_extras(&state.store.lock().unwrap())
        {
            extras.push((kind, name));
        }
        extras
    }

    fn market_catalog_extras(state: &State<'_, AppState>) -> Vec<(String, String, String)> {
        state.market.lock().unwrap().market_components()
    }

    fn registered_algorithm_extras(state: &State<'_, AppState>) -> Vec<(String, String, String)> {
        harnesslab_core::algorithms::catalog_extras(&state.store.lock().unwrap())
    }
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub engine: String,
}

/// Basic app metadata for the frontend status bar.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "HarnessLab".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        engine: "harness-rt".into(),
    }
}

// ── Registry / catalog ──────────────────────────────────────────────────────

#[tauri::command]
pub fn registry_catalog(state: State<'_, AppState>) -> BTreeMap<String, Vec<ComponentSpec>> {
    let mut catalog = registry::catalog_merged(&AppState::market_catalog_extras(&state));
    registry::append_extras(
        &mut catalog,
        &AppState::registered_algorithm_extras(&state),
        "registered",
    );
    catalog
}

#[derive(Serialize)]
pub struct ToolsList {
    pub tools: Vec<ToolSpec>,
}

#[tauri::command]
pub fn registry_tools() -> ToolsList {
    ToolsList {
        tools: registry::tools(),
    }
}

#[tauri::command]
pub fn component_reference(kind: String, name: String) -> ComponentReference {
    registry::component_reference(&kind, &name)
}

// ── Projects ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Vec<ProjectSummary> {
    state.store.lock().unwrap().list_projects()
}

#[tauri::command]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<Project, String> {
    state
        .store
        .lock()
        .unwrap()
        .get_project(&id)
        .ok_or_else(|| format!("project {id} not found"))
}

#[tauri::command]
pub fn create_project(state: State<'_, AppState>, project: Project) -> Project {
    state.store.lock().unwrap().create_project(project)
}

#[tauri::command]
pub fn update_project(
    state: State<'_, AppState>,
    id: String,
    project: Project,
) -> Result<Project, String> {
    state
        .store
        .lock()
        .unwrap()
        .update_project(&id, project)
        .ok_or_else(|| format!("project {id} not found"))
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.store.lock().unwrap().delete_project(&id) {
        Ok(())
    } else {
        Err(format!("project {id} not found"))
    }
}

// ── Layout (canvas view-state) ──────────────────────────────────────────────

#[tauri::command]
pub fn get_layout(state: State<'_, AppState>, id: String) -> Layout {
    state.store.lock().unwrap().get_layout(&id)
}

#[tauri::command]
pub fn save_layout(state: State<'_, AppState>, id: String, layout: Layout) -> Layout {
    state.store.lock().unwrap().save_layout(&id, layout)
}

// ── Edit Ops (P2) ───────────────────────────────────────────────────────────

/// Serializable error payload for `apply_ops`, carrying an HTTP-like `status`
/// so the frontend can keep its 409 (stale) / 422 (invalid) / 404 handling.
#[derive(Serialize)]
pub struct ApplyErr {
    pub status: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problems: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_rev: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Project>,
}

#[tauri::command]
pub fn apply_ops(
    state: State<'_, AppState>,
    id: String,
    ops: Vec<Value>,
    dry_run: bool,
    base_rev: Option<u64>,
) -> Result<OpsResult, ApplyErr> {
    let extras = AppState::market_name_extras(&state);
    state
        .store
        .lock()
        .unwrap()
        .apply_ops(&id, &ops, dry_run, base_rev, &extras)
        .map_err(|e| match e {
            OpsError::NotFound => ApplyErr {
                status: 404,
                message: format!("project {id} not found"),
                problems: None,
                current_rev: None,
                project: None,
            },
            OpsError::Stale {
                current_rev,
                project,
            } => ApplyErr {
                status: 409,
                message: "project was modified (stale revision)".into(),
                problems: None,
                current_rev: Some(current_rev),
                project: Some(project),
            },
            OpsError::Invalid { problems } => ApplyErr {
                status: 422,
                message: "invalid edit ops".into(),
                problems: Some(problems),
                current_rev: None,
                project: None,
            },
        })
}

// ── Run console / live execution (P3) ────────────────────────────────────────

/// Streaming messages pushed to the frontend over a Tauri IPC channel during a
/// run. Mirrors the web SSE envelope (`{type:"event"|"result"|"error", …}`).
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunMsg {
    Event { event: UiEvent },
    Result { result: String, run: CanvasRun },
    Error { message: String },
}

#[derive(Serialize)]
pub struct RunsResponse {
    pub runs: Vec<CanvasRun>,
}

/// Run a task against the project's primary agent over `harness-rt`,
/// streaming translated events to `on_event` and persisting the run on success.
#[tauri::command]
pub async fn run_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    task: String,
    on_event: Channel<RunMsg>,
) -> Result<(), String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&id)
    };
    let Some(project) = project else {
        let _ = on_event.send(RunMsg::Error {
            message: format!("project {id} not found"),
        });
        return Ok(());
    };

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let workspace = runner::project_workspace(&base, &id);
    let _ = std::fs::create_dir_all(&workspace);

    let run_id = new_id();
    let started = now_secs();
    let cancel = Arc::new(AtomicBool::new(false));

    let (tx, mut rx) = mpsc::channel::<UiEvent>(256);
    let proj = project.clone();
    let task_run = task.clone();
    let ws = workspace.clone();
    let cancel_run = cancel.clone();
    let handle = tokio::spawn(async move {
        runner::run_project_task(&proj, &task_run, &ws, tx, cancel_run).await
    });

    let mut collected: Vec<UiEvent> = Vec::new();
    while let Some(ev) = rx.recv().await {
        collected.push(ev.clone());
        let _ = on_event.send(RunMsg::Event { event: ev });
    }

    let outcome = handle
        .await
        .map_err(|e| format!("run task join error: {e}"))?;

    // Persist any algorithms the Architect hot-registered during this run.
    persist_drained_algorithms(&state);

    match outcome {
        Ok(out) => {
            let metrics: BTreeMap<String, NodeMetrics> = runner::metrics_from_events(&collected)
                .into_iter()
                .collect();
            let run = CanvasRun {
                id: run_id,
                project_id: id.clone(),
                project_rev: project.rev,
                parent: None,
                task,
                result: out.result.clone(),
                created_at: started,
                duration_ms: ((now_secs() - started) * 1000.0).round() as u64,
                total_tokens: out.total_input_tokens + out.total_output_tokens,
                metrics,
            };
            {
                let store = state.store.lock().unwrap();
                store.save_canvas_run(&run);
            }
            let _ = on_event.send(RunMsg::Result {
                result: out.result,
                run,
            });
            Ok(())
        }
        Err(e) => {
            let _ = on_event.send(RunMsg::Error { message: e });
            Ok(())
        }
    }
}

#[tauri::command]
pub fn list_canvas_runs(
    state: State<'_, AppState>,
    id: String,
    limit: Option<u32>,
) -> RunsResponse {
    let runs = state
        .store
        .lock()
        .unwrap()
        .list_canvas_runs(&id, limit.unwrap_or(25));
    RunsResponse { runs }
}

#[tauri::command]
pub fn session_tree(state: State<'_, AppState>, id: String, limit: Option<u32>) -> SessionTree {
    state
        .store
        .lock()
        .unwrap()
        .session_tree(&id, limit.unwrap_or(200))
}

// ── Factory codegen (P4) ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LanguagesResponse {
    pub languages: Vec<String>,
}

#[tauri::command]
pub fn languages() -> LanguagesResponse {
    LanguagesResponse {
        languages: factory::languages(),
    }
}

#[tauri::command]
pub fn generate(
    state: State<'_, AppState>,
    id: String,
    language: String,
    options: Option<Value>,
) -> Result<GenerateOutput, String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&id)
    }
    .ok_or_else(|| format!("project {id} not found"))?;
    factory::generate(&project, &language, &options.unwrap_or(Value::Null))
}

/// Generate a zip and save it via the native file dialog.
#[tauri::command]
pub async fn download_factory_zip(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    language: String,
    options: Option<Value>,
) -> Result<Option<String>, String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&id)
    }
    .ok_or_else(|| format!("project {id} not found"))?;

    let zip_bytes = factory::generate_zip(&project, &language, &options.unwrap_or(Value::Null))?;
    let filename = format!("{}_{}.zip", factory::project_slug(&project.name), language);

    let picked = app
        .dialog()
        .file()
        .set_file_name(&filename)
        .add_filter("Zip archive", &["zip"])
        .blocking_save_file();

    match picked {
        Some(path) => {
            let p = path
                .into_path()
                .map_err(|_| "invalid save path".to_string())?;
            std::fs::write(&p, zip_bytes).map_err(|e| e.to_string())?;
            Ok(Some(p.to_string_lossy().into()))
        }
        None => Ok(None),
    }
}

// ── Market (P6) ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct InstalledList {
    pub packages: Vec<InstalledPackage>,
}

#[tauri::command]
pub fn market_installed(state: State<'_, AppState>) -> InstalledList {
    InstalledList {
        packages: state.market.lock().unwrap().list_installed(),
    }
}

#[tauri::command]
pub fn market_sample_source() -> String {
    format!(
        "file:{}/examples/packages/cool-loop",
        env!("CARGO_MANIFEST_DIR").replace("/src-tauri", "/crates/harnesslab-core")
    )
}

#[tauri::command]
pub fn market_inspect(state: State<'_, AppState>, source: String) -> Result<InspectResult, String> {
    state.market.lock().unwrap().inspect(&source)
}

#[derive(Serialize)]
pub struct MarketInstallResult {
    #[serde(flatten)]
    pub summary: InstallSummary,
}

#[tauri::command]
pub fn market_install(
    state: State<'_, AppState>,
    source: String,
    consent_capabilities: bool,
    verify_signature: bool,
) -> Result<MarketInstallResult, String> {
    let summary =
        state
            .market
            .lock()
            .unwrap()
            .install(&source, consent_capabilities, verify_signature)?;
    Ok(MarketInstallResult { summary })
}

#[tauri::command]
pub fn market_uninstall(state: State<'_, AppState>, name: String) -> Result<(), String> {
    if state.market.lock().unwrap().uninstall(&name)? {
        Ok(())
    } else {
        Err(format!("package {name} not installed"))
    }
}

#[derive(Serialize)]
pub struct RegistriesResponse {
    pub registries: Vec<String>,
}

#[tauri::command]
pub fn market_get_registries(state: State<'_, AppState>) -> RegistriesResponse {
    RegistriesResponse {
        registries: state.market.lock().unwrap().get_registries(),
    }
}

#[tauri::command]
pub fn market_set_registries(
    state: State<'_, AppState>,
    registries: Vec<String>,
) -> Result<(), String> {
    state.market.lock().unwrap().set_registries(registries)
}

#[derive(Serialize)]
pub struct CatalogResponse {
    pub entries: Vec<harnesslab_core::market::CatalogEntry>,
}

#[tauri::command]
pub fn market_catalog(state: State<'_, AppState>) -> Result<CatalogResponse, String> {
    Ok(CatalogResponse {
        entries: state.market.lock().unwrap().catalog()?,
    })
}

// ── Templates (P6) ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TemplatesResponse {
    pub templates: Vec<ProjectTemplate>,
}

#[tauri::command]
pub fn list_templates() -> TemplatesResponse {
    TemplatesResponse {
        templates: templates::list_templates(),
    }
}

#[tauri::command]
pub fn create_from_template(
    state: State<'_, AppState>,
    template_id: String,
    name: Option<String>,
) -> Result<Project, String> {
    let project = templates::instantiate_template(&template_id, name.as_deref())?;
    Ok(state.store.lock().unwrap().create_project(project))
}

// ── Design Copilot (P5) ──────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct EditPreview {
    pub valid: bool,
    pub problems: Vec<String>,
    pub diff: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct EditProposal {
    pub ops: Vec<Value>,
    pub explanation: String,
    pub source: String,
    pub preview: EditPreview,
}

/// Copilot: propose validated Edit Ops for an instruction (review-before-apply).
#[tauri::command]
pub async fn design_edit(
    state: State<'_, AppState>,
    id: String,
    instruction: String,
) -> Result<EditProposal, String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&id)
    }
    .ok_or_else(|| format!("project {id} not found"))?;

    let proposal = copilot::propose_ops_llm(&project, &instruction).await;
    let extras = AppState::market_name_extras(&state);
    let names = registry::names_by_kind_merged(&extras);
    let tools = registry::tool_names();
    let result = ops::apply_ops(&project, &proposal.ops, &names, &tools);
    let valid = result.ok();
    let problems = result.problems;
    let diff = if valid {
        ops::diff_projects(&project, &result.project)
    } else {
        BTreeMap::new()
    };
    let preview = EditPreview {
        valid,
        problems,
        diff,
    };
    Ok(EditProposal {
        ops: proposal.ops,
        explanation: proposal.explanation,
        source: proposal.source,
        preview,
    })
}

// ── Agent library ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LibraryAgentsResponse {
    pub agents: Vec<LibraryAgentSummary>,
}

#[tauri::command]
pub fn list_library_agents(state: State<'_, AppState>) -> LibraryAgentsResponse {
    LibraryAgentsResponse {
        agents: state.store.lock().unwrap().list_library_agents(),
    }
}

#[tauri::command]
pub fn get_library_agent(state: State<'_, AppState>, id: String) -> Option<LibraryAgent> {
    state.store.lock().unwrap().get_library_agent(&id)
}

#[tauri::command]
pub fn create_library_agent(state: State<'_, AppState>, agent: LibraryAgent) -> LibraryAgent {
    state.store.lock().unwrap().create_library_agent(agent)
}

#[tauri::command]
pub fn update_library_agent(
    state: State<'_, AppState>,
    id: String,
    agent: LibraryAgent,
) -> Option<LibraryAgent> {
    state.store.lock().unwrap().update_library_agent(&id, agent)
}

#[tauri::command]
pub fn delete_library_agent(state: State<'_, AppState>, id: String) -> bool {
    state.store.lock().unwrap().delete_library_agent(&id)
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LibraryRunMsg {
    Event { event: UiEvent },
    Result { result: String, total_tokens: u32 },
    Error { message: String },
}

/// Run a library agent standalone (try-chat) without creating a project.
#[tauri::command]
pub async fn run_library_agent(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    task: String,
    on_event: Channel<LibraryRunMsg>,
) -> Result<(), String> {
    let agent = {
        let store = state.store.lock().unwrap();
        store.get_library_agent(&id)
    };
    let Some(lib) = agent else {
        let _ = on_event.send(LibraryRunMsg::Error {
            message: format!("library agent {id} not found"),
        });
        return Ok(());
    };

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let workspace = runner::library_agent_workspace(&base, &id);
    let _ = std::fs::create_dir_all(&workspace);

    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) = mpsc::channel::<UiEvent>(256);
    let spec = lib.spec.clone();
    let task_run = task.clone();
    let ws = workspace.clone();
    let cancel_run = cancel.clone();
    let ctx = runner::AgentRunContext {
        workspace: ws,
        events: tx,
        cancel: cancel_run,
    };
    let handle =
        tokio::spawn(async move { runner::run_agent_task(&ctx, &spec, &task_run, "").await });

    while let Some(ev) = rx.recv().await {
        let _ = on_event.send(LibraryRunMsg::Event { event: ev });
    }

    let joined = handle.await.map_err(|e| format!("run join error: {e}"))?;
    // Persist any algorithms the Architect hot-registered during this run.
    persist_drained_algorithms(&state);
    match joined {
        Ok(out) => {
            let total = out.total_input_tokens + out.total_output_tokens;
            let _ = on_event.send(LibraryRunMsg::Result {
                result: out.result,
                total_tokens: total,
            });
        }
        Err(e) => {
            let _ = on_event.send(LibraryRunMsg::Error { message: e });
        }
    }
    Ok(())
}

/// Smoke command retained from P0: parse a YAML agent config and return its
/// agent name (proves the config→harness path is linked).
#[tauri::command]
pub fn parse_agent_config(yaml: String) -> Result<String, String> {
    let cfg = harnesslab_core::config::AgentConfig::from_yaml(&yaml).map_err(|e| e.to_string())?;
    Ok(cfg.agent.name)
}

// ── Skill library (ClawHub) ─────────────────────────────────────────────────

const CLAWHUB_API: &str = "https://clawhub.ai";

#[derive(Debug, Clone, Serialize)]
pub struct ClawHubSkill {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub stars: u64,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClawHubSearchResult {
    pub items: Vec<ClawHubSkill>,
    pub total: usize,
    pub query: String,
}

#[derive(Serialize)]
pub struct SkillsResponse {
    pub skills: Vec<InstalledSkill>,
}

/// Republish the enabled installed skills into the runtime prompt-injection slot.
fn republish_active_skills(state: &State<'_, AppState>) {
    let skills = state.store.lock().unwrap().list_skills();
    harnesslab_core::skills::publish_from(&skills);
}

/// Load installed skills from the store and publish them at startup.
pub fn publish_skills_on_startup(store: &Store) {
    let skills = store.list_skills();
    harnesslab_core::skills::publish_from(&skills);
}

async fn clawhub_get_with_retry(
    client: &reqwest::Client,
    url: &str,
    max_retries: u32,
) -> Result<reqwest::Response, String> {
    let base_delay_ms: u64 = 1000;
    let mut attempt = 0u32;
    loop {
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("network request failed: {e}"))?;
        let status = resp.status();
        if status.is_success() || (status.is_client_error() && status.as_u16() != 429) {
            return Ok(resp);
        }
        if attempt >= max_retries {
            return Ok(resp);
        }
        let retry_after_ms = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(|secs| secs * 1000)
            .unwrap_or(0);
        let backoff_ms = if retry_after_ms > 0 {
            retry_after_ms.min(30_000)
        } else {
            (base_delay_ms * (1u64 << attempt.min(4))).min(16_000)
        };
        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        attempt += 1;
    }
}

fn parse_skill_name(content: &str, fallback: &str) -> String {
    if let Some(rest) = content.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            for line in fm.lines() {
                if let Some(v) = line.strip_prefix("name:") {
                    let n = v.trim().trim_matches('"').trim_matches('\'');
                    if !n.is_empty() {
                        return n.to_string();
                    }
                }
            }
        }
    }
    for line in content.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            let t = h.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    fallback.to_string()
}

fn parse_skill_description(content: &str) -> String {
    if let Some(rest) = content.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            for line in fm.lines() {
                if let Some(v) = line.strip_prefix("description:") {
                    let d = v.trim().trim_matches('"').trim_matches('\'');
                    if !d.is_empty() {
                        return d.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

fn sanitize_slug(slug: &str) -> Result<String, String> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!("invalid skill slug: '{slug}'"));
    }
    Ok(slug.to_string())
}

fn extract_skill_md_from_zip(zip_bytes: &[u8]) -> Result<String, String> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_lowercase();
        if name == "skill.md" || name.ends_with("/skill.md") {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| e.to_string())?;
            return Ok(content);
        }
    }
    Err("SKILL.md not found in zip archive".to_string())
}

/// Search ClawHub for installable skills.
#[tauri::command]
pub async fn clawhub_search(
    query: String,
    limit: Option<u32>,
) -> Result<ClawHubSearchResult, String> {
    let limit = limit.unwrap_or(20).min(50);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("HarnessLab-Desktop/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let q = query.trim().to_string();
    let (url, use_search) = if q.is_empty() {
        (
            format!("{CLAWHUB_API}/api/v1/skills?sort=stars&limit={limit}"),
            false,
        )
    } else {
        (
            format!(
                "{CLAWHUB_API}/api/v1/search?q={}&limit={limit}",
                urlencoding::encode(&q)
            ),
            true,
        )
    };

    let resp = clawhub_get_with_retry(&client, &url, 3)
        .await
        .map_err(|e| format!("cannot reach ClawHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("ClawHub HTTP {}", resp.status()));
    }
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;

    let items: Vec<ClawHubSkill> = if use_search {
        body["results"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|r| {
                let slug = r["slug"].as_str()?.to_string();
                Some(ClawHubSkill {
                    name: r["displayName"].as_str().unwrap_or(&slug).to_string(),
                    description: r["summary"].as_str().unwrap_or("").to_string(),
                    version: r["version"].as_str().unwrap_or("").to_string(),
                    stars: 0,
                    tags: vec![],
                    slug,
                })
            })
            .collect()
    } else {
        body["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|item| {
                let slug = item["slug"].as_str()?.to_string();
                let tags: Vec<String> = item["tags"]
                    .as_object()
                    .map(|obj| obj.keys().cloned().collect())
                    .unwrap_or_default();
                Some(ClawHubSkill {
                    name: item["displayName"].as_str().unwrap_or(&slug).to_string(),
                    description: item["summary"].as_str().unwrap_or("").to_string(),
                    version: item["latestVersion"]["version"]
                        .as_str()
                        .unwrap_or("latest")
                        .to_string(),
                    stars: item["stats"]["stars"].as_u64().unwrap_or(0),
                    tags,
                    slug,
                })
            })
            .collect()
    };

    let total = items.len();
    Ok(ClawHubSearchResult {
        items,
        total,
        query: q,
    })
}

/// Install a skill from ClawHub into the local skills table.
#[tauri::command]
pub async fn clawhub_install(
    state: State<'_, AppState>,
    slug: String,
    version: Option<String>,
) -> Result<InstalledSkill, String> {
    let slug = sanitize_slug(slug.trim())?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("HarnessLab-Desktop/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let ver = version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty() && *v != "latest");
    let file_url = if let Some(v) = ver {
        format!("{CLAWHUB_API}/api/v1/skills/{slug}/file?path=SKILL.md&version={v}")
    } else {
        format!("{CLAWHUB_API}/api/v1/skills/{slug}/file?path=SKILL.md")
    };

    let resp = clawhub_get_with_retry(&client, &file_url, 3).await?;
    let content = if resp.status().is_success() {
        resp.text().await.map_err(|e| e.to_string())?
    } else {
        let zip_url = if let Some(v) = ver {
            format!("{CLAWHUB_API}/api/v1/download?slug={slug}&version={v}")
        } else {
            format!("{CLAWHUB_API}/api/v1/download?slug={slug}")
        };
        let zip_resp = clawhub_get_with_retry(&client, &zip_url, 3).await?;
        if !zip_resp.status().is_success() {
            return Err(format!(
                "ClawHub install failed for '{slug}': HTTP {}",
                zip_resp.status()
            ));
        }
        let bytes = zip_resp.bytes().await.map_err(|e| e.to_string())?;
        extract_skill_md_from_zip(&bytes)?
    };

    let skill = InstalledSkill {
        name: parse_skill_name(&content, &slug),
        description: parse_skill_description(&content),
        version: ver.unwrap_or("latest").to_string(),
        content,
        enabled: true,
        installed_at: now_secs(),
        slug,
    };
    {
        let store = state.store.lock().unwrap();
        store.put_skill(&skill).map_err(|e| e.to_string())?;
    }
    republish_active_skills(&state);
    Ok(skill)
}

/// List locally-installed skills.
#[tauri::command]
pub fn list_skills(state: State<'_, AppState>) -> SkillsResponse {
    SkillsResponse {
        skills: state.store.lock().unwrap().list_skills(),
    }
}

/// Uninstall a skill by slug.
#[tauri::command]
pub fn uninstall_skill(state: State<'_, AppState>, slug: String) -> bool {
    let removed = state.store.lock().unwrap().delete_skill(&slug);
    if removed {
        republish_active_skills(&state);
    }
    removed
}

/// Enable / disable a skill's prompt injection.
#[tauri::command]
pub fn set_skill_enabled(
    state: State<'_, AppState>,
    slug: String,
    enabled: bool,
) -> Option<InstalledSkill> {
    let updated = state
        .store
        .lock()
        .unwrap()
        .set_skill_enabled(&slug, enabled);
    if updated.is_some() {
        republish_active_skills(&state);
    }
    updated
}

// ── Evaluation pipeline ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct DatasetsResponse {
    pub datasets: Vec<eval::DatasetSummary>,
}

/// List the built-in evaluation datasets.
#[tauri::command]
pub fn list_datasets() -> DatasetsResponse {
    DatasetsResponse {
        datasets: eval::builtin_datasets()
            .iter()
            .map(|d| d.summary())
            .collect(),
    }
}

/// Run an evaluation: score `project` against `dataset` and persist the result.
#[tauri::command]
pub async fn run_eval(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    dataset_id: String,
) -> Result<eval::EvalRunSummary, String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&project_id)
    }
    .ok_or_else(|| format!("project {project_id} not found"))?;

    let dataset = eval::dataset_by_id(&dataset_id)
        .ok_or_else(|| format!("dataset {dataset_id} not found"))?;

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let ws_base = base.join("eval-workspaces").join(&project_id);
    let _ = std::fs::create_dir_all(&ws_base);

    let run = eval::run_eval(&project, &dataset, &ws_base).await?;
    {
        let store = state.store.lock().unwrap();
        store.put_eval_run(&run).map_err(|e| e.to_string())?;
    }
    Ok(run.summary())
}

/// Fetch a full eval run by id (task-level results).
#[tauri::command]
pub fn get_eval_run(state: State<'_, AppState>, id: String) -> Option<eval::EvalRun> {
    state.store.lock().unwrap().get_eval_run(&id)
}

#[derive(Serialize)]
pub struct EvalRunsResponse {
    pub runs: Vec<eval::EvalRunSummary>,
}

/// List past eval runs, optionally filtered to one project.
#[tauri::command]
pub fn list_eval_runs(state: State<'_, AppState>, project_id: Option<String>) -> EvalRunsResponse {
    let runs = state
        .store
        .lock()
        .unwrap()
        .list_eval_runs(project_id.as_deref())
        .iter()
        .map(|r| r.summary())
        .collect();
    EvalRunsResponse { runs }
}

#[derive(Serialize)]
pub struct LeaderboardResponse {
    pub entries: Vec<eval::EvalRunSummary>,
}

/// The accuracy leaderboard across all eval runs.
#[tauri::command]
pub fn eval_leaderboard(state: State<'_, AppState>, limit: Option<usize>) -> LeaderboardResponse {
    LeaderboardResponse {
        entries: state.store.lock().unwrap().leaderboard(limit.unwrap_or(20)),
    }
}

// ── Declarative algorithm registry (meta-agent) ─────────────────────────────

use harness_rt::agent::declarative::DeclarativeAlgorithm;
use harnesslab_core::algorithms;

#[derive(Serialize)]
pub struct AlgorithmsResponse {
    pub algorithms: Vec<DeclarativeAlgorithm>,
}

/// Register persisted algorithms into the running engine. Call once at startup.
pub fn register_algorithms_on_startup(store: &Store) {
    algorithms::register_persisted(store);
}

/// Persist any algorithms the running engine registered live since the last drain.
fn persist_drained_algorithms(state: &State<'_, AppState>) {
    let store = state.store.lock().unwrap();
    algorithms::persist_drained(&store);
}

/// Author + hot-register a declarative algorithm, persisting it for future runs.
#[tauri::command]
pub fn register_algorithm(
    state: State<'_, AppState>,
    algorithm: DeclarativeAlgorithm,
) -> Result<DeclarativeAlgorithm, String> {
    harness_rt::agent::declarative::register(&algorithm)?;
    // `register` adds to the drain log; persist it now.
    persist_drained_algorithms(&state);
    Ok(algorithm)
}

/// List persisted declarative algorithms.
#[tauri::command]
pub fn list_algorithms(state: State<'_, AppState>) -> AlgorithmsResponse {
    AlgorithmsResponse {
        algorithms: state.store.lock().unwrap().list_algorithms(),
    }
}

/// Delete a persisted declarative algorithm (it remains live until restart).
#[tauri::command]
pub fn delete_algorithm(state: State<'_, AppState>, name: String) -> bool {
    state.store.lock().unwrap().delete_algorithm(&name)
}
