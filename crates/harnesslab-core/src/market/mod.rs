//! Decentralized community market — package install, inspect, signatures.

pub mod client;
pub mod manifest;
pub mod registry;
pub mod signing;
pub mod sources;

pub use client::{CatalogEntry, InspectResult, InstallSummary, InstalledPackage, MarketStore};
pub use registry::RegistriesConfig;
