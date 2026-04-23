//! Wire types for rename operations.
//!
//! [`RenameOp`] is the unit a preview produces and an apply consumes.
//! It is `pub` and `serde`-able by design — CLI, JSON pipes (`progest
//! lint --format=json | progest rename --apply --from-stdin`), and the
//! future Tauri IPC layer all share this exact shape, so we accept the
//! cost of stabilizing it early in exchange for not maintaining a
//! parallel DTO.
//!
//! [`Conflict`] entries are the *non-fatal* warnings discovered at
//! preview time. An apply caller should refuse to run any op whose
//! `conflicts` is non-empty unless the user has explicitly overridden
//! the warning — preview is the only place these are computed, so the
//! apply path can stay focused on FS correctness.

use serde::{Deserialize, Serialize};

use crate::fs::ProjectPath;

/// A single proposed rename. Produced by [`crate::rename::preview`] and,
/// in a future commit, consumed by `apply`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameOp {
    /// Current path of the file (project-root-relative).
    pub from: ProjectPath,
    /// Proposed new path. Equal to `from` when the candidate is
    /// unresolved (`Conflict::Unresolved` is recorded), so the wire
    /// format never contains a placeholder string a user could
    /// accidentally accept.
    pub to: ProjectPath,
    /// Naming rule that drove this proposal, if any. Mirrors
    /// [`crate::history::Operation::Rename::rule_id`] so apply can
    /// forward it straight to the history log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Identifier shared by every op in a bulk preview (e.g. all
    /// members of a frame sequence). `None` when the op stands alone.
    /// Apply forwards this through to [`crate::history::AppendRequest::group_id`]
    /// so undo can roll the group back as a unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Warnings discovered at preview time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Conflict>,
}

impl RenameOp {
    /// `true` when the op carries no [`Conflict`] entries and is safe
    /// to apply without operator override.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// One non-fatal warning attached to a [`RenameOp`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    /// Human-readable explanation for CLI/UI display.
    pub message: String,
}

/// The kind of warning a [`Conflict`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// `from` and `to` resolve to the same path; applying would be a no-op.
    Identity,
    /// `to` already exists on disk and is **not** the `from` of another
    /// op in the same preview (chains are allowed). The apply caller
    /// must decide whether to overwrite.
    TargetExists,
    /// Two or more ops in the same preview point to the same `to`.
    /// Every offending op carries this kind so the CLI can render the
    /// full collision set.
    DuplicateTarget,
    /// The candidate could not be collapsed into a disk-safe basename
    /// under the chosen [`crate::naming::FillMode`] (holes remain, or
    /// `Prompt` was passed without an interactive resolver). `to`
    /// equals `from` for these ops.
    Unresolved,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    #[test]
    fn clean_op_has_no_conflicts() {
        let op = RenameOp {
            from: p("a.txt"),
            to: p("b.txt"),
            rule_id: None,
            group_id: None,
            conflicts: Vec::new(),
        };
        assert!(op.is_clean());
    }

    #[test]
    fn op_with_conflicts_is_not_clean() {
        let op = RenameOp {
            from: p("a.txt"),
            to: p("a.txt"),
            rule_id: None,
            group_id: None,
            conflicts: vec![Conflict {
                kind: ConflictKind::Identity,
                message: "no-op".into(),
            }],
        };
        assert!(!op.is_clean());
    }

    #[test]
    fn round_trips_through_json_omitting_empty_optionals() {
        let op = RenameOp {
            from: p("assets/foo.psd"),
            to: p("assets/bar.psd"),
            rule_id: Some("shot-assets-v1".into()),
            group_id: Some("seq-1".into()),
            conflicts: Vec::new(),
        };
        let json = serde_json::to_string(&op).unwrap();
        // Empty vec / None are skipped to keep the wire compact.
        assert!(!json.contains("conflicts"));
        assert!(json.contains("rule_id"));
        assert!(json.contains("group_id"));
        let back: RenameOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn round_trips_with_conflict_payload() {
        let op = RenameOp {
            from: p("a.txt"),
            to: p("b.txt"),
            rule_id: None,
            group_id: None,
            conflicts: vec![
                Conflict {
                    kind: ConflictKind::TargetExists,
                    message: "b.txt already exists".into(),
                },
                Conflict {
                    kind: ConflictKind::DuplicateTarget,
                    message: "two ops claim b.txt".into(),
                },
            ],
        };
        let back: RenameOp = serde_json::from_str(&serde_json::to_string(&op).unwrap()).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn conflict_kind_serializes_snake_case() {
        let kinds = [
            (ConflictKind::Identity, "\"identity\""),
            (ConflictKind::TargetExists, "\"target_exists\""),
            (ConflictKind::DuplicateTarget, "\"duplicate_target\""),
            (ConflictKind::Unresolved, "\"unresolved\""),
        ];
        for (k, want) in kinds {
            assert_eq!(serde_json::to_string(&k).unwrap(), want);
        }
    }
}
