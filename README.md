# Agent Workbench Configuration System

This module implements the YAML-based configuration parser for the Agent Workbench, enabling **0-code agent design** through declarative configuration files.

## Architecture

The configuration system consists of three layers:

1. **YAML Structures** (`src/config.rs`): Serde-annotated Rust structs that map directly to YAML fields
2. **Configuration Loader** (`src/loader.rs`): Bridges YAML config to `pisci_kernel::HarnessConfig`
3. **Plugin Resolution**: Maps string identifiers to concrete trait implementations

## Quick Start

### Load a Configuration File

```rust
use agent_workbench::{AgentConfig, build_harness_config};
use pisci_kernel::agent::tool::ToolRegistry;
use pisci_kernel::policy::PolicyGate;
use std::sync::Arc;
use std::path::PathBuf;

// Load from YAML file
let config = AgentConfig::from_file("configs/react_agent.yaml")?;

// Build runtime dependencies
let registry = Arc::new(ToolRegistry::new());
let policy = Arc::new(PolicyGate::new(PathBuf::from(".")));

// Construct HarnessConfig
let harness = build_harness_config(
    &config,
    registry,
    policy,
    Some(&PathBuf::from("/path/to/project"))
)?;
```

### Parse from YAML String

```rust
let yaml = r#"
agent:
  name: "Researcher"
  model: "claude-sonnet-4.5"
  context_window: 200000
  max_tokens: 8192

plugins:
  memory: "InMemoryVectorStore"
  context: "ProjectContextManager"

policy:
  confirm_shell: false
  confirm_file_write: true
"#;

let config = AgentConfig::from_yaml(yaml)?;
```

## Configuration Reference

### Agent Definition

```yaml
agent:
  name: "MyAgent"                    # Human-readable name
  model: "claude-sonnet-4.5"         # Primary LLM model
  context_window: 200000             # Model context window (0 = auto)
  max_tokens: 8192                   # Max output tokens per turn
  fallback_models:                   # Fallback models on failure
    - "gpt-4o"
    - "qwen-max"
  enable_streaming: true             # Enable SSE streaming
  vision_override: null              # Vision capability override (null = auto)
  vision_model: "qwen-vl"            # Separate vision model (optional)
```

### Plugin Configuration

```yaml
plugins:
  memory: "InMemoryVectorStore"      # Memory plugin
  context: "ProjectContextManager"   # Context manager plugin
```

**Supported Memory Plugins:**
- `NoOpMemory` - Ephemeral, no persistence (for fish/short tasks)
- `InMemoryVectorStore` - In-memory vector store for session-scoped memory
- `SqliteMemory` - Persistent SQLite-backed memory (WIP)

**Supported Context Managers:**
- `ProjectContextManager` - Scans ancestor directories for instruction files (PISCI.md, etc.)
- `WorkspaceRootManager` - Root-only scanning (WIP)

### Policy Configuration

```yaml
policy:
  confirm_shell: false               # Require confirmation for shell commands
  confirm_file_write: true           # Require confirmation for file writes
  auto_compact_threshold: 100000     # Auto-compaction trigger (tokens, 0 = disabled)
  compaction_tiers:
    micro_percent: 50                # Micro compaction at 50% capacity
    auto_percent: 70                 # Auto compaction at 70% capacity
    full_percent: 90                 # Full compaction at 90% capacity
  max_tool_result_tokens: 4000       # Cap per tool result
  summary_model: "claude-haiku"      # Model for rolling summaries (optional)
```

### Hooks Configuration

```yaml
hooks:
  - type: "FileJournal"
    path: ".pisci/journal.db"
  - type: "AuditLog"
    level: "debug"
```

Hooks are injected at key lifecycle events:
- `before_tool` / `after_tool` - Tool execution hooks
- `on_prompt_event` - Prompt assembly hooks
- `on_llm_call_event` - LLM call hooks
- `on_context_event` - Context management hooks

## Strategy Switching

The key benefit of this configuration system is **strategy switching without code changes**. To switch between ReAct and Plan-and-Solve patterns:

### ReAct Strategy (`configs/react_agent.yaml`)
- Direct tool usage based on immediate reasoning
- No explicit planning phase
- Lower `auto_compact_threshold` for faster iteration

### Plan-and-Solve Strategy (`configs/plan_and_solve_agent.yaml`)
- Explicit planning with `pool_org` todo management
- Higher `auto_compact_threshold` for longer context retention
- Uses `summary_model` for better long-context handling
- Enables `PlanTracker` hook for plan state monitoring

To switch strategies, simply change the config file path:

```rust
// Switch from ReAct to Plan-and-Solve
let config = AgentConfig::from_file("configs/plan_and_solve_agent.yaml")?;
// Rest of the code remains identical
```

## Example Configurations

See the `configs/` directory for complete examples:
- `react_agent.yaml` - ReAct pattern configuration
- `plan_and_solve_agent.yaml` - Plan-and-Solve pattern configuration

## Extending the System

### Adding a New Plugin

1. Implement the trait in `pisci-kernel` (e.g., `impl MemoryPlugin for MyCustomMemory`)
2. Add the plugin identifier to `resolve_memory_plugin()` in `src/loader.rs`
3. Document the new plugin in this README

### Adding a New Hook Type

1. Implement `AgentHooks` trait in `pisci-kernel`
2. Add hook instantiation logic in the loader (future enhancement)
3. Configure via YAML `hooks` section

## Testing

Run the configuration tests:

```bash
cd /home/agent/Projects/pisci/agent-workbench
cargo test
```

Tests cover:
- Minimal config parsing
- Full config parsing with all fields
- Round-trip serialization/deserialization
- Harness config construction
- Error handling for unknown plugins
