//! Workbench-side registration of *contributed* (design-assistant-authored)
//! kernel strategies.
//!
//! New strategies the design assistant develops are inserted into the GENERATED
//! regions below (between the `>>>` / `<<<` marker lines, edited programmatically
//! by `backend/core/component_builder.py`). [`register_all`] — called once at
//! process startup from `main` — installs them into the kernel's runtime
//! [`contrib`](piscis_kernel::agent::contrib) registry so a YAML config can
//! select them by name (the built-in resolvers fall through to that registry).
//!
//! Authoring contract:
//! - **Items region**: full Rust items (e.g. a `struct` + `impl LoopStrategy`
//!   or `impl CompactionStrategy`).
//! - **Registrations region**: one `contrib::register_*` call per component.
//!
//! Keep the marker lines exactly as-is; the builder locates them by string match.

#![allow(unused_imports)]

use std::sync::Arc;

use harness_rt::agent::contrib;
use harness_rt::agent::loop_strategy::{LoopStrategy, PromptPatternStrategy};

/// Install every contributed strategy into the kernel contrib registry.
///
/// Idempotent (registration is last-writer-wins), so calling it more than once
/// is harmless.
pub fn register_all() {
    register_generated_items();
    register_generated();
}

/// Bring generated item modules into scope. The builder inserts `mod`/`use`
/// lines or inline items between the markers.
fn register_generated_items() {
    // >>> GENERATED ITEMS (do not edit this marker line)
    // (design-assistant-authored Rust items are inserted above this line)
    // <<< GENERATED ITEMS
}

/// Register generated strategies by name. The builder inserts one
/// `contrib::register_loop_strategy(...)` / `register_compaction_strategy(...)`
/// call per component between the markers.
fn register_generated() {
    // A worked example proving the path: a named loop strategy selectable as
    // `strategy.type: tree_of_thought` in any workbench YAML config.
    contrib::register_loop_strategy("tree_of_thought", || {
        Arc::new(PromptPatternStrategy::new("tree_of_thought"))
    });

    // >>> GENERATED REGISTRATIONS (do not edit this marker line)
    // (design-assistant-authored registrations are inserted above this line)
    // <<< GENERATED REGISTRATIONS
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_rt::agent::loop_strategy::resolve_loop_strategy;

    #[test]
    fn register_all_installs_contributed_strategies() {
        register_all();
        // The built-in resolver now falls through to the contrib registry.
        let resolved = resolve_loop_strategy("tree_of_thought");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name(), "tree_of_thought");
    }
}
