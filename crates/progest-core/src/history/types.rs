//! Value types for the history log.
//!
//! An [`Entry`] records one completed operation — history is
//! append-only and arrives *after* the operation has already been
//! applied. Callers are responsible for their own atomicity; history
//! only stores the inverse so that undo can re-invoke the same
//! operation layer with the flipped payload.
//!
//! [`Operation`] is the initial op vocabulary: `rename` /
//! `tag_add` / `tag_remove` / `meta_edit` / `import`. Adding an op
//! later is a two-step change:
//!
//! 1. Add the variant here (and in [`OpKind`]).
//! 2. Extend [`crate::history::inverse::invert`] with its inverse
//!    recipe.
//!
//! The wire encoding lives in `payload_json` / `inverse_json` inside
//! the `SQLite` row — the enum here is the in-memory shape. Keeping
//! the wire format as JSON (rather than one column per field) lets
//! us add fields to an op without a schema migration; the cost is
//! that "find every rename that touched path X" needs a full table
//! scan plus JSON parsing. That's fine at the 50-entry retention.

use serde::{Deserialize, Serialize};

use crate::fs::ProjectPath;
use crate::meta::MetaDocument;

/// Stable identifier of a history entry. Backed by `SQLite`'s ROWID
/// so callers can round-trip freely without parsing.
pub type EntryId = i64;

/// One completed operation, as stored in the history log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: EntryId,
    /// ISO 8601 UTC timestamp. Stored as string for easy grep and
    /// because `SQLite` has no native UTC-normalizing datetime type.
    pub ts: String,
    /// Operation that was applied. Same shape that callers append.
    pub op: Operation,
    /// Pre-computed inverse. Stored rather than re-derived on read
    /// so an old entry's inverse semantics are pinned to whatever
    /// version of `core::history::inverse` was running when the op
    /// was recorded.
    pub inverse: Operation,
    /// `true` when [`crate::history::Store::undo`] has walked past
    /// this entry. The entry stays on disk until a new append pushes
    /// it off the redo branch or retention evicts it.
    pub consumed: bool,
    /// Optional user-facing grouping. A single `progest rename
    /// --bulk` or `progest import <dir>` emits many entries with
    /// the same `group_id`, so undo can roll them back as a unit.
    /// `None` for one-shot ops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

/// Which operation kind an entry represents. Lives separate from
/// [`Operation`] so store-level code can index / filter without
/// deserializing the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Rename,
    TagAdd,
    TagRemove,
    MetaEdit,
    Import,
}

impl OpKind {
    /// Canonical string used on the wire and in log lines. Kept
    /// `#[must_use]`-friendly so match arms don't need to `.as_str()`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rename => "rename",
            Self::TagAdd => "tag_add",
            Self::TagRemove => "tag_remove",
            Self::MetaEdit => "meta_edit",
            Self::Import => "import",
        }
    }
}

/// A single operation the history log can record.
///
/// Each variant holds the minimum information needed to *replay*
/// (or invert) the operation at the same layer that originally
/// produced it — the history log itself never touches `.meta` or
/// the filesystem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op_kind", rename_all = "snake_case")]
pub enum Operation {
    /// File rename. `from` → `to`. `rule_id` records which naming
    /// rule drove the change (if any), so undo UIs can explain why
    /// a rename is on the stack.
    Rename {
        from: ProjectPath,
        to: ProjectPath,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule_id: Option<String>,
    },
    /// Tag added to a file's `.meta`.
    TagAdd { path: ProjectPath, tag: String },
    /// Tag removed from a file's `.meta`.
    TagRemove { path: ProjectPath, tag: String },
    /// Full-document `.meta` swap. Storing before + after lets
    /// undo restore the whole sidecar without the caller having to
    /// remember which fields changed.
    MetaEdit {
        path: ProjectPath,
        before: Box<MetaDocument>,
        after: Box<MetaDocument>,
    },
    /// New file first seen by the project. `import` has no natural
    /// inverse on the filesystem side, so the inverse is synthesized
    /// as `Import` again with a matching `inverse = true` flag —
    /// undoers interpret that as "delete the file". See the
    /// `inverse` module.
    Import {
        path: ProjectPath,
        /// `false` for the forward op, `true` for the synthesized
        /// inverse. Kept on the variant so serializers don't need a
        /// parallel "reverse" wrapper.
        #[serde(default)]
        is_inverse: bool,
    },
}

impl Operation {
    #[must_use]
    pub fn kind(&self) -> OpKind {
        match self {
            Self::Rename { .. } => OpKind::Rename,
            Self::TagAdd { .. } => OpKind::TagAdd,
            Self::TagRemove { .. } => OpKind::TagRemove,
            Self::MetaEdit { .. } => OpKind::MetaEdit,
            Self::Import { .. } => OpKind::Import,
        }
    }
}

/// Builder used by [`crate::history::Store::append`].
///
/// The public API asks for an [`Operation`]; the store derives the
/// inverse and timestamps on its own so callers can't forget either
/// or fabricate a bogus inverse. `group_id` is the one knob the
/// caller owns.
#[derive(Debug, Clone, PartialEq)]
pub struct AppendRequest {
    pub op: Operation,
    pub group_id: Option<String>,
}

impl AppendRequest {
    #[must_use]
    pub fn new(op: Operation) -> Self {
        Self { op, group_id: None }
    }

    #[must_use]
    pub fn with_group(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }
}

/// Current moment in ISO 8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`). Split
/// out so tests can stub it via a feature of their own; for now the
/// store always calls the wall-clock variant.
pub(crate) fn now_iso8601() -> String {
    // `toml`'s `Datetime` renders in RFC 3339 form, which is an
    // ISO 8601 profile compatible with SQLite's `datetime('now')`
    // and with most log tooling. We convert from `SystemTime` via a
    // manual format so we don't drag chrono into `progest-core`.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Deliberately bypass leap-second adjustments: we only need
    // monotonic-ish UTC strings for human display.
    let days = now / 86_400;
    let secs_today = now % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = secs_today / 3600;
    let minute = (secs_today % 3600) / 60;
    let second = secs_today % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Howard Hinnant's civil_from_days — MIT/BSD, widely used:
// https://howardhinnant.github.io/date_algorithms.html
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn civil_from_days(days_since_epoch: u64) -> (i64, u64, u64) {
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{FileId, Fingerprint};

    fn meta_doc() -> MetaDocument {
        MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff"
                .parse::<Fingerprint>()
                .unwrap(),
        )
    }

    #[test]
    fn operation_kind_mapping_is_exhaustive() {
        let cases = [
            (
                Operation::Rename {
                    from: ProjectPath::new("a").unwrap(),
                    to: ProjectPath::new("b").unwrap(),
                    rule_id: None,
                },
                OpKind::Rename,
            ),
            (
                Operation::TagAdd {
                    path: ProjectPath::new("a").unwrap(),
                    tag: "x".into(),
                },
                OpKind::TagAdd,
            ),
            (
                Operation::TagRemove {
                    path: ProjectPath::new("a").unwrap(),
                    tag: "x".into(),
                },
                OpKind::TagRemove,
            ),
            (
                Operation::MetaEdit {
                    path: ProjectPath::new("a").unwrap(),
                    before: Box::new(meta_doc()),
                    after: Box::new(meta_doc()),
                },
                OpKind::MetaEdit,
            ),
            (
                Operation::Import {
                    path: ProjectPath::new("a").unwrap(),
                    is_inverse: false,
                },
                OpKind::Import,
            ),
        ];
        for (op, want) in cases {
            assert_eq!(op.kind(), want, "kind mapping for {op:?}");
        }
    }

    #[test]
    fn operation_round_trips_through_json() {
        let op = Operation::Rename {
            from: ProjectPath::new("assets/a.png").unwrap(),
            to: ProjectPath::new("assets/b.png").unwrap(),
            rule_id: Some("shot-assets-v1".into()),
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: Operation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn import_is_inverse_flag_round_trips() {
        let forward = Operation::Import {
            path: ProjectPath::new("a.png").unwrap(),
            is_inverse: false,
        };
        let back: Operation =
            serde_json::from_str(&serde_json::to_string(&forward).unwrap()).unwrap();
        assert_eq!(back, forward);

        let reverse = Operation::Import {
            path: ProjectPath::new("a.png").unwrap(),
            is_inverse: true,
        };
        let back: Operation =
            serde_json::from_str(&serde_json::to_string(&reverse).unwrap()).unwrap();
        assert_eq!(back, reverse);
    }

    #[test]
    fn append_request_builder_sets_group() {
        let op = Operation::TagAdd {
            path: ProjectPath::new("a").unwrap(),
            tag: "hero".into(),
        };
        let req = AppendRequest::new(op.clone()).with_group("bulk-1");
        assert_eq!(req.op, op);
        assert_eq!(req.group_id.as_deref(), Some("bulk-1"));
    }

    #[test]
    fn now_iso8601_has_expected_shape() {
        let s = now_iso8601();
        assert_eq!(s.len(), 20, "{s} — expected YYYY-MM-DDTHH:MM:SSZ");
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }
}
