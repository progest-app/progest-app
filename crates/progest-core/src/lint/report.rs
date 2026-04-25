//! Shape of the grouped lint report emitted by the orchestrator.
//!
//! Serialized JSON form (stable wire, pinned by the CLI smoke tests):
//!
//! ```json
//! {
//!   "naming":   [Violation, ...],
//!   "placement":[Violation, ...],
//!   "sequence": [Violation, ...],
//!   "summary":  { "scanned": N, "naming_count": n1, ... }
//! }
//! ```
//!
//! Within each slot, violations are sorted by `(path, rule_id)` so the
//! output is stable across runs. Consumers that want a flat stream can
//! chain the three vectors.

use serde::Serialize;

use crate::rules::Violation;

/// Three-slot grouped report: one vector per [`crate::rules::Category`]
/// plus a pre-aggregated summary for the text renderer.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LintReport {
    pub naming: Vec<Violation>,
    pub placement: Vec<Violation>,
    pub sequence: Vec<Violation>,
    pub summary: LintSummary,
}

/// Counts folded up from the three slots. The CLI's text renderer
/// consumes this so it doesn't need to re-count in user-facing code.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LintSummary {
    pub scanned: usize,
    pub naming_count: usize,
    pub placement_count: usize,
    pub sequence_count: usize,
    /// `Severity::Strict` across all three slots.
    pub strict_count: usize,
    /// `Severity::EvaluationError` across all three slots.
    pub evaluation_error_count: usize,
    /// `Severity::Warn` across all three slots.
    pub warn_count: usize,
    /// `Severity::Hint` across all three slots.
    pub hint_count: usize,
}

impl LintReport {
    /// True when at least one violation carries a severity that must
    /// fail the CLI per DSL §8.2 (strict or evaluation-error).
    #[must_use]
    pub fn fails_ci(&self) -> bool {
        self.naming
            .iter()
            .chain(&self.placement)
            .chain(&self.sequence)
            .any(|v| v.severity.fails_ci())
    }

    /// Number of violation rows across all three slots.
    #[must_use]
    pub fn total(&self) -> usize {
        self.naming.len() + self.placement.len() + self.sequence.len()
    }

    /// Iterate every [`Violation`] in the report, naming → placement
    /// → sequence order. Convenience for consumers (e.g. the index
    /// writer) that want a single flat pass.
    pub fn iter_all(&self) -> impl Iterator<Item = &Violation> {
        self.naming
            .iter()
            .chain(self.placement.iter())
            .chain(self.sequence.iter())
    }
}
