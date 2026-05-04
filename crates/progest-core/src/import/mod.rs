//! File import engine (`core::import`).
//!
//! Implements REQUIREMENTS.md §3.14 (import / D&D) + the M4 design in
//! `docs/M4_HANDOFF.md §3`.
//!
//! 1. [`types`] — wire types (`ImportRequest`, `ImportOp`,
//!    `ImportConflict`, `ImportPreview`, `ImportMode`).
//! 2. [`ranking`] — destination ranking: score project directories by
//!    how well their `[accepts]` matches the imported file's extension.
//! 3. [`preview`] — build an [`ImportPreview`] from requests, detecting
//!    conflicts (dest exists, source missing, source is project,
//!    duplicate dests).
//! 4. [`apply`] — atomic commit: copy/move → staging → final dest →
//!    `.meta` creation → index registration → history append.

pub mod apply;
pub mod preview;
pub mod ranking;
pub mod types;

pub use apply::{Import, ImportApplyError, ImportOutcome, ImportWarning, ImportedFile};
pub use preview::build_preview;
pub use ranking::{
    SuggestedDestination, merge_rankings, rank_by_frequency, rank_destinations, score_dir,
};
pub use types::{ImportConflict, ImportMode, ImportOp, ImportPreview, ImportRequest};
