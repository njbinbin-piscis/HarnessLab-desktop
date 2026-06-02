//! Agent Workbench command-line entry point.
//!
//! Provides a thin CLI over the configuration-driven harness builder so a
//! YAML config can be validated and inspected without writing Rust.
//!
//! Usage:
//!   agent-workbench validate --config <path>
//!   agent-workbench run      --config <path> [--project <dir>]
//!   agent-workbench --help
//!
//! `run` performs a dry run: it loads the config, builds the `HarnessConfig`
//! (resolving the context manager and hook chain), and prints a summary plus
//! the fully assembled system prompt. Executing a live agent loop additionally
//! requires an `LlmClient`, which is out of scope for the workbench CLI.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use agent_workbench::config::AgentConfig;
use agent_workbench::loader::build_harness_config;
use pisci_kernel::agent::tool::ToolRegistry;
use pisci_kernel::policy::PolicyGate;

const HELP: &str = "\
agent-workbench — configuration-driven agent harness

USAGE:
    agent-workbench <COMMAND> [OPTIONS]

COMMANDS:
    validate    Parse a config and build the harness, reporting any errors
    run         Dry-run: build the harness and print a summary + system prompt
    help        Show this help

OPTIONS:
    --config <PATH>     Path to the agent YAML config (required)
    --project <DIR>     Project directory for context discovery (default: .)
";

struct Args {
    command: String,
    config: Option<String>,
    project: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut iter = std::env::args().skip(1);
    let command = iter.next().unwrap_or_else(|| "help".to_string());
    let mut config = None;
    let mut project = None;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--config" => {
                config = Some(iter.next().ok_or("--config requires a value")?);
            }
            "--project" => {
                project = Some(iter.next().ok_or("--project requires a value")?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args { command, config, project })
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
        println!("OK: '{config_path}' is valid (agent='{}')", config.agent.name);
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
    println!("memory plugin : {}", config.plugins.memory);
    println!("context mgr   : {}", config.plugins.context);
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
    println!("\nNote: executing a live loop requires an LlmClient (not wired in the CLI).");
    Ok(())
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
        other => {
            eprintln!("error: unknown command '{other}'\n\n{HELP}");
            ExitCode::FAILURE
        }
    }
}
