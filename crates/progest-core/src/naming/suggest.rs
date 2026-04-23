//! Populate [`Violation::suggested_names`] from the cleanup pipeline.
//!
//! The kickoff decision was: only naming-category violations get a
//! mechanical suggestion; placement violations are the
//! `suggested_destinations` system's job (which ships later in a
//! different follow-up). That keeps the two UI surfaces from
//! competing — a single file with both a naming and a placement
//! violation should show a "rename to X" hint alongside a "move to
//! Y" hint, not one combined suggestion.
//!
//! Suggestions also only get recorded when the pipeline produces a
//! candidate with **no holes**. Holes mean the pipeline refused to
//! drop content silently, and emitting `⟨cjk-1⟩` in a user-facing
//! list of disk-safe suggestions would be misleading at best. Users
//! who want to fill holes run `progest clean --fill-mode=...`, which
//! opts into a known substitution.

use crate::rules::types::{Category, Violation};

use super::pipeline::clean_basename;
use super::types::CleanupConfig;

/// Walk `violations`, run the cleanup pipeline over each naming
/// violation's basename, and attach the result to
/// [`Violation::suggested_names`] when the pipeline produced a
/// hole-free candidate that differs from the original.
///
/// Placement violations are left untouched.
///
/// Idempotent: calling with the same config twice doesn't create
/// duplicate entries.
pub fn fill_suggested_names(
    violations: &mut [Violation],
    cleanup: &CleanupConfig,
    compound_exts: &[&str],
) {
    for v in violations
        .iter_mut()
        .filter(|v| matches!(v.category, Category::Naming))
    {
        let Some(basename) = v.path.file_name() else {
            continue;
        };
        let candidate = clean_basename(basename, cleanup, compound_exts);
        let Some(suggestion) = super::fill::try_resolve_clean(&candidate) else {
            continue;
        };
        if suggestion == basename {
            continue;
        }
        if !v.suggested_names.contains(&suggestion) {
            v.suggested_names.push(suggestion);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::ProjectPath;
    use crate::identity::FileId;
    use crate::naming::types::CaseStyle;
    use crate::rules::types::{AcceptsSource, Category, PlacementDetails, RuleKind, Severity};

    fn naming_violation(path: &str) -> Violation {
        Violation {
            file_id: Some(FileId::new_v7()),
            path: ProjectPath::new(path).unwrap(),
            rule_id: "naming-demo".parse().unwrap(),
            category: Category::Naming,
            kind: RuleKind::Template,
            severity: Severity::Warn,
            reason: "template mismatch".into(),
            trace: Vec::new(),
            suggested_names: Vec::new(),
            placement_details: None,
        }
    }

    fn placement_violation(path: &str) -> Violation {
        Violation {
            file_id: None,
            path: ProjectPath::new(path).unwrap(),
            rule_id: "placement".parse().unwrap(),
            category: Category::Placement,
            kind: RuleKind::Constraint,
            severity: Severity::Warn,
            reason: "extension not accepted".into(),
            trace: Vec::new(),
            suggested_names: Vec::new(),
            placement_details: Some(PlacementDetails {
                expected_exts: vec!["png".into()],
                winning_rule_source: AcceptsSource::Own,
                suggested_destinations: Vec::new(),
            }),
        }
    }

    fn cfg_aggressive() -> CleanupConfig {
        CleanupConfig {
            remove_copy_suffix: true,
            remove_cjk: true,
            convert_case: CaseStyle::Snake,
        }
    }

    #[test]
    fn naming_violation_receives_cleaned_suggestion() {
        // `MainRole_v01 (1).png` → stage1 strips ` (1)`, stage2 no CJK
        // so no holes, stage3 snake → `main_role_v01.png`.
        let mut violations = vec![naming_violation("assets/shots/MainRole_v01 (1).png")];
        fill_suggested_names(&mut violations, &cfg_aggressive(), &[]);
        assert_eq!(
            violations[0].suggested_names,
            vec!["main_role_v01.png".to_string()]
        );
    }

    #[test]
    fn candidate_with_cjk_hole_suppresses_suggestion() {
        // Pipeline leaves a CJK hole → not safe as a disk suggestion.
        let mut violations = vec![naming_violation("assets/shots/カット_v01.png")];
        fill_suggested_names(&mut violations, &cfg_aggressive(), &[]);
        assert!(violations[0].suggested_names.is_empty());
    }

    #[test]
    fn placement_violations_are_left_untouched() {
        let mut violations = vec![placement_violation("assets/shots/MainRole_v01 (1).psd")];
        fill_suggested_names(&mut violations, &cfg_aggressive(), &[]);
        assert!(violations[0].suggested_names.is_empty());
    }

    #[test]
    fn no_op_when_pipeline_produces_original_name() {
        // All stages off → candidate equals the original basename.
        let cfg = CleanupConfig {
            remove_copy_suffix: false,
            remove_cjk: false,
            convert_case: CaseStyle::Off,
        };
        let mut violations = vec![naming_violation("assets/shots/already_clean.png")];
        fill_suggested_names(&mut violations, &cfg, &[]);
        assert!(violations[0].suggested_names.is_empty());
    }

    #[test]
    fn repeated_calls_do_not_duplicate_entries() {
        let mut violations = vec![naming_violation("assets/shots/MainRole_v01 (1).png")];
        fill_suggested_names(&mut violations, &cfg_aggressive(), &[]);
        fill_suggested_names(&mut violations, &cfg_aggressive(), &[]);
        assert_eq!(violations[0].suggested_names.len(), 1);
    }
}
