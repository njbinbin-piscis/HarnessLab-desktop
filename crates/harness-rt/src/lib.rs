//! Native agent runtime for HarnessLab Desktop.
//!
//! This crate is a self-contained reimplementation of the agent execution
//! surface the workbench previously consumed from the external `piscis-kernel`
//! crate. It owns:
//!
//! * [`llm`] — an `LlmClient` trait plus OpenAI-compatible and Claude clients
//!   (built on `reqwest`), and `build_client_with_timeout` provider routing.
//! * [`agent`] — the [`AgentLoop`](agent::loop_::AgentLoop) ReAct tool-calling
//!   loop, the [`HarnessConfig`](agent::harness::HarnessConfig) builder, the
//!   [`AgentEvent`](agent::messages::AgentEvent) stream, loop strategies,
//!   compaction strategies, lifecycle hooks and the tool execution context.
//! * [`tools`] — the neutral tool set (file/shell/search/web) and registry.
//! * [`memory`] — pluggable long-term memory backends.
//! * [`context`] — project context managers.
//! * [`policy`] — a workspace policy gate.
//!
//! The public module layout intentionally mirrors the old `piscis_kernel::*`
//! paths so the workbench's config/mapping layer only needs an import rename.

pub mod agent;
pub mod context;
pub mod llm;
pub mod memory;
pub mod policy;
pub mod scoring;
pub mod tools;
