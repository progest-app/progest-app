//! End-to-end tests for the `core::naming` pipeline.
//!
//! These exercise the public surface (`clean_basename`, `resolve`,
//! `fill_suggested_names`) as a black box so regressions surface even
//! if the internal module layout changes.
//!
//! Shape-style fixtures live inline rather than under
//! `tests/naming_golden/`: the pipeline is linear and deterministic,
//! and the per-scenario YAML machinery that powers `rules_golden` /
//! `accepts_golden` earns its keep when the behavior is table-driven.
//! If we grow CJK-adjacent edge cases (e.g. grapheme-cluster hole
//! boundaries), this file can grow a fixture directory alongside.

use progest_core::fs::ProjectPath;
use progest_core::identity::FileId;
use progest_core::naming::{
    CaseStyle, CleanupConfig, FillMode, Segment, clean_basename, fill_suggested_names, resolve,
};
use progest_core::rules::{BUILTIN_COMPOUND_EXTS, Category, RuleKind, Severity, Violation};

fn cfg_all_on() -> CleanupConfig {
    CleanupConfig {
        remove_copy_suffix: true,
        remove_cjk: true,
        convert_case: CaseStyle::Snake,
    }
}

// --- Pipeline cases --------------------------------------------------------

/// `(label, input, config, expected_sentinel)` — the pipeline is
/// deterministic so sentinel rendering is a safe proxy for the
/// candidate structure.
fn pipeline_cases() -> Vec<(&'static str, &'static str, CleanupConfig, &'static str)> {
    vec![
        (
            "ascii snake case + copy suffix",
            "MainRole_v01 (1).png",
            cfg_all_on(),
            "main_role_v01.png",
        ),
        (
            "dash-copy numbered suffix",
            "Draft - Copy (3).png",
            cfg_all_on(),
            "draft.png",
        ),
        (
            "japanese copy counter",
            "design のコピー 2.png",
            cfg_all_on(),
            "design.png",
        ),
        (
            "cjk run becomes hole",
            "カット_v01.png",
            cfg_all_on(),
            "\u{27E8}cjk-1\u{27E9}v01.png",
        ),
        (
            "multiple cjk runs number holes 1..N",
            "カット_主役_v01.png",
            cfg_all_on(),
            "\u{27E8}cjk-1\u{27E9}\u{27E8}cjk-2\u{27E9}v01.png",
        ),
        (
            "case off preserves original tokens",
            "Shot_V01.PNG",
            CleanupConfig {
                remove_copy_suffix: false,
                remove_cjk: false,
                convert_case: CaseStyle::Off,
            },
            "Shot_V01.PNG",
        ),
        (
            "compound extension survives",
            "Archive.tar.gz",
            cfg_all_on(),
            "archive.tar.gz",
        ),
    ]
}

#[test]
fn pipeline_sentinel_rendering_matches_table() {
    let mut failures = Vec::new();
    for (label, input, cfg, expected) in pipeline_cases() {
        let cand = clean_basename(input, &cfg, BUILTIN_COMPOUND_EXTS);
        let got = cand.to_sentinel_string();
        if got != expected {
            failures.push(format!(
                "[{label}] input = {input:?}\n  expected = {expected:?}\n  got      = {got:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "naming pipeline sentinel mismatches:\n\n{}",
        failures.join("\n\n")
    );
}

// --- Fill-mode ------------------------------------------------------------

#[test]
fn fill_mode_skip_refuses_holes_but_succeeds_on_clean_candidates() {
    let clean = clean_basename("MainRole_v01 (1).png", &cfg_all_on(), BUILTIN_COMPOUND_EXTS);
    assert_eq!(
        resolve(&clean, &FillMode::Skip).unwrap().basename,
        "main_role_v01.png"
    );

    let with_hole = clean_basename("カット_v01.png", &cfg_all_on(), BUILTIN_COMPOUND_EXTS);
    assert!(resolve(&with_hole, &FillMode::Skip).is_err());
}

#[test]
fn fill_mode_placeholder_substitutes_every_hole() {
    let cand = clean_basename("カット_主役_v01.png", &cfg_all_on(), BUILTIN_COMPOUND_EXTS);
    let resolution = resolve(&cand, &FillMode::Placeholder("_".into())).unwrap();
    // Two holes → two "_", then the literal ("v01" because snake_case
    // stripped the leading "_"), then ".png".
    assert_eq!(resolution.basename, "__v01.png");
    assert_eq!(resolution.filled_holes.len(), 2);
}

// --- Suggested names fill --------------------------------------------------

fn naming_violation(path: &str) -> Violation {
    Violation {
        file_id: Some(FileId::new_v7()),
        path: ProjectPath::new(path).unwrap(),
        rule_id: "shot-assets-v1".parse().unwrap(),
        category: Category::Naming,
        kind: RuleKind::Template,
        severity: Severity::Warn,
        reason: "template mismatch".into(),
        trace: Vec::new(),
        suggested_names: Vec::new(),
        placement_details: None,
    }
}

#[test]
fn fill_suggested_names_produces_clean_rename_for_snakeable_input() {
    let mut violations = vec![naming_violation("assets/shots/ch010/MainRole_v01 (1).png")];
    fill_suggested_names(&mut violations, &cfg_all_on(), &[]);
    assert_eq!(
        violations[0].suggested_names,
        vec!["main_role_v01.png".to_string()]
    );
}

#[test]
fn fill_suggested_names_omits_candidates_with_holes() {
    let mut violations = vec![naming_violation("assets/shots/ch010/カット_v01.png")];
    fill_suggested_names(&mut violations, &cfg_all_on(), &[]);
    assert!(violations[0].suggested_names.is_empty());
}

#[test]
fn cleanup_pipeline_preserves_segment_structure_for_candidate_rendering() {
    let cand = clean_basename("カット_v01.png", &cfg_all_on(), BUILTIN_COMPOUND_EXTS);
    // Shape contract: first segment is a Hole, second is Literal.
    // UI layers rely on this to render the "カット" swatch next to
    // the inferred literal tail.
    assert_eq!(cand.segments.len(), 2);
    assert!(matches!(cand.segments[0], Segment::Hole(_)));
    assert!(matches!(cand.segments[1], Segment::Literal(_)));
}
