//! HarnessLab Desktop core.
//!
//! Two layers live here:
//!
//! * The **config → harness** parser (`config`, `loader`, …) that builds a
//!   runnable `harness_rt::agent::harness::HarnessConfig` from YAML — driven
//!   by the headless CLI.
//! * The **design-time model** (`project`, `registry`, `store`) that backs the
//!   desktop visual designer: the `Project`/`AgentSpec`/`Topology` model, the
//!   component catalog, and the (P1: in-memory) project + layout store. These
//!   mirror HarnessLab's web platform so the ported React frontend round-trips
//!   unchanged over Tauri IPC.

pub mod agent_library;
pub mod algorithms;
pub mod autoloop;
pub mod config;
pub mod context_managers;
pub mod contrib;
pub mod copilot;
pub mod embeddings;
pub mod eval;
pub mod factory;
pub mod hooks;
pub mod loader;
pub mod market;
pub mod ops;
pub mod plugins;
pub mod project;
pub mod registry;
pub mod runner;
pub mod skills;
pub mod stages;
pub mod store;
pub mod templates;

pub use config::*;
pub use loader::*;
