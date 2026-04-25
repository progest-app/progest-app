//! `progest lint` orchestrator.
//!
//! Composes the three violation sources (`core::rules`,
//! `core::accepts`, `core::sequence::drift`) into a single grouped
//! report that CLI / Tauri / future UIs can render without each layer
//! having to re-implement the walking loop. Pure over the inputs
//! (`CompiledRuleSet`, `AliasCatalog`, `CleanupConfig`) — the caller
//! is responsible for reading `.progest/rules.toml`, `schema.toml`,
//! and `project.toml` and passing the compiled artifacts in.
//!
//! File walking happens outside this module: callers hand over the
//! already-filtered list of paths (scanner output, stdin batch, etc.),
//! so sandbox / worktree / dry-run modes all share the same core pass.

pub mod index_writer;
pub mod orchestrator;
pub mod report;

pub use index_writer::write_to_index;
pub use orchestrator::{LintError, LintOptions, SEQUENCE_DRIFT_RULE_ID, lint_paths};
pub use report::{LintReport, LintSummary};
