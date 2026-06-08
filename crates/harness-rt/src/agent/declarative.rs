//! Declarative algorithms — data-defined components the meta-agent ("Architect")
//! can author and hot-register into the running engine without a recompile.
//!
//! A [`DeclarativeAlgorithm`] expresses a loop strategy purely as data (a system
//! prompt addendum plus behavior flags). Registering one installs a factory into
//! the process-global [`contrib`](super::contrib) registry so it is immediately
//! resolvable by `strategy.type`, and records it in a drainable log so the host
//! application can persist it for future sessions.

use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::compaction::TemplatedSummaryCompaction;
use super::contrib;
use super::loop_strategy::PromptPatternStrategy;
use crate::context::DeclarativeContextManager;

/// Kinds that are fully wired into the runtime (resolvable/dispatchable). Other
/// kinds (coordinator/memory) are accepted as catalog-level templates but are
/// surfaced as "advanced" in the UI since the runtime cannot dispatch them yet.
pub const RUNTIME_KINDS: &[&str] = &["loop", "compaction", "context", "scorer"];

/// A data-only algorithm definition that can be registered at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeAlgorithm {
    /// Component kind: `loop`, `compaction`, `context`, or `scorer` (wired);
    /// `coordinator`/`memory` are accepted as advanced templates.
    pub kind: String,
    /// Unique selectable name (e.g. used as `strategy.type`).
    pub name: String,
    /// Human-readable description for the catalog.
    #[serde(default)]
    pub description: String,
    /// System-prompt guidance / template injected for this component.
    #[serde(default)]
    pub addendum: String,
    /// When true, the loop runs a self-reflection pass before finishing.
    #[serde(default)]
    pub reflective: bool,
    /// Optional hard cap on loop iterations.
    #[serde(default)]
    pub max_iterations: Option<usize>,
    /// Kind-specific configuration:
    /// * `context`: `{ "keywords": ["..."] }`
    /// * `scorer`: `{ "base": "rubric_overlap|weighted_rubric|...", "keywords": [...], "criteria": [...] }`
    #[serde(default)]
    pub params: Value,
}

fn pending() -> &'static Mutex<Vec<DeclarativeAlgorithm>> {
    static PENDING: OnceLock<Mutex<Vec<DeclarativeAlgorithm>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a declarative algorithm into the running engine.
///
/// Records it in the drainable log (for host persistence) and installs a live
/// factory in the contrib registry. Returns an error for unsupported kinds or
/// invalid definitions.
pub fn register(spec: &DeclarativeAlgorithm) -> Result<(), String> {
    register_inner(spec, true)
}

/// Register without adding to the drain log — used when the host reloads
/// persisted algorithms at startup (they're already stored).
pub fn register_silent(spec: &DeclarativeAlgorithm) -> Result<(), String> {
    register_inner(spec, false)
}

fn register_inner(spec: &DeclarativeAlgorithm, log: bool) -> Result<(), String> {
    if spec.name.trim().is_empty() {
        return Err("algorithm name must not be empty".to_string());
    }
    // Prompt-based kinds need guidance text; scorers are configured via params.
    let prompt_based = matches!(spec.kind.as_str(), "loop" | "compaction" | "context");
    if prompt_based && spec.addendum.trim().is_empty() {
        return Err("algorithm addendum (prompt guidance) must not be empty".to_string());
    }
    match spec.kind.as_str() {
        "loop" => {
            let name = spec.name.clone();
            let addendum = spec.addendum.clone();
            let reflective = spec.reflective;
            let max_iterations = spec.max_iterations;
            contrib::register_loop_strategy(&spec.name, move || {
                Arc::new(PromptPatternStrategy::custom(
                    &name,
                    &addendum,
                    reflective,
                    max_iterations,
                ))
            });
        }
        "compaction" => {
            let name = spec.name.clone();
            let header = spec.addendum.clone();
            contrib::register_compaction_strategy(&spec.name, move || {
                Arc::new(TemplatedSummaryCompaction::new(
                    name.clone(),
                    Some(header.clone()),
                ))
            });
        }
        "context" => {
            let name = spec.name.clone();
            let header = spec.addendum.clone();
            let keywords = string_array(&spec.params, "keywords");
            contrib::register_context_manager(&spec.name, move || {
                Arc::new(DeclarativeContextManager::new(
                    name.clone(),
                    header.clone(),
                    keywords.clone(),
                ))
            });
        }
        "scorer" => {
            // Wrap a built-in base scorer with the authored rubric/criteria so
            // the eval pipeline can dispatch it by name.
            let base = spec
                .params
                .get("base")
                .and_then(|v| v.as_str())
                .unwrap_or("rubric_overlap")
                .to_string();
            let params = spec.params.clone();
            contrib::register_scorer(&spec.name, move |pred, refr, task_params| {
                // Prefer the authored rubric; fall back to per-task params.
                let p = if params.is_null() { task_params } else { &params };
                crate::scoring::score(&base, pred, refr, p)
            });
        }
        "coordinator" | "memory" => {
            // Accepted as an advanced catalog template; no runtime factory yet.
            // Persisted for catalog visibility and future wiring.
        }
        other => {
            return Err(format!(
                "unsupported declarative kind '{other}' (expected loop/compaction/context/scorer/coordinator/memory)"
            ))
        }
    }
    if log {
        pending().lock().unwrap().push(spec.clone());
    }
    Ok(())
}

/// Extract a string array field from a JSON config object.
fn string_array(params: &Value, key: &str) -> Vec<String> {
    params
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Drain algorithms registered (via [`register`]) since the last drain, so the
/// host application can persist them. Returns them in registration order.
pub fn drain_registered() -> Vec<DeclarativeAlgorithm> {
    std::mem::take(&mut *pending().lock().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_resolve_live() {
        let spec = DeclarativeAlgorithm {
            kind: "loop".into(),
            name: "unit_test_strategy".into(),
            description: "test".into(),
            addendum: "Be extremely concise.".into(),
            reflective: true,
            max_iterations: Some(3),
            params: Value::Null,
        };
        register(&spec).unwrap();
        let resolved = super::super::loop_strategy::resolve_loop_strategy("unit_test_strategy");
        assert!(resolved.is_some());
        let s = resolved.unwrap();
        assert!(s.reflective());
        assert_eq!(s.max_iterations_hint(), Some(3));
        let drained = drain_registered();
        assert!(drained.iter().any(|a| a.name == "unit_test_strategy"));
    }

    #[test]
    fn registers_compaction_context_scorer() {
        register(&DeclarativeAlgorithm {
            kind: "compaction".into(),
            name: "unit_compaction".into(),
            description: String::new(),
            addendum: "## Summary".into(),
            reflective: false,
            max_iterations: None,
            params: Value::Null,
        })
        .unwrap();
        assert!(contrib::resolve_compaction_strategy("unit_compaction").is_some());

        register(&DeclarativeAlgorithm {
            kind: "context".into(),
            name: "unit_context".into(),
            description: String::new(),
            addendum: "# Guidance".into(),
            reflective: false,
            max_iterations: None,
            params: serde_json::json!({ "keywords": ["alpha"] }),
        })
        .unwrap();
        assert!(contrib::resolve_context_manager("unit_context").is_some());

        register(&DeclarativeAlgorithm {
            kind: "scorer".into(),
            name: "unit_scorer".into(),
            description: String::new(),
            addendum: String::new(),
            reflective: false,
            max_iterations: None,
            params: serde_json::json!({ "base": "rubric_overlap", "keywords": ["paris"] }),
        })
        .unwrap();
        let s = contrib::score_with_contrib("unit_scorer", "the answer is paris", "", &Value::Null);
        assert_eq!(s, Some(1.0));
        let _ = drain_registered();
    }

    #[test]
    fn rejects_bad_specs() {
        assert!(register(&DeclarativeAlgorithm {
            kind: "loop".into(),
            name: "".into(),
            description: String::new(),
            addendum: "x".into(),
            reflective: false,
            max_iterations: None,
            params: Value::Null,
        })
        .is_err());
        assert!(register(&DeclarativeAlgorithm {
            kind: "nonsense".into(),
            name: "z".into(),
            description: String::new(),
            addendum: "x".into(),
            reflective: false,
            max_iterations: None,
            params: Value::Null,
        })
        .is_err());
    }
}
