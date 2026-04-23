//! Numbered file-sequence detection.
//!
//! Pipelines (VFX, animation, render farms) commonly emit groups of
//! files that share a stem and differ only in a trailing zero-padded
//! frame number — `frame_0001.exr`, `frame_0002.exr`, ….
//! Treating such a group as a single unit lets `progest rename`
//! offer one preview row instead of N and lets bulk operations
//! (renumber, prefix-replace) preserve the per-file index.
//!
//! This module is intentionally side-effect free: it consumes a slice
//! of [`ProjectPath`]s already harvested by the caller (typically the
//! scanner) and returns a [`SequenceDetection`] partitioning every
//! input into either a [`Sequence`] (≥ [`MIN_MEMBERS`]) or a
//! singleton. Apply-side wiring lives in `core::rename`.
//!
//! # Detection rule
//!
//! A basename matches the regex
//! `^(.*?)([._-]?)(\d+)\.([^.]+)$`. Two basenames belong to the same
//! group iff they share **all** of:
//!
//! 1. the parent directory ([`ProjectPath::parent`]),
//! 2. the stem prefix (capture 1),
//! 3. the separator before the digits (capture 2 — `_` / `.` / `-`,
//!    or empty),
//! 4. the **padding width** (digit count — `0001` and `1` form
//!    different groups so a partially-renumbered batch surfaces as
//!    two sequences rather than collapsing into one),
//! 5. the extension (capture 4 — single component only; compound
//!    extensions like `.exr.gz` are out of scope for v1).
//!
//! Gaps are allowed: `frame_001`, `frame_002`, `frame_005` form one
//! sequence of three members because retake-driven gaps are normal in
//! VFX pipelines.

pub mod detect;
pub mod types;

pub use detect::{MIN_MEMBERS, detect_sequences};
pub use types::{Sequence, SequenceDetection, SequenceMember};
