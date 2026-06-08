//! Runtime registry for contributed (design-assistant / meta-agent authored)
//! strategies, registered by name at process start or live by the meta-agent.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;

use super::compaction::CompactionStrategy;
use super::loop_strategy::LoopStrategy;
use crate::context::ContextManager;

type LoopFactory = Arc<dyn Fn() -> Arc<dyn LoopStrategy> + Send + Sync>;
type CompactionFactory = Arc<dyn Fn() -> Arc<dyn CompactionStrategy> + Send + Sync>;
type ContextFactory = Arc<dyn Fn() -> Arc<dyn ContextManager> + Send + Sync>;
/// A scorer is `(prediction, reference, params) -> score in [0,1]`.
type ScorerFn = Arc<dyn Fn(&str, &str, &Value) -> f64 + Send + Sync>;

struct Registry {
    loops: RwLock<HashMap<String, LoopFactory>>,
    compactions: RwLock<HashMap<String, CompactionFactory>>,
    contexts: RwLock<HashMap<String, ContextFactory>>,
    scorers: RwLock<HashMap<String, ScorerFn>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        loops: RwLock::new(HashMap::new()),
        compactions: RwLock::new(HashMap::new()),
        contexts: RwLock::new(HashMap::new()),
        scorers: RwLock::new(HashMap::new()),
    })
}

/// Register a loop strategy factory under `name` (last writer wins).
pub fn register_loop_strategy<F>(name: &str, factory: F)
where
    F: Fn() -> Arc<dyn LoopStrategy> + Send + Sync + 'static,
{
    registry()
        .loops
        .write()
        .unwrap()
        .insert(name.to_string(), Arc::new(factory));
}

/// Register a compaction strategy factory under `name` (last writer wins).
pub fn register_compaction_strategy<F>(name: &str, factory: F)
where
    F: Fn() -> Arc<dyn CompactionStrategy> + Send + Sync + 'static,
{
    registry()
        .compactions
        .write()
        .unwrap()
        .insert(name.to_string(), Arc::new(factory));
}

/// Resolve a contributed loop strategy by name.
pub fn resolve_loop_strategy(name: &str) -> Option<Arc<dyn LoopStrategy>> {
    registry().loops.read().unwrap().get(name).map(|f| f())
}

/// Resolve a contributed compaction strategy by name.
pub fn resolve_compaction_strategy(name: &str) -> Option<Arc<dyn CompactionStrategy>> {
    registry()
        .compactions
        .read()
        .unwrap()
        .get(name)
        .map(|f| f())
}

/// Register a context-manager factory under `name` (last writer wins).
pub fn register_context_manager<F>(name: &str, factory: F)
where
    F: Fn() -> Arc<dyn ContextManager> + Send + Sync + 'static,
{
    registry()
        .contexts
        .write()
        .unwrap()
        .insert(name.to_string(), Arc::new(factory));
}

/// Resolve a contributed context manager by name.
pub fn resolve_context_manager(name: &str) -> Option<Arc<dyn ContextManager>> {
    registry().contexts.read().unwrap().get(name).map(|f| f())
}

/// Register a scorer function under `name` (last writer wins).
pub fn register_scorer<F>(name: &str, scorer: F)
where
    F: Fn(&str, &str, &Value) -> f64 + Send + Sync + 'static,
{
    registry()
        .scorers
        .write()
        .unwrap()
        .insert(name.to_string(), Arc::new(scorer));
}

/// Resolve a contributed scorer by name and run it.
pub fn score_with_contrib(
    name: &str,
    prediction: &str,
    reference: &str,
    params: &Value,
) -> Option<f64> {
    let f = registry().scorers.read().unwrap().get(name).cloned()?;
    Some(f(prediction, reference, params))
}

/// Whether a contributed scorer is registered under `name`.
pub fn has_scorer(name: &str) -> bool {
    registry().scorers.read().unwrap().contains_key(name)
}

/// Names of all contributed loop strategies (for catalog display).
pub fn contributed_loop_names() -> Vec<String> {
    registry().loops.read().unwrap().keys().cloned().collect()
}
