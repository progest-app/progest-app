//! Inheritance resolver and per-rule compiler (§7.1 / §7.2).
//!
//! Takes an ordered stack of [`RuleSetLayer`] entries — own dirmeta
//! first, ancestors next (nearest first), project-wide last — and
//! produces one [`CompiledRuleSet`] with full-replace override applied.
//!
//! The "CSS cascade" behavior (spec §7.1) is implemented by iterating
//! layers in the order callers pass them: the first layer that
//! mentions a given `id` is the one that ends up in the output; later
//! layers with the same `id` are dropped silently when they would
//! match in kind, and rejected with a hard error when they would
//! change the rule's kind without `override = true` on the child
//! (spec §7.2).
//!
//! This module owns the inter-layer machinery only; per-layer id
//! uniqueness and structural validation are the loader's job
//! (`core::rules::loader`). What reaches the inheritance resolver is
//! already a valid single-file ruleset.

use std::collections::BTreeMap;

use thiserror::Error;

use super::applies_to::{AppliesToError, CompiledAppliesTo};
use super::constraint::{CompiledConstraint, ConstraintCompileError, compile_constraint};
use super::loader::{LoadWarning, RawRule, RawRuleBody};
use super::template::{CompiledTemplate, TemplateError, compile as compile_template_body};
use super::types::{Mode, RuleId, RuleKind, RuleSource};
use crate::fs::ProjectPath;

// --- Public types ----------------------------------------------------------

/// A single layer in the inheritance chain.
#[derive(Debug, Clone)]
pub struct RuleSetLayer {
    /// Where, logically, these rules were defined. Propagated into
    /// each rule's [`RuleProvenance`] for the tie-break step later.
    pub source: RuleSource,
    /// Base directory for `applies_to` normalization (spec §3.1).
    /// Pass [`ProjectPath::root`] for project-wide layers.
    pub base_dir: ProjectPath,
    pub rules: Vec<RawRule>,
}

/// Source metadata a compiled rule carries into the evaluator.
#[derive(Debug, Clone)]
pub struct RuleProvenance {
    pub source: RuleSource,
    pub base_dir: ProjectPath,
}

/// A rule with all compilation work done (`applies_to` glob, template
/// regex / constraint regex and casing table, etc).
#[derive(Debug)]
pub struct CompiledRule {
    pub id: RuleId,
    pub mode: Mode,
    pub description: Option<String>,
    pub applies_to: CompiledAppliesTo,
    pub provenance: RuleProvenance,
    pub body: CompiledRuleBody,
}

impl CompiledRule {
    #[must_use]
    pub fn kind(&self) -> RuleKind {
        match self.body {
            CompiledRuleBody::Template(_) => RuleKind::Template,
            CompiledRuleBody::Constraint(_) => RuleKind::Constraint,
        }
    }
}

/// The compiled body for a rule.
#[derive(Debug)]
pub enum CompiledRuleBody {
    Template(CompiledTemplate),
    Constraint(CompiledConstraint),
}

/// A fully resolved ruleset ready for evaluation.
#[derive(Debug)]
pub struct CompiledRuleSet {
    pub rules: Vec<CompiledRule>,
    pub warnings: Vec<LoadWarning>,
}

// --- Errors ----------------------------------------------------------------

#[derive(Debug, Error)]
pub enum InheritanceError {
    #[error("applies_to compilation failed for rule `{id}`: {source}")]
    AppliesTo {
        id: RuleId,
        #[source]
        source: AppliesToError,
    },
    #[error("template compilation failed for rule `{id}`: {source}")]
    Template {
        id: RuleId,
        #[source]
        source: TemplateError,
    },
    #[error("constraint compilation failed for rule `{id}`: {source}")]
    Constraint {
        id: RuleId,
        #[source]
        source: ConstraintCompileError,
    },
    #[error(
        "rule `{id}` changes kind ({parent:?} → {child:?}) across layers without `override = true`"
    )]
    KindChangeNeedsOverride {
        id: RuleId,
        parent: RuleKind,
        child: RuleKind,
    },
}

// --- Compile ---------------------------------------------------------------

/// Compile an inheritance chain into a single ruleset.
///
/// Layers must be passed most-specific first (own dirmeta → nearest
/// ancestor → ... → project-wide). The first layer to mention a given
/// `id` wins (§7.1). A later layer with the same `id` is silently
/// dropped when `kind` matches, and rejected when `kind` differs
/// unless the child already carried `override = true`.
///
/// # Errors
///
/// Returns [`InheritanceError`] on glob / template / constraint
/// compilation failures and on cross-layer kind changes without
/// `override = true` on the child.
pub fn compile_ruleset(layers: Vec<RuleSetLayer>) -> Result<CompiledRuleSet, InheritanceError> {
    let mut compiled: Vec<CompiledRule> = Vec::new();
    // id → (child kind, child override_flag) for the nearest layer
    // that already installed this id. The kind is copied out so we
    // can validate kind changes without re-reading `compiled`.
    let mut seen: BTreeMap<RuleId, SeenEntry> = BTreeMap::new();
    let mut warnings: Vec<LoadWarning> = Vec::new();

    for layer in layers {
        let RuleSetLayer {
            source,
            base_dir,
            rules,
        } = layer;
        for raw in rules {
            if let Some(entry) = seen.get(&raw.id).copied() {
                // A nearer layer already installed this id — check the
                // contract and drop the current (farther-layer) entry.
                let parent_kind = raw.kind;
                let child_kind = entry.child_kind;
                if parent_kind != child_kind && !entry.child_override {
                    return Err(InheritanceError::KindChangeNeedsOverride {
                        id: raw.id,
                        parent: parent_kind,
                        child: child_kind,
                    });
                }
                if parent_kind == child_kind && !entry.child_override {
                    warnings.push(LoadWarning::OverrideWithoutExplicitFlag {
                        rule_id: raw.id.as_str().to_owned(),
                    });
                }
                continue;
            }

            let compiled_rule = compile_single_rule(&raw, source, &base_dir)?;
            seen.insert(
                raw.id.clone(),
                SeenEntry {
                    child_kind: raw.kind,
                    child_override: raw.override_flag,
                },
            );
            compiled.push(compiled_rule);
        }
    }

    Ok(CompiledRuleSet {
        rules: compiled,
        warnings,
    })
}

#[derive(Debug, Clone, Copy)]
struct SeenEntry {
    child_kind: RuleKind,
    child_override: bool,
}

fn compile_single_rule(
    raw: &RawRule,
    source: RuleSource,
    base_dir: &ProjectPath,
) -> Result<CompiledRule, InheritanceError> {
    let applies_to = CompiledAppliesTo::compile(&raw.applies_to, base_dir).map_err(|e| {
        InheritanceError::AppliesTo {
            id: raw.id.clone(),
            source: e,
        }
    })?;

    let body = match &raw.body {
        RawRuleBody::Template(t) => {
            let tpl =
                compile_template_body(&t.template).map_err(|e| InheritanceError::Template {
                    id: raw.id.clone(),
                    source: e,
                })?;
            CompiledRuleBody::Template(tpl)
        }
        RawRuleBody::Constraint(c) => {
            let cst = compile_constraint(c).map_err(|e| InheritanceError::Constraint {
                id: raw.id.clone(),
                source: e,
            })?;
            CompiledRuleBody::Constraint(cst)
        }
    };

    Ok(CompiledRule {
        id: raw.id.clone(),
        mode: raw.mode,
        description: raw.description.clone(),
        applies_to,
        provenance: RuleProvenance {
            source,
            base_dir: base_dir.clone(),
        },
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::loader::load_document;

    fn layer_project(raw_toml: &str) -> RuleSetLayer {
        let doc = load_document(raw_toml).unwrap();
        RuleSetLayer {
            source: RuleSource::ProjectWide,
            base_dir: ProjectPath::root(),
            rules: doc.rules,
        }
    }

    fn layer_own(base: &str, raw_toml: &str) -> RuleSetLayer {
        let doc = load_document(raw_toml).unwrap();
        RuleSetLayer {
            source: RuleSource::Own,
            base_dir: ProjectPath::new(base).unwrap(),
            rules: doc.rules,
        }
    }

    // --- Simple single-layer ----------------------------------------------

    #[test]
    fn compiles_project_wide_layer_as_is() {
        let layer = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "shot-assets-v1"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
"#,
        );
        let set = compile_ruleset(vec![layer]).unwrap();
        assert_eq!(set.rules.len(), 1);
        assert!(matches!(set.rules[0].body, CompiledRuleBody::Template(_)));
        assert!(matches!(
            set.rules[0].provenance.source,
            RuleSource::ProjectWide
        ));
    }

    // --- Full-replace override --------------------------------------------

    #[test]
    fn child_replaces_parent_same_id_same_kind() {
        // Project-wide: casing = snake. Child (in references/): casing = any.
        let own = layer_own(
            "references",
            r#"
schema_version = 1

[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "any"
"#,
        );
        let project = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "snake"
"#,
        );
        let set = compile_ruleset(vec![own, project]).unwrap();
        assert_eq!(set.rules.len(), 1);
        // The "own" entry wins — its base_dir is `references`.
        assert_eq!(set.rules[0].provenance.base_dir.as_str(), "references");
        // `casing = any` came from the child, which for the evaluator
        // means "no casing regex".
        if let CompiledRuleBody::Constraint(c) = &set.rules[0].body {
            assert_eq!(c.casing, crate::rules::Casing::Any);
        } else {
            panic!("expected constraint body");
        }
    }

    #[test]
    fn child_replacing_same_kind_without_override_warns() {
        let own = layer_own(
            "references",
            r#"
schema_version = 1

[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "any"
"#,
        );
        let project = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "snake"
"#,
        );
        let set = compile_ruleset(vec![own, project]).unwrap();
        assert!(set.warnings.iter().any(|w| matches!(
            w,
            LoadWarning::OverrideWithoutExplicitFlag { rule_id } if rule_id == "project-default"
        )));
    }

    #[test]
    fn explicit_override_silences_warning() {
        let own = layer_own(
            "references",
            r#"
schema_version = 1

[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "any"
override = true
"#,
        );
        let project = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "snake"
"#,
        );
        let set = compile_ruleset(vec![own, project]).unwrap();
        assert!(set.warnings.is_empty());
    }

    // --- Kind change guardrail --------------------------------------------

    #[test]
    fn kind_change_without_override_is_an_error() {
        let own = layer_own(
            "references",
            r#"
schema_version = 1

[[rules]]
id = "cross-kind"
kind = "template"
applies_to = "./**"
template = "{prefix}.{ext}"
"#,
        );
        let project = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "cross-kind"
kind = "constraint"
applies_to = "./**"
casing = "snake"
"#,
        );
        let err = compile_ruleset(vec![own, project]).unwrap_err();
        assert!(matches!(
            err,
            InheritanceError::KindChangeNeedsOverride { .. }
        ));
    }

    #[test]
    fn kind_change_with_override_succeeds() {
        let own = layer_own(
            "references",
            r#"
schema_version = 1

[[rules]]
id = "cross-kind"
kind = "template"
applies_to = "./**"
template = "{prefix}.{ext}"
override = true
"#,
        );
        let project = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "cross-kind"
kind = "constraint"
applies_to = "./**"
casing = "snake"
"#,
        );
        let set = compile_ruleset(vec![own, project]).unwrap();
        assert_eq!(set.rules.len(), 1);
        assert!(matches!(set.rules[0].body, CompiledRuleBody::Template(_)));
    }

    // --- Different ids coexist --------------------------------------------

    #[test]
    fn child_and_parent_with_different_ids_both_survive() {
        let own = layer_own(
            "assets",
            r#"
schema_version = 1

[[rules]]
id = "assets-local"
kind = "constraint"
applies_to = "./**"
casing = "snake"
"#,
        );
        let project = layer_project(
            r#"
schema_version = 1

[[rules]]
id = "project-wide"
kind = "constraint"
applies_to = "./**"
charset = "ascii"
"#,
        );
        let set = compile_ruleset(vec![own, project]).unwrap();
        assert_eq!(set.rules.len(), 2);
        let ids: Vec<_> = set.rules.iter().map(|r| r.id.as_str().to_owned()).collect();
        assert!(ids.contains(&"assets-local".to_owned()));
        assert!(ids.contains(&"project-wide".to_owned()));
    }

    // --- Applies-to base resolution ---------------------------------------

    #[test]
    fn dirmeta_applies_to_is_normalized_to_project_root() {
        let own = layer_own(
            "references",
            r#"
schema_version = 1

[[rules]]
id = "refs-any"
kind = "constraint"
applies_to = "./**"
casing = "any"
"#,
        );
        let set = compile_ruleset(vec![own]).unwrap();
        let rule = &set.rules[0];
        // ./**  under base `references` should match
        // `references/doc.pdf` but not `assets/foo.psd`.
        let inside = ProjectPath::new("references/doc.pdf").unwrap();
        let outside = ProjectPath::new("assets/foo.psd").unwrap();
        assert!(rule.applies_to.match_best(&inside).is_some());
        assert!(rule.applies_to.match_best(&outside).is_none());
    }
}
