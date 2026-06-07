//! Component catalog (the Palette / Library data source).
//!
//! P1 ships a static catalog of the built-in components by kind, mirroring
//! HarnessLab's library. Each entry carries a name + description and an (empty
//! for now) JSON-Schema-ish `params_schema`; later phases populate schemas and
//! real reference snippets, and make the catalog hot-reloadable from packages.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSpec {
    pub kind: String,
    pub name: String,
    pub description: String,
    pub params_schema: serde_json::Value,
    pub source: String,
    pub reference_langs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentReference {
    pub kind: String,
    pub name: String,
    pub languages: Vec<String>,
    pub snippets: BTreeMap<String, String>,
}

/// (kind, name, description) rows for every built-in component.
const COMPONENTS: &[(&str, &str, &str)] = &[
    // loop strategies
    ("loop", "react", "Reason+Act loop: interleave thoughts and tool calls."),
    ("loop", "plan_and_solve", "Plan first, then execute the plan step by step."),
    ("loop", "reflexion", "Self-reflect on failures and retry with lessons learned."),
    ("loop", "tree_of_thought", "Explore multiple reasoning branches and pick the best."),
    ("loop", "rewoo", "Reason Without Observation: plan tool calls up front."),
    ("loop", "tool_calling", "Natural tool-calling loop ending on completion."),
    ("loop", "budgeted", "Step-ceiling loop that stops after a fixed number of turns."),
    ("loop", "reflect_retry", "Reflect-and-retry on a token/cost ceiling."),
    // compaction
    ("compaction", "none", "No compaction (transparent default)."),
    ("compaction", "summary", "LLM rolling-summary compaction."),
    ("compaction", "sliding_window", "Token-budget truncation, no LLM call."),
    ("compaction", "vector_retrieval", "Relevance-ranked retention of older messages."),
    // memory
    ("memory", "noop", "Ephemeral, no memory injection."),
    ("memory", "in_memory_vector", "In-memory vector store for session-scoped memory."),
    ("memory", "sqlite", "Persistent SQLite-backed memory."),
    ("memory", "persistent_vector", "On-disk vector store (hashing/bow/openai embeddings)."),
    // context
    ("context", "none", "No extra context."),
    ("context", "workspace_root", "Shallow scan of README/PISCI at the workspace root."),
    ("context", "project_context", "Scan project rules and architecture docs."),
    ("context", "rag", "Retrieval-augmented context over a corpus dir."),
    // coordinators
    ("coordinator", "sequential_pipeline", "Run agents in sequence, piping outputs."),
    ("coordinator", "supervisor", "A supervisor delegates to worker agents."),
    ("coordinator", "group_chat", "Agents converse in a shared group chat."),
    ("coordinator", "debate", "Agents debate to converge on an answer."),
    ("coordinator", "majority_vote", "Aggregate independent answers by majority."),
    ("coordinator", "auction", "Agents bid for tasks; best bid wins."),
    ("coordinator", "blackboard", "Shared blackboard coordination."),
    ("coordinator", "map_reduce", "Map work across agents then reduce."),
    ("coordinator", "hierarchical", "Manager → workers → manager hierarchy."),
    ("coordinator", "tournament", "Round-based tournament with a judge agent."),
    ("coordinator", "coscientist", "AI co-scientist research-board multi-agent loop."),
    // scorers
    ("scorer", "exact_match", "Exact string match."),
    ("scorer", "f1", "Token-level F1 overlap."),
    ("scorer", "numeric", "Numeric closeness."),
    ("scorer", "contains", "Substring containment."),
    ("scorer", "mcq_letter", "Single/multi-letter MCQ grading."),
    ("scorer", "rubric_overlap", "Deterministic keyword-recall rubric."),
    ("scorer", "llm_judge", "LLM-as-judge for open-ended answers."),
    ("scorer", "weighted_rubric", "Multi-criteria weighted rubric aggregation."),
    // lifecycle stages (empty slots by default; concrete variants listed)
    ("credentials", "static", "Fixed endpoint/key/model override."),
    ("capabilities", "model_table", "Vision/streaming/context-window lookup by model."),
    ("skills", "static", "Inject a fixed skill instruction block."),
    ("skills", "directory", "Summarise every *.md under a folder into skills."),
    ("prompt", "template", "Assemble the system prompt from a template."),
    ("approval", "tool_policy", "Allow/deny tool calls by name."),
    ("budget", "token", "Stop the loop on a token ceiling."),
    ("budget", "cost", "Stop the loop on a USD ceiling."),
    ("hooks", "logging", "Log lifecycle events to stderr."),
    ("hooks", "jsonl_audit", "Append an audit record per event."),
    ("session", "jsonl", "Append-only session event log."),
    ("session", "branching_tree", "A real branching session tree."),
    ("tracing", "jsonl", "One span record per scope."),
    ("tracing", "otel", "Real OpenTelemetry spans when the SDK is present."),
];

const TOOLS: &[(&str, &str)] = &[
    ("file_read", "Read a file from the workspace."),
    ("file_write", "Write a file in the workspace."),
    ("file_edit", "Search/replace edit a file."),
    ("shell", "Run a shell command (policy-gated)."),
    ("python", "Run isolated Python code (policy-gated)."),
    ("finish", "Signal task completion and end the loop."),
];

/// The full catalog grouped by kind (insertion-stable per kind).
pub fn catalog() -> BTreeMap<String, Vec<ComponentSpec>> {
    let mut out: BTreeMap<String, Vec<ComponentSpec>> = BTreeMap::new();
    for (kind, name, desc) in COMPONENTS {
        out.entry((*kind).to_string()).or_default().push(ComponentSpec {
            kind: (*kind).to_string(),
            name: (*name).to_string(),
            description: (*desc).to_string(),
            params_schema: serde_json::json!({}),
            source: "builtin".to_string(),
            reference_langs: Vec::new(),
        });
    }
    out
}

/// Component names grouped by kind — the validation table for the Edit-Ops
/// engine (`set_slot`, `set_topology` coordinator, `set_scorer`, …).
pub fn names_by_kind() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (kind, name, _desc) in COMPONENTS {
        out.entry((*kind).to_string())
            .or_default()
            .insert((*name).to_string());
    }
    out
}

/// The set of built-in tool names.
pub fn tool_names() -> BTreeSet<String> {
    TOOLS.iter().map(|(name, _)| (*name).to_string()).collect()
}

/// The built-in tool list.
pub fn tools() -> Vec<ToolSpec> {
    TOOLS
        .iter()
        .map(|(name, desc)| ToolSpec {
            name: (*name).to_string(),
            description: (*desc).to_string(),
        })
        .collect()
}

/// Look up a single component reference (P1: empty snippets placeholder).
pub fn component_reference(kind: &str, name: &str) -> ComponentReference {
    ComponentReference {
        kind: kind.to_string(),
        name: name.to_string(),
        languages: Vec::new(),
        snippets: BTreeMap::new(),
    }
}
