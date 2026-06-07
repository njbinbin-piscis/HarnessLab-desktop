//! Persistence bridge for meta-agent hot-registered declarative algorithms.
//!
//! The native runtime ([`harness_rt::agent::declarative`]) holds algorithms in a
//! process-global registry that is lost on restart. This module ties that
//! registry to the SQLite store: persisted algorithms are re-registered at
//! startup, and algorithms the Architect registers live during a run are drained
//! and persisted so they survive restarts and appear in the catalog.

use harness_rt::agent::declarative::{self, DeclarativeAlgorithm};

use crate::store::Store;

/// Re-register every persisted algorithm into the running engine. Called once at
/// startup so previously-authored algorithms are immediately selectable.
pub fn register_persisted(store: &Store) {
    for algo in store.list_algorithms() {
        let _ = declarative::register_silent(&algo);
    }
}

/// Persist any algorithms registered live (e.g. by the Architect) since the last
/// drain. Returns the number persisted. Call after agent/project runs.
pub fn persist_drained(store: &Store) -> usize {
    let drained = declarative::drain_registered();
    let n = drained.len();
    for algo in &drained {
        let _ = store.put_algorithm(algo);
    }
    n
}

/// Catalog extras `(kind, name, description)` for persisted algorithms so they
/// appear in the component library alongside built-ins and market packages.
pub fn catalog_extras(store: &Store) -> Vec<(String, String, String)> {
    store
        .list_algorithms()
        .into_iter()
        .map(|a: DeclarativeAlgorithm| {
            let desc = if a.description.is_empty() {
                a.addendum.clone()
            } else {
                a.description.clone()
            };
            (a.kind, a.name, desc)
        })
        .collect()
}
