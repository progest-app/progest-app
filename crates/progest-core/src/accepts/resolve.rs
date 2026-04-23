//! Resolve per-dir `effective_accepts` sets per REQUIREMENTS.md §3.13.2.
//!
//! Inheritance semantics (opt-in, spec §3.13.2):
//!
//! ```text
//! effective_accepts(dir) =
//!     own_accepts(dir) ∪ (dir.inherit ? effective_accepts(parent) : ∅)
//! ```
//!
//! The ancestor's own `inherit` flag doesn't enter the equation —
//! only the child decides whether to walk the chain. We keep that
//! invariant explicit in the resolver by computing each ancestor's
//! **own** set once and unioning in upward iff the child asked for it.
//!
//! The resolver also tracks provenance (own vs inherited) for each
//! extension so the placement lint can populate
//! [`crate::rules::AcceptsSource`] on its violation payload without a
//! second pass.

use std::collections::BTreeMap;

use thiserror::Error;

use super::schema::AliasCatalog;
use super::types::{AcceptsToken, Ext, RawAccepts};
use crate::rules::{AcceptsSource, Mode};

/// One directory's expanded accepts information, ready for placement
/// lint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveAccepts {
    /// Every accepted extension, with provenance. Keys are normalized
    /// extensions (lowercase, no leading dot); `""` is the
    /// no-extension sentinel. When the same ext appears both on the
    /// dir itself and via inheritance, `Own` wins.
    pub exts: BTreeMap<Ext, AcceptsSource>,
    /// Severity for placement lint (from the dir's own `[accepts]`,
    /// or [`Mode::Warn`] if the dir has no declaration — matching
    /// REQUIREMENTS.md §3.13.6 default).
    pub mode: Mode,
}

impl EffectiveAccepts {
    /// Whether the given extension is in the accepted set.
    #[must_use]
    pub fn accepts(&self, ext: &Ext) -> bool {
        self.exts.contains_key(ext)
    }

    /// How the ext was accepted — own-set or inherited. `None` if
    /// the ext is not in the set at all.
    #[must_use]
    pub fn source_of(&self, ext: &Ext) -> Option<AcceptsSource> {
        self.exts.get(ext).copied()
    }

    /// Flat list of accepted extensions, lexicographically sorted,
    /// used by the placement lint to populate
    /// [`crate::rules::PlacementDetails::expected_exts`].
    #[must_use]
    pub fn expected_exts(&self) -> Vec<String> {
        self.exts.keys().map(|e| e.as_str().to_owned()).collect()
    }
}

/// Fatal error while computing effective accepts.
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("`accepts.exts` references unknown alias `:{0}`")]
    UnknownAlias(String),
}

/// Expand one dir's `[accepts]` tokens into its **own** accepted set,
/// resolving alias references against the catalog.
///
/// # Errors
///
/// Returns [`ResolveError::UnknownAlias`] when a token references an
/// alias that isn't in the catalog — this mirrors `[accepts].exts =
/// [":typo"]` and stops placement lint from silently rejecting every
/// file in the dir because of a typo.
pub fn expand_own_accepts(
    raw: &RawAccepts,
    catalog: &AliasCatalog,
) -> Result<BTreeMap<Ext, AcceptsSource>, ResolveError> {
    let mut out: BTreeMap<Ext, AcceptsSource> = BTreeMap::new();
    for token in &raw.exts {
        match token {
            AcceptsToken::Ext(e) => {
                out.insert(e.clone(), AcceptsSource::Own);
            }
            AcceptsToken::Alias(name) => {
                let Some(exts) = catalog.lookup(name) else {
                    return Err(ResolveError::UnknownAlias(name.clone()));
                };
                for e in exts {
                    out.entry(e.clone()).or_insert(AcceptsSource::Own);
                }
            }
        }
    }
    Ok(out)
}

/// Compose `effective_accepts(dir)` from the dir's own declaration
/// and the chain of ancestor declarations.
///
/// `chain` is ordered **parent first, root last**: the topmost entry
/// is the dir's direct parent, each subsequent entry is one step
/// further up the tree. The dir's own declaration is passed
/// separately in `own` (or `None` if the dir has no `[accepts]`).
///
/// Inheritance per REQUIREMENTS.md §3.13.2: we only walk the chain
/// when `own` exists and `own.inherit == true`. The ancestor's own
/// `inherit` flag is ignored; we keep iterating up as long as each
/// ancestor's own set is kept available. This is the strict reading
/// of the spec — a transitive chain of `inherit = true` isn't the
/// same as "inherit pulls in grandparent automatically".
///
/// # Errors
///
/// Propagates [`ResolveError`] from [`expand_own_accepts`].
pub fn compute_effective_accepts(
    own: Option<&RawAccepts>,
    chain: &[&RawAccepts],
    catalog: &AliasCatalog,
) -> Result<Option<EffectiveAccepts>, ResolveError> {
    // Spec §3.13.1: an absent [accepts] means "no placement
    // constraint", not "empty accept set". The resolver returns None
    // so the evaluator can short-circuit without pretending the dir
    // rejects everything.
    let Some(own) = own else {
        return Ok(None);
    };

    let mut exts = expand_own_accepts(own, catalog)?;

    if own.inherit {
        // Walk the chain, collecting each ancestor's own set. Mark
        // newly-seen exts as Inherited. Existing Own entries keep
        // their provenance (union, not override).
        let mut cursor: Option<&RawAccepts> = chain.first().copied();
        let mut idx = 1;
        while let Some(ancestor) = cursor {
            let ancestor_own = expand_own_accepts(ancestor, catalog)?;
            for (ext, _) in ancestor_own {
                exts.entry(ext).or_insert(AcceptsSource::Inherited);
            }
            // Spec: ancestors' own `inherit` flag does not propagate.
            // But we still keep walking because the *child* asked for
            // the recursive union. So we just advance through the
            // chain regardless.
            cursor = chain.get(idx).copied();
            idx += 1;
        }
    }

    Ok(Some(EffectiveAccepts {
        exts,
        mode: own.mode,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accepts::types::normalize_ext;

    fn raw(inherit: bool, tokens: Vec<AcceptsToken>) -> RawAccepts {
        RawAccepts {
            inherit,
            exts: tokens,
            mode: Mode::Warn,
        }
    }

    fn ext(s: &str) -> AcceptsToken {
        AcceptsToken::Ext(normalize_ext(s))
    }

    fn alias(name: &str) -> AcceptsToken {
        AcceptsToken::Alias(name.into())
    }

    // --- Own expansion -------------------------------------------------------

    #[test]
    fn expand_own_merges_alias_and_literal() {
        let catalog = AliasCatalog::builtin();
        let r = raw(false, vec![alias("image"), ext(".psd")]);
        let got = expand_own_accepts(&r, &catalog).unwrap();
        assert!(got.contains_key(&normalize_ext("jpg")));
        assert!(got.contains_key(&normalize_ext("psd")));
        for source in got.values() {
            assert_eq!(*source, AcceptsSource::Own);
        }
    }

    #[test]
    fn unknown_alias_is_resolve_error() {
        let catalog = AliasCatalog::builtin();
        let r = raw(false, vec![alias("nope")]);
        assert!(matches!(
            expand_own_accepts(&r, &catalog).unwrap_err(),
            ResolveError::UnknownAlias(ref name) if name == "nope"
        ));
    }

    // --- effective_accepts ---------------------------------------------------

    #[test]
    fn no_own_section_returns_none() {
        let catalog = AliasCatalog::builtin();
        assert!(
            compute_effective_accepts(None, &[], &catalog)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn inherit_false_ignores_parent() {
        let catalog = AliasCatalog::builtin();
        let parent = raw(false, vec![ext(".aep")]);
        let child = raw(false, vec![ext(".psd")]);
        let got = compute_effective_accepts(Some(&child), &[&parent], &catalog)
            .unwrap()
            .unwrap();
        assert!(got.accepts(&normalize_ext("psd")));
        assert!(
            !got.accepts(&normalize_ext("aep")),
            "inherit=false must not pull in parent"
        );
    }

    #[test]
    fn inherit_true_unions_with_parent() {
        let catalog = AliasCatalog::builtin();
        let parent = raw(false, vec![ext(".aep")]);
        let child = raw(true, vec![ext(".psd")]);
        let got = compute_effective_accepts(Some(&child), &[&parent], &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            got.source_of(&normalize_ext("psd")),
            Some(AcceptsSource::Own)
        );
        assert_eq!(
            got.source_of(&normalize_ext("aep")),
            Some(AcceptsSource::Inherited)
        );
    }

    #[test]
    fn own_wins_over_inherited_for_same_ext() {
        let catalog = AliasCatalog::builtin();
        let parent = raw(false, vec![ext(".psd")]);
        let child = raw(true, vec![ext(".psd")]);
        let got = compute_effective_accepts(Some(&child), &[&parent], &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            got.source_of(&normalize_ext("psd")),
            Some(AcceptsSource::Own),
            "own declaration must shadow inherited entry"
        );
    }

    #[test]
    fn ancestor_inherit_flag_does_not_propagate() {
        // Per spec: ancestor's own `inherit` flag does not affect
        // the child's chain walk. The child explicitly asking for
        // inherit=true is enough to pull grandparent into the union.
        let catalog = AliasCatalog::builtin();
        let grandparent = raw(false, vec![ext(".ai")]);
        let parent = raw(false, vec![ext(".aep")]); // inherit=false on parent
        let child = raw(true, vec![ext(".psd")]);
        let got = compute_effective_accepts(Some(&child), &[&parent, &grandparent], &catalog)
            .unwrap()
            .unwrap();
        assert!(got.accepts(&normalize_ext("aep")), "parent merged in");
        assert!(
            got.accepts(&normalize_ext("ai")),
            "grandparent merged in — child's inherit flag drives the walk"
        );
    }

    #[test]
    fn expected_exts_is_sorted_and_normalized() {
        let catalog = AliasCatalog::builtin();
        let r = raw(false, vec![ext(".PSD"), ext(".tif"), ext(".jpg")]);
        let got = compute_effective_accepts(Some(&r), &[], &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(got.expected_exts(), vec!["jpg", "psd", "tif"]);
    }

    #[test]
    fn mode_is_copied_from_own_declaration() {
        let catalog = AliasCatalog::builtin();
        let r = RawAccepts {
            inherit: false,
            exts: vec![ext(".psd")],
            mode: Mode::Strict,
        };
        let got = compute_effective_accepts(Some(&r), &[], &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(got.mode, Mode::Strict);
    }
}
