//! Progest core: domain logic for metadata, naming rules, indexing, and search.
//!
//! This crate is the authoritative home for all business logic. UI, CLI, and
//! IPC layers are thin wrappers over this library. See the project
//! `docs/IMPLEMENTATION_PLAN.md` for the full module layout.
//!
//! Subsequent milestones will populate `rules`, `search`, `thumbnail`,
//! `template`, `ai`, `history`, `rename`, and `doctor` modules alongside
//! the existing [`fs`], [`identity`], [`meta`], [`index`], [`reconcile`],
//! and [`watch`] modules.

pub mod accepts;
pub mod fs;
pub mod history;
pub mod identity;
pub mod index;
pub mod lint;
pub mod meta;
pub mod naming;
pub mod project;
pub mod reconcile;
pub mod rename;
pub mod rules;
pub mod search;
pub mod sequence;
pub mod watch;

/// The crate version, synced with the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
