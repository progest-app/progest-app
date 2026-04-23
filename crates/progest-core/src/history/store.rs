//! SQLite-backed history store.
//!
//! Exposes a [`Store`] trait plus the [`SqliteStore`] implementation
//! so reconcile / rename / CLI code can depend on `&dyn Store` and
//! tests can swap in a fake later. The on-disk layout is described
//! in `migrations/0001_initial.sql`.
//!
//! ## Pointer + consumed semantics
//!
//! The history log is append-only on disk, but presents an
//! undo/redo stack to callers:
//!
//! - The `meta` table's `pointer` row holds the id of the most
//!   recently applied, *not-yet-undone* entry.
//! - Undoing an entry flips its `consumed` flag and moves the
//!   pointer to the previous non-consumed entry (or clears it if
//!   the log is empty).
//! - Redoing flips the flag back on the next-newer consumed entry
//!   and re-advances the pointer.
//! - Any new append erases the redo branch: every `consumed = 1`
//!   row with id strictly greater than the current pointer is
//!   deleted before the new entry lands.
//!
//! Retention ([`RETENTION_LIMIT`]) is enforced at the tail on every
//! append. Deleting a row the pointer was aimed at is treated as
//! "the undo stack got out of reach"; the pointer is reconciled to
//! the latest surviving non-consumed entry (or cleared).

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

use super::error::HistoryError;
use super::inverse::invert;
use super::migration;
use super::types::{AppendRequest, Entry, EntryId, OpKind, Operation, now_iso8601};

/// Maximum entries kept on disk (REQUIREMENTS §3.4).
pub const RETENTION_LIMIT: usize = 50;

const POINTER_KEY: &str = "pointer";

/// Seam used by reconcile, rename, and CLI code to read and write
/// the history log.
pub trait Store: Send + Sync {
    /// Append a completed operation. The store synthesizes the
    /// inverse + timestamp and returns the persisted [`Entry`].
    fn append(&self, req: &AppendRequest) -> Result<Entry, HistoryError>;

    /// Snapshot entries newest-first, up to `limit` rows. Pass
    /// `usize::MAX` to get everything still on disk.
    fn list(&self, limit: usize) -> Result<Vec<Entry>, HistoryError>;

    /// Return the [`Entry`] at the current pointer, or `None` when
    /// the undo stack is empty.
    fn head(&self) -> Result<Option<Entry>, HistoryError>;

    /// Pop the top of the undo stack and return the operation to
    /// replay (the entry's `inverse`).
    fn undo(&self) -> Result<Entry, HistoryError>;

    /// Re-apply the most recently undone operation (the entry's
    /// `op` — see [`Entry::op`]).
    fn redo(&self) -> Result<Entry, HistoryError>;
}

/// SQLite-backed [`Store`].
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, HistoryError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(mut conn: Connection) -> Result<Self, HistoryError> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migration::apply(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with_conn<R>(&self, f: impl FnOnce(&mut Connection) -> R) -> R {
        let mut guard = self.conn.lock().expect("history connection mutex poisoned");
        f(&mut guard)
    }
}

impl Store for SqliteStore {
    fn append(&self, req: &AppendRequest) -> Result<Entry, HistoryError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let pointer = get_pointer(&tx)?;

            // Erase the redo branch: anything past the pointer that
            // had already been undone is now permanently gone.
            if let Some(pid) = pointer {
                tx.execute(
                    "DELETE FROM entries WHERE consumed = 1 AND id > ?1",
                    params![pid],
                )?;
            } else {
                // No pointer means no applied history; *everything*
                // consumed was already pushed off by prior undo.
                tx.execute("DELETE FROM entries WHERE consumed = 1", [])?;
            }

            // Synthesize and insert the new row.
            let inverse = invert(&req.op);
            let ts = now_iso8601();
            let payload_json =
                serde_json::to_string(&req.op).map_err(HistoryError::EncodePayload)?;
            let inverse_json =
                serde_json::to_string(&inverse).map_err(HistoryError::EncodePayload)?;
            let op_kind = req.op.kind().as_str();
            tx.execute(
                "INSERT INTO entries (ts, op_kind, payload_json, inverse_json, consumed, group_id) \
                 VALUES (?1, ?2, ?3, ?4, 0, ?5)",
                params![
                    ts,
                    op_kind,
                    payload_json,
                    inverse_json,
                    req.group_id.as_deref(),
                ],
            )?;
            let new_id = tx.last_insert_rowid();

            set_pointer(&tx, Some(new_id))?;
            enforce_retention(&tx)?;

            // Read back the row so callers always see the persisted
            // shape (including the server-assigned id and ts).
            let entry = read_entry_by_id(&tx, new_id)?.expect("entry vanished after insert");
            tx.commit()?;
            Ok(entry)
        })
    }

    fn list(&self, limit: usize) -> Result<Vec<Entry>, HistoryError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, ts, op_kind, payload_json, inverse_json, consumed, group_id \
                 FROM entries ORDER BY id DESC LIMIT ?1",
            )?;
            let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
            let rows = stmt.query_map(params![limit_i64], row_to_entry)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row??);
            }
            Ok(out)
        })
    }

    fn head(&self) -> Result<Option<Entry>, HistoryError> {
        self.with_conn(|conn| {
            let pointer = get_pointer(conn)?;
            match pointer {
                Some(id) => read_entry_by_id(conn, id),
                None => Ok(None),
            }
        })
    }

    fn undo(&self) -> Result<Entry, HistoryError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let Some(pointer_id) = get_pointer(&tx)? else {
                return Err(HistoryError::UndoEmpty);
            };
            let Some(target) = read_entry_by_id(&tx, pointer_id)? else {
                // Pointer stale (retention evicted it). Reconcile
                // by walking back to whatever survives.
                let fallback = latest_non_consumed_id(&tx)?;
                set_pointer(&tx, fallback)?;
                tx.commit()?;
                return Err(HistoryError::UndoEmpty);
            };

            tx.execute(
                "UPDATE entries SET consumed = 1 WHERE id = ?1",
                params![pointer_id],
            )?;
            let new_pointer = max_non_consumed_id_below(&tx, pointer_id)?;
            set_pointer(&tx, new_pointer)?;

            tx.commit()?;
            Ok(target)
        })
    }

    fn redo(&self) -> Result<Entry, HistoryError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let pointer = get_pointer(&tx)?;
            let next_id = next_consumed_id_above(&tx, pointer.unwrap_or(0))?;
            let Some(next_id) = next_id else {
                return Err(HistoryError::RedoEmpty);
            };
            let Some(entry) = read_entry_by_id(&tx, next_id)? else {
                return Err(HistoryError::RedoEmpty);
            };

            tx.execute(
                "UPDATE entries SET consumed = 0 WHERE id = ?1",
                params![next_id],
            )?;
            set_pointer(&tx, Some(next_id))?;

            tx.commit()?;
            Ok(entry)
        })
    }
}

// --- Helpers ---------------------------------------------------------------

fn get_pointer(conn: &Connection) -> Result<Option<EntryId>, HistoryError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![POINTER_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match raw {
        None => Ok(None),
        Some(s) => Ok(Some(s.parse::<EntryId>().unwrap_or(0))),
    }
}

fn set_pointer(conn: &Connection, value: Option<EntryId>) -> Result<(), HistoryError> {
    match value {
        Some(id) => {
            conn.execute(
                "INSERT INTO meta(key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![POINTER_KEY, id.to_string()],
            )?;
        }
        None => {
            conn.execute("DELETE FROM meta WHERE key = ?1", params![POINTER_KEY])?;
        }
    }
    Ok(())
}

fn enforce_retention(conn: &Connection) -> Result<(), HistoryError> {
    // Keep the newest RETENTION_LIMIT rows, independent of consumed
    // state. The cutoff id is the (RETENTION_LIMIT)-th newest id —
    // anything strictly smaller gets evicted.
    let offset = i64::try_from(RETENTION_LIMIT).unwrap_or(i64::MAX);
    let cutoff: Option<EntryId> = conn
        .query_row(
            "SELECT id FROM entries ORDER BY id DESC LIMIT 1 OFFSET ?1",
            params![offset - 1],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(cutoff) = cutoff {
        conn.execute("DELETE FROM entries WHERE id < ?1", params![cutoff])?;
        // Reconcile pointer if it pointed at an evicted row.
        let pointer = get_pointer(conn)?;
        if let Some(pid) = pointer {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE id = ?1 AND consumed = 0)",
                params![pid],
                |row| row.get(0),
            )?;
            if !exists {
                let fallback = latest_non_consumed_id(conn)?;
                set_pointer(conn, fallback)?;
            }
        }
    }
    Ok(())
}

fn latest_non_consumed_id(conn: &Connection) -> Result<Option<EntryId>, HistoryError> {
    Ok(conn
        .query_row(
            "SELECT id FROM entries WHERE consumed = 0 ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

fn max_non_consumed_id_below(
    conn: &Connection,
    id: EntryId,
) -> Result<Option<EntryId>, HistoryError> {
    Ok(conn
        .query_row(
            "SELECT id FROM entries WHERE consumed = 0 AND id < ?1 ORDER BY id DESC LIMIT 1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

fn next_consumed_id_above(conn: &Connection, id: EntryId) -> Result<Option<EntryId>, HistoryError> {
    Ok(conn
        .query_row(
            "SELECT id FROM entries WHERE consumed = 1 AND id > ?1 ORDER BY id ASC LIMIT 1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

fn read_entry_by_id(conn: &Connection, id: EntryId) -> Result<Option<Entry>, HistoryError> {
    conn.query_row(
        "SELECT id, ts, op_kind, payload_json, inverse_json, consumed, group_id \
         FROM entries WHERE id = ?1",
        params![id],
        row_to_entry,
    )
    .optional()?
    .transpose()
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Entry, HistoryError>> {
    let id: EntryId = row.get(0)?;
    let ts: String = row.get(1)?;
    let op_kind_raw: String = row.get(2)?;
    let payload_json: String = row.get(3)?;
    let inverse_json: String = row.get(4)?;
    let consumed_int: i64 = row.get(5)?;
    let group_id: Option<String> = row.get(6)?;

    // Lift the JSON / op_kind decoding out of the rusqlite Result
    // so the outer caller can surface HistoryError variants cleanly.
    let parsed = (|| -> Result<Entry, HistoryError> {
        let op_kind = parse_op_kind(&op_kind_raw)?;
        let op: Operation =
            serde_json::from_str(&payload_json).map_err(HistoryError::DecodePayload)?;
        let inverse: Operation =
            serde_json::from_str(&inverse_json).map_err(HistoryError::DecodePayload)?;
        // Sanity: on-disk op_kind must agree with the payload's
        // discriminant. Divergence here means a migration bug, not a
        // user-data issue.
        debug_assert_eq!(op.kind(), op_kind);
        Ok(Entry {
            id,
            ts,
            op,
            inverse,
            consumed: consumed_int != 0,
            group_id,
        })
    })();
    Ok(parsed)
}

fn parse_op_kind(raw: &str) -> Result<OpKind, HistoryError> {
    match raw {
        "rename" => Ok(OpKind::Rename),
        "tag_add" => Ok(OpKind::TagAdd),
        "tag_remove" => Ok(OpKind::TagRemove),
        "meta_edit" => Ok(OpKind::MetaEdit),
        "import" => Ok(OpKind::Import),
        other => Err(HistoryError::InvalidOpKind(other.to_owned())),
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

    fn store() -> SqliteStore {
        SqliteStore::open_in_memory().unwrap()
    }

    fn meta() -> MetaDocument {
        MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff"
                .parse::<Fingerprint>()
                .unwrap(),
        )
    }

    // --- append / list ---------------------------------------------------

    #[test]
    fn append_returns_entry_with_assigned_id_and_inverse() {
        let s = store();
        let e = s.append(&rename("a.png", "b.png")).unwrap();
        assert!(e.id > 0);
        assert!(!e.consumed);
        match &e.inverse {
            Operation::Rename { from, to, .. } => {
                assert_eq!(from, &p("b.png"));
                assert_eq!(to, &p("a.png"));
            }
            other => panic!("expected inverse rename, got {other:?}"),
        }
    }

    #[test]
    fn list_returns_newest_first() {
        let s = store();
        let e1 = s.append(&rename("a", "b")).unwrap();
        let e2 = s.append(&rename("b", "c")).unwrap();
        let list = s.list(10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, e2.id);
        assert_eq!(list[1].id, e1.id);
    }

    #[test]
    fn list_honors_limit() {
        let s = store();
        for i in 0..5 {
            s.append(&rename(&format!("a{i}"), &format!("b{i}")))
                .unwrap();
        }
        assert_eq!(s.list(3).unwrap().len(), 3);
    }

    #[test]
    fn head_returns_latest_applied_entry() {
        let s = store();
        assert!(s.head().unwrap().is_none());
        let e = s.append(&rename("a", "b")).unwrap();
        assert_eq!(s.head().unwrap().unwrap().id, e.id);
    }

    // --- undo / redo ------------------------------------------------------

    #[test]
    fn undo_returns_the_entry_at_pointer_and_marks_consumed() {
        let s = store();
        let e = s.append(&rename("a", "b")).unwrap();
        let popped = s.undo().unwrap();
        assert_eq!(popped.id, e.id);
        // The returned entry still reports its pre-undo state.
        assert!(!popped.consumed);
        // But the row is now marked consumed on disk.
        let list = s.list(10).unwrap();
        assert!(list[0].consumed);
        assert!(s.head().unwrap().is_none());
    }

    #[test]
    fn undo_empty_log_is_an_error() {
        let s = store();
        assert!(matches!(s.undo(), Err(HistoryError::UndoEmpty)));
    }

    #[test]
    fn redo_re_applies_most_recently_undone_entry() {
        let s = store();
        let e = s.append(&rename("a", "b")).unwrap();
        s.undo().unwrap();
        let redone = s.redo().unwrap();
        assert_eq!(redone.id, e.id);
        assert_eq!(s.head().unwrap().unwrap().id, e.id);
    }

    #[test]
    fn redo_empty_stack_is_an_error() {
        let s = store();
        s.append(&rename("a", "b")).unwrap();
        assert!(matches!(s.redo(), Err(HistoryError::RedoEmpty)));
    }

    #[test]
    fn new_append_erases_redo_branch() {
        let s = store();
        let _e1 = s.append(&rename("a", "b")).unwrap();
        let _e2 = s.append(&rename("b", "c")).unwrap();
        s.undo().unwrap(); // undoes e2
        // Append a new op — the e2 row must be gone and redo must
        // now fail.
        let e3 = s.append(&tag_add("a", "hero")).unwrap();
        let list = s.list(10).unwrap();
        // Expect 2 rows on disk: e1 + e3. e2 was erased.
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, e3.id);
        assert!(matches!(s.redo(), Err(HistoryError::RedoEmpty)));
    }

    #[test]
    fn multiple_undo_redo_walks_the_stack_in_order() {
        let s = store();
        let e1 = s.append(&rename("a", "b")).unwrap();
        let e2 = s.append(&rename("b", "c")).unwrap();
        let e3 = s.append(&rename("c", "d")).unwrap();

        assert_eq!(s.undo().unwrap().id, e3.id);
        assert_eq!(s.undo().unwrap().id, e2.id);
        assert_eq!(s.head().unwrap().unwrap().id, e1.id);

        assert_eq!(s.redo().unwrap().id, e2.id);
        assert_eq!(s.redo().unwrap().id, e3.id);
        assert!(matches!(s.redo(), Err(HistoryError::RedoEmpty)));
    }

    // --- meta_edit payload round-trip ------------------------------------

    #[test]
    fn meta_edit_round_trips_before_and_after_documents() {
        let s = store();
        let mut before = meta();
        before
            .custom
            .insert("scene".into(), toml::Value::Integer(10));
        let mut after = meta();
        after
            .custom
            .insert("scene".into(), toml::Value::Integer(20));

        let req = AppendRequest::new(Operation::MetaEdit {
            path: p("a"),
            before: Box::new(before.clone()),
            after: Box::new(after.clone()),
        });
        let e = s.append(&req).unwrap();
        match &e.op {
            Operation::MetaEdit {
                before: b,
                after: a,
                ..
            } => {
                assert_eq!(**b, before);
                assert_eq!(**a, after);
            }
            other => panic!("expected meta_edit, got {other:?}"),
        }
    }

    // --- group id --------------------------------------------------------

    #[test]
    fn append_preserves_group_id() {
        let s = store();
        let req = rename("a", "b").with_group("bulk-1");
        let e = s.append(&req).unwrap();
        assert_eq!(e.group_id.as_deref(), Some("bulk-1"));
    }

    // --- retention -------------------------------------------------------

    #[test]
    fn retention_caps_total_entries_at_the_limit() {
        let s = store();
        for i in 0..(u32::try_from(RETENTION_LIMIT).unwrap_or(u32::MAX) + 5) {
            s.append(&rename(&format!("a{i}"), &format!("b{i}")))
                .unwrap();
        }
        let list = s.list(usize::MAX).unwrap();
        assert_eq!(list.len(), RETENTION_LIMIT);
        // Oldest surviving entry should be the (5+1)-th append we made.
        let oldest_to = match &list.last().unwrap().op {
            Operation::Rename { to, .. } => to.clone(),
            _ => panic!(),
        };
        assert_eq!(oldest_to, p("b5"));
    }

    #[test]
    fn retention_reconciles_pointer_when_head_gets_evicted() {
        let s = store();
        // Append one entry, undo it so the pointer goes stale-ish,
        // then exceed retention so the undone entry evicts.
        s.append(&rename("a0", "b0")).unwrap();
        s.undo().unwrap();
        assert!(s.head().unwrap().is_none());
        for i in 1..=(u32::try_from(RETENTION_LIMIT).unwrap_or(u32::MAX) + 1) {
            s.append(&rename(&format!("a{i}"), &format!("b{i}")))
                .unwrap();
        }
        // Pointer should still be valid (points at the newest
        // applied entry), not lingering on an evicted id.
        assert!(s.head().unwrap().is_some());
        assert_eq!(s.list(usize::MAX).unwrap().len(), RETENTION_LIMIT);
    }
}
