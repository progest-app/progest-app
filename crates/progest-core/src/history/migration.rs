//! Schema migration runner for the history database.
//!
//! Mirrors `core::index::migration` line-for-line so the two stores
//! age the same way. Kept separate (rather than sharing a single
//! runner) so that a breaking change in one database doesn't drag
//! the other along — the two are independent even when colocated
//! inside `.progest/local/`.

use rusqlite::{Connection, params};
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: include_str!("migrations/0001_initial.sql"),
}];

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("sqlite error while migrating history: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "history migrations declared out of order: expected version {expected}, got {found} ({name})"
    )]
    OutOfOrder {
        expected: u32,
        found: u32,
        name: &'static str,
    },
}

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
            "applying history migration"
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

pub fn current_version(conn: &Connection) -> Result<u32, MigrationError> {
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

    #[test]
    fn fresh_database_starts_at_version_zero() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn apply_installs_entries_and_meta() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        for table in ["entries", "meta", "schema_version"] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing table {table}");
        }
        assert_eq!(current_version(&conn).unwrap(), 1);
    }

    #[test]
    fn apply_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
        let rows: usize = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, MIGRATIONS.len());
    }

    #[test]
    fn migrations_are_sequentially_versioned() {
        validate_order().unwrap();
    }
}
