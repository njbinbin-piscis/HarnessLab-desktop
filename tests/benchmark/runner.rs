use agent_workbench::config::AgentConfig;
use agent_workbench::loader::build_harness_config;
use piscis_kernel::agent::tool::ToolRegistry;
use piscis_kernel::policy::PolicyGate;
use std::sync::Arc;

/// Runs a benchmark test for a given configuration file.
///
/// Unlike the original simulation, this drives the real library loader so the
/// benchmark validates the same `HarnessConfig` construction path used in
/// production. A full LLM loop (`into_agent_loop`) still requires a live
/// `LlmClient`, so strategy validation inspects the assembled system prompt.
pub async fn run_benchmark(
    config_path: &str,
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    // 1. Build a real HarnessConfig through the library loader.
    let config = AgentConfig::from_file(std::path::Path::new(config_path))?;
    let registry = Arc::new(ToolRegistry::new());
    let policy = Arc::new(PolicyGate::new(std::path::PathBuf::from(".")));

    let start_time = std::time::Instant::now();
    let harness = build_harness_config(&config, registry, policy, Some(std::path::Path::new(".")))?;
    let execution_time = start_time.elapsed();

    // 2. Validate that the strategy prompt actually shaped the harness.
    let rendered_prompt = harness.layered_prompt.render();
    let strategy_type = config.strategy.strategy_type.clone();
    let strategy_valid = match strategy_type.as_str() {
        "react" => rendered_prompt.contains("ReAct") || rendered_prompt.contains("Thought"),
        "plan_and_solve" => rendered_prompt.contains("Plan"),
        _ => false,
    };

    Ok(BenchmarkResult {
        agent_name: config.agent.name.clone(),
        strategy: strategy_type,
        execution_time_ms: execution_time.as_millis(),
        strategy_valid,
        tool_calls_count: 0, // Populated once a live agent loop is wired in.
    })
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub agent_name: String,
    pub strategy: String,
    pub execution_time_ms: u128,
    pub strategy_valid: bool,
    pub tool_calls_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_react_benchmark() {
        let result = run_benchmark("configs/react_agent.yaml")
            .await
            .expect("Benchmark failed");
        assert_eq!(result.strategy, "react");
        assert!(result.strategy_valid);
    }

    #[tokio::test]
    async fn test_plan_and_solve_benchmark() {
        let result = run_benchmark("configs/plan_and_solve_agent.yaml")
            .await
            .expect("Benchmark failed");
        assert_eq!(result.strategy, "plan_and_solve");
        assert!(result.strategy_valid);
    }
}
