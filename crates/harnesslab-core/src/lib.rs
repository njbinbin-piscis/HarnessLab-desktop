//! HarnessLab Desktop core.
//!
//! Two layers live here:
//!
//! * The **config → harness** parser (`config`, `loader`, …) that builds a
//!   runnable `piscis_kernel::agent::harness::HarnessConfig` from YAML — driven
//!   by the headless CLI.
//! * The **design-time model** (`project`, `registry`, `store`) that backs the
//!   desktop visual designer: the `Project`/`AgentSpec`/`Topology` model, the
//!   component catalog, and the (P1: in-memory) project + layout store. These
//!   mirror HarnessLab's web platform so the ported React frontend round-trips
//!   unchanged over Tauri IPC.

pub mod config;
pub mod context_managers;
pub mod contrib;
pub mod hooks;
pub mod loader;
pub mod plugins;
pub mod project;
pub mod registry;
pub mod store;

pub use config::*;
pub use loader::*;
