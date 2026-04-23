//! Naming-rules engine (`core::rules`).
//!
//! Reads `.progest/rules.toml` plus per-directory `.dirmeta.toml` rule
//! sections, resolves inheritance and override, and produces a
//! `Vec<Violation>` + `rule_id` trace per evaluated file. The parser
//!
//! and evaluator follow `docs/NAMING_RULES_DSL.md` bit-for-bit; any
//! behavior change must be reflected there first.
//!
//! This file currently wires in the shared value types (via
//! [`types`]). Subsequent commits add the TOML loader, `applies_to`
//! matcher, template parser, constraint evaluator, inheritance
//! resolver, and the top-level `evaluate` entry point.

pub mod applies_to;
pub mod constraint;
pub mod inheritance;
pub mod loader;
pub mod template;
pub mod types;

pub use applies_to::{AppliesToError, CompiledAppliesTo, CompiledPattern, compute_specificity};
pub use constraint::{
    BUILTIN_COMPOUND_EXTS, CompiledConstraint, ConstraintCompileError, ConstraintFailure,
    compile_constraint, evaluate_constraint, split_basename,
};
pub use inheritance::{
    CompiledRule, CompiledRuleBody, CompiledRuleSet, InheritanceError, RuleProvenance,
    RuleSetLayer, compile_ruleset,
};
pub use loader::{
    AppliesToRaw, LoadError, LoadWarning, RULES_SCHEMA_VERSION, RawConstraintBody, RawRule,
    RawRuleBody, RawTemplateBody, RulesDocument, load_document,
};
pub use template::{
    Atom, CompiledTemplate, DateFormat, DateToken, DynamicAtom, DynamicSource, EvaluationError,
    FormatSpec, StaticAtom, StaticKind, TemplateError, TemplateMatch, compile as compile_template,
    match_basename,
};
pub use types::{
    Casing, Category, Charset, Decision, Mode, RuleHit, RuleId, RuleIdError, RuleKind, RuleSource,
    Severity, SpecificityScore, Violation,
};
