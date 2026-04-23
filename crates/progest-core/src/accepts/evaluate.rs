//! Placement lint: compare a file's extension against its direct
//! parent dir's `effective_accepts` and emit a
//! [`crate::rules::Violation`] with `category = Placement` when the
//! file lands in a dir that does not accept it.
//!
//! Section references target REQUIREMENTS.md §3.13.6.
//!
//! The ranking for `suggested_destinations` (own-match → inherited →
//! MRU → shallow path) needs a project-wide dir index; that work
//! lives in the import-ranking follow-up PR. This module populates
//! the violation shape and leaves `suggested_destinations` empty.

use super::resolve::EffectiveAccepts;
use super::types::Ext;
use crate::fs::ProjectPath;
use crate::identity::FileId;
use crate::rules::{Category, PlacementDetails, RuleId, RuleIdError, RuleKind, Violation};

/// Rule id used for every placement violation. Kept stable so lint
/// UIs and saved-search filters can key off it.
///
/// # Panics
///
/// Never in practice — the literal is a valid `RuleId`. The function
/// is `unwrap`-free at runtime because the parse is tested below.
#[must_use]
pub fn placement_rule_id() -> RuleId {
    placement_rule_id_impl().expect("`placement` must parse as a valid RuleId")
}

fn placement_rule_id_impl() -> Result<RuleId, RuleIdError> {
    "placement".parse()
}

/// Run placement lint for a single file.
///
/// `parent_effective` is the `EffectiveAccepts` for the file's
/// **direct parent** dir — the caller walks the `.dirmeta` chain.
/// Passing `None` (no `[accepts]` on the parent) means REQUIREMENTS.md
/// §3.13.1 "no constraint" — no violation is emitted.
///
/// `compound_exts` is the longest-match compound extension catalog
/// (builtin + project `[extension_compounds]`); reuse
/// [`BUILTIN_COMPOUND_EXTS`] when the project has no customization.
#[must_use]
pub fn evaluate_placement_for_file(
    path: &ProjectPath,
    file_id: Option<FileId>,
    parent_effective: Option<&EffectiveAccepts>,
    compound_exts: &[&str],
) -> Option<Violation> {
    let effective = parent_effective?;

    let basename = path.file_name()?;
    let ext = super::types::normalize_ext_from_basename(basename, compound_exts);

    if effective.accepts(&ext) {
        return None;
    }

    let severity = effective.mode.violation_severity()?;
    let source = effective
        .source_of_any()
        .unwrap_or(crate::rules::AcceptsSource::Own);

    Some(Violation {
        file_id,
        path: path.clone(),
        rule_id: placement_rule_id(),
        category: Category::Placement,
        kind: RuleKind::Constraint,
        severity,
        reason: format!(
            "extension `{}` is not accepted by the parent directory (expected one of: {})",
            display_ext(&ext),
            display_expected(&effective.expected_exts()),
        ),
        trace: Vec::new(),
        suggested_names: Vec::new(),
        placement_details: Some(PlacementDetails {
            expected_exts: effective.expected_exts(),
            winning_rule_source: source,
            suggested_destinations: Vec::new(),
        }),
    })
}

fn display_ext(ext: &Ext) -> String {
    if ext.is_none() {
        "<no extension>".to_owned()
    } else {
        format!(".{ext}")
    }
}

fn display_expected(exts: &[String]) -> String {
    if exts.is_empty() {
        return "<empty set>".to_owned();
    }
    exts.iter()
        .map(|e| {
            if e.is_empty() {
                "<no extension>".to_owned()
            } else {
                format!(".{e}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// Convenience helper on `EffectiveAccepts` for the violation
// payload: the REQUIREMENTS.md §3.13.6 winning_rule_source refers to
// "how this dir's accept decision was reached" at the dir level, not
// per-ext. Modeling it as "own if the dir has *any* own exts, else
// inherited" matches the spec's intent — inherited means "this dir
// only accepts thanks to the inherit chain".
impl EffectiveAccepts {
    /// Whether the dir contributes any own-set entry. `Own` when the
    /// dir declared at least one ext itself; `Inherited` when every
    /// accepted ext came from the chain (dir is a pure pass-through).
    #[must_use]
    pub fn source_of_any(&self) -> Option<crate::rules::AcceptsSource> {
        use crate::rules::AcceptsSource;
        if self.exts.is_empty() {
            return None;
        }
        if self.exts.values().any(|s| *s == AcceptsSource::Own) {
            Some(AcceptsSource::Own)
        } else {
            Some(AcceptsSource::Inherited)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accepts::RawAccepts;
    use crate::accepts::resolve::compute_effective_accepts;
    use crate::accepts::schema::AliasCatalog;
    use crate::accepts::types::{AcceptsToken, normalize_ext};
    use crate::rules::{AcceptsSource, BUILTIN_COMPOUND_EXTS, Mode, Severity};

    fn accepts_with(exts: Vec<AcceptsToken>, mode: Mode) -> RawAccepts {
        RawAccepts {
            inherit: false,
            exts,
            mode,
        }
    }

    fn effective_from(own: &RawAccepts) -> EffectiveAccepts {
        compute_effective_accepts(Some(own), &[], &AliasCatalog::builtin())
            .unwrap()
            .unwrap()
    }

    fn path(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    #[test]
    fn placement_rule_id_is_valid() {
        // If this ever breaks, the RuleId grammar diverged from the
        // reserved "placement" token and every placement violation
        // starts panicking.
        placement_rule_id_impl().unwrap();
    }

    #[test]
    fn no_parent_accepts_means_no_violation() {
        let p = path("assets/foo.psd");
        let v = evaluate_placement_for_file(&p, None, None, BUILTIN_COMPOUND_EXTS);
        assert!(v.is_none());
    }

    #[test]
    fn ext_in_accepts_set_passes() {
        let own = accepts_with(vec![AcceptsToken::Ext(normalize_ext(".psd"))], Mode::Warn);
        let eff = effective_from(&own);
        let p = path("assets/foo.psd");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS);
        assert!(v.is_none());
    }

    #[test]
    fn ext_missing_from_accepts_set_emits_warn_violation() {
        let own = accepts_with(vec![AcceptsToken::Ext(normalize_ext(".psd"))], Mode::Warn);
        let eff = effective_from(&own);
        let p = path("assets/bad.mp4");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS).unwrap();
        assert_eq!(v.category, Category::Placement);
        assert_eq!(v.severity, Severity::Warn);
        let details = v.placement_details.unwrap();
        assert_eq!(details.expected_exts, vec!["psd"]);
        assert_eq!(details.winning_rule_source, AcceptsSource::Own);
        assert!(details.suggested_destinations.is_empty());
    }

    #[test]
    fn mode_off_suppresses_violation_even_on_miss() {
        let own = accepts_with(vec![AcceptsToken::Ext(normalize_ext(".psd"))], Mode::Off);
        let eff = effective_from(&own);
        let p = path("assets/bad.mp4");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS);
        assert!(v.is_none(), "mode=off must never emit");
    }

    #[test]
    fn mode_strict_bubbles_to_strict_severity() {
        let own = accepts_with(vec![AcceptsToken::Ext(normalize_ext(".psd"))], Mode::Strict);
        let eff = effective_from(&own);
        let p = path("assets/bad.mp4");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS).unwrap();
        assert_eq!(v.severity, Severity::Strict);
    }

    #[test]
    fn compound_extension_is_split_before_comparison() {
        // `.tar.gz` is a builtin compound — splitting it as just `.gz`
        // would let an archive directory accept only `.gz` and still
        // flag `.tar.gz` files. Compound splitting prevents that.
        let own = accepts_with(
            vec![AcceptsToken::Ext(normalize_ext(".tar.gz"))],
            Mode::Warn,
        );
        let eff = effective_from(&own);
        let p = path("backups/archive.tar.gz");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS);
        assert!(v.is_none());
    }

    #[test]
    fn no_extension_file_is_accepted_when_empty_sentinel_in_set() {
        let own = accepts_with(vec![AcceptsToken::Ext(normalize_ext(""))], Mode::Warn);
        let eff = effective_from(&own);
        let p = path("docs/README");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS);
        assert!(
            v.is_none(),
            "`\"\"` in exts must accept extensionless files"
        );
    }

    #[test]
    fn no_extension_file_is_rejected_when_set_excludes_empty() {
        let own = accepts_with(vec![AcceptsToken::Ext(normalize_ext(".md"))], Mode::Warn);
        let eff = effective_from(&own);
        let p = path("docs/README");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(
            v.reason.contains("<no extension>"),
            "reason should mention the no-extension marker, got {}",
            v.reason
        );
    }

    #[test]
    fn inherited_only_dir_marks_winning_source_inherited() {
        // Child has no own entries but inherits everything.
        let parent = accepts_with(vec![AcceptsToken::Ext(normalize_ext(".psd"))], Mode::Warn);
        let child = RawAccepts {
            inherit: true,
            exts: vec![],
            mode: Mode::Warn,
        };
        let eff = compute_effective_accepts(Some(&child), &[&parent], &AliasCatalog::builtin())
            .unwrap()
            .unwrap();
        let p = path("assets/nested/bad.mp4");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS).unwrap();
        assert_eq!(
            v.placement_details.unwrap().winning_rule_source,
            AcceptsSource::Inherited,
        );
    }

    #[test]
    fn expected_exts_is_sorted_in_violation() {
        let own = accepts_with(
            vec![
                AcceptsToken::Ext(normalize_ext(".tif")),
                AcceptsToken::Ext(normalize_ext(".psd")),
                AcceptsToken::Ext(normalize_ext(".jpg")),
            ],
            Mode::Warn,
        );
        let eff = effective_from(&own);
        let p = path("assets/bad.mp4");
        let v = evaluate_placement_for_file(&p, None, Some(&eff), BUILTIN_COMPOUND_EXTS).unwrap();
        let exts = v.placement_details.unwrap().expected_exts;
        assert_eq!(exts, vec!["jpg", "psd", "tif"]);
    }
}
