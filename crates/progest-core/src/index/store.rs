//! `SqliteIndex` — the on-disk implementation of [`Index`].
//!
//! The trait lives alongside the implementation because both are meaningful
//! only in conjunction: downstream modules (reconcile, rename, doctor) hold
//! an `&dyn Index` and let tests slot in either the real backend or a future
//! in-memory fake.
//!
//! Translation between domain types ([`FileRow`], [`Kind`], [`Status`]) and
//! `rusqlite` rows is kept local to this module so that the identity and meta
//! modules don't pick up a `rusqlite` dependency.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

use crate::fs::ProjectPath;
use crate::identity::{FileId, Fingerprint};
use crate::meta::{Kind, Status};

use super::error::IndexError;
use super::migration;
use super::row::FileRow;

/// Seam used by reconcile, rename, and doctor to read and write the index.
///
/// All methods take `&self`: writers synchronize through interior mutability
/// so the trait composes the same way as [`crate::fs::FileSystem`] and
/// [`crate::meta::MetaStore`].
pub trait Index: Send + Sync {
    /// Insert `row`, or replace any existing row with the same `file_id`
    /// **or** the same `path`. Reconcile relies on both keys collapsing to
    /// a single row so that neither a renamed file nor a re-imported
    /// fingerprint leaves a stale duplicate behind.
    fn upsert_file(&self, row: &FileRow) -> Result<(), IndexError>;

    /// Look up a row by its stable identifier.
    fn get_file(&self, file_id: &FileId) -> Result<Option<FileRow>, IndexError>;

    /// Look up a row by its project-root-relative path.
    fn get_file_by_path(&self, path: &ProjectPath) -> Result<Option<FileRow>, IndexError>;

    /// Remove the row keyed by `file_id` along with its dependent tag rows
    /// (cascaded by the schema).
    fn delete_file(&self, file_id: &FileId) -> Result<(), IndexError>;

    /// Snapshot every row, ordered by path for stable diff-friendly output.
    fn list_files(&self) -> Result<Vec<FileRow>, IndexError>;
}

/// `SQLite`-backed [`Index`] implementation.
///
/// Construct via [`SqliteIndex::open`] for an on-disk database or
/// [`SqliteIndex::open_in_memory`] for tests. Both variants apply pending
/// migrations and enable the foreign-key pragma before returning.
///
/// The [`rusqlite::Connection`] is wrapped in a [`Mutex`] so that the type
/// is both [`Send`] and [`Sync`] — rusqlite connections are `Send` but not
/// `Sync`, which would otherwise force every caller into an external lock.
/// Inside a single process the contention is negligible compared to sqlite's
/// own internal locking.
pub struct SqliteIndex {
    conn: Mutex<Connection>,
}

impl SqliteIndex {
    /// Open (or create) the database at `path` and bring the schema up to
    /// the current version.
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Open an in-memory database — primarily useful for unit and integration
    /// tests that don't want to touch the filesystem.
    pub fn open_in_memory() -> Result<Self, IndexError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(mut conn: Connection) -> Result<Self, IndexError> {
        // Foreign keys are off by default per connection in SQLite; turning
        // them on is what makes the `tags` cascade actually cascade.
        conn.pragma_update(None, "foreign_keys", true)?;
        migration::apply(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = self.conn.lock().expect("index connection mutex poisoned");
        f(&guard)
    }
}

const UPSERT_SQL: &str = "\
INSERT INTO files ( \
    file_id, path, fingerprint, source_file_id, \
    kind, status, size, mtime, created_at, last_seen_at \
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
ON CONFLICT(file_id) DO UPDATE SET \
    path            = excluded.path, \
    fingerprint     = excluded.fingerprint, \
    source_file_id  = excluded.source_file_id, \
    kind            = excluded.kind, \
    status          = excluded.status, \
    size            = excluded.size, \
    mtime           = excluded.mtime, \
    created_at      = excluded.created_at, \
    last_seen_at    = excluded.last_seen_at \
ON CONFLICT(path) DO UPDATE SET \
    file_id         = excluded.file_id, \
    fingerprint     = excluded.fingerprint, \
    source_file_id  = excluded.source_file_id, \
    kind            = excluded.kind, \
    status          = excluded.status, \
    size            = excluded.size, \
    mtime           = excluded.mtime, \
    created_at      = excluded.created_at, \
    last_seen_at    = excluded.last_seen_at";

const SELECT_COLUMNS: &str = "file_id, path, fingerprint, source_file_id, kind, status, size, mtime, created_at, last_seen_at";

impl Index for SqliteIndex {
    fn upsert_file(&self, row: &FileRow) -> Result<(), IndexError> {
        self.with_conn(|conn| {
            conn.execute(
                UPSERT_SQL,
                params![
                    row.file_id.to_string(),
                    row.path.as_str(),
                    row.fingerprint.to_string(),
                    row.source_file_id.as_ref().map(ToString::to_string),
                    kind_to_str(row.kind),
                    status_to_str(row.status),
                    row.size,
                    row.mtime,
                    row.created_at.as_deref(),
                    row.last_seen_at.as_deref(),
                ],
            )?;
            Ok(())
        })
    }

    fn get_file(&self, file_id: &FileId) -> Result<Option<FileRow>, IndexError> {
        self.with_conn(|conn| {
            let query = format!("SELECT {SELECT_COLUMNS} FROM files WHERE file_id = ?1");
            conn.query_row(&query, params![file_id.to_string()], row_from_sqlite)
                .optional()?
                .transpose()
        })
    }

    fn get_file_by_path(&self, path: &ProjectPath) -> Result<Option<FileRow>, IndexError> {
        self.with_conn(|conn| {
            let query = format!("SELECT {SELECT_COLUMNS} FROM files WHERE path = ?1");
            conn.query_row(&query, params![path.as_str()], row_from_sqlite)
                .optional()?
                .transpose()
        })
    }

    fn delete_file(&self, file_id: &FileId) -> Result<(), IndexError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM files WHERE file_id = ?1",
                params![file_id.to_string()],
            )?;
            Ok(())
        })
    }

    fn list_files(&self) -> Result<Vec<FileRow>, IndexError> {
        self.with_conn(|conn| {
            let query = format!("SELECT {SELECT_COLUMNS} FROM files ORDER BY path");
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map([], row_from_sqlite)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row??);
            }
            Ok(out)
        })
    }
}

/// Map a raw `rusqlite` row into an in-memory [`FileRow`].
///
/// Returned as `Result<Result<...>>` so that a `rusqlite` column-read error
/// and a domain-level parse error remain distinguishable — the call site
/// collapses both arms into [`IndexError`].
fn row_from_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<FileRow, IndexError>> {
    let file_id_raw: String = row.get("file_id")?;
    let path_raw: String = row.get("path")?;
    let fingerprint_raw: String = row.get("fingerprint")?;
    let source_file_id_raw: Option<String> = row.get("source_file_id")?;
    let kind_raw: String = row.get("kind")?;
    let status_raw: String = row.get("status")?;
    let size: Option<i64> = row.get("size")?;
    let mtime: Option<i64> = row.get("mtime")?;
    let created_at: Option<String> = row.get("created_at")?;
    let last_seen_at: Option<String> = row.get("last_seen_at")?;

    Ok(build_row(
        &file_id_raw,
        &path_raw,
        &fingerprint_raw,
        source_file_id_raw.as_deref(),
        &kind_raw,
        &status_raw,
        size,
        mtime,
        created_at,
        last_seen_at,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_row(
    file_id_raw: &str,
    path_raw: &str,
    fingerprint_raw: &str,
    source_file_id_raw: Option<&str>,
    kind_raw: &str,
    status_raw: &str,
    size: Option<i64>,
    mtime: Option<i64>,
    created_at: Option<String>,
    last_seen_at: Option<String>,
) -> Result<FileRow, IndexError> {
    let file_id: FileId = file_id_raw.parse()?;
    let path = ProjectPath::new(path_raw)?;
    let fingerprint: Fingerprint = fingerprint_raw.parse()?;
    let source_file_id = source_file_id_raw.map(str::parse::<FileId>).transpose()?;
    let kind = kind_from_str(kind_raw)?;
    let status = status_from_str(status_raw)?;
    // SQLite stores INTEGER as i64; negative sizes are nonsensical but we
    // defensively coerce rather than losing the row.
    let size = size.and_then(|s| u64::try_from(s).ok());
    Ok(FileRow {
        file_id,
        path,
        fingerprint,
        source_file_id,
        kind,
        status,
        size,
        mtime,
        created_at,
        last_seen_at,
    })
}

fn kind_to_str(k: Kind) -> &'static str {
    match k {
        Kind::Asset => "asset",
        Kind::Directory => "directory",
        Kind::Derived => "derived",
    }
}

fn kind_from_str(s: &str) -> Result<Kind, IndexError> {
    match s {
        "asset" => Ok(Kind::Asset),
        "directory" => Ok(Kind::Directory),
        "derived" => Ok(Kind::Derived),
        other => Err(IndexError::InvalidKind(other.to_string())),
    }
}

fn status_to_str(s: Status) -> &'static str {
    match s {
        Status::Active => "active",
        Status::Archived => "archived",
        Status::Deprecated => "deprecated",
    }
}

fn status_from_str(s: &str) -> Result<Status, IndexError> {
    match s {
        "active" => Ok(Status::Active),
        "archived" => Ok(Status::Archived),
        "deprecated" => Ok(Status::Deprecated),
        other => Err(IndexError::InvalidStatus(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> FileRow {
        FileRow {
            file_id: FileId::new_v7(),
            path: ProjectPath::new("assets/hero.psd").unwrap(),
            fingerprint: "blake3:00112233445566778899aabbccddeeff".parse().unwrap(),
            source_file_id: None,
            kind: Kind::Asset,
            status: Status::Active,
            size: Some(2048),
            mtime: Some(1_713_600_000),
            created_at: Some("2026-04-20T10:00:00Z".into()),
            last_seen_at: Some("2026-04-21T08:00:00Z".into()),
        }
    }

    #[test]
    fn upsert_then_get_round_trips_every_field() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        let row = sample_row();
        idx.upsert_file(&row).unwrap();
        let reloaded = idx.get_file(&row.file_id).unwrap().unwrap();
        assert_eq!(reloaded, row);
    }

    #[test]
    fn get_by_path_returns_matching_row() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        let row = sample_row();
        idx.upsert_file(&row).unwrap();
        let reloaded = idx.get_file_by_path(&row.path).unwrap().unwrap();
        assert_eq!(reloaded.file_id, row.file_id);
    }

    #[test]
    fn get_missing_returns_none() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        let id = FileId::new_v7();
        assert!(idx.get_file(&id).unwrap().is_none());
        let p = ProjectPath::new("nope.psd").unwrap();
        assert!(idx.get_file_by_path(&p).unwrap().is_none());
    }

    #[test]
    fn upsert_replaces_row_with_same_file_id() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        let mut row = sample_row();
        idx.upsert_file(&row).unwrap();
        // Simulate a rename: same file_id, new path.
        row.path = ProjectPath::new("assets/hero_v2.psd").unwrap();
        idx.upsert_file(&row).unwrap();

        let reloaded = idx.get_file(&row.file_id).unwrap().unwrap();
        assert_eq!(reloaded.path.as_str(), "assets/hero_v2.psd");
        let all = idx.list_files().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn upsert_replaces_row_with_same_path_but_new_file_id() {
        // Covers the copy-detection flow: the user re-imported a file at an
        // existing path and the importer generated a fresh file_id.
        let idx = SqliteIndex::open_in_memory().unwrap();
        let original = sample_row();
        idx.upsert_file(&original).unwrap();

        let mut replacement = sample_row();
        replacement.path = original.path.clone();
        replacement.source_file_id = Some(original.file_id);
        idx.upsert_file(&replacement).unwrap();

        // The old row is gone, the replacement owns the path, and the two
        // file_ids are different so we haven't accidentally mutated the
        // identity of the original document.
        assert!(idx.get_file(&original.file_id).unwrap().is_none());
        let reloaded = idx.get_file(&replacement.file_id).unwrap().unwrap();
        assert_eq!(reloaded.path, original.path);
        assert_eq!(reloaded.source_file_id, Some(original.file_id));
    }

    #[test]
    fn delete_file_removes_the_row() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        let row = sample_row();
        idx.upsert_file(&row).unwrap();
        idx.delete_file(&row.file_id).unwrap();
        assert!(idx.get_file(&row.file_id).unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_is_a_noop() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        // A reconcile that races with an external unlink must not error.
        idx.delete_file(&FileId::new_v7()).unwrap();
    }

    #[test]
    fn list_files_is_ordered_by_path() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        for path in ["shots/s020/c001.mov", "assets/hero.psd", "notes.md"] {
            let mut r = sample_row();
            r.file_id = FileId::new_v7();
            r.path = ProjectPath::new(path).unwrap();
            idx.upsert_file(&r).unwrap();
        }
        let all = idx.list_files().unwrap();
        let paths: Vec<&str> = all.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["assets/hero.psd", "notes.md", "shots/s020/c001.mov"]
        );
    }

    #[test]
    fn unknown_kind_in_db_surfaces_as_error() {
        let idx = SqliteIndex::open_in_memory().unwrap();
        idx.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (file_id, path, fingerprint, kind, status) \
                 VALUES ('0190f3d7-5dbc-7abc-8000-0123456789ab', 'weird.psd', \
                         'blake3:00112233445566778899aabbccddeeff', 'glork', 'active')",
                [],
            )
            .unwrap();
        });
        let p = ProjectPath::new("weird.psd").unwrap();
        let err = idx.get_file_by_path(&p).unwrap_err();
        assert!(matches!(err, IndexError::InvalidKind(_)));
    }
}
