//! Derive the inverse of an [`Operation`].
//!
//! `Operation::inverse` semantics:
//!
//! - `Rename { from, to }` → `Rename { from: to, to: from }`. Undo
//!   replays it at the rename layer, which handles FS + `.meta` +
//!   index atomicity the same way forward renames do.
//! - `TagAdd { path, tag }` ↔ `TagRemove { path, tag }`.
//! - `MetaEdit { path, before, after }` → `MetaEdit { path,
//!   before: after, after: before }`. The store keeps the full
//!   before/after document so undo doesn't need to diff.
//! - `Import { path, is_inverse: false }` →
//!   `Import { path, is_inverse: true }`. There's no "un-import" as a
//!   distinct op kind; undoers check the `is_inverse` flag and
//!   translate to a delete at their layer. Forward replay (redo) of
//!   the original forward import after an undo is the caller's
//!   responsibility — usually just re-running the original import.
//!
//! Keeping inverse derivation pure (no store, no IO) means we can
//! compute it before append, store it on the row, and trust it at
//! undo-time even if the inverse recipe later changes.

use super::types::Operation;

/// Produce the inverse of `op`.
///
/// Pure — safe to call on arbitrary inputs, including invariants
/// the store will reject (e.g. empty tag strings). The store owns
/// validation.
#[must_use]
pub fn invert(op: &Operation) -> Operation {
    match op {
        Operation::Rename { from, to, rule_id } => Operation::Rename {
            from: to.clone(),
            to: from.clone(),
            rule_id: rule_id.clone(),
        },
        Operation::TagAdd { path, tag } => Operation::TagRemove {
            path: path.clone(),
            tag: tag.clone(),
        },
        Operation::TagRemove { path, tag } => Operation::TagAdd {
            path: path.clone(),
            tag: tag.clone(),
        },
        Operation::MetaEdit {
            path,
            before,
            after,
        } => Operation::MetaEdit {
            path: path.clone(),
            before: after.clone(),
            after: before.clone(),
        },
        Operation::Import {
            path,
            is_inverse: false,
        } => Operation::Import {
            path: path.clone(),
            is_inverse: true,
        },
        // Inverting a reverse-import produces a forward-import
        // again, so redo works without special-casing.
        Operation::Import {
            path,
            is_inverse: true,
        } => Operation::Import {
            path: path.clone(),
            is_inverse: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::ProjectPath;
    use crate::identity::{FileId, Fingerprint};
    use crate::meta::MetaDocument;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn meta() -> MetaDocument {
        MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff"
                .parse::<Fingerprint>()
                .unwrap(),
        )
    }

    #[test]
    fn rename_flips_from_and_to() {
        let op = Operation::Rename {
            from: p("a.png"),
            to: p("b.png"),
            rule_id: Some("shot-v1".into()),
        };
        let inv = invert(&op);
        assert_eq!(
            inv,
            Operation::Rename {
                from: p("b.png"),
                to: p("a.png"),
                rule_id: Some("shot-v1".into()),
            }
        );
    }

    #[test]
    fn tag_pair_inverts_across_add_and_remove() {
        let add = Operation::TagAdd {
            path: p("a"),
            tag: "hero".into(),
        };
        let rem = Operation::TagRemove {
            path: p("a"),
            tag: "hero".into(),
        };
        assert_eq!(invert(&add), rem);
        assert_eq!(invert(&rem), add);
    }

    #[test]
    fn meta_edit_swaps_before_and_after() {
        let mut before = meta();
        before
            .custom
            .insert("scene".into(), toml::Value::Integer(10));
        let mut after = meta();
        after
            .custom
            .insert("scene".into(), toml::Value::Integer(20));

        let op = Operation::MetaEdit {
            path: p("a"),
            before: Box::new(before.clone()),
            after: Box::new(after.clone()),
        };
        let inv = invert(&op);
        match inv {
            Operation::MetaEdit {
                before: inv_before,
                after: inv_after,
                ..
            } => {
                assert_eq!(inv_before, Box::new(after));
                assert_eq!(inv_after, Box::new(before));
            }
            other => panic!("expected MetaEdit, got {other:?}"),
        }
    }

    #[test]
    fn import_toggles_is_inverse_flag() {
        let fwd = Operation::Import {
            path: p("new.png"),
            is_inverse: false,
        };
        let inv = invert(&fwd);
        assert_eq!(
            inv,
            Operation::Import {
                path: p("new.png"),
                is_inverse: true,
            }
        );
        // Double inversion must round-trip so redo works.
        assert_eq!(invert(&inv), fwd);
    }

    #[test]
    fn double_inverse_is_identity_for_all_variants() {
        let ops = [
            Operation::Rename {
                from: p("a"),
                to: p("b"),
                rule_id: None,
            },
            Operation::TagAdd {
                path: p("a"),
                tag: "x".into(),
            },
            Operation::TagRemove {
                path: p("a"),
                tag: "x".into(),
            },
            Operation::MetaEdit {
                path: p("a"),
                before: Box::new(meta()),
                after: Box::new(meta()),
            },
            Operation::Import {
                path: p("a"),
                is_inverse: false,
            },
        ];
        for op in ops {
            assert_eq!(
                invert(&invert(&op)),
                op,
                "double-invert identity for {op:?}"
            );
        }
    }
}
