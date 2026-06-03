//! YAML-mappable configuration structures for Agent Workbench.
//!
//! These structures mirror the architecture design in
//! `kb/architecture/core_architecture_design.md` and provide a
//! declarative way to define agent behavior through YAML files.

use serde::{Deserialize, Serialize};

/// Top-level agent configuration.
///
/// Example YAML:
/// ```yaml
/// agent:
///   name: "Researcher"
///   model: "claude-sonnet-4.5"
///   context_window: 200000
///   max_tokens: 8192
///   fallback_models:
///     - "gpt-4o"
///     - "qwen-max"
///
/// plugins:
///   memory: "InMemoryVectorStore"
///   context: "ProjectContextManager"
///
/// policy:
///   confirm_shell: false
///   confirm_file_write: true
///   auto_compact_threshold: 100000
///
/// strategy:
///   type: "react"
///   system_prompt_template: "..."
///
/// hooks:
///   - type: "FileJournal"
///     path: ".pisci/journal.db"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent definition section.
    pub agent: AgentDefinition,
    /// Plugin configuration section.
    #[serde(default)]
    pub plugins: PluginConfig,
    /// Policy and confirmation settings.
    #[serde(default)]
    pub policy: PolicyConfig,
    /// Strategy configuration (optional).
    #[serde(default)]
    pub strategy: StrategyConfig,
    /// Context-compaction algorithm selection (optional).
    #[serde(default)]
    pub compression: CompressionConfig,
    /// Tool whitelist. Absent (`None`) = all built-in tools enabled; an empty
    /// list (`[]`) = no tools (reasoning-only); a non-empty list = only those
    /// tools. The CLI `--tools` / `--no-tools` flags override this at run time.
    #[serde(default)]
    pub enabled_tools: Option<Vec<String>>,
    /// Hook configuration (optional).
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
}

/// Context-compaction algorithm configuration.
///
/// Selects which [`CompactionStrategy`] the kernel uses to shrink the
/// conversation before each LLM call.
///
/// [`CompactionStrategy`]: piscis_kernel::agent::compaction_strategy::CompactionStrategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Strategy identifier. Supported values:
    /// `"summary_based"` (default, LLM rolling summary), `"sliding_window"`
    /// (token-budget truncation, no LLM), `"vector_retrieval"`
    /// (relevance-ranked retention).
    #[serde(default = "default_compression_strategy")]
    pub strategy: String,
    /// Optional override prompt for summary-based strategies (reserved).
    #[serde(default)]
    pub summarizer_prompt: Option<String>,
}

fn default_compression_strategy() -> String {
    "summary_based".into()
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            strategy: default_compression_strategy(),
            summarizer_prompt: None,
        }
    }
}

/// Agent identity and model settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Human-readable agent name (used for logging/display).
    pub name: String,
    /// Primary model identifier (e.g., "claude-sonnet-4.5", "gpt-4o").
    pub model: String,
    /// Model context window in tokens (0 = auto-detect).
    #[serde(default)]
    pub context_window: u32,
    /// Maximum output tokens per turn.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Fallback models to try on rate-limit or failure.
    #[serde(default)]
    pub fallback_models: Vec<String>,
    /// Enable SSE streaming for incremental response rendering.
    #[serde(default)]
    pub enable_streaming: bool,
    /// Vision capability override (null = auto from model id).
    pub vision_override: Option<bool>,
    /// Optional separate vision model for image analysis delegation.
    pub vision_model: Option<String>,
}

fn default_max_tokens() -> u32 {
    4096
}

impl Default for AgentDefinition {
    fn default() -> Self {
        Self {
            name: "DefaultAgent".into(),
            model: "claude-haiku-4.5".into(),
            context_window: 0,
            max_tokens: default_max_tokens(),
            fallback_models: Vec::new(),
            enable_streaming: false,
            vision_override: None,
            vision_model: None,
        }
    }
}

/// Plugin selection configuration.
///
/// Maps plugin type names to their implementation identifiers.
/// The kernel resolves these strings to concrete trait objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Memory plugin identifier.
    /// Supported values: "NoOpMemory", "InMemoryVectorStore", "SqliteMemory"
    #[serde(default = "default_memory_plugin")]
    pub memory: String,
    /// Context manager plugin identifier.
    /// Supported values: "ProjectContextManager", "WorkspaceRootManager"
    #[serde(default = "default_context_manager")]
    pub context: String,
    /// Optional template for formatting retrieved memories into the system
    /// prompt. The literal `{memories}` placeholder is replaced with the
    /// rendered hit list. When absent the kernel uses a built-in heading.
    #[serde(default)]
    pub memory_retrieval_prompt: Option<String>,
}

fn default_memory_plugin() -> String {
    "InMemoryVectorStore".into()
}

fn default_context_manager() -> String {
    "ProjectContextManager".into()
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            memory: default_memory_plugin(),
            context: default_context_manager(),
            memory_retrieval_prompt: None,
        }
    }
}

/// Policy and confirmation preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Require user confirmation before executing shell commands.
    #[serde(default)]
    pub confirm_shell: bool,
    /// Require user confirmation before writing files.
    #[serde(default = "default_confirm_file_write")]
    pub confirm_file_write: bool,
    /// Auto-compaction threshold in cumulative input tokens.
    /// 0 disables threshold-driven compaction.
    #[serde(default)]
    pub auto_compact_threshold: u32,
    /// Compaction tier percentages (micro, auto, full).
    #[serde(default)]
    pub compaction_tiers: CompactionTiers,
    /// Maximum tokens allowed per tool result.
    #[serde(default = "default_max_tool_result_tokens")]
    pub max_tool_result_tokens: u32,
    /// Optional model for generating rolling summaries.
    pub summary_model: Option<String>,
    /// Self-evolution policy configuration.
    #[serde(default)]
    pub evolution: EvolutionPolicyConfig,
}

/// Configuration for the self-evolution mechanism.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionPolicyConfig {
    /// Enable or disable the evolution hook.
    #[serde(default)]
    pub enabled: bool,
    /// Metrics to track for evolution triggers.
    #[serde(default = "default_evolution_metrics")]
    pub metrics: Vec<String>,
    /// Thresholds for triggering evolution actions.
    #[serde(default)]
    pub threshold: EvolutionThresholdConfig,
    /// Action to take when thresholds are breached.
    #[serde(default = "default_evolution_action")]
    pub action: String,
}

fn default_evolution_metrics() -> Vec<String> {
    vec!["tool_call_efficiency".into(), "user_correction_rate".into()]
}

fn default_evolution_action() -> String {
    "propose_config_update".into()
}

impl Default for EvolutionPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            metrics: default_evolution_metrics(),
            threshold: EvolutionThresholdConfig::default(),
            action: default_evolution_action(),
        }
    }
}

/// Thresholds for evolution triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionThresholdConfig {
    /// User correction rate threshold (0.0 to 1.0).
    #[serde(default = "default_correction_rate")]
    pub user_correction_rate: f64,
}

fn default_correction_rate() -> f64 {
    0.2
}

impl Default for EvolutionThresholdConfig {
    fn default() -> Self {
        Self {
            user_correction_rate: default_correction_rate(),
        }
    }
}

fn default_confirm_file_write() -> bool {
    true
}

fn default_max_tool_result_tokens() -> u32 {
    4000
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            confirm_shell: false,
            confirm_file_write: default_confirm_file_write(),
            auto_compact_threshold: 0,
            compaction_tiers: CompactionTiers::default(),
            max_tool_result_tokens: default_max_tool_result_tokens(),
            summary_model: None,
            evolution: EvolutionPolicyConfig::default(),
        }
    }
}

/// Compaction tier percentages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionTiers {
    /// Micro compaction trigger percentage.
    #[serde(default = "default_micro_percent")]
    pub micro_percent: u8,
    /// Auto compaction trigger percentage.
    #[serde(default = "default_auto_percent")]
    pub auto_percent: u8,
    /// Full compaction trigger percentage.
    #[serde(default = "default_full_percent")]
    pub full_percent: u8,
}

fn default_micro_percent() -> u8 {
    50
}

fn default_auto_percent() -> u8 {
    70
}

fn default_full_percent() -> u8 {
    90
}

impl Default for CompactionTiers {
    fn default() -> Self {
        Self {
            micro_percent: default_micro_percent(),
            auto_percent: default_auto_percent(),
            full_percent: default_full_percent(),
        }
    }
}

/// Hook configuration entry.
///
/// Hooks are injected at key lifecycle events (before/after tool execution,
/// prompt assembly, LLM calls, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Hook type identifier (e.g., "FileJournal", "AuditLog").
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Optional hook-specific configuration parameters.
    #[serde(flatten)]
    pub params: serde_yaml::Value,
}

/// Strategy configuration for ReAct vs Plan-and-Solve patterns.
///
/// Different strategies can be selected by changing the system prompt
/// template and tool registry composition in the YAML config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy type (e.g., "react", "plan_and_solve").
    #[serde(rename = "type")]
    pub strategy_type: String,
    /// System prompt template path or inline content.
    #[serde(default = "default_system_prompt")]
    pub system_prompt_template: String,
}

fn default_system_prompt() -> String {
    "You are a helpful assistant.".into()
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            strategy_type: "react".into(),
            system_prompt_template: default_system_prompt(),
        }
    }
}

/// Scene-specific configuration presets.
///
/// Provides factory-like defaults for common scenarios (main chat, koi, fish).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenePreset {
    /// Scene identifier ("main", "koi", "fish", "scheduler", "debug").
    pub scene: String,
    /// Agent definition overrides for this scene.
    pub agent: Option<AgentDefinition>,
    /// Policy overrides for this scene.
    pub policy: Option<PolicyConfig>,
}

/// Complete workbench configuration with multiple scene presets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbenchConfig {
    /// Default agent configuration.
    #[serde(flatten)]
    pub default: AgentConfig,
    /// Optional scene-specific presets.
    #[serde(default)]
    pub scenes: Vec<ScenePreset>,
    /// Optional strategy definitions.
    #[serde(default)]
    pub strategies: Vec<StrategyConfig>,
}

impl AgentConfig {
    /// Load an agent configuration from a YAML file.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Parse an agent configuration from a YAML string.
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        let config: AgentConfig = serde_yaml::from_str(yaml)?;
        Ok(config)
    }

    /// Save the configuration to a YAML file.
    pub fn to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
agent:
  name: "TestAgent"
  model: "claude-haiku-4.5"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.agent.name, "TestAgent");
        assert_eq!(config.agent.model, "claude-haiku-4.5");
        assert_eq!(config.plugins.memory, "InMemoryVectorStore");
        assert!(!config.policy.confirm_shell);
        assert!(config.policy.confirm_file_write);
    }

    #[test]
    fn test_parse_full_config() {
        let yaml = r#"
agent:
  name: "Researcher"
  model: "claude-sonnet-4.5"
  context_window: 200000
  max_tokens: 8192
  fallback_models:
    - "gpt-4o"
    - "qwen-max"
  enable_streaming: true

plugins:
  memory: "SqliteMemory"
  context: "WorkspaceRootManager"

policy:
  confirm_shell: true
  confirm_file_write: true
  auto_compact_threshold: 100000
  compaction_tiers:
    micro_percent: 55
    auto_percent: 75
    full_percent: 92
  max_tool_result_tokens: 6000

hooks:
  - type: "FileJournal"
    path: ".pisci/journal.db"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.agent.name, "Researcher");
        assert_eq!(config.agent.context_window, 200000);
        assert_eq!(config.agent.max_tokens, 8192);
        assert_eq!(config.agent.fallback_models.len(), 2);
        assert!(config.agent.enable_streaming);
        assert_eq!(config.plugins.memory, "SqliteMemory");
        assert_eq!(config.plugins.context, "WorkspaceRootManager");
        assert!(config.policy.confirm_shell);
        assert_eq!(config.policy.auto_compact_threshold, 100000);
        assert_eq!(config.policy.compaction_tiers.micro_percent, 55);
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].hook_type, "FileJournal");
    }

    #[test]
    fn test_parse_compression_and_tools() {
        let yaml = r#"
agent:
  name: "Tooled"
  model: "m"

compression:
  strategy: "sliding_window"

enabled_tools:
  - "file_read"
  - "shell"

plugins:
  memory: "InMemoryVectorStore"
  memory_retrieval_prompt: "Recall:\n{memories}"
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.compression.strategy, "sliding_window");
        assert_eq!(
            config.enabled_tools.as_deref(),
            Some(["file_read".to_string(), "shell".to_string()].as_slice())
        );
        assert_eq!(
            config.plugins.memory_retrieval_prompt.as_deref(),
            Some("Recall:\n{memories}")
        );
    }

    #[test]
    fn test_compression_defaults_to_summary_based() {
        let yaml = "agent:\n  name: \"D\"\n  model: \"m\"\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.compression.strategy, "summary_based");
        assert!(config.enabled_tools.is_none());
    }

    #[test]
    fn test_roundtrip_serialization() {
        let original = AgentConfig {
            agent: AgentDefinition {
                name: "RoundTripTest".into(),
                model: "gpt-4o".into(),
                context_window: 128000,
                max_tokens: 4096,
                fallback_models: vec!["claude-haiku".into()],
                enable_streaming: true,
                vision_override: Some(true),
                vision_model: Some("qwen-vl".into()),
            },
            plugins: PluginConfig {
                memory: "NoOpMemory".into(),
                context: "ProjectContextManager".into(),
                memory_retrieval_prompt: None,
            },
            policy: PolicyConfig {
                confirm_shell: false,
                confirm_file_write: false,
                auto_compact_threshold: 50000,
                compaction_tiers: CompactionTiers {
                    micro_percent: 60,
                    auto_percent: 80,
                    full_percent: 95,
                },
                max_tool_result_tokens: 3000,
                summary_model: Some("claude-haiku".into()),
                evolution: EvolutionPolicyConfig::default(),
            },
            strategy: StrategyConfig::default(),
            compression: CompressionConfig::default(),
            enabled_tools: None,
            hooks: vec![HookConfig {
                hook_type: "AuditLog".into(),
                params: serde_yaml::Value::Mapping(Default::default()),
            }],
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let parsed: AgentConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(parsed.agent.name, original.agent.name);
        assert_eq!(parsed.agent.model, original.agent.model);
        assert_eq!(parsed.plugins.memory, original.plugins.memory);
        assert_eq!(parsed.policy.confirm_shell, original.policy.confirm_shell);
    }
}
