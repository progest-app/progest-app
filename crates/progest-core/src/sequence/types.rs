//! Value types for sequence detection results.

use serde::Serialize;

use crate::fs::ProjectPath;

/// One detected numbered sequence.
///
/// Members are sorted by [`SequenceMember::index`] ascending. The
/// canonical basename of any member is
/// `{stem_prefix}{separator}{index:0padding}.{extension}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sequence {
    /// Project-root-relative directory the members live in. Empty
    /// (`ProjectPath::root()`) when members live at the project root.
    pub parent: ProjectPath,
    /// Everything before the trailing-digit run, minus the optional
    /// separator. Often empty (e.g. `0001.exr` → `""`).
    pub stem_prefix: String,
    /// `_`, `.`, `-`, or empty. Captured separately so renames can
    /// preserve the original stylistic choice.
    pub separator: String,
    /// Number of digits in each member's index (e.g. `4` for `0001`).
    /// Padding is part of the group key — `frame_001.exr` and
    /// `frame_1.exr` form distinct sequences.
    pub padding: usize,
    /// File extension without the leading dot (`exr`, `png`).
    pub extension: String,
    /// Members in ascending [`SequenceMember::index`] order. Always
    /// at least [`super::MIN_MEMBERS`] long.
    pub members: Vec<SequenceMember>,
}

/// One file inside a [`Sequence`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SequenceMember {
    pub path: ProjectPath,
    /// Numeric value of the trailing-digit capture. `frame_0042.exr`
    /// → `42`. Holds the parsed integer; render padding by combining
    /// with [`Sequence::padding`].
    pub index: u64,
}

/// Output of [`super::detect_sequences`].
///
/// Every input path appears in exactly one place: either inside a
/// [`Sequence`]'s `members`, or in `singletons`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct SequenceDetection {
    /// Detected sequences, each with at least [`super::MIN_MEMBERS`]
    /// members. Sorted by `(parent, stem_prefix)` for deterministic
    /// output across runs.
    pub sequences: Vec<Sequence>,
    /// Paths that did not join a sequence — either they didn't match
    /// the trailing-digits pattern at all, or the group they would
    /// have joined had fewer than [`super::MIN_MEMBERS`] members.
    /// Sorted lexicographically.
    pub singletons: Vec<ProjectPath>,
}
