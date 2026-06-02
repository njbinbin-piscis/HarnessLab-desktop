# Phase 1: Interface Standardization & Decoupling — Implementation Summary

## Overview

This document summarizes the implementation of **Phase 1** from the pisci-engine architecture refactor plan: defining plugin interfaces for `ContextManager` and `MemoryPlugin`, and integrating them into the `HarnessConfig` builder system.

## Changes Made

### 1. New Module: `pisci-kernel/src/context/mod.rs`

Defines the `ContextManager` trait that abstracts how project context (instructions, rules, environment info) is discovered and injected into the agent's prompt.

**Key types:**
- `trait ContextManager`: Core interface with `discover_instructions()`, `render_context()`, and `name()` methods.
- `InstructionFile`: A single instruction file with path and content.
- `ContextInjection`: Rendered context string with character budget tracking.
- `ContextError`: Error enum for I/O, discovery, and rendering failures.
- `ProjectContextManager`: Default implementation that replicates the original `project_context.rs` behavior (ancestor-chain scanning for PISCI.md files).

**Design decisions:**
- Async trait methods allow future implementations to fetch context from remote sources.
- The `render_context` method returns `Option<ContextInjection>` so callers can easily handle the "no context found" case.
- Character budgeting is preserved from the original implementation to prevent prompt overflow.

### 2. New Module: `pisci-kernel/src/memory/plugin.rs`

Defines the `MemoryPlugin` trait that abstracts how agent memories are stored and retrieved.

**Key types:**
- `trait MemoryPlugin`: Core interface with `store()`, `get()`, `delete()`, `search()`, `list_ids()`, `name()`, and `supports_vector_search()` methods.
- `MemoryEntry`: A memory entry with id, content, optional embedding, metadata, and timestamp.
- `MemorySearchResult`: Search result with entry and similarity score.
- `MemoryQuery`: Query parameters supporting both text and vector search.
- `MemoryError`: Error enum for storage, serialization, dimension mismatch, and not-found cases.
- `NoOpMemory`: Ephemeral implementation that discards all data (for Fish sub-agents).
- `InMemoryVectorStore`: In-memory storage with cosine similarity search (for testing/short sessions).

**Design decisions:**
- Vector search uses cosine similarity (reusing logic from the existing `memory/vector.rs` module).
- Embedding dimension validation prevents silent mismatches between different embedding models.
- Metadata filtering allows flexible querying beyond just content/embedding.
- The `supports_vector_search()` method lets callers know if vector queries will be effective.

### 3. Updated: `pisci-kernel/src/memory/mod.rs`

Added `pub mod plugin;` to export the new plugin module alongside the existing `vector` module.

### 4. Updated: `pisci-kernel/src/lib.rs`

Added `pub mod context;` to expose the new context module at the crate root.

### 5. Updated: `pisci-kernel/src/agent/harness/config.rs`

Extended `HarnessConfig` with two new optional fields:
- `context_manager: Option<Arc<dyn crate::context::ContextManager>>`
- `memory_plugin: Option<Arc<dyn crate::memory::plugin::MemoryPlugin>>`

Added builder methods:
- `with_context_manager(mgr: Arc<dyn ContextManager>) -> Self`
- `with_memory_plugin(mem: Arc<dyn MemoryPlugin>) -> Self`

Both fields default to `None`, maintaining backward compatibility with existing scene factories (`for_main_chat`, `for_koi`, `for_fish`, etc.). When `None`, callers can fall back to default implementations.

## Testing

All new code includes comprehensive unit tests:

**Context module tests (3 tests):**
- `discovers_instruction_files_from_ancestor_chain`: Verifies ancestor scanning works correctly.
- `renders_budgeted_context`: Verifies character budgeting and truncation.
- `returns_none_for_empty_budget`: Verifies graceful handling of zero budget.

**Memory plugin tests (4 tests):**
- `noop_memory_discards_all_data`: Verifies NoOpMemory doesn't persist anything.
- `in_memory_store_crud_operations`: Verifies basic CRUD on InMemoryVectorStore.
- `in_memory_vector_search`: Verifies cosine similarity ranking.
- `in_memory_keyword_search`: Verifies text-based fallback search.

**Existing tests:** All 4 harness config tests continue to pass, confirming backward compatibility.

## Next Steps (Phase 2 Preview)

With these interfaces in place, Phase 2 can proceed to:
1. Move `LoopDetector` out of `loop_.rs` into a `PolicyPlugin` or `Guard` component.
2. Encapsulate compaction logic into a `CompactionStrategy` plugin (already partially done via `compaction_strategy.rs`).
3. Simplify `AgentLoop` to retain only the core `fetch_tools → call_llm → parse_response → execute_tools` loop.

## File Inventory

| File | Action | Purpose |
|------|--------|---------|
| `pisci-kernel/src/context/mod.rs` | Created | ContextManager trait + ProjectContextManager impl |
| `pisci-kernel/src/memory/plugin.rs` | Created | MemoryPlugin trait + NoOpMemory + InMemoryVectorStore impls |
| `pisci-kernel/src/memory/mod.rs` | Modified | Added `pub mod plugin;` |
| `pisci-kernel/src/lib.rs` | Modified | Added `pub mod context;` |
| `pisci-kernel/src/agent/harness/config.rs` | Modified | Added context_manager and memory_plugin fields + builder setters |
