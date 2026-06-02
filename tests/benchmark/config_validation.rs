mod config_loader;

use std::path::PathBuf;
use pisci_kernel::agent::tool::ToolRegistry;
use pisci_kernel::policy::PolicyGate;
use std::sync::Arc;
use agent_workbench::config::AgentConfig as KernelAgentConfig;
use agent_workbench::loader::build_harness_config;

/// Validates that the YAML configuration is correctly parsed and mapped to the kernel's HarnessConfig.
pub fn validate_config_mapping(config_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load using the benchmark loader (verifies YAML structure).
    let benchmark_config = config_loader::load_config(config_path)?;

    // 2. Load using the library loader (verifies integration with HarnessConfig).
    let kernel_config = KernelAgentConfig::from_file(PathBuf::from(config_path).as_path())?;

    // 3. Cross-check that the two independent parsers agree on key fields.
    assert_eq!(
        benchmark_config.agent.name, kernel_config.agent.name,
        "agent name mismatch between benchmark and library parsers"
    );
    assert_eq!(
        benchmark_config.agent.model, kernel_config.agent.model,
        "model mismatch between benchmark and library parsers"
    );
    assert_eq!(
        benchmark_config.agent.max_tokens, kernel_config.agent.max_tokens,
        "max_tokens mismatch between benchmark and library parsers"
    );
    assert_eq!(
        benchmark_config.strategy.strategy_type, kernel_config.strategy.strategy_type,
        "strategy type mismatch between benchmark and library parsers"
    );
    assert_eq!(
        benchmark_config.plugins.memory, kernel_config.plugins.memory,
        "memory plugin mismatch between benchmark and library parsers"
    );
    assert_eq!(
        benchmark_config.plugins.context, kernel_config.plugins.context,
        "context manager mismatch between benchmark and library parsers"
    );
    assert_eq!(
        benchmark_config.policy.confirm_shell, kernel_config.policy.confirm_shell,
        "confirm_shell mismatch between benchmark and library parsers"
    );

    let registry = Arc::new(ToolRegistry::new());
    let policy = Arc::new(PolicyGate::new(PathBuf::from(".")));
    let project_dir = PathBuf::from(".");

    let harness = build_harness_config(&kernel_config, registry, policy, Some(&project_dir))?;

    // 4. Verify key mappings landed in the harness.
    println!("Validating config: {}", config_path);
    println!("  Agent Name: {}", kernel_config.agent.name);
    println!("  Model: {}", kernel_config.agent.model);
    println!("  Harness Scene: {}", harness.scene);
    println!("  Confirm Shell: {}", harness.confirm_flags.confirm_shell);
    println!("  Max Tokens: {}", harness.max_tokens);
    assert_eq!(harness.model, kernel_config.agent.model);
    assert_eq!(harness.max_tokens, kernel_config.agent.max_tokens);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_react_config_validation() {
        validate_config_mapping("configs/react_agent.yaml").expect("ReAct config validation failed");
    }

    #[test]
    fn test_plan_and_solve_config_validation() {
        validate_config_mapping("configs/plan_and_solve_agent.yaml").expect("Plan-and-Solve config validation failed");
    }

    #[test]
    fn test_strategy_differentiation() {
        let react_config = KernelAgentConfig::from_file(PathBuf::from("configs/react_agent.yaml").as_path()).unwrap();
        let pas_config = KernelAgentConfig::from_file(PathBuf::from("configs/plan_and_solve_agent.yaml").as_path()).unwrap();

        // Verify that the agents have different names and potentially different policy settings
        assert_ne!(react_config.agent.name, pas_config.agent.name);
        
        // In a real scenario, we might check if the system prompt templates differ
        // or if specific hooks (like PlanTracker) are present in one but not the other.
        println!("ReAct Agent: {}", react_config.agent.name);
        println!("P&S Agent: {}", pas_config.agent.name);
    }
}
