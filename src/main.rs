//! Agent Workbench command-line entry point.
//!
//! Provides a thin CLI over the configuration-driven harness builder so a
//! YAML config can be validated and inspected without writing Rust.
//!
//! Usage:
//!   agent-workbench validate --config <path>
//!   agent-workbench run      --config <path> [--project <dir>]
//!   agent-workbench exec     --config <path> --prompt <text> [--project <dir>] [--workspace <dir>]
//!   agent-workbench --help
//!
//! `run` performs a dry run: it loads the config, builds the `HarnessConfig`
//! (resolving the context manager and hook chain), and prints a summary plus
//! the fully assembled system prompt.
//!
//! `exec` runs a *live* single-turn agent loop: it builds the harness, wires a
//! real `LlmClient` (from env vars) and the kernel's neutral tool registry, and
//! streams every `AgentEvent` to stdout as NDJSON (one JSON object per line) so
//! a host (e.g. the FastAPI backend) can forward them over SSE. Credentials are
//! read from the environment:
//!   PISCIS_PROVIDER  (default: "qwen")
//!   PISCIS_API_KEY   (required)
//!   PISCIS_BASE_URL  (optional, for custom/openai-compatible endpoints)

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agent_workbench::config::AgentConfig;
use agent_workbench::loader::build_harness_config;
use piscis_kernel::agent::loop_::AgentLoop;
use piscis_kernel::agent::messages::AgentEvent;
use piscis_kernel::agent::tool::{ToolContext, ToolRegistry, ToolSettings};
use piscis_kernel::llm::{build_client_with_timeout, LlmMessage, MessageContent};
use piscis_kernel::policy::PolicyGate;
use piscis_kernel::tools::{register_neutral_into, NeutralToolsConfig};
use tokio::sync::mpsc::Sender;

const HELP: &str = "\
agent-workbench — configuration-driven agent harness

USAGE:
    agent-workbench <COMMAND> [OPTIONS]

COMMANDS:
    validate    Parse a config and build the harness, reporting any errors
    run         Dry-run: build the harness and print a summary + system prompt
    exec        Live run: stream AgentEvents as NDJSON.
                With --prompt: one-shot single turn.
                Without --prompt: long-lived interactive session driven by
                NDJSON stdin commands (prompt / inject / set_model / stop).
    help        Show this help

OPTIONS:
    --config <PATH>     Path to the agent YAML config (required)
    --project <DIR>     Project directory for context discovery (default: .)
    --prompt <TEXT>     One-shot prompt for `exec`. Omit to enter interactive mode.
    --workspace <DIR>   Workspace root for tool execution (default: --project)
    --tools <LIST>      Comma-separated tool whitelist for `exec` (default: all)
    --no-tools          Disable the entire tool registry (reasoning-only ablation)

INTERACTIVE STDIN COMMANDS (exec without --prompt):
    {\"cmd\":\"prompt\",\"text\":\"...\"}     run a turn (history is retained)
    {\"cmd\":\"inject\",\"text\":\"...\"}     steer the active turn mid-run
    {\"cmd\":\"set_model\",\"model\":\"...\"}  acknowledged (model fixed per process)
    {\"cmd\":\"stop\"}                      end the session

ENVIRONMENT (exec):
    PISCIS_PROVIDER     LLM provider id (default: qwen)
    PISCIS_API_KEY      API key for the provider (required)
    PISCIS_BASE_URL     Custom base URL for openai-compatible endpoints
";

struct Args {
    command: String,
    config: Option<String>,
    project: Option<String>,
    prompt: Option<String>,
    workspace: Option<String>,
    tools: Option<Vec<String>>,
    no_tools: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut iter = std::env::args().skip(1);
    let command = iter.next().unwrap_or_else(|| "help".to_string());
    let mut config = None;
    let mut project = None;
    let mut prompt = None;
    let mut workspace = None;
    let mut tools = None;
    let mut no_tools = false;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--config" => {
                config = Some(iter.next().ok_or("--config requires a value")?);
            }
            "--project" => {
                project = Some(iter.next().ok_or("--project requires a value")?);
            }
            "--prompt" => {
                prompt = Some(iter.next().ok_or("--prompt requires a value")?);
            }
            "--workspace" => {
                workspace = Some(iter.next().ok_or("--workspace requires a value")?);
            }
            "--tools" => {
                let raw = iter.next().ok_or("--tools requires a value")?;
                tools = Some(
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
            }
            "--no-tools" => {
                no_tools = true;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        command,
        config,
        project,
        prompt,
        workspace,
        tools,
        no_tools,
    })
}

fn build(args: &Args) -> Result<(), String> {
    let config_path = args
        .config
        .as_ref()
        .ok_or("missing required --config <PATH>")?;

    let config = AgentConfig::from_file(std::path::Path::new(config_path))
        .map_err(|e| format!("failed to load config '{config_path}': {e}"))?;

    let project_dir = PathBuf::from(args.project.clone().unwrap_or_else(|| ".".to_string()));
    let registry = Arc::new(ToolRegistry::new());
    let policy = Arc::new(PolicyGate::new(project_dir.clone()));

    let harness = build_harness_config(&config, registry, policy, Some(&project_dir))
        .map_err(|e| format!("failed to build harness: {e}"))?;

    if args.command == "validate" {
        println!(
            "OK: '{config_path}' is valid (agent='{}')",
            config.agent.name
        );
        return Ok(());
    }

    // run (dry-run) summary
    println!("=== Agent Workbench :: dry run ===");
    println!("config        : {config_path}");
    println!("project_dir   : {}", project_dir.display());
    println!("agent.name    : {}", config.agent.name);
    println!("scene         : {}", harness.scene);
    println!("model         : {}", harness.model);
    println!("max_tokens    : {}", harness.max_tokens);
    println!("context_window: {}", harness.context_window);
    println!("strategy      : {}", config.strategy.strategy_type);
    println!("compression   : {}", config.compression.strategy);
    println!("memory plugin : {}", config.plugins.memory);
    println!("context mgr   : {}", config.plugins.context);
    println!(
        "enabled_tools : {}",
        match &config.enabled_tools {
            None => "(all)".to_string(),
            Some(v) if v.is_empty() => "(none)".to_string(),
            Some(v) => v.join(", "),
        }
    );
    println!(
        "confirm_shell : {} | confirm_file_write: {}",
        harness.confirm_flags.confirm_shell, harness.confirm_flags.confirm_file_write
    );
    println!(
        "hooks         : {}",
        if harness.hooks.is_some() {
            describe_hooks(&config)
        } else {
            "(none)".to_string()
        }
    );
    println!("\n--- assembled system prompt ---");
    println!("{}", harness.layered_prompt.render());

    // Project context is now rendered at run time via the context-manager slot;
    // surface it here so the dry run still shows what the agent will receive.
    if let Ok(ctx) = harness.render_project_context(&project_dir, 8_000) {
        if !ctx.trim().is_empty() {
            println!("\n--- project context (rendered at run time) ---");
            println!("{ctx}");
        }
    }
    println!("\nNote: executing a live loop requires an LlmClient (not wired in the CLI).");
    Ok(())
}

/// Emit a single NDJSON line on stdout. Each line is a self-contained JSON
/// object so a streaming consumer can parse incrementally.
fn emit_ndjson(value: &serde_json::Value) {
    if let Ok(line) = serde_json::to_string(value) {
        println!("{line}");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }
}

/// Live agent execution. Two modes:
/// - **One-shot** (`--prompt <TEXT>`): run a single turn and exit (legacy
///   behaviour used by the Layer-1 Python runner).
/// - **Interactive** (no `--prompt`): a long-lived bidirectional process that
///   reads NDJSON commands on stdin (`prompt` / `inject` / `set_model` /
///   `stop`) and streams `AgentEvent`s + envelopes on stdout. `inject` reuses
///   the kernel's notification channel for mid-turn steering.
fn exec(args: &Args) -> Result<(), String> {
    let config_path = args
        .config
        .as_ref()
        .ok_or("missing required --config <PATH>")?;

    let config = AgentConfig::from_file(std::path::Path::new(config_path))
        .map_err(|e| format!("failed to load config '{config_path}': {e}"))?;

    let project_dir = PathBuf::from(args.project.clone().unwrap_or_else(|| ".".to_string()));
    let workspace_dir = PathBuf::from(
        args.workspace
            .clone()
            .unwrap_or_else(|| project_dir.to_string_lossy().into_owned()),
    );

    // Resolve LLM credentials from the environment.
    let provider = std::env::var("PISCIS_PROVIDER").unwrap_or_else(|_| "qwen".to_string());
    let api_key = std::env::var("PISCIS_API_KEY")
        .or_else(|_| std::env::var("PISCIS_LLM_API_KEY"))
        .unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err(
            "no API key found: set PISCIS_API_KEY (and optionally PISCIS_PROVIDER / PISCIS_BASE_URL)"
                .to_string(),
        );
    }
    let base_url = std::env::var("PISCIS_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    // Register the kernel's neutral tools (file/shell/search/etc.). DB-backed
    // tools (memory_store, recall_tool_result) stay disabled in this MVP since
    // no SQLite session store is wired yet.
    //
    // Tool availability is the controlled variable for several reproductions
    // (GAIA tool-ablation, ReAct CoT-only). The YAML `enabled_tools` list sets
    // the default whitelist; `--no-tools` disables the registry entirely and
    // `--tools a,b,c` overrides the YAML list at run time.
    let registry = Arc::new(build_tool_registry(
        config.enabled_tools.as_deref(),
        args.tools.as_ref(),
        args.no_tools,
    ));

    let policy = Arc::new(PolicyGate::new(workspace_dir.clone()));

    let harness = build_harness_config(&config, registry, policy, Some(&project_dir))
        .map_err(|e| format!("failed to build harness: {e}"))?;

    let model = harness.model.clone();
    let client = build_client_with_timeout(&provider, &api_key, base_url.as_deref(), 120);

    // Interactive mode is selected by the *absence* of --prompt. The steering
    // channel (notification_rx) is only wired in interactive mode.
    let interactive = args.prompt.is_none();
    let one_shot_prompt = args.prompt.clone();
    let (notify_tx, notify_rx) = tokio::sync::mpsc::channel::<String>(64);
    let agent = Arc::new(harness.into_agent_loop(
        client,
        if interactive { Some(notify_rx) } else { None },
        None,
    ));

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to start tokio runtime: {e}"))?;

    runtime.block_on(async move {
        emit_ndjson(&serde_json::json!({
            "type": "run_start",
            "model": model,
            "provider": provider,
            "workspace": workspace_dir.to_string_lossy(),
            "mode": if interactive { "interactive" } else { "one_shot" },
        }));

        if interactive {
            interactive_session(agent, notify_tx, workspace_dir).await
        } else {
            one_shot_run(agent, one_shot_prompt.unwrap_or_default(), workspace_dir).await
        }
    })
}

/// Build a fresh `ToolContext` for one agent run.
fn make_tool_context(workspace_dir: &Path, cancel: Arc<AtomicBool>) -> ToolContext {
    ToolContext {
        session_id: format!("workbench-{}", std::process::id()),
        workspace_root: workspace_dir.to_path_buf(),
        bypass_permissions: true,
        settings: Arc::new(ToolSettings::default()),
        max_iterations: Some(50),
        memory_owner_id: "workbench".to_string(),
        pool_session_id: None,
        tool_use_id: None,
        cancel,
    }
}

/// Serialise and emit an `AgentEvent` as one NDJSON line.
fn emit_event(event: &AgentEvent) {
    if let Ok(value) = serde_json::to_value(event) {
        emit_ndjson(&value);
    }
}

/// One-shot path: run a single prompt to completion and exit.
async fn one_shot_run(
    agent: Arc<AgentLoop>,
    prompt: String,
    workspace_dir: PathBuf,
) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);
    let cancel = Arc::new(AtomicBool::new(false));
    let ctx = make_tool_context(&workspace_dir, cancel.clone());
    let messages = vec![LlmMessage {
        role: "user".to_string(),
        content: MessageContent::text(&prompt),
    }];

    let handle = tokio::spawn(async move { agent.run(messages, tx, cancel, ctx).await });

    while let Some(event) = rx.recv().await {
        emit_event(&event);
    }

    match handle.await {
        Ok(Ok((_msgs, total_in, total_out))) => {
            emit_ndjson(&serde_json::json!({
                "type": "run_complete",
                "total_input_tokens": total_in,
                "total_output_tokens": total_out,
            }));
            Ok(())
        }
        Ok(Err(e)) => {
            emit_ndjson(&serde_json::json!({
                "type": "error",
                "message": format!("agent loop failed: {e}"),
            }));
            Err(format!("agent loop failed: {e}"))
        }
        Err(e) => {
            emit_ndjson(&serde_json::json!({
                "type": "error",
                "message": format!("agent task panicked: {e}"),
            }));
            Err(format!("agent task panicked: {e}"))
        }
    }
}

/// Interactive path: a long-lived process driven by NDJSON stdin commands.
///
/// Commands (one JSON object per line):
/// - `{"cmd":"prompt","text":"..."}`  run a turn (carrying prior history)
/// - `{"cmd":"inject","text":"..."}`  steer the *active* turn (mid-run)
/// - `{"cmd":"set_model","model":"..."}`  (acknowledged; not hot-swappable yet)
/// - `{"cmd":"stop"}`                  end the session
async fn interactive_session(
    agent: Arc<AgentLoop>,
    notify_tx: Sender<String>,
    workspace_dir: PathBuf,
) -> Result<(), String> {
    use tokio::io::AsyncBufReadExt;
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut history: Vec<LlmMessage> = Vec::new();

    emit_ndjson(&serde_json::json!({ "type": "session_ready" }));

    loop {
        let line = match lines.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => break,
            Err(e) => {
                emit_ndjson(&serde_json::json!({
                    "type": "error",
                    "message": format!("stdin read error: {e}"),
                }));
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cmd: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                emit_ndjson(&serde_json::json!({
                    "type": "error",
                    "message": format!("malformed command json: {e}"),
                }));
                continue;
            }
        };
        match cmd.get("cmd").and_then(|c| c.as_str()).unwrap_or("") {
            "prompt" => {
                let text = cmd
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                run_interactive_prompt(
                    &agent,
                    &mut history,
                    text,
                    &notify_tx,
                    &mut lines,
                    &workspace_dir,
                )
                .await;
            }
            "inject" => {
                emit_ndjson(&serde_json::json!({
                    "type": "warning",
                    "message": "no active turn to inject into; send inject during a prompt",
                }));
            }
            "set_model" => {
                emit_ndjson(&serde_json::json!({
                    "type": "warning",
                    "message": "runtime set_model is not supported; model is fixed per process",
                }));
            }
            "stop" => {
                emit_ndjson(&serde_json::json!({ "type": "session_end", "reason": "stop" }));
                return Ok(());
            }
            other => {
                emit_ndjson(&serde_json::json!({
                    "type": "error",
                    "message": format!("unknown cmd: {other}"),
                }));
            }
        }
    }

    emit_ndjson(&serde_json::json!({ "type": "session_end", "reason": "eof" }));
    Ok(())
}

/// Run one interactive turn, concurrently forwarding events to stdout and
/// honouring `inject` / `stop` commands that arrive mid-run.
async fn run_interactive_prompt(
    agent: &Arc<AgentLoop>,
    history: &mut Vec<LlmMessage>,
    text: String,
    notify_tx: &Sender<String>,
    lines: &mut tokio::io::Lines<tokio::io::BufReader<tokio::io::Stdin>>,
    workspace_dir: &Path,
) {
    history.push(LlmMessage {
        role: "user".to_string(),
        content: MessageContent::text(&text),
    });

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);
    let cancel = Arc::new(AtomicBool::new(false));
    let ctx = make_tool_context(workspace_dir, cancel.clone());
    let run_history = history.clone();
    let agent_handle = agent.clone();
    let cancel_run = cancel.clone();
    let mut handle =
        tokio::spawn(async move { agent_handle.run(run_history, tx, cancel_run, ctx).await });

    emit_ndjson(&serde_json::json!({ "type": "turn_start" }));

    let run_result = loop {
        tokio::select! {
            biased;
            res = &mut handle => break Some(res),
            maybe_event = rx.recv() => {
                if let Some(event) = maybe_event {
                    emit_event(&event);
                }
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(l)) => handle_inflight_command(l.trim(), notify_tx, &cancel).await,
                    // stdin EOF mid-turn: cancel the active run, then finish.
                    Ok(None) => cancel.store(true, Ordering::Relaxed),
                    Err(_) => {}
                }
            }
        }
    };

    // Drain any events buffered before the task finished.
    while let Ok(event) = rx.try_recv() {
        emit_event(&event);
    }

    match run_result {
        Some(Ok(Ok((new_msgs, total_in, total_out)))) => {
            history.extend(new_msgs);
            emit_ndjson(&serde_json::json!({
                "type": "turn_complete",
                "total_input_tokens": total_in,
                "total_output_tokens": total_out,
            }));
        }
        Some(Ok(Err(e))) => emit_ndjson(&serde_json::json!({
            "type": "error",
            "message": format!("agent loop failed: {e}"),
        })),
        Some(Err(e)) => emit_ndjson(&serde_json::json!({
            "type": "error",
            "message": format!("agent task panicked: {e}"),
        })),
        None => {}
    }
}

/// Handle a command that arrived while a turn is running. Only `inject` and
/// `stop` are actionable mid-turn; anything else is ignored.
async fn handle_inflight_command(line: &str, notify_tx: &Sender<String>, cancel: &Arc<AtomicBool>) {
    if line.is_empty() {
        return;
    }
    let Ok(cmd) = serde_json::from_str::<serde_json::Value>(line) else {
        emit_ndjson(&serde_json::json!({
            "type": "error",
            "message": "malformed command json (mid-turn)",
        }));
        return;
    };
    match cmd.get("cmd").and_then(|c| c.as_str()).unwrap_or("") {
        "inject" => {
            let msg = cmd
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if !msg.is_empty() {
                // Reuse the kernel @mention notification channel for steering.
                let _ = notify_tx.send(msg.clone()).await;
                emit_ndjson(&serde_json::json!({ "type": "injected", "text": msg }));
            }
        }
        "stop" => {
            cancel.store(true, Ordering::Relaxed);
            emit_ndjson(&serde_json::json!({ "type": "stopping" }));
        }
        _ => {}
    }
}

/// Built-in tool identifiers the workbench can selectively enable.
const KNOWN_TOOLS: &[&str] = &[
    "file_read",
    "file_write",
    "file_edit",
    "file_diff",
    "code_run",
    "file_search",
    "file_list",
    "process_control",
    "shell",
    "web_search",
    "email",
];

/// Build the tool registry from the YAML `enabled_tools` setting, with CLI
/// overrides. Precedence (highest first):
/// - `--no-tools`            → empty registry
/// - `--tools a,b,c`         → CLI whitelist
/// - YAML `enabled_tools`    → `None` (absent) = all; `[]` = none; list = whitelist
/// - default                 → all built-in tools
fn build_tool_registry(
    yaml_enabled: Option<&[String]>,
    cli_tools: Option<&Vec<String>>,
    no_tools: bool,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    if no_tools {
        return registry;
    }

    // Resolve the effective whitelist. `None` => all tools; `Some(list)` =>
    // exactly `list` (an empty list registers no tools).
    let whitelist: Option<&[String]> = match cli_tools {
        Some(list) => Some(list.as_slice()),
        None => yaml_enabled,
    };

    let mut tools_cfg = NeutralToolsConfig::default();
    if let Some(list) = whitelist {
        let mut enabled = std::collections::HashMap::new();
        for tool in KNOWN_TOOLS {
            enabled.insert(tool.to_string(), list.iter().any(|w| w == tool));
        }
        tools_cfg.builtin_tool_enabled = Some(enabled);
    }
    register_neutral_into(&mut registry, &tools_cfg);
    registry
}

fn describe_hooks(config: &AgentConfig) -> String {
    let names: Vec<&str> = config.hooks.iter().map(|h| h.hook_type.as_str()).collect();
    if names.is_empty() {
        "(adapter attached)".to_string()
    } else {
        names.join(", ")
    }
}

fn main() -> ExitCode {
    // Install design-assistant-authored (contributed) strategies into the
    // kernel's runtime registry so YAML configs can select them by name.
    agent_workbench::contrib::register_all();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{HELP}");
            return ExitCode::FAILURE;
        }
    };

    match args.command.as_str() {
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        "validate" | "run" => match build(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        "exec" => match exec(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("error: unknown command '{other}'\n\n{HELP}");
            ExitCode::FAILURE
        }
    }
}
