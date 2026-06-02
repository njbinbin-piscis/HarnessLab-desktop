# Phase 3: Context Manager Plugin System & Self-Evolution Mechanism

## 1. Overview
Phase 3 focuses on two critical architectural advancements to meet the `org_spec` success criteria:
1. **Context Manager Plugin System**: Formalizing the interface for context discovery and injection, and implementing three distinct strategies.
2. **Self-Evolution Mechanism**: Designing a feedback loop that allows the Agent to refine its own configuration (`YAML`) based on execution performance.

## 2. Context Manager Plugin System

### 2.1 Interface Definition (`IContextManager`)
The `ContextManager` trait is responsible for discovering and rendering project-specific instructions into the Agent's prompt.

```rust
pub trait ContextManager: Send + Sync {
    /// Discover and load context from the given root directory.
    fn discover(&self, root: &Path) -> Result<Vec<ContextEntry>>;
    
    /// Render discovered context into a prompt-ready string.
    fn render(&self, entries: &[ContextEntry]) -> String;
}

pub struct ContextEntry {
    pub path: PathBuf,
    pub content: String,
    pub priority: u8, // Higher priority entries are placed closer to the system prompt
}
```

### 2.2 Implementation Strategies (3 Types)

#### A. `ProjectContextManager` (Deep Scan)
- **Behavior**: Recursively scans ancestor directories for `PISCI.md`, `.pisci/rules.md`, or `kb/architecture/*.md`.
- **Use Case**: Complex projects with distributed documentation.
- **Config**: `plugins.context: "ProjectContextManager"`

#### B. `WorkspaceRootManager` (Shallow Scan)
- **Behavior**: Only scans the immediate project root for `README.md` and `PISCI.md`.
- **Use Case**: Large monorepos where deep scanning is too slow or noisy.
- **Config**: `plugins.context: "WorkspaceRootManager"`

#### C. `DynamicContextManager` (Semantic Search)
- **Behavior**: Uses a vector store to semantically search `kb/` and `docs/` based on the current user query.
- **Use Case**: Knowledge-intensive tasks requiring precise retrieval.
- **Config**: `plugins.context: "DynamicContextManager"`
- **Dependency**: Requires a configured `embedding_model` in `AgentConfig`.

## 3. Self-Evolution Mechanism

### 3.1 Concept
The Agent should be able to "learn" from its interactions by updating its own `AgentConfig` (YAML). This is achieved through a specialized **Evolution Hook**.

### 3.2 Architecture Components
1. **Performance Monitor (Hook)**: Tracks metrics like `tool_call_efficiency`, `user_correction_rate`, and `token_usage_per_turn`.
2. **Evolution Policy**: A set of rules in `policy.evolution` that trigger config updates.
   - Example: If `user_correction_rate > 0.2` for 5 turns, suggest a `system_prompt` refinement.
3. **Config Refiner (Tool)**: A protected tool `refine_config` that allows the Agent to propose YAML changes.

### 3.3 YAML Configuration for Evolution
```yaml
policy:
  evolution:
    enabled: true
    metrics:
      - "tool_call_efficiency"
      - "user_correction_rate"
    threshold:
      user_correction_rate: 0.2
    action: "propose_config_update"
```

### 3.4 Workflow
1. **Monitor**: The `EvolutionHook` records performance data in `.pisci/evolution_log.jsonl`.
2. **Trigger**: When thresholds are breached, the Agent is notified via a `SystemMessage`.
3. **Propose**: The Agent uses `refine_config` to generate a new `system_prompt` or adjust `policy` settings.
4. **Review**: Pisci or the User reviews the proposed YAML diff before applying it.

## 4. Implementation Plan for Coder

1. **Define Traits**: Add `ContextManager` and `EvolutionHook` traits to `src/plugins.rs`.
2. **Implement Managers**: 
   - `ProjectContextManager`: Reuse existing logic from `piscis-engine`.
   - `WorkspaceRootManager`: Simple file reader.
   - `DynamicContextManager`: Stub implementation using `InMemoryVectorStore`.
3. **Implement Evolution Hook**: 
   - Create `src/hooks/evolution.rs`.
   - Integrate with `AgentLoop` to capture `after_tool` metrics.
4. **Update Config Schema**: Add `evolution` section to `PolicyConfig` in `src/config.rs`.

## 5. Success Metrics for Phase 3
- [ ] Agent can switch between `ProjectContextManager` and `WorkspaceRootManager` via YAML without code changes.
- [ ] `DynamicContextManager` successfully retrieves relevant `kb/` entries for a test query.
- [ ] `EvolutionHook` correctly logs performance metrics to `.pisci/evolution_log.jsonl`.
- [ ] Agent can propose a valid YAML config update when triggered.

[ProjectStatus] follow_up_needed
@!Coder Please implement the Phase 3 architecture as defined above. Start with the `ContextManager` trait and the three implementations, then proceed to the `EvolutionHook`. Ensure all new components are covered by unit tests in `tests/benchmark/`.
