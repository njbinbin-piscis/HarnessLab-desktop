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
    /// Target platform: `any` (platform-independent) | `linux` | `windows` | `macos`.
    pub platform: String,
    /// Functional category used to group tools in the Tool Library UI.
    pub category: String,
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
    (
        "loop",
        "react",
        "Reason+Act loop: interleave thoughts and tool calls.",
    ),
    (
        "loop",
        "plan_and_solve",
        "Plan first, then execute the plan step by step.",
    ),
    (
        "loop",
        "reflexion",
        "Self-reflect on failures and retry with lessons learned.",
    ),
    (
        "loop",
        "tree_of_thought",
        "Explore multiple reasoning branches and pick the best.",
    ),
    (
        "loop",
        "rewoo",
        "Reason Without Observation: plan tool calls up front.",
    ),
    (
        "loop",
        "tool_calling",
        "Natural tool-calling loop ending on completion.",
    ),
    (
        "loop",
        "budgeted",
        "Step-ceiling loop that stops after a fixed number of turns.",
    ),
    (
        "loop",
        "reflect_retry",
        "Reflect-and-retry on a token/cost ceiling.",
    ),
    // compaction
    ("compaction", "none", "No compaction (transparent default)."),
    ("compaction", "summary", "LLM rolling-summary compaction."),
    (
        "compaction",
        "sliding_window",
        "Token-budget truncation, no LLM call.",
    ),
    (
        "compaction",
        "vector_retrieval",
        "Relevance-ranked retention of older messages.",
    ),
    // memory
    ("memory", "noop", "Ephemeral, no memory injection."),
    (
        "memory",
        "in_memory_vector",
        "In-memory vector store for session-scoped memory.",
    ),
    ("memory", "sqlite", "Persistent SQLite-backed memory."),
    (
        "memory",
        "persistent_vector",
        "On-disk vector store (hashing/bow/openai embeddings).",
    ),
    // context
    ("context", "none", "No extra context."),
    (
        "context",
        "workspace_root",
        "Shallow scan of README/PISCI at the workspace root.",
    ),
    (
        "context",
        "project_context",
        "Scan project rules and architecture docs.",
    ),
    (
        "context",
        "rag",
        "Retrieval-augmented context over a corpus dir.",
    ),
    // coordinators
    (
        "coordinator",
        "sequential_pipeline",
        "Run agents in sequence, piping outputs.",
    ),
    (
        "coordinator",
        "supervisor",
        "A supervisor delegates to worker agents.",
    ),
    (
        "coordinator",
        "group_chat",
        "Agents converse in a shared group chat.",
    ),
    (
        "coordinator",
        "debate",
        "Agents debate to converge on an answer.",
    ),
    (
        "coordinator",
        "majority_vote",
        "Aggregate independent answers by majority.",
    ),
    (
        "coordinator",
        "auction",
        "Agents bid for tasks; best bid wins.",
    ),
    (
        "coordinator",
        "blackboard",
        "Shared blackboard coordination.",
    ),
    (
        "coordinator",
        "map_reduce",
        "Map work across agents then reduce.",
    ),
    (
        "coordinator",
        "hierarchical",
        "Manager → workers → manager hierarchy.",
    ),
    (
        "coordinator",
        "tournament",
        "Round-based tournament with a judge agent.",
    ),
    (
        "coordinator",
        "coscientist",
        "AI co-scientist research-board multi-agent loop.",
    ),
    // scorers
    ("scorer", "exact_match", "Exact string match."),
    ("scorer", "f1", "Token-level F1 overlap."),
    ("scorer", "numeric", "Numeric closeness."),
    ("scorer", "contains", "Substring containment."),
    ("scorer", "mcq_letter", "Single/multi-letter MCQ grading."),
    (
        "scorer",
        "rubric_overlap",
        "Deterministic keyword-recall rubric.",
    ),
    (
        "scorer",
        "llm_judge",
        "LLM-as-judge for open-ended answers.",
    ),
    (
        "scorer",
        "weighted_rubric",
        "Multi-criteria weighted rubric aggregation.",
    ),
    // lifecycle stages (empty slots by default; credentials defaults to env)
    (
        "credentials",
        "env",
        "Use PISCIS_PROVIDER / PISCIS_API_KEY / PISCIS_BASE_URL from the environment.",
    ),
    (
        "credentials",
        "static",
        "Fixed endpoint/key/model override (OpenAI-compatible).",
    ),
    (
        "capabilities",
        "model_table",
        "Vision/streaming/context-window lookup by model.",
    ),
    (
        "skills",
        "static",
        "Inject a fixed skill instruction block.",
    ),
    (
        "skills",
        "directory",
        "Summarise every *.md under a folder into skills.",
    ),
    (
        "prompt",
        "template",
        "Assemble the system prompt from a template.",
    ),
    ("approval", "tool_policy", "Allow/deny tool calls by name."),
    ("budget", "token", "Stop the loop on a token ceiling."),
    ("budget", "cost", "Stop the loop on a USD ceiling."),
    ("hooks", "logging", "Log lifecycle events to stderr."),
    ("hooks", "jsonl_audit", "Append an audit record per event."),
    ("session", "jsonl", "Append-only session event log."),
    (
        "session",
        "branching_tree",
        "A real branching session tree.",
    ),
    ("tracing", "jsonl", "One span record per scope."),
    (
        "tracing",
        "otel",
        "Real OpenTelemetry spans when the SDK is present.",
    ),
];

/// (name, description, platform, category) rows for every built-in tool.
///
/// `platform` is the taxonomy used by the Tool Library: `any` (works on every
/// OS via a cross-platform implementation), or `linux`/`windows`/`macos` for
/// OS-specific tools. The runtime only registers tools whose platform matches
/// the host (and that the agent's `enabled_tools` permits).
const TOOLS: &[(&str, &str, &str, &str)] = &[
    (
        "file_read",
        "Read a UTF-8 text file from the workspace.",
        "any",
        "filesystem",
    ),
    (
        "file_write",
        "Create or overwrite a file in the workspace.",
        "any",
        "filesystem",
    ),
    (
        "file_edit",
        "Search/replace edit a file in the workspace.",
        "any",
        "filesystem",
    ),
    (
        "file_diff",
        "Show a line diff between a file and proposed content.",
        "any",
        "filesystem",
    ),
    (
        "file_list",
        "List the entries of a workspace directory.",
        "any",
        "filesystem",
    ),
    (
        "file_search",
        "Recursively search workspace files for a substring.",
        "any",
        "filesystem",
    ),
    (
        "shell",
        "Run a shell command in the workspace.",
        "any",
        "execution",
    ),
    (
        "code_run",
        "Execute a python or bash snippet.",
        "any",
        "execution",
    ),
    (
        "process_control",
        "Inspect host processes.",
        "any",
        "execution",
    ),
    (
        "web_search",
        "Search the web and return result snippets.",
        "any",
        "network",
    ),
    (
        "url_fetcher",
        "Fetch a URL and return its text content.",
        "any",
        "network",
    ),
    (
        "register_algorithm",
        "Author and hot-register a new declarative loop strategy into the running engine.",
        "any",
        "meta",
    ),
    (
        "finish",
        "Signal task completion with a final answer.",
        "any",
        "control",
    ),
];

/// The full catalog grouped by kind (insertion-stable per kind).
pub fn catalog() -> BTreeMap<String, Vec<ComponentSpec>> {
    let mut out: BTreeMap<String, Vec<ComponentSpec>> = BTreeMap::new();
    for (kind, name, desc) in COMPONENTS {
        let langs = if reference_source(kind, name).is_some() {
            vec!["rust".to_string()]
        } else {
            Vec::new()
        };
        out.entry((*kind).to_string())
            .or_default()
            .push(ComponentSpec {
                kind: (*kind).to_string(),
                name: (*name).to_string(),
                description: (*desc).to_string(),
                params_schema: serde_json::json!({}),
                source: "builtin".to_string(),
                reference_langs: langs,
            });
    }
    out
}

/// Component names grouped by kind, merged with installed market packages.
pub fn names_by_kind_merged(extras: &[(String, String)]) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = names_by_kind();
    for (kind, name) in extras {
        out.entry(kind.clone()).or_default().insert(name.clone());
    }
    out
}

/// Full catalog merged with installed market algorithm packages.
pub fn catalog_merged(extras: &[(String, String, String)]) -> BTreeMap<String, Vec<ComponentSpec>> {
    let mut out = catalog();
    append_extras(&mut out, extras, "market");
    out
}

/// Append `(kind, name, description)` extras to a catalog, tagging each with
/// `source` (e.g. `"market"` or `"registered"`).
pub fn append_extras(
    catalog: &mut BTreeMap<String, Vec<ComponentSpec>>,
    extras: &[(String, String, String)],
    source: &str,
) {
    for (kind, name, desc) in extras {
        catalog
            .entry(kind.clone())
            .or_default()
            .push(ComponentSpec {
                kind: kind.clone(),
                name: name.clone(),
                description: desc.clone(),
                params_schema: serde_json::json!({}),
                source: source.into(),
                reference_langs: Vec::new(),
            });
    }
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
    TOOLS.iter().map(|(name, ..)| (*name).to_string()).collect()
}

/// The built-in tool list (with platform + category metadata).
pub fn tools() -> Vec<ToolSpec> {
    TOOLS
        .iter()
        .map(|(name, desc, platform, category)| ToolSpec {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            platform: (*platform).to_string(),
            category: (*category).to_string(),
        })
        .collect()
}

// ── Real source references (view-code) ──────────────────────────────────────

// Embed the actual native-runtime + workbench source so the algorithm library's
// "view code" panel shows the real implementation, not a placeholder.
const LOOP_SRC: &str = include_str!("../../harness-rt/src/agent/loop_strategy.rs");
const COMPACTION_SRC: &str = include_str!("../../harness-rt/src/agent/compaction.rs");
const MEMORY_SRC: &str = include_str!("../../harness-rt/src/memory/plugin.rs");
const SCORING_SRC: &str = include_str!("../../harness-rt/src/scoring.rs");
const CONTEXT_SRC: &str = include_str!("context_managers.rs");
const COORD_SRC: &str = include_str!("runner/coordinator.rs");
const PLUGINS_SRC: &str = include_str!("plugins.rs");

/// Map a `(kind, name)` to `(source_file, anchor)` for source extraction.
///
/// The anchor is a unique substring on the line that begins the relevant item
/// (a `struct`/`impl`/`fn`/match arm); the extractor then captures the balanced
/// block that follows.
fn reference_source(kind: &str, name: &str) -> Option<(&'static str, String)> {
    let pair: (&'static str, String) = match (kind, name) {
        // Loop strategies.
        ("loop", "react") => (LOOP_SRC, "impl LoopStrategy for ReActStrategy".into()),
        ("loop", n)
            if BTreeSet::from([
                "plan_and_solve",
                "reflexion",
                "tree_of_thought",
                "rewoo",
                "tool_calling",
                "budgeted",
                "reflect_retry",
            ])
            .contains(n) =>
        {
            (LOOP_SRC, format!("\"{n}\" =>"))
        }

        // Compaction strategies.
        ("compaction", "none") => (
            COMPACTION_SRC,
            "impl CompactionStrategy for NoneCompaction".into(),
        ),
        ("compaction", "sliding_window") => (
            COMPACTION_SRC,
            "impl CompactionStrategy for SlidingWindowCompaction".into(),
        ),
        ("compaction", "summary") => (
            COMPACTION_SRC,
            "impl CompactionStrategy for SummaryCompaction".into(),
        ),
        ("compaction", "vector_retrieval") => (
            COMPACTION_SRC,
            "impl CompactionStrategy for VectorRetrievalCompaction".into(),
        ),

        // Memory backends (the trait itself for the ephemeral no-op case).
        ("memory", "noop") => (MEMORY_SRC, "pub trait MemoryPlugin".into()),
        ("memory", "in_memory_vector") => (
            MEMORY_SRC,
            "impl MemoryPlugin for InMemoryVectorStore".into(),
        ),
        ("memory", "sqlite") => (MEMORY_SRC, "impl MemoryPlugin for SqliteMemory".into()),
        ("memory", "persistent_vector") => {
            (MEMORY_SRC, "impl MemoryPlugin for SqliteMemory".into())
        }

        // Context managers.
        ("context", "none") => (
            PLUGINS_SRC,
            "impl ContextManager for NoOpContextManager".into(),
        ),
        ("context", "workspace_root") => (
            CONTEXT_SRC,
            "impl ContextManager for WorkspaceRootManager".into(),
        ),
        ("context", "project_context") => (
            CONTEXT_SRC,
            "impl ContextManager for ProjectContextManager".into(),
        ),
        ("context", "rag") => (
            CONTEXT_SRC,
            "impl ContextManager for DynamicContextManager".into(),
        ),

        // Coordinators.
        ("coordinator", n) => (COORD_SRC, format!("async fn {n}")),

        // Scorers.
        ("scorer", "exact_match") => (SCORING_SRC, "pub fn exact_match".into()),
        ("scorer", "f1") => (SCORING_SRC, "pub fn f1".into()),
        ("scorer", "numeric") => (SCORING_SRC, "pub fn numeric".into()),
        ("scorer", "contains") => (SCORING_SRC, "pub fn contains".into()),
        ("scorer", "mcq_letter") => (SCORING_SRC, "pub fn mcq_letter".into()),
        ("scorer", "rubric_overlap") => (SCORING_SRC, "pub fn rubric_overlap".into()),
        ("scorer", "llm_judge") => (SCORING_SRC, "pub fn llm_judge".into()),
        ("scorer", "weighted_rubric") => (SCORING_SRC, "pub fn weighted_rubric".into()),

        _ => return None,
    };
    // Only return if the anchor is actually present in the embedded source.
    if pair.0.contains(pair.1.as_str()) {
        Some(pair)
    } else {
        None
    }
}

/// Extract the balanced `{...}` block that starts at the first `{` after the
/// line containing `anchor`, returning that whole line through the closing brace.
fn extract_braced(src: &str, anchor: &str) -> Option<String> {
    let pos = src.find(anchor)?;
    // Back up to the start of the anchor's line.
    let line_start = src[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let open_rel = src[pos..].find('{')?;
    let open = pos + open_rel;
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(src[line_start..=i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Extract a `match` arm whose key line contains `anchor` (e.g. `"reflexion" =>`),
/// capturing the balanced `(...)` tuple that follows.
fn extract_arm(src: &str, anchor: &str) -> Option<String> {
    let pos = src.find(anchor)?;
    let line_start = src[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let open_rel = src[pos..].find('(')?;
    let open = pos + open_rel;
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    // Include a trailing comma if present.
                    let end = if i + 1 < bytes.len() && bytes[i + 1] == b',' {
                        i + 1
                    } else {
                        i
                    };
                    return Some(src[line_start..=end].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Look up a single component's real source reference for the view-code panel.
pub fn component_reference(kind: &str, name: &str) -> ComponentReference {
    let mut snippets: BTreeMap<String, String> = BTreeMap::new();
    let mut languages: Vec<String> = Vec::new();

    if let Some((src, anchor)) = reference_source(kind, name) {
        // Match arms use `(...)`; everything else uses `{...}`.
        let extracted = if anchor.contains("=>") {
            extract_arm(src, &anchor)
        } else {
            extract_braced(src, &anchor)
        };
        if let Some(code) = extracted {
            languages.push("rust".to_string());
            snippets.insert("rust".to_string(), code);
        }
    }

    ComponentReference {
        kind: kind.to_string(),
        name: name.to_string(),
        languages,
        snippets,
    }
}

#[cfg(test)]
mod ref_tests {
    use super::*;

    #[test]
    fn references_extract_real_source() {
        // A representative component per kind yields non-empty rust source.
        for (kind, name) in [
            ("loop", "react"),
            ("loop", "reflexion"),
            ("compaction", "sliding_window"),
            ("memory", "sqlite"),
            ("context", "rag"),
            ("coordinator", "debate"),
            ("scorer", "f1"),
        ] {
            let r = component_reference(kind, name);
            assert_eq!(
                r.languages,
                vec!["rust".to_string()],
                "{kind}/{name} has rust"
            );
            let code = r.snippets.get("rust").expect("rust snippet");
            assert!(code.len() > 20, "{kind}/{name} snippet non-trivial: {code}");
        }
    }

    #[test]
    fn unknown_component_has_no_reference() {
        let r = component_reference("loop", "does_not_exist");
        assert!(r.languages.is_empty());
        assert!(r.snippets.is_empty());
    }

    #[test]
    fn catalog_marks_reference_langs() {
        let cat = catalog();
        let loops = &cat["loop"];
        let react = loops.iter().find(|c| c.name == "react").unwrap();
        assert_eq!(react.reference_langs, vec!["rust".to_string()]);
    }
}
