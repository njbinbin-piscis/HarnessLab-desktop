//! Agent Workbench configuration parser.
//!
//! This module defines YAML-mappable structures and provides a parser
//! that builds `piscis_kernel::agent::harness::HarnessConfig` from
//! configuration files.

pub mod config;
pub mod context_managers;
pub mod contrib;
pub mod hooks;
pub mod loader;
pub mod plugins;

pub use config::*;
pub use loader::*;
