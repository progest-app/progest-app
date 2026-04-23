//! File rename operations.
//!
//! `core::rename` turns naming-driven proposals into a previewable plan
//! ([`RenamePreview`]) and, in a follow-up commit, will execute that plan
//! as an atomic batch with rollback. This module lands the preview half:
//! types that downstream consumers (CLI, future Tauri layer) can
//! serialize and show to users without committing to disk mutation.
//!
//! Apply (shadow-copy + 2-phase rename + rollback), `history::Store`
//! wiring with `group_id`, and sequence-aware bulk rename land in
//! follow-up commits on this branch.

pub mod apply;
pub mod ops;
pub mod preview;
pub mod sequence;

pub use apply::{
    AppliedOp, ApplyError, ApplyOutcome, HistoryWarning, IndexWarning, Rename, STAGING_PREFIX,
    StageStep,
};
pub use ops::{Conflict, ConflictKind, RenameOp};
pub use preview::{
    PreviewError, RenamePreview, RenameRequest, build_preview, build_preview_with_prompter,
};
pub use sequence::requests_from_sequence;
