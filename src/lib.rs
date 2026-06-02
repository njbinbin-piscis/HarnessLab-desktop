//! Agent Workbench configuration parser.
//!
//! This module defines YAML-mappable structures and provides a parser
//! that builds `pisci_kernel::agent::harness::HarnessConfig` from
//! configuration files.

pub mod config;
pub mod loader;
pub mod plugins;
pub mod context_managers;
pub mod hooks;

pub use config::*;
pub use loader::*;
