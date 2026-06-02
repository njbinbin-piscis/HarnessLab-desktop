//! Configuration loader that builds `HarnessConfig` from YAML.
//!
//! This module provides the bridge between declarative YAML configuration
//! and the runtime `pisci_kernel::agent::harness::HarnessConfig`.

use std::sync::Arc;

use pisci_kernel::agent::harness::{HarnessConfig, LayeredPrompt};
use pisci_kernel::agent::harness::config::{CompactionSettings, ConfirmFlags};
use pisci_kernel::agent::tool::ToolRegistry;
use pisci_kernel::policy::PolicyGate;

use crate::config::AgentConfig;
use crate::context_managers::{
    DynamicContextManager, ProjectContextManager, WorkspaceRootManager,
};
use crate::hooks::WorkbenchHooks;
use crate::plugins::ContextManager;

/// Error type for configuration loading failures.
#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(#[from] anyhow::Error),

    #[error("Unknown memory plugin: {0}")]
    UnknownMemoryPlugin(String),

    #[error("Unknown context manager: {0}")]
    UnknownContextManager(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Build a `HarnessConfig` from an `AgentConfig` and runtime dependencies.
///
/// This is the core integration point: it takes the declarative YAML
/// configuration and wires it up with the concrete trait objects needed
/// by the agent loop.
///
/// # Arguments
/// * `agent_config` - Parsed YAML configuration
/// * `registry` - Tool registry (built by caller based on enabled tools)
/// * `policy` - Policy gate (constructed from policy settings)
/// * `project_dir` - Project directory for context discovery
///
/// # Returns
/// A fully configured `HarnessConfig` ready for `into_agent_loop()`.
pub fn build_harness_config(
    agent_config: &AgentConfig,
    registry: Arc<ToolRegistry>,
    policy: Arc<PolicyGate>,
    project_dir: Option<&std::path::Path>,
) -> Result<HarnessConfig, ConfigLoadError> {
    let agent = &agent_config.agent;
    let plugins = &agent_config.plugins;
    let policy_cfg = &agent_config.policy;

    // Validate the configured memory plugin up front so typos fail loudly.
    validate_memory_plugin(&plugins.memory)?;

    // Build confirmation flags from policy config.
    let confirm_flags = ConfirmFlags {
        confirm_shell: policy_cfg.confirm_shell,
        confirm_file_write: policy_cfg.confirm_file_write,
    };

    // Build compaction settings from policy config.
    let compaction = CompactionSettings {
        micro_percent: policy_cfg.compaction_tiers.micro_percent,
        auto_percent: policy_cfg.compaction_tiers.auto_percent,
        full_percent: policy_cfg.compaction_tiers.full_percent,
        max_tool_result_tokens: policy_cfg.max_tool_result_tokens,
        summary_model: policy_cfg.summary_model.clone(),
    };

    // Create the base system prompt from the strategy config.
    let base_prompt = if agent_config.strategy.system_prompt_template.is_empty() {
        format!(
            "You are {}. Follow the user's instructions carefully.",
            agent.name
        )
    } else {
        agent_config.strategy.system_prompt_template.clone()
    };

    // Resolve the configured context manager and inject any discovered
    // project context ahead of the strategy prompt. The kernel `HarnessConfig`
    // has no dedicated context-manager slot, so the workbench performs context
    // discovery here and folds the rendered result into the layered prompt.
    let system_prompt = match project_dir {
        Some(dir) => {
            let manager = resolve_context_manager(&plugins.context)?;
            let rendered = manager
                .discover(dir)
                .map(|entries| manager.render(&entries))
                .unwrap_or_default();
            if rendered.trim().is_empty() {
                base_prompt
            } else {
                format!("{rendered}\n\n{base_prompt}")
            }
        }
        None => base_prompt,
    };

    // Start building the harness config using the builder pattern.
    let mut builder = HarnessConfig::builder("workbench", &agent.model, registry, policy)
        .with_fallback_models(agent.fallback_models.clone())
        .with_layered_prompt(LayeredPrompt::from_monolithic(system_prompt))
        .with_max_tokens(agent.max_tokens)
        .with_context_window(agent.context_window)
        .with_confirm_flags(confirm_flags)
        .with_vision_override(agent.vision_override)
        .with_auto_compact_threshold(policy_cfg.auto_compact_threshold)
        .with_compaction_settings(&compaction)
        .with_streaming(agent.enable_streaming);

    // Wire vision delegate model if specified.
    if let Some(ref vision_model) = agent.vision_model {
        builder = builder.with_vision_model(vision_model.clone());
    }

    // Translate the declarative `hooks` section into a runtime AgentHooks
    // adapter and attach it to the harness when it has observable behavior.
    if let Some(hooks) = build_workbench_hooks(agent_config, project_dir) {
        builder = builder.with_hooks(Arc::new(hooks));
    }

    Ok(builder.build())
}

/// Build a [`WorkbenchHooks`] adapter from the YAML `hooks` section.
///
/// Returns `None` when no configured hook produces observable behavior, so the
/// loop runs without hook overhead in the default case.
pub fn build_workbench_hooks(
    agent_config: &AgentConfig,
    project_dir: Option<&std::path::Path>,
) -> Option<WorkbenchHooks> {
    let mut hooks = WorkbenchHooks::new();
    for hook in &agent_config.hooks {
        match hook.hook_type.as_str() {
            "AuditLog" => {
                hooks = hooks.with_audit(true);
            }
            "FileJournal" => {
                let rel = hook
                    .params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".pisci/journal.jsonl");
                let path = match project_dir {
                    Some(dir) => dir.join(rel),
                    None => std::path::PathBuf::from(rel),
                };
                hooks = hooks.with_journal(path);
            }
            // PlanTracker and other types are accepted but not yet wired to a
            // concrete behavior; they are intentional no-ops for now.
            _ => {}
        }
    }
    hooks.is_active().then_some(hooks)
}

/// Resolve a context-manager identifier to a concrete workbench implementation.
///
/// Unknown identifiers return [`ConfigLoadError::UnknownContextManager`] so
/// configuration typos fail loudly rather than silently degrading behavior.
pub fn resolve_context_manager(
    name: &str,
) -> Result<Box<dyn ContextManager>, ConfigLoadError> {
    match name {
        "ProjectContextManager" => Ok(Box::new(ProjectContextManager)),
        "WorkspaceRootManager" => Ok(Box::new(WorkspaceRootManager)),
        "DynamicContextManager" => Ok(Box::new(DynamicContextManager)),
        other => Err(ConfigLoadError::UnknownContextManager(other.to_string())),
    }
}

/// Validate that a memory-plugin identifier is one the workbench understands.
///
/// The kernel `HarnessConfig` exposes no memory-plugin slot, so this performs
/// validation only; resolution to a concrete store happens at run time.
pub fn validate_memory_plugin(name: &str) -> Result<(), ConfigLoadError> {
    match name {
        "NoOpMemory" | "InMemoryVectorStore" | "SqliteMemory" => Ok(()),
        other => Err(ConfigLoadError::UnknownMemoryPlugin(other.to_string())),
    }
}

/// Load an agent configuration from a YAML file and build a `HarnessConfig`.
///
/// Convenience wrapper that combines file loading and harness construction.
pub fn load_and_build(
    config_path: &std::path::Path,
    registry: Arc<ToolRegistry>,
    policy: Arc<PolicyGate>,
    project_dir: Option<&std::path::Path>,
) -> Result<HarnessConfig, ConfigLoadError> {
    let agent_config = AgentConfig::from_file(config_path)?;
    build_harness_config(&agent_config, registry, policy, project_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_scaffolding() -> (Arc<ToolRegistry>, Arc<PolicyGate>) {
        let reg = Arc::new(ToolRegistry::new());
        let policy = Arc::new(PolicyGate::new(PathBuf::from(".")));
        (reg, policy)
    }

    #[test]
    fn test_build_from_minimal_config() {
        let yaml = r#"
agent:
  name: "TestAgent"
  model: "claude-haiku-4.5"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let project_dir = PathBuf::from("/tmp");

        let harness = build_harness_config(&config, reg, policy, Some(&project_dir)).unwrap();

        assert_eq!(harness.scene, "workbench");
        assert_eq!(harness.model, "claude-haiku-4.5");
        assert!(!harness.confirm_flags.confirm_shell);
        assert!(harness.confirm_flags.confirm_file_write);
    }

    #[test]
    fn test_build_with_custom_policy() {
        let yaml = r#"
agent:
  name: "Coder"
  model: "gpt-4o"
  max_tokens: 8192

policy:
  confirm_shell: true
  confirm_file_write: false
  auto_compact_threshold: 50000
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let project_dir = PathBuf::from("/tmp");

        let harness = build_harness_config(&config, reg, policy, Some(&project_dir)).unwrap();

        assert!(harness.confirm_flags.confirm_shell);
        assert!(!harness.confirm_flags.confirm_file_write);
        assert_eq!(harness.max_tokens, 8192);
    }

    #[test]
    fn test_unknown_memory_plugin_error() {
        let yaml = r#"
agent:
  name: "Test"
  model: "claude-haiku"

plugins:
  memory: "NonExistentPlugin"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let result = build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp")));
        assert!(matches!(
            result,
            Err(ConfigLoadError::UnknownMemoryPlugin(_))
        ));
    }

    #[test]
    fn test_unknown_context_manager_error() {
        assert!(matches!(
            resolve_context_manager("NoSuchManager"),
            Err(ConfigLoadError::UnknownContextManager(_))
        ));
        assert!(resolve_context_manager("ProjectContextManager").is_ok());
        assert!(resolve_context_manager("WorkspaceRootManager").is_ok());
        assert!(resolve_context_manager("DynamicContextManager").is_ok());
    }

    #[test]
    fn test_context_injected_into_system_prompt() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("PISCI.md"), "PROJECT_RULE_MARKER").unwrap();

        let yaml = r#"
agent:
  name: "CtxAgent"
  model: "claude-haiku-4.5"

plugins:
  context: "ProjectContextManager"

strategy:
  type: "react"
  system_prompt_template: "STRATEGY_PROMPT_MARKER"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let harness =
            build_harness_config(&config, reg, policy, Some(dir.path())).unwrap();

        let prompt = harness.layered_prompt.render();
        assert!(prompt.contains("PROJECT_RULE_MARKER"), "context should be injected");
        assert!(prompt.contains("STRATEGY_PROMPT_MARKER"), "strategy prompt should remain");
    }

    #[test]
    fn test_hooks_attached_when_configured() {
        let yaml = r#"
agent:
  name: "HookAgent"
  model: "claude-haiku-4.5"

hooks:
  - type: "AuditLog"
  - type: "FileJournal"
    path: ".pisci/journal.jsonl"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let dir = tempfile::TempDir::new().unwrap();
        let harness = build_harness_config(&config, reg, policy, Some(dir.path())).unwrap();
        assert!(harness.hooks.is_some(), "hooks should be wired into harness");

        // No hooks configured => no adapter attached.
        let bare = AgentConfig::from_yaml("agent:\n  name: \"Bare\"\n  model: \"m\"\n").unwrap();
        let (reg2, policy2) = test_scaffolding();
        let h2 = build_harness_config(&bare, reg2, policy2, Some(dir.path())).unwrap();
        assert!(h2.hooks.is_none());
    }
}
