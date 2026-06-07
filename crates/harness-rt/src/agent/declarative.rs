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

use super::contrib;
use super::loop_strategy::PromptPatternStrategy;

/// A data-only algorithm definition that can be registered at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeAlgorithm {
    /// Component kind. Currently only `"loop"` is supported.
    pub kind: String,
    /// Unique selectable name (e.g. used as `strategy.type`).
    pub name: String,
    /// Human-readable description for the catalog.
    #[serde(default)]
    pub description: String,
    /// System-prompt guidance injected for this strategy.
    pub addendum: String,
    /// When true, the loop runs a self-reflection pass before finishing.
    #[serde(default)]
    pub reflective: bool,
    /// Optional hard cap on loop iterations.
    #[serde(default)]
    pub max_iterations: Option<usize>,
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
    if spec.addendum.trim().is_empty() {
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
        other => {
            return Err(format!(
                "unsupported declarative kind '{other}' (only 'loop')"
            ))
        }
    }
    if log {
        pending().lock().unwrap().push(spec.clone());
    }
    Ok(())
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
    fn rejects_bad_specs() {
        assert!(register(&DeclarativeAlgorithm {
            kind: "loop".into(),
            name: "".into(),
            description: String::new(),
            addendum: "x".into(),
            reflective: false,
            max_iterations: None,
        })
        .is_err());
        assert!(register(&DeclarativeAlgorithm {
            kind: "scorer".into(),
            name: "z".into(),
            description: String::new(),
            addendum: "x".into(),
            reflective: false,
            max_iterations: None,
        })
        .is_err());
    }
}
