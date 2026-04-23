//! Fill-mode resolution.
//!
//! A [`super::types::NameCandidate`] may contain holes (e.g. from
//! `remove_cjk`). Writing such a candidate to disk would either drop
//! meaningful content silently or produce a filename with `⟨cjk-1⟩`
//! in it — both unacceptable. [`FillMode`] is the contract that
//! callers opt into when they want a disk-ready string:
//!
//! - [`FillMode::Skip`] — refuse: the caller should either bail out
//!   of the rename or surface the candidate as advisory only. The
//!   non-TTY default for `progest clean --apply` (future), and the
//!   only mode that's safe for "populate `violation.suggested_names[]`"
//!   because unresolved candidates must not end up in user-facing
//!   rename suggestions.
//! - [`FillMode::Placeholder`] — substitute every hole with the
//!   configured replacement string. Used by `progest clean` to ship
//!   a filename even when the original had CJK content.
//! - [`FillMode::Prompt`] — reserved for the interactive `--apply`
//!   path landing alongside `core::rename`. Not implemented here
//!   because it needs a TTY-bound I/O surface this crate doesn't
//!   expose; the variant exists so downstream code can pattern-match
//!   exhaustively.

use thiserror::Error;

use super::types::{NameCandidate, Segment};

/// How to collapse holes in a [`NameCandidate`] into a disk string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillMode {
    /// Refuse to resolve a candidate that still has holes.
    Skip,
    /// Replace every hole with the given literal string.
    Placeholder(String),
    /// Ask the user for each hole. Unavailable in this crate; see the
    /// module comment.
    Prompt,
}

impl FillMode {
    /// Default for non-TTY invocations (CI, scripts): refuse.
    #[must_use]
    pub fn non_tty_default() -> Self {
        Self::Skip
    }

    /// Default when a user-facing `--placeholder` flag is passed
    /// without a value: substitute a literal `_` for each hole.
    #[must_use]
    pub fn placeholder_default() -> Self {
        Self::Placeholder("_".into())
    }
}

/// Outcome of [`resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillResolution {
    /// Disk-safe basename including extension.
    pub basename: String,
    /// Holes the caller had to fill (empty when the candidate was
    /// already resolved). Preserved for audit / history purposes.
    pub filled_holes: Vec<FilledHole>,
}

/// One hole that was collapsed into a substitute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilledHole {
    pub origin: String,
    pub substitute: String,
}

/// Errors surfaced by [`resolve`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UnresolvedHoleError {
    /// [`FillMode::Skip`] was chosen and the candidate still had holes.
    /// The vector carries the original hole bodies in order so the
    /// caller can tell the user what would have been dropped.
    #[error("candidate has {count} unresolved hole(s); refusing to write under FillMode::Skip")]
    HolesRemain { count: usize, origins: Vec<String> },
    /// [`FillMode::Prompt`] was chosen but the caller did not hook an
    /// interactive responder. See module doc.
    #[error(
        "FillMode::Prompt is not available in core::naming; resolve interactively at the CLI/UI layer"
    )]
    PromptUnavailable,
}

/// Collapse a candidate into a disk-ready basename under the chosen
/// fill mode.
///
/// # Errors
///
/// See [`UnresolvedHoleError`].
pub fn resolve(
    candidate: &NameCandidate,
    mode: &FillMode,
) -> Result<FillResolution, UnresolvedHoleError> {
    let mut out = String::new();
    let mut filled = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();

    for seg in &candidate.segments {
        match seg {
            Segment::Literal(s) => out.push_str(s),
            Segment::Hole(h) => match mode {
                FillMode::Skip => {
                    unresolved.push(h.origin.clone());
                }
                FillMode::Placeholder(subst) => {
                    out.push_str(subst);
                    filled.push(FilledHole {
                        origin: h.origin.clone(),
                        substitute: subst.clone(),
                    });
                }
                FillMode::Prompt => {
                    return Err(UnresolvedHoleError::PromptUnavailable);
                }
            },
        }
    }

    if !unresolved.is_empty() {
        return Err(UnresolvedHoleError::HolesRemain {
            count: unresolved.len(),
            origins: unresolved,
        });
    }

    if let Some(ext) = &candidate.ext {
        out.push('.');
        out.push_str(ext);
    }

    Ok(FillResolution {
        basename: out,
        filled_holes: filled,
    })
}

/// Convenience: resolve under [`FillMode::Skip`] and return `Some`
/// only when the candidate had no holes. Used by
/// `violation.suggested_names[]` population — if the pipeline couldn't
/// produce a clean name, we'd rather emit no suggestion than a
/// sentinel the user could accidentally accept.
#[must_use]
pub fn try_resolve_clean(candidate: &NameCandidate) -> Option<String> {
    if !candidate.is_resolved() {
        return None;
    }
    resolve(candidate, &FillMode::Skip).ok().map(|r| r.basename)
}

/// Count holes in the candidate. Cheap helper for callers that need
/// the count but not the origins.
#[must_use]
pub fn hole_count(candidate: &NameCandidate) -> usize {
    candidate
        .segments
        .iter()
        .filter(|s| matches!(s, Segment::Hole(_)))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::types::{Hole, HoleKind, Segment};

    fn cand_with_holes() -> NameCandidate {
        NameCandidate {
            segments: vec![
                Segment::Hole(Hole {
                    origin: "カット".into(),
                    kind: HoleKind::Cjk,
                    pos: 0,
                }),
                Segment::Literal("_v01".into()),
            ],
            ext: Some("png".into()),
        }
    }

    fn clean_cand() -> NameCandidate {
        NameCandidate {
            segments: vec![Segment::Literal("shot_v01".into())],
            ext: Some("png".into()),
        }
    }

    #[test]
    fn skip_refuses_candidates_with_holes() {
        let err = resolve(&cand_with_holes(), &FillMode::Skip).unwrap_err();
        match err {
            UnresolvedHoleError::HolesRemain { count, origins } => {
                assert_eq!(count, 1);
                assert_eq!(origins, vec!["カット".to_string()]);
            }
            UnresolvedHoleError::PromptUnavailable => panic!("wrong variant"),
        }
    }

    #[test]
    fn skip_passes_clean_candidates_through() {
        let r = resolve(&clean_cand(), &FillMode::Skip).unwrap();
        assert_eq!(r.basename, "shot_v01.png");
        assert!(r.filled_holes.is_empty());
    }

    #[test]
    fn placeholder_substitutes_each_hole() {
        let r = resolve(&cand_with_holes(), &FillMode::Placeholder("_".into())).unwrap();
        assert_eq!(r.basename, "__v01.png");
        assert_eq!(r.filled_holes.len(), 1);
        assert_eq!(r.filled_holes[0].origin, "カット");
        assert_eq!(r.filled_holes[0].substitute, "_");
    }

    #[test]
    fn placeholder_default_is_underscore() {
        assert!(matches!(
            FillMode::placeholder_default(),
            FillMode::Placeholder(ref s) if s == "_"
        ));
    }

    #[test]
    fn prompt_is_unavailable_in_core() {
        let err = resolve(&cand_with_holes(), &FillMode::Prompt).unwrap_err();
        assert!(matches!(err, UnresolvedHoleError::PromptUnavailable));
    }

    #[test]
    fn try_resolve_clean_only_accepts_candidates_without_holes() {
        assert_eq!(
            try_resolve_clean(&clean_cand()),
            Some("shot_v01.png".into())
        );
        assert_eq!(try_resolve_clean(&cand_with_holes()), None);
    }

    #[test]
    fn hole_count_matches_holes() {
        assert_eq!(hole_count(&cand_with_holes()), 1);
        assert_eq!(hole_count(&clean_cand()), 0);
    }
}
