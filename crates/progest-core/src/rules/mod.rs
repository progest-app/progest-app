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

pub mod types;

pub use types::{
    Category, Decision, Mode, RuleHit, RuleId, RuleIdError, RuleKind, RuleSource, Severity,
    SpecificityScore, Violation,
};
