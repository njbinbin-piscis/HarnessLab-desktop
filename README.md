# Agent Workbench Configuration System

This module implements the YAML-based configuration parser for the Agent Workbench, enabling **0-code agent design** through declarative configuration files.

## Experiment Platform (Phase 5)

Beyond the configuration parser, the workbench is a full **pluggable / tunable /
observable** experiment platform built on three pillars (see
`kb/architecture/phase5_experiment_platform.md`):

1. **Runtime** — the multi-layer pluggable harness (kernel + `exec`).
2. **AI design assistant ("Architect", `configs/meta_agent.yaml`)** — a
   first-class, itself-designable agent that authors new harness components and
   agent configs. Component authoring + a `cargo build` regression gate live in
   `backend/core/component_builder.py`; new Rust strategies register through the
   kernel `contrib` registry (`piscis_kernel::agent::contrib`) and are selectable
   by name in any YAML config.
3. **Validation sets + observability** — `backend/core/datasets.py` (built-in
   smoke sets + user JSONL), `evaluator.py` (agent × dataset, per-task
   trajectory capture), and a persistent SQLite `store.py` for replay and
   cross-run comparison.

A self-improving loop (`backend/core/autoloop.py`) ties them together:
propose → evaluate on a validation set → read score → revise.

**Key HTTP endpoints** (FastAPI, `/api/v1`):

| area | endpoints |
|------|-----------|
| component registry | `GET /registry`, `GET /registry/{kind}/{name}` |
| validation sets | `GET/POST/DELETE /datasets`, `GET /datasets/{id}/tasks` |
| evaluation runs | `POST /eval-runs`, `GET /eval-runs`, `/{id}`, `/{id}/stream`, `/{id}/tasks/{tid}/trajectory`, `/compare` |
| design assistant | `GET /design/architect`, `POST /design/generate`, `POST /design/components/{python,rust,rust-alias}` |
| self-improving loop | `POST /autoloop` |

The unified component registry (`backend/core/registry.py`) catalogues every
swappable part (coordinators, scorers, memory/context/loop/compaction) with a
parameter schema, so the UI and the Architect can discover what is available.

## `piscis-engine` dependency

The Rust CLI links `piscis-kernel` as a **git dependency** pinned to tag
**`v0.7.38`** (same rev as openpiscis and CodeZ). To hack the kernel locally,
uncomment the `[patch]` block at the bottom of `Cargo.toml` and point it at a
sibling `../piscis-engine` checkout.

## Architecture

The configuration system consists of three layers:

1. **YAML Structures** (`src/config.rs`): Serde-annotated Rust structs that map directly to YAML fields
2. **Configuration Loader** (`src/loader.rs`): Bridges YAML config to `piscis_kernel::HarnessConfig`
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

**Supported Memory Plugins:** (wired into the kernel `memory_plugin` slot; retrieved at run time and injected into the system prompt)
- `NoOpMemory` - Ephemeral, no injection (behaviour-preserving default path)
- `InMemoryVectorStore` - In-memory vector store for session-scoped memory
- `SqliteMemory` - Persistent SQLite-backed memory (WIP; currently falls back to the in-memory store)

Use `memory_retrieval_prompt` to control how retrieved memories are formatted into the prompt. The literal `{memories}` placeholder is replaced with the rendered hit list:

```yaml
plugins:
  memory: "InMemoryVectorStore"
  memory_retrieval_prompt: |
    ## Relevant past context
    {memories}
```

**Supported Context Managers:** (wired into the kernel `context_manager` slot; rendered at run time from the workspace root)
- `ProjectContextManager` - Scans for `PISCI.md`, `.pisci/rules.md`, and `kb/architecture/*.md`
- `WorkspaceRootManager` - Shallow scan of `README.md` / `PISCI.md`
- `DynamicContextManager` - Semantic-search stub

### Compression Configuration

Selects the kernel `CompactionStrategy` algorithm (separate from the tier thresholds in `policy`):

```yaml
compression:
  strategy: "summary_based"   # default: LLM rolling summary (DefaultCompaction)
  # strategy: "sliding_window"   # token-budget truncation, no LLM call
  # strategy: "vector_retrieval" # relevance-ranked retention of older messages
  summarizer_prompt: null        # optional override for summary-based strategies (reserved)
```

### Tool Configuration

```yaml
enabled_tools:        # absent = all tools; [] = no tools; list = whitelist
  - "file_read"
  - "shell"
```

- Omit `enabled_tools` entirely → all built-in tools are registered.
- `enabled_tools: []` → no tools (reasoning-only ablation).
- `enabled_tools: [a, b]` → only those tools.

The CLI `--tools a,b,c` / `--no-tools` flags override `enabled_tools` at run time.

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

> Note: today `strategy.type` selects only the system-prompt template; the agent
> loop algorithm itself is the kernel's built-in ReAct loop. A pluggable
> `LoopStrategy` abstraction (true Plan-and-Solve / Reflexion algorithm
> switching, plus run-time steering) lands in Layer 2 of the workbench roadmap.
> Until then, "strategy switching" means prompt + tool-set + plugin composition,
> not a different control loop.

To switch between ReAct and Plan-and-Solve prompt patterns:

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
