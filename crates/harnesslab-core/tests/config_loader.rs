// This is a self-contained YAML parser used by the benchmark harness. Some
// fields are only read in certain test targets, so suppress per-target
// dead-code warnings rather than littering each field with attributes.
#![allow(dead_code)]

use serde::Deserialize;
use std::fs;

/// Represents the top-level structure of an agent configuration file.
#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub agent: AgentMeta,
    pub plugins: PluginConfig,
    pub policy: PolicyConfig,
    pub hooks: Vec<HookConfig>,
    pub strategy: StrategyConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentMeta {
    pub name: String,
    pub model: String,
    pub context_window: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PluginConfig {
    pub memory: String,
    pub context: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PolicyConfig {
    pub confirm_shell: bool,
    pub confirm_file_write: bool,
    pub auto_compact_threshold: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HookConfig {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    #[serde(rename = "type")]
    pub strategy_type: String,
    pub system_prompt_template: String,
}

/// Loads an agent configuration from a YAML file.
pub fn load_config(path: &str) -> Result<AgentConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let config: AgentConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve a path relative to the workspace root (member tests run with the
    /// CWD set to the crate dir; shared `configs/` live two levels up).
    fn workspace_path(rel: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join(rel)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn test_load_react_config() {
        let config = load_config(&workspace_path("configs/react_agent.yaml"))
            .expect("Failed to load react config");
        assert_eq!(config.agent.name, "ReAct-Explorer");
        assert_eq!(config.strategy.strategy_type, "react");
    }

    #[test]
    fn test_load_plan_and_solve_config() {
        let config = load_config(&workspace_path("configs/plan_and_solve_agent.yaml"))
            .expect("Failed to load plan-and-solve config");
        assert_eq!(config.agent.name, "Plan-and-Solve-Strategist");
        assert_eq!(config.strategy.strategy_type, "plan_and_solve");
    }
}
