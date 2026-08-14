//! Spec-driven BDD/TDD CLI core.
//!
//! Clean architecture, dependency rule inward:
//!
//! - [`domain`] — pure business logic: the requirement model, spec
//!   validation, wording refinement, and the Red/Green/Refactor state
//!   machine. No IO, no framework types. Ported faithfully from the Java
//!   `tdd-workflow-server` so replies match byte for byte.
//! - [`ports`] — the trait boundary the domain and application layers
//!   depend on (spec storage, feature-file queries, LLM model catalog and
//!   persistence). Inversion of control: outer layers implement these.
//! - [`application`] — use-case services composed from domain logic and
//!   ports via constructor injection.
//! - [`adapters`] — the outermost ring: filesystem, Ollama HTTP, and TOML
//!   configuration implementations of the ports.
//!
//! The binary's `main.rs`, the MCP delivery in [`mcp`], and the
//! greenfield orchestrator in [`greenfield`] are the composition roots:
//! the only places that name concrete adapter types.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod greenfield;
pub mod mcp;
pub mod ports;
pub mod repl;
pub mod workspace;

#[cfg(test)]
pub(crate) mod test_support;
