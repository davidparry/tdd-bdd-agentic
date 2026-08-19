//! The outermost ring: concrete implementations of [`crate::ports`] that
//! touch the filesystem, the network, and configuration files. Only the
//! composition roots - `main.rs`, `mcp.rs`, and `greenfield.rs` - may
//! name these types (matching the layering described in `lib.rs`).

pub mod config;
pub mod console_prompt;
pub mod fs_project;
pub mod fs_scaffold;
pub mod fs_sources;
pub mod fs_spec;
pub mod fs_staging;
pub mod fs_state;
pub mod gherkin_features;
pub mod llm_cache;
pub mod ollama;
pub mod overlay;
pub mod process_exec;
pub mod process_runtime;
pub mod readline_prompt;
pub mod readline_shell;
pub mod runners;
pub mod spinner;
