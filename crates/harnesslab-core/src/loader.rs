//! Configuration loader that builds `HarnessConfig` from YAML.
//!
//! This module provides the bridge between declarative YAML configuration
//! and the runtime `piscis_kernel::agent::harness::HarnessConfig`.

use std::sync::Arc;

use harness_rt::agent::harness::config::{CompactionSettings, ConfirmFlags};
use harness_rt::agent::harness::{HarnessConfig, LayeredPrompt};
use harness_rt::agent::loop_::resolve_compaction_strategy;
use harness_rt::agent::loop_strategy::resolve_loop_strategy;
use harness_rt::agent::tool::ToolRegistry;
use harness_rt::memory::plugin::{InMemoryVectorStore, MemoryPlugin, SqliteMemory};
use harness_rt::policy::PolicyGate;

use crate::config::AgentConfig;
use crate::context_managers::{
    DynamicContextManager, KernelContextAdapter, ProjectContextManager, WorkspaceRootManager,
};
use crate::hooks::WorkbenchHooks;
use crate::plugins::{ContextManager, NoOpContextManager};

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

    #[error("Unknown compaction strategy: {0}")]
    UnknownCompactionStrategy(String),

    #[error("Unknown loop strategy: {0}")]
    UnknownLoopStrategy(String),

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
    // project_dir is retained for API stability; project-context discovery now
    // happens at run time inside the kernel via the wired context-manager slot,
    // keyed off the agent loop's workspace root.
    let _ = project_dir;

    let agent = &agent_config.agent;
    let plugins = &agent_config.plugins;
    let policy_cfg = &agent_config.policy;

    // Build confirmation flags from policy config.
    let confirm_flags = ConfirmFlags {
        confirm_shell: policy_cfg.confirm_shell,
        confirm_file_write: policy_cfg.confirm_file_write,
    };

    // Build compaction settings (tiers / caps) from policy config.
    let compaction = CompactionSettings {
        micro_percent: policy_cfg.compaction_tiers.micro_percent,
        auto_percent: policy_cfg.compaction_tiers.auto_percent,
        full_percent: policy_cfg.compaction_tiers.full_percent,
        max_tool_result_tokens: policy_cfg.max_tool_result_tokens,
        summary_model: policy_cfg.summary_model.clone(),
    };

    // Resolve the pluggable compaction *algorithm* (separate from the tier
    // thresholds above). Unknown names fail loudly.
    let compaction_strategy = resolve_compaction_strategy(
        &agent_config.compression.strategy,
        agent_config.compression.summarizer_prompt.clone(),
    )
    .ok_or_else(|| {
        ConfigLoadError::UnknownCompactionStrategy(agent_config.compression.strategy.clone())
    })?;

    // Resolve the kernel-side context manager (drives runtime context injection)
    // and the optional long-term memory backend.
    let context_manager = resolve_kernel_context_manager(&plugins.context)?;
    let memory_plugin = resolve_memory_plugin(&plugins.memory, project_dir)?;

    // Resolve the loop-control strategy from `strategy.type`.
    let loop_strategy =
        resolve_loop_strategy(&agent_config.strategy.strategy_type).ok_or_else(|| {
            ConfigLoadError::UnknownLoopStrategy(agent_config.strategy.strategy_type.clone())
        })?;

    // Create the base system prompt from the strategy config. Project context
    // is no longer folded in here; the kernel renders it at run time.
    let base_prompt = if agent_config.strategy.system_prompt_template.is_empty() {
        format!(
            "You are {}. Follow the user's instructions carefully.",
            agent.name
        )
    } else {
        agent_config.strategy.system_prompt_template.clone()
    };

    // Inject installed ClawHub skills (SKILL.md instruction blocks) as extra
    // prompt layers so every agent run can use the user's installed skills.
    let mut prompt = LayeredPrompt::from_monolithic(base_prompt);
    for block in crate::skills::active_blocks() {
        prompt.push_layer(block);
    }

    // Start building the harness config using the builder pattern.
    let mut builder = HarnessConfig::builder("workbench", &agent.model, registry, policy)
        .with_fallback_models(agent.fallback_models.clone())
        .with_layered_prompt(prompt)
        .with_max_tokens(agent.max_tokens)
        .with_context_window(agent.context_window)
        .with_confirm_flags(confirm_flags)
        .with_vision_override(agent.vision_override)
        .with_auto_compact_threshold(policy_cfg.auto_compact_threshold)
        .with_compaction_settings(&compaction)
        .with_compaction_strategy(compaction_strategy)
        .with_context_manager(context_manager)
        .with_loop_strategy(loop_strategy)
        .with_streaming(agent.enable_streaming);

    // Wire the long-term memory backend + retrieval prompt when configured.
    if let Some(memory) = memory_plugin {
        builder = builder
            .with_memory_plugin(memory)
            .with_memory_retrieval_prompt(plugins.memory_retrieval_prompt.clone());
    }

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
pub fn resolve_context_manager(name: &str) -> Result<Box<dyn ContextManager>, ConfigLoadError> {
    match name {
        "ProjectContextManager" => Ok(Box::new(ProjectContextManager)),
        "WorkspaceRootManager" => Ok(Box::new(WorkspaceRootManager)),
        "DynamicContextManager" => Ok(Box::new(DynamicContextManager)),
        "NoOpContextManager" => Ok(Box::new(NoOpContextManager)),
        other => Err(ConfigLoadError::UnknownContextManager(other.to_string())),
    }
}

/// Resolve a context-manager identifier to a kernel-trait object suitable for
/// the [`HarnessConfig`] context slot. Wraps the workbench manager in a
/// [`KernelContextAdapter`] so its richer scan behaviour is preserved while the
/// kernel drives runtime injection.
pub fn resolve_kernel_context_manager(
    name: &str,
) -> Result<Arc<dyn harness_rt::context::ContextManager>, ConfigLoadError> {
    let inner = resolve_context_manager(name)?;
    Ok(Arc::new(KernelContextAdapter::new(name, inner)))
}

/// Resolve a memory-plugin identifier to an optional kernel memory backend.
///
/// `NoOpMemory` resolves to `None` (no injection, behaviour-preserving), while
/// `InMemoryVectorStore` / `SqliteMemory` resolve to a concrete store.
/// `SqliteMemory` currently falls back to the in-memory store (persistent
/// backend WIP). Unknown identifiers fail loudly.
pub fn resolve_memory_plugin(
    name: &str,
    project_dir: Option<&std::path::Path>,
) -> Result<Option<Arc<dyn MemoryPlugin>>, ConfigLoadError> {
    match name {
        "NoOpMemory" => Ok(None),
        "InMemoryVectorStore" => Ok(Some(Arc::new(InMemoryVectorStore::new()))),
        // Persistent backend: store under `<project>/.pisci/memory.db` so
        // memories survive across runs. When no project dir is known (e.g.
        // config validation), fall back to a session-scoped in-memory store.
        "SqliteMemory" => match project_dir {
            Some(dir) => {
                let path = dir.join(".pisci").join("memory.db");
                let store = SqliteMemory::open(&path).map_err(|e| {
                    ConfigLoadError::InvalidConfig(format!("sqlite memory backend: {e}"))
                })?;
                Ok(Some(Arc::new(store)))
            }
            None => Ok(Some(Arc::new(InMemoryVectorStore::new()))),
        },
        other => Err(ConfigLoadError::UnknownMemoryPlugin(other.to_string())),
    }
}

/// Validate that a memory-plugin identifier is one the workbench understands.
pub fn validate_memory_plugin(name: &str) -> Result<(), ConfigLoadError> {
    resolve_memory_plugin(name, None).map(|_| ())
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
    fn test_memory_plugin_resolution() {
        // NoOpMemory => no plugin wired (behaviour-preserving default path).
        assert!(resolve_memory_plugin("NoOpMemory", None).unwrap().is_none());
        // Concrete stores resolve to Some.
        assert!(resolve_memory_plugin("InMemoryVectorStore", None)
            .unwrap()
            .is_some());
        assert!(resolve_memory_plugin("SqliteMemory", None)
            .unwrap()
            .is_some());
        // Unknown identifiers fail loudly.
        assert!(matches!(
            resolve_memory_plugin("Nonsense", None),
            Err(ConfigLoadError::UnknownMemoryPlugin(_))
        ));
    }

    #[test]
    fn test_compaction_strategy_selectable_via_yaml() {
        for strategy in ["summary_based", "sliding_window", "vector_retrieval"] {
            let yaml = format!(
                "agent:\n  name: \"C\"\n  model: \"m\"\ncompression:\n  strategy: \"{strategy}\"\n"
            );
            let config = AgentConfig::from_yaml(&yaml).unwrap();
            let (reg, policy) = test_scaffolding();
            let harness =
                build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp"))).unwrap();
            assert!(
                harness.compaction_strategy.is_some(),
                "{strategy} should wire a compaction strategy"
            );
        }
    }

    #[test]
    fn test_unknown_compaction_strategy_error() {
        let yaml = "agent:\n  name: \"C\"\n  model: \"m\"\ncompression:\n  strategy: \"bogus\"\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let result = build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp")));
        assert!(matches!(
            result,
            Err(ConfigLoadError::UnknownCompactionStrategy(_))
        ));
    }

    #[test]
    fn test_memory_plugin_wired_into_harness() {
        let yaml = "agent:\n  name: \"M\"\n  model: \"m\"\nplugins:\n  memory: \"InMemoryVectorStore\"\n  memory_retrieval_prompt: \"## Recall\\n{memories}\"\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let harness =
            build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp"))).unwrap();
        assert!(
            harness.memory_plugin.is_some(),
            "memory plugin should be wired"
        );
        assert_eq!(
            harness.memory_retrieval_prompt.as_deref(),
            Some("## Recall\n{memories}")
        );

        // NoOpMemory leaves the slot empty (behaviour-preserving).
        let bare = AgentConfig::from_yaml(
            "agent:\n  name: \"M\"\n  model: \"m\"\nplugins:\n  memory: \"NoOpMemory\"\n",
        )
        .unwrap();
        let (reg2, policy2) = test_scaffolding();
        let h2 = build_harness_config(&bare, reg2, policy2, Some(&PathBuf::from("/tmp"))).unwrap();
        assert!(h2.memory_plugin.is_none());
    }

    #[test]
    fn test_loop_strategy_selected_from_type() {
        for (ty, expected) in [
            ("react", "react"),
            ("plan_and_solve", "plan_and_solve"),
            ("reflexion", "reflexion"),
        ] {
            let yaml =
                format!("agent:\n  name: \"S\"\n  model: \"m\"\nstrategy:\n  type: \"{ty}\"\n");
            let config = AgentConfig::from_yaml(&yaml).unwrap();
            let (reg, policy) = test_scaffolding();
            let harness =
                build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp"))).unwrap();
            let strat = harness
                .loop_strategy
                .expect("loop strategy should be wired");
            assert_eq!(strat.name(), expected);
        }
    }

    #[test]
    fn test_unknown_loop_strategy_error() {
        let yaml = "agent:\n  name: \"S\"\n  model: \"m\"\nstrategy:\n  type: \"telepathy\"\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let result = build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp")));
        assert!(matches!(
            result,
            Err(ConfigLoadError::UnknownLoopStrategy(_))
        ));
    }

    #[test]
    fn test_context_manager_always_wired() {
        let yaml = "agent:\n  name: \"C\"\n  model: \"m\"\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let (reg, policy) = test_scaffolding();
        let harness =
            build_harness_config(&config, reg, policy, Some(&PathBuf::from("/tmp"))).unwrap();
        assert!(harness.context_manager.is_some());
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
        let harness = build_harness_config(&config, reg, policy, Some(dir.path())).unwrap();

        // The strategy prompt stays in the layered prompt; project context is
        // now rendered at run time via the wired context-manager slot.
        let prompt = harness.layered_prompt.render();
        assert!(
            prompt.contains("STRATEGY_PROMPT_MARKER"),
            "strategy prompt should remain"
        );
        assert!(
            !prompt.contains("PROJECT_RULE_MARKER"),
            "context is no longer folded into the prompt"
        );

        let rendered_ctx = harness.render_project_context(dir.path(), 8_000).unwrap();
        assert!(
            rendered_ctx.contains("PROJECT_RULE_MARKER"),
            "context should be discoverable via the context-manager slot"
        );
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
        assert!(
            harness.hooks.is_some(),
            "hooks should be wired into harness"
        );

        // No hooks configured => no adapter attached.
        let bare = AgentConfig::from_yaml("agent:\n  name: \"Bare\"\n  model: \"m\"\n").unwrap();
        let (reg2, policy2) = test_scaffolding();
        let h2 = build_harness_config(&bare, reg2, policy2, Some(dir.path())).unwrap();
        assert!(h2.hooks.is_none());
    }
}
