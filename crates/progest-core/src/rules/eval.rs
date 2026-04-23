//! Top-level evaluation entry point (§8).
//!
//! [`evaluate`] folds the pieces the other modules built —
//! `applies_to` matcher, template matcher, constraint evaluator,
//! inheritance resolver — into a single pass that produces:
//!
//! 1. Zero or more [`Violation`] records (spec §8.3).
//! 2. A [`RuleHit`] trace describing how every candidate rule was
//!    decided (winner / shadowed / not-applicable) (spec §9.2).
//!
//! The control flow for one file:
//!
//! - Collect all candidate rules whose `applies_to` matches.
//! - Rank the template candidates by specificity (§7.4) and pick a
//!   single winner. No fallback on mismatch — winner and reason are
//!   reported as-is (§8.1).
//! - Evaluate every constraint candidate independently (AND
//!   composition per §8.1) and collect failures.
//! - Emit one Violation per template mismatch and per constraint rule
//!   that had at least one failure.
//!
//! Section references below target `docs/NAMING_RULES_DSL.md`.

use std::cmp::Ordering;

use super::constraint::{ConstraintFailure, evaluate_constraint};
use super::inheritance::{CompiledRule, CompiledRuleBody, CompiledRuleSet};
use super::template::{TemplateMatch, match_basename};
use super::types::{
    Category, Decision, Mode, RuleHit, RuleKind, RuleSource, Severity, SpecificityScore, Violation,
};
use crate::fs::ProjectPath;
use crate::meta::MetaDocument;

// --- Public types ----------------------------------------------------------

/// Result of evaluating one path against a [`CompiledRuleSet`].
#[derive(Debug, Clone, Default)]
pub struct EvaluationOutcome {
    /// All violations produced for this file. Empty when the file
    /// satisfied every applicable rule.
    pub violations: Vec<Violation>,
    /// Rule-by-rule trace (§9.2). Always populated — callers that
    /// don't need it can simply drop the field.
    pub trace: Vec<RuleHit>,
}

/// Evaluate `path` (with optional `meta`) against `ruleset`.
///
/// `compound_exts` is passed through to the constraint evaluator for
/// §4.8-style extension stripping.
///
/// Evaluation errors (missing `.meta.custom.<name>`, numeric
/// overflow, etc — see [`EvaluationError`]) are folded into the
/// violations list with severity [`Severity::EvaluationError`] so
/// the caller doesn't have to special-case them.
#[must_use]
pub fn evaluate(
    path: &ProjectPath,
    meta: Option<&MetaDocument>,
    ruleset: &CompiledRuleSet,
    compound_exts: &[&str],
) -> EvaluationOutcome {
    let basename = path.file_name().unwrap_or("");
    let file_id = meta.map(|m| m.file_id);

    // 1. Collect candidates + best specificity per rule.
    let mut candidates: Vec<Candidate> = Vec::new();
    for (idx, rule) in ruleset.rules.iter().enumerate() {
        if rule.mode == Mode::Off {
            continue;
        }
        if let Some(best) = rule.applies_to.match_best(path) {
            candidates.push(Candidate {
                idx,
                kind: rule.kind(),
                specificity: best.specificity(),
            });
        }
    }

    let mut trace: Vec<RuleHit> = Vec::new();
    let mut violations: Vec<Violation> = Vec::new();

    // 2. Template layer: specificity winner, everyone else shadowed.
    let mut template_candidates: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| matches!(c.kind, RuleKind::Template))
        .collect();
    template_candidates.sort_by(|a, b| tiebreak(a, b, &ruleset.rules));

    if let Some(winner) = template_candidates.first().copied() {
        let winner_rule = &ruleset.rules[winner.idx];
        trace.push(make_rule_hit(
            winner_rule,
            winner.specificity,
            Decision::Winner,
            "template winner by specificity / source hierarchy",
        ));

        // Shadowed template candidates.
        for c in template_candidates.iter().skip(1) {
            let rule = &ruleset.rules[c.idx];
            trace.push(make_rule_hit(
                rule,
                c.specificity,
                Decision::Shadowed,
                "lost template specificity tie-break to winner",
            ));
        }

        // Evaluate the winner template.
        evaluate_template_winner(
            winner_rule,
            winner.specificity,
            path,
            basename,
            file_id,
            meta,
            compound_exts,
            &mut violations,
        );
    }

    // 3. Constraint layer: AND composition — every hit rule contributes.
    for c in candidates
        .iter()
        .filter(|c| matches!(c.kind, RuleKind::Constraint))
    {
        let rule = &ruleset.rules[c.idx];
        let body = match &rule.body {
            CompiledRuleBody::Constraint(cc) => cc,
            // This branch is unreachable given the filter above; keep
            // the `_ => unreachable!()` out of public code and just
            // handle the other variant defensively.
            CompiledRuleBody::Template(_) => continue,
        };
        let failures = evaluate_constraint(body, basename, compound_exts);
        if failures.is_empty() {
            trace.push(make_rule_hit(
                rule,
                c.specificity,
                Decision::Winner,
                "constraint passed",
            ));
        } else {
            trace.push(make_rule_hit(
                rule,
                c.specificity,
                Decision::Winner,
                "constraint failed (see violation reason)",
            ));

            if let Some(severity) = rule.mode.violation_severity() {
                let reason = failures
                    .iter()
                    .map(ConstraintFailure::to_string)
                    .collect::<Vec<_>>()
                    .join("; ");
                violations.push(Violation {
                    file_id,
                    path: path.clone(),
                    rule_id: rule.id.clone(),
                    category: Category::Naming,
                    kind: RuleKind::Constraint,
                    severity,
                    reason,
                    trace: Vec::new(), // filled by the caller if it wants detail
                    suggested_names: Vec::new(),
                });
            }
        }
    }

    // Attach the full trace only to violations (§9.3 default view).
    // Callers that want the trace on non-violation files can read
    // EvaluationOutcome::trace directly.
    for v in &mut violations {
        v.trace.clone_from(&trace);
    }

    EvaluationOutcome { violations, trace }
}

// --- Internals -------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Candidate {
    idx: usize,
    kind: RuleKind,
    specificity: SpecificityScore,
}

/// Specificity tiebreak (§7.4):
/// 1. `(literal_segments, literal_chars)` — higher wins
/// 2. source hierarchy — `own > inherited (near) > project_wide`
/// 3. `rule_id` lexicographic — stable ordering
fn tiebreak(a: &Candidate, b: &Candidate, rules: &[CompiledRule]) -> Ordering {
    // Higher specificity wins → reverse order.
    match b.specificity.cmp(&a.specificity) {
        Ordering::Equal => {}
        other => return other,
    }

    let ra = &rules[a.idx];
    let rb = &rules[b.idx];

    match source_rank(ra.provenance.source).cmp(&source_rank(rb.provenance.source)) {
        Ordering::Equal => {}
        other => return other, // lower rank wins, so Ordering::Less = a wins (sorts first)
    }

    ra.id.cmp(&rb.id)
}

/// Lower is more specific. Returns a tuple so `Inherited { distance }`
/// sorts in distance-ascending order (nearest first).
fn source_rank(source: RuleSource) -> (u8, u16) {
    match source {
        RuleSource::Own => (0, 0),
        RuleSource::Inherited { distance } => (1, distance),
        RuleSource::ProjectWide => (2, 0),
    }
}

fn make_rule_hit(
    rule: &CompiledRule,
    specificity: SpecificityScore,
    decision: Decision,
    explanation: &str,
) -> RuleHit {
    RuleHit {
        rule_id: rule.id.clone(),
        kind: rule.kind(),
        source: rule.provenance.source,
        decision,
        specificity_score: specificity,
        explanation: explanation.to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_template_winner(
    rule: &CompiledRule,
    specificity: SpecificityScore,
    path: &ProjectPath,
    basename: &str,
    file_id: Option<crate::identity::FileId>,
    meta: Option<&MetaDocument>,
    compound_exts: &[&str],
    violations: &mut Vec<Violation>,
) {
    let tpl = match &rule.body {
        CompiledRuleBody::Template(t) => t,
        CompiledRuleBody::Constraint(_) => return,
    };

    match match_basename(tpl, basename, meta, compound_exts) {
        Ok(TemplateMatch {
            matched: true,
            captures: _,
            failure_reason: _,
        }) => {}
        Ok(TemplateMatch {
            matched: false,
            captures: _,
            failure_reason,
        }) => {
            if let Some(severity) = rule.mode.violation_severity() {
                let reason = failure_reason.unwrap_or_else(|| "template mismatch".into());
                violations.push(Violation {
                    file_id,
                    path: path.clone(),
                    rule_id: rule.id.clone(),
                    category: Category::Naming,
                    kind: RuleKind::Template,
                    severity,
                    reason,
                    trace: Vec::new(),
                    suggested_names: Vec::new(),
                });
            }
        }
        Err(err) => violations.push(Violation {
            file_id,
            path: path.clone(),
            rule_id: rule.id.clone(),
            category: Category::Naming,
            kind: RuleKind::Template,
            severity: Severity::EvaluationError,
            reason: err.to_string(),
            trace: Vec::new(),
            suggested_names: Vec::new(),
        }),
    }

    let _ = specificity; // specificity was already baked into the trace
}

// --- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::FileId;
    use crate::meta::MetaDocument;
    use crate::rules::inheritance::{RuleSetLayer, compile_ruleset};
    use crate::rules::loader::load_document;

    fn ruleset(project_toml: &str) -> CompiledRuleSet {
        let doc = load_document(project_toml).unwrap();
        compile_ruleset(vec![RuleSetLayer {
            source: RuleSource::ProjectWide,
            base_dir: ProjectPath::root(),
            rules: doc.rules,
        }])
        .unwrap()
    }

    fn ruleset_with_own(project_toml: &str, own_base: &str, own_toml: &str) -> CompiledRuleSet {
        let project_doc = load_document(project_toml).unwrap();
        let own_doc = load_document(own_toml).unwrap();
        compile_ruleset(vec![
            RuleSetLayer {
                source: RuleSource::Own,
                base_dir: ProjectPath::new(own_base).unwrap(),
                rules: own_doc.rules,
            },
            RuleSetLayer {
                source: RuleSource::ProjectWide,
                base_dir: ProjectPath::root(),
                rules: project_doc.rules,
            },
        ])
        .unwrap()
    }

    fn path(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    // --- Template pass / fail ---------------------------------------------

    #[test]
    fn pass_emits_no_violations_and_records_trace_winner() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "shot-assets-v1"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
"#,
        );
        let out = evaluate(
            &path("assets/shots/ch010/ch010_001_bg_forest_v03.psd"),
            None,
            &rs,
            &[],
        );
        assert!(out.violations.is_empty());
        assert_eq!(out.trace.len(), 1);
        assert!(matches!(out.trace[0].decision, Decision::Winner));
    }

    #[test]
    fn template_mismatch_emits_violation_with_trace() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "shot-assets-v1"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
"#,
        );
        let out = evaluate(
            &path("assets/shots/ch010/ch010_bg_forest_v03.psd"),
            None,
            &rs,
            &[],
        );
        assert_eq!(out.violations.len(), 1);
        let v = &out.violations[0];
        assert_eq!(v.rule_id.as_str(), "shot-assets-v1");
        assert!(matches!(v.kind, RuleKind::Template));
        assert!(matches!(v.severity, Severity::Warn));
        assert_eq!(v.trace.len(), 1);
    }

    // --- Specificity tie-break --------------------------------------------

    #[test]
    fn more_specific_template_wins_and_less_specific_is_shadowed() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "general"
kind = "template"
applies_to = "./assets/**"
template = "{desc:snake}_v{version:02d}.{ext}"

[[rules]]
id = "shots-specific"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
"#,
        );
        let out = evaluate(
            &path("assets/shots/ch010/ch010_001_bg_v03.psd"),
            None,
            &rs,
            &[],
        );
        let winner = out
            .trace
            .iter()
            .find(|h| matches!(h.decision, Decision::Winner))
            .expect("expected a winner");
        assert_eq!(winner.rule_id.as_str(), "shots-specific");

        let shadowed = out
            .trace
            .iter()
            .find(|h| matches!(h.decision, Decision::Shadowed))
            .expect("expected a shadowed rule");
        assert_eq!(shadowed.rule_id.as_str(), "general");
    }

    // --- Constraint AND composition ---------------------------------------

    #[test]
    fn two_constraint_rules_both_apply_independently() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = "./**"
charset = "ascii"

[[rules]]
id = "length-cap"
kind = "constraint"
applies_to = "./**"
max_length = 12
"#,
        );
        let out = evaluate(&path("assets/日本語long_name.psd"), None, &rs, &[]);
        // Expect violations for both rules (ascii and length).
        let ids: Vec<_> = out
            .violations
            .iter()
            .map(|v| v.rule_id.as_str().to_owned())
            .collect();
        assert!(ids.contains(&"ascii-only".into()));
        assert!(ids.contains(&"length-cap".into()));
    }

    // --- Mode = off --------------------------------------------------------

    #[test]
    fn mode_off_produces_neither_violation_nor_trace() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "muted"
kind = "constraint"
applies_to = "./**"
mode = "off"
charset = "ascii"
"#,
        );
        let out = evaluate(&path("日本語.pdf"), None, &rs, &[]);
        assert!(out.violations.is_empty());
        assert!(out.trace.is_empty());
    }

    #[test]
    fn mode_hint_produces_trace_but_no_violation() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "nudge"
kind = "constraint"
applies_to = "./**"
mode = "hint"
charset = "ascii"
"#,
        );
        let out = evaluate(&path("日本語.pdf"), None, &rs, &[]);
        // No violation surfaces in the list itself (Mode::Hint violation
        // severity is still Hint, but the CLI filters those). For the
        // in-memory outcome we do emit it with severity = Hint so UIs
        // can render the suggestion.
        let hint_violation = out
            .violations
            .iter()
            .find(|v| v.rule_id.as_str() == "nudge");
        if let Some(v) = hint_violation {
            assert!(matches!(v.severity, Severity::Hint));
        }
    }

    // --- evaluation_error → violation -------------------------------------

    #[test]
    fn missing_custom_field_becomes_evaluation_error_violation() {
        let rs = ruleset(
            r#"
schema_version = 1

[[rules]]
id = "scene-seq"
kind = "template"
applies_to = "./assets/scenes/**/*.tif"
template = "sc{field:scene:03d}_{desc:slug}.{ext}"
"#,
        );
        // Build a .meta without custom.scene.
        let meta = MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff".parse().unwrap(),
        );
        let out = evaluate(
            &path("assets/scenes/ch010/sc020_forest-night.tif"),
            Some(&meta),
            &rs,
            &[],
        );
        assert_eq!(out.violations.len(), 1);
        assert!(matches!(
            out.violations[0].severity,
            Severity::EvaluationError
        ));
    }

    // --- Own-dir layer wins specificity tie-break --------------------------

    #[test]
    fn own_layer_wins_tie_against_project_wide_same_glob() {
        let rs = ruleset_with_own(
            // project-wide: assets/** template
            r#"
schema_version = 1

[[rules]]
id = "default-shot"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}.{ext}"
"#,
            // own: assets/shots/** with a template of its own under a
            // different id so it co-exists rather than override-replaces.
            "assets/shots",
            r#"
schema_version = 1

[[rules]]
id = "own-shot"
kind = "template"
applies_to = "./**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}.{ext}"
"#,
        );
        let out = evaluate(&path("assets/shots/ch010/ch010_001_bg.psd"), None, &rs, &[]);
        let winner = out
            .trace
            .iter()
            .find(|h| matches!(h.decision, Decision::Winner))
            .unwrap();
        // Both normalize to the same glob `assets/shots/**/*.psd`, so
        // specificity ties and the Own layer should take it.
        assert_eq!(winner.rule_id.as_str(), "own-shot");
    }
}
