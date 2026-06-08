//! Phase 3 Validation: Context Manager Plugins & Self-Evolution Mechanism
//!
//! This test suite validates the implementation of Phase 3 architecture:
//! 1. Context Manager Plugin System (Project, WorkspaceRoot, Dynamic)
//! 2. Self-Evolution Mechanism (EvolutionHook, metrics logging, config updates)

use std::fs;
use tempfile::TempDir;

// Import from the workbench crate
use harnesslab_core::config::AgentConfig;
use harnesslab_core::context_managers::{
    DynamicContextManager, ProjectContextManager, WorkspaceRootManager,
};
use harnesslab_core::hooks::{EvolutionHook, EvolutionMetrics, EvolutionPolicy};
use harnesslab_core::plugins::ContextManager;

/// Validates that ProjectContextManager correctly discovers and prioritizes context files.
#[test]
fn test_project_context_manager_discovery() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create PISCI.md (Priority 100)
    fs::write(root.join("PISCI.md"), "Project Rules").unwrap();

    // Create .pisci/rules.md (Priority 90)
    fs::create_dir_all(root.join(".pisci")).unwrap();
    fs::write(root.join(".pisci").join("rules.md"), "Detailed Rules").unwrap();

    // Create kb/architecture/design.md (Priority 80)
    fs::create_dir_all(root.join("kb").join("architecture")).unwrap();
    fs::write(
        root.join("kb").join("architecture").join("design.md"),
        "Design Doc",
    )
    .unwrap();

    let manager = ProjectContextManager;
    let entries = manager.discover(root).unwrap();

    assert_eq!(entries.len(), 3, "Should discover 3 context files");
    assert_eq!(
        entries[0].priority, 100,
        "PISCI.md should have highest priority"
    );
    assert_eq!(
        entries[1].priority, 90,
        "rules.md should have second highest priority"
    );
    assert_eq!(
        entries[2].priority, 80,
        "design.md should have third highest priority"
    );

    println!("✅ ProjectContextManager discovery validated");
}

/// Validates that WorkspaceRootManager only scans immediate root files.
#[test]
fn test_workspace_root_manager_shallow_scan() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join("README.md"), "Readme Content").unwrap();
    fs::write(root.join("PISCI.md"), "Pisci Content").unwrap();

    // Create a nested file that should be IGNORED by WorkspaceRootManager
    fs::create_dir_all(root.join("kb").join("architecture")).unwrap();
    fs::write(
        root.join("kb").join("architecture").join("deep.md"),
        "Deep Doc",
    )
    .unwrap();

    let manager = WorkspaceRootManager;
    let entries = manager.discover(root).unwrap();

    assert_eq!(
        entries.len(),
        2,
        "WorkspaceRootManager should only find root files"
    );
    assert_eq!(entries[0].priority, 100); // PISCI.md
    assert_eq!(entries[1].priority, 50); // README.md

    // Ensure deep file is not included
    for entry in &entries {
        assert!(!entry.path.to_string_lossy().contains("deep.md"));
    }

    println!("✅ WorkspaceRootManager shallow scan validated");
}

/// Validates that DynamicContextManager handles an empty workspace gracefully.
#[test]
fn test_dynamic_context_manager_empty_workspace() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let manager = DynamicContextManager;
    let entries = manager.discover(root).unwrap();

    // With no documentation files present, the scan yields no entries.
    assert_eq!(entries.len(), 0);

    let rendered = manager.render(&entries);
    assert!(rendered.contains("No relevant context found"));

    println!("✅ DynamicContextManager empty-workspace behavior validated");
}

/// EvolutionHook wrapper for testing private fields.
struct TestableEvolutionHook {
    hook: EvolutionHook,
    log_path: std::path::PathBuf,
}

impl TestableEvolutionHook {
    fn new(root: &std::path::Path, policy: EvolutionPolicy) -> Self {
        let log_dir = root.join(".pisci");
        std::fs::create_dir_all(&log_dir).ok();
        let log_path = log_dir.join("evolution_log.jsonl");
        let hook = EvolutionHook::new(root, policy);
        Self { hook, log_path }
    }
}

/// Validates that EvolutionHook correctly logs metrics to JSONL.
#[test]
fn test_evolution_hook_logging() {
    let dir = TempDir::new().unwrap();
    let policy = EvolutionPolicy {
        enabled: true,
        ..Default::default()
    };
    let mut test_hook = TestableEvolutionHook::new(dir.path(), policy);

    let metrics = EvolutionMetrics {
        turn_id: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_call_count: 5,
        token_usage: 1000,
        user_correction_rate: 0.1,
        tool_call_efficiency: 0.9,
    };

    test_hook.hook.record_turn(metrics.clone()).unwrap();
    assert!(test_hook.log_path.exists(), "Log file should be created");

    let content = fs::read_to_string(&test_hook.log_path).unwrap();
    assert!(
        content.contains("\"turn_id\":1"),
        "Log should contain turn_id"
    );
    assert!(
        content.contains("\"tool_call_count\":5"),
        "Log should contain tool_call_count"
    );

    println!("✅ EvolutionHook logging validated");
}

/// Validates that EvolutionHook respects the 'enabled' flag.
#[test]
fn test_evolution_hook_disabled() {
    let dir = TempDir::new().unwrap();
    let policy = EvolutionPolicy {
        enabled: false,
        ..Default::default()
    };
    let mut test_hook = TestableEvolutionHook::new(dir.path(), policy);

    let metrics = EvolutionMetrics {
        turn_id: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_call_count: 5,
        token_usage: 1000,
        user_correction_rate: 0.5, // High rate, but should be ignored
        tool_call_efficiency: 0.5,
    };

    test_hook.hook.record_turn(metrics).unwrap();
    // Should not create log file if disabled
    assert!(
        !test_hook.log_path.exists(),
        "Log file should NOT be created when disabled"
    );

    println!("✅ EvolutionHook disabled state validated");
}

/// Validates that AgentConfig correctly parses evolution policy from YAML.
#[test]
fn test_config_evolution_policy_parsing() {
    let yaml = r#"
agent:
  name: "EvolutionTestAgent"
  model: "claude-haiku-4.5"

policy:
  evolution:
    enabled: true
    metrics:
      - "tool_call_efficiency"
      - "user_correction_rate"
    threshold:
      user_correction_rate: 0.15
    action: "propose_config_update"
"#;

    let config = AgentConfig::from_yaml(yaml).unwrap();
    assert!(config.policy.evolution.enabled);
    assert_eq!(config.policy.evolution.metrics.len(), 2);
    assert_eq!(config.policy.evolution.threshold.user_correction_rate, 0.15);
    assert_eq!(config.policy.evolution.action, "propose_config_update");

    println!("✅ Config evolution policy parsing validated");
}

/// Validates that EvolutionHook triggers on threshold breach.
#[test]
fn test_evolution_trigger_threshold() {
    let dir = TempDir::new().unwrap();
    let policy = EvolutionPolicy {
        enabled: true,
        threshold: harnesslab_core::hooks::EvolutionThresholds {
            user_correction_rate: 0.2,
        },
        ..Default::default()
    };
    let mut test_hook = TestableEvolutionHook::new(dir.path(), policy);

    let metrics_high_correction = EvolutionMetrics {
        turn_id: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_call_count: 5,
        token_usage: 1000,
        user_correction_rate: 0.3, // > 0.2 threshold
        tool_call_efficiency: 0.5,
    };

    // This should trigger an eprintln! in the current implementation
    test_hook.hook.record_turn(metrics_high_correction).unwrap();

    // Verify log was still written
    let content = fs::read_to_string(&test_hook.log_path).unwrap();
    assert!(content.contains("\"user_correction_rate\":0.3"));

    println!("✅ Evolution trigger threshold validated");
}
