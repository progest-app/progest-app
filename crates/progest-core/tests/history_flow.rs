//! End-to-end tests for `core::history`.
//!
//! The unit tests on [`progest_core::history::store`] cover each
//! API in isolation. This file exercises cross-module scenarios
//! that matter for the future `core::rename` / CLI wiring:
//!
//! - Mixing op kinds on the same stack.
//! - A realistic undo-redo-append sequence that mirrors how a UI
//!   would drive the store.
//! - Persistence across a [`SqliteStore`] reopen, proving the
//!   pointer survives process restart.
//! - `group_id` lets callers find every row belonging to a bulk
//!   operation with a single SQL pass.

mod support;

use progest_core::history::{AppendRequest, Operation, SqliteStore, Store};
use progest_core::identity::{FileId, Fingerprint};
use progest_core::meta::MetaDocument;
use tempfile::TempDir;

use support::p;

fn rename(from: &str, to: &str) -> AppendRequest {
    AppendRequest::new(Operation::Rename {
        from: p(from),
        to: p(to),
        rule_id: None,
    })
}

fn tag_add(path: &str, tag: &str) -> AppendRequest {
    AppendRequest::new(Operation::TagAdd {
        path: p(path),
        tag: tag.into(),
    })
}

fn meta_edit(path: &str, before_scene: i64, after_scene: i64) -> AppendRequest {
    let mut before = MetaDocument::new(
        FileId::new_v7(),
        "blake3:00112233445566778899aabbccddeeff"
            .parse::<Fingerprint>()
            .unwrap(),
    );
    before
        .custom
        .insert("scene".into(), toml::Value::Integer(before_scene));
    let mut after = before.clone();
    after
        .custom
        .insert("scene".into(), toml::Value::Integer(after_scene));
    AppendRequest::new(Operation::MetaEdit {
        path: p(path),
        before: Box::new(before),
        after: Box::new(after),
    })
}

fn import(path: &str) -> AppendRequest {
    AppendRequest::new(Operation::Import {
        path: p(path),
        is_inverse: false,
    })
}

// --- Scenarios --------------------------------------------------------------

#[test]
fn mixed_op_kinds_coexist_on_the_stack() {
    let s = SqliteStore::open_in_memory().unwrap();
    let e_r = s.append(&rename("a.png", "b.png")).unwrap();
    let e_t = s.append(&tag_add("b.png", "hero")).unwrap();
    let e_m = s.append(&meta_edit("b.png", 10, 20)).unwrap();
    let e_i = s.append(&import("c.png")).unwrap();

    let list = s.list(10).unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(
        list.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![e_i.id, e_m.id, e_t.id, e_r.id]
    );
    // Each entry carries its own kind + inverse kind (rename→rename,
    // tag_add→tag_remove, etc.). Sanity check the two most distinct.
    assert!(matches!(&e_t.inverse, Operation::TagRemove { .. }));
    assert!(matches!(
        &e_i.inverse,
        Operation::Import {
            is_inverse: true,
            ..
        }
    ));
}

#[test]
fn undo_then_redo_then_new_append_drops_redo_branch() {
    let s = SqliteStore::open_in_memory().unwrap();
    let e1 = s.append(&rename("a", "b")).unwrap();
    let e2 = s.append(&rename("b", "c")).unwrap();
    let e3 = s.append(&rename("c", "d")).unwrap();

    // User clicks undo twice — e3 then e2 end up on the redo stack.
    assert_eq!(s.undo().unwrap().id, e3.id);
    assert_eq!(s.undo().unwrap().id, e2.id);
    assert_eq!(s.head().unwrap().unwrap().id, e1.id);

    // Redo e2 once — e3 is still redoable.
    assert_eq!(s.redo().unwrap().id, e2.id);

    // New forward op lands — the redo branch (just e3) must be gone.
    let e4 = s.append(&tag_add("a", "hero")).unwrap();
    let list = s.list(10).unwrap();
    let ids: Vec<_> = list.iter().map(|e| e.id).collect();
    assert!(
        !ids.contains(&e3.id),
        "e3 should have been erased; got {ids:?}"
    );
    assert_eq!(ids, vec![e4.id, e2.id, e1.id]);
    assert!(s.redo().is_err());
}

#[test]
fn pointer_survives_store_reopen() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("history.db");

    let (e1_id, e2_id) = {
        let s = SqliteStore::open(&db_path).unwrap();
        let e1 = s.append(&rename("a", "b")).unwrap();
        let e2 = s.append(&rename("b", "c")).unwrap();
        s.undo().unwrap(); // e2 → redo stack
        (e1.id, e2.id)
    };

    // Reopen. The pointer should still be at e1, and the redo stack
    // should still hold e2.
    let s = SqliteStore::open(&db_path).unwrap();
    assert_eq!(s.head().unwrap().unwrap().id, e1_id);
    let redone = s.redo().unwrap();
    assert_eq!(redone.id, e2_id);
}

#[test]
fn group_id_threads_through_bulk_operation() {
    let s = SqliteStore::open_in_memory().unwrap();
    let _a = s.append(&rename("a0", "b0").with_group("bulk-1")).unwrap();
    let _b = s.append(&rename("a1", "b1").with_group("bulk-1")).unwrap();
    let _c = s.append(&rename("a2", "b2").with_group("bulk-1")).unwrap();
    // Unrelated op with no group.
    let _solo = s.append(&tag_add("x", "hero")).unwrap();

    let bulk: Vec<_> = s
        .list(usize::MAX)
        .unwrap()
        .into_iter()
        .filter(|e| e.group_id.as_deref() == Some("bulk-1"))
        .collect();
    assert_eq!(bulk.len(), 3);
}

#[test]
fn meta_edit_round_trips_across_reopen_with_full_custom_map() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("history.db");

    let original_id = {
        let s = SqliteStore::open(&db_path).unwrap();
        s.append(&meta_edit("a", 10, 20)).unwrap().id
    };

    let s = SqliteStore::open(&db_path).unwrap();
    let list = s.list(10).unwrap();
    assert_eq!(list.len(), 1);
    let entry = &list[0];
    assert_eq!(entry.id, original_id);
    match &entry.op {
        Operation::MetaEdit { before, after, .. } => {
            assert_eq!(
                before.custom.get("scene").and_then(toml::Value::as_integer),
                Some(10)
            );
            assert_eq!(
                after.custom.get("scene").and_then(toml::Value::as_integer),
                Some(20)
            );
        }
        other => panic!("expected MetaEdit, got {other:?}"),
    }
    match &entry.inverse {
        Operation::MetaEdit { before, after, .. } => {
            // Inverse swaps before/after.
            assert_eq!(
                before.custom.get("scene").and_then(toml::Value::as_integer),
                Some(20)
            );
            assert_eq!(
                after.custom.get("scene").and_then(toml::Value::as_integer),
                Some(10)
            );
        }
        other => panic!("expected MetaEdit inverse, got {other:?}"),
    }
}
