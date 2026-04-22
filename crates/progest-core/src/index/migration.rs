//! Schema migration runner for the `SQLite` index.
//!
//! Migrations are numbered SQL files embedded via [`include_str!`]. On every
//! [`apply`] call the runner:
//!
//! 1. Creates the `schema_version` tracking table if missing.
//! 2. Looks up the highest version already applied.
//! 3. Executes each migration whose version exceeds that watermark inside a
//!    transaction, recording the row in `schema_version` on success.
//!
//! Keeping the tracking table schema-owned (rather than using `SQLite`'s
//! built-in `PRAGMA user_version`) lets us keep `applied_at` and the
//! migration name for troubleshooting — a user who hits a broken migration
//! in the wild needs to know which one it was without cross-referencing a
//! version-to-name table from source.
//!
//! The runner is deliberately minimal: no down-migrations, no checksum
//! verification of already-applied SQL. Both are cheap to add later if a
//! concrete need arises.

use rusqlite::{Connection, params};
use thiserror::Error;
use tracing::debug;

/// A single embedded migration step.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Monotonically increasing version. Gaps are not permitted.
    pub version: u32,
    /// Short identifier used in the `schema_version` table and log lines.
    pub name: &'static str,
    /// SQL body, executed via [`Connection::execute_batch`].
    pub sql: &'static str,
}

/// All migrations known to this build, in ascending version order.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: include_str!("migrations/0001_initial.sql"),
}];

/// Errors surfaced by [`apply`].
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("sqlite error while migrating: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migrations declared out of order: expected version {expected}, got {found} ({name})")]
    OutOfOrder {
        expected: u32,
        found: u32,
        name: &'static str,
    },
}

/// Run every pending migration in [`MIGRATIONS`] against `conn`.
///
/// Already-applied migrations are skipped, so the call is safe to run on
/// every index open. Each pending migration executes in its own transaction
/// so that a partial failure leaves the database in the previous known-good
/// state.
pub fn apply(conn: &mut Connection) -> Result<(), MigrationError> {
    ensure_schema_version_table(conn)?;
    validate_order()?;

    let applied: u32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get::<_, u32>(0),
    )?;

    for migration in MIGRATIONS {
        if migration.version <= applied {
            continue;
        }
        debug!(
            version = migration.version,
            name = migration.name,
            "applying migration"
        );
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_version (version, name, applied_at) \
             VALUES (?1, ?2, datetime('now'))",
            params![migration.version, migration.name],
        )?;
        tx.commit()?;
    }
    Ok(())
}

/// Current schema version after [`apply`] (0 when no migrations have run).
pub fn current_version(conn: &Connection) -> Result<u32, MigrationError> {
    // The table may not exist yet on a brand-new database; treat that as 0.
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Ok(0);
    }
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get::<_, u32>(0),
    )?)
}

fn ensure_schema_version_table(conn: &Connection) -> Result<(), MigrationError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (\
             version    INTEGER PRIMARY KEY,\
             name       TEXT NOT NULL,\
             applied_at TEXT NOT NULL\
         )",
    )?;
    Ok(())
}

fn validate_order() -> Result<(), MigrationError> {
    for (expected, migration) in (1_u32..).zip(MIGRATIONS.iter()) {
        if migration.version != expected {
            return Err(MigrationError::OutOfOrder {
                expected,
                found: migration.version,
                name: migration.name,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            params![name],
            |row| row.get::<_, bool>(0),
        )
        .unwrap()
    }

    fn index_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
            params![name],
            |row| row.get::<_, bool>(0),
        )
        .unwrap()
    }

    #[test]
    fn fresh_database_starts_at_version_zero() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn apply_installs_initial_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();

        for table in ["files", "tags", "schema_version"] {
            assert!(table_exists(&conn, table), "missing table {table}");
        }
        for index in ["idx_files_path", "idx_files_fingerprint", "idx_tags_tag"] {
            assert!(index_exists(&conn, index), "missing index {index}");
        }
        assert_eq!(current_version(&conn).unwrap(), 1);
    }

    #[test]
    fn apply_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
        // Exactly one row in schema_version per migration.
        let rows: usize = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, MIGRATIONS.len());
    }

    #[test]
    fn schema_version_row_records_name_and_timestamp() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        let (name, applied_at): (String, String) = conn
            .query_row(
                "SELECT name, applied_at FROM schema_version WHERE version = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "initial");
        // SQLite's datetime('now') yields "YYYY-MM-DD HH:MM:SS"; sanity-check
        // that it's a non-empty string rather than pinning the format.
        assert!(!applied_at.is_empty());
    }

    #[test]
    fn migrations_are_sequentially_versioned() {
        // Catches accidental gaps like 1, 3 — the runner relies on contiguous
        // versions to compute the next-to-apply watermark correctly.
        validate_order().unwrap();
    }
}
