//! Runtime registry for contributed (design-assistant / meta-agent authored)
//! strategies, registered by name at process start or live by the meta-agent.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use super::compaction::CompactionStrategy;
use super::loop_strategy::LoopStrategy;

type LoopFactory = Arc<dyn Fn() -> Arc<dyn LoopStrategy> + Send + Sync>;
type CompactionFactory = Arc<dyn Fn() -> Arc<dyn CompactionStrategy> + Send + Sync>;

struct Registry {
    loops: RwLock<HashMap<String, LoopFactory>>,
    compactions: RwLock<HashMap<String, CompactionFactory>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        loops: RwLock::new(HashMap::new()),
        compactions: RwLock::new(HashMap::new()),
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

/// Names of all contributed loop strategies (for catalog display).
pub fn contributed_loop_names() -> Vec<String> {
    registry().loops.read().unwrap().keys().cloned().collect()
}
