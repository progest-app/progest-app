//! Search DSL executor (`docs/SEARCH_DSL.md` §7).
//!
//! Runs a [`PlannedQuery`] against a `SQLite` connection that has the
//! M3 search migration applied (`core::index` migration 0002). Returns
//! a deterministic list of [`SearchHit`]s — minimal `(file_id, path)`
//! pairs.
//!
//! [`project_hits`] joins each hit back to the rich shape declared in
//! `docs/SEARCH_DSL.md` §8.1 (tags / violations / custom fields).
//! CLI and Tauri IPC both call it for serialized output.

use rusqlite::{Connection, ToSql, types::ToSqlOutput, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use crate::identity::FileId;
use crate::index::{
    CustomFieldEntry, CustomFieldValue, Index, IndexError, RichRow, ViolationCounts,
};

use super::plan::{BindValue, PlannedQuery};

/// One row returned by [`execute`]. Minimal info to identify the hit;
/// callers can re-fetch additional columns if they need them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub file_id: String,
    pub path: String,
}

/// Executor error.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    #[error("sqlite error while executing search: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("index error while projecting search hits: {0}")]
    Index(#[from] IndexError),
    #[error("malformed file_id in row: {0}")]
    MalformedFileId(String),
}

/// Rich search hit shape — mirrors `docs/SEARCH_DSL.md` §8.1.
///
/// Returned by [`project_hits`] after augmenting the minimal
/// [`SearchHit`] with tags, violation summary, and custom fields via
/// the [`Index`] API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichSearchHit {
    pub file_id: String,
    pub path: String,
    pub name: Option<String>,
    pub kind: String,
    pub ext: Option<String>,
    pub tags: Vec<String>,
    pub violations: RichViolationCounts,
    pub custom_fields: Vec<RichCustomField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RichViolationCounts {
    pub naming: u32,
    pub placement: u32,
    pub sequence: u32,
}

impl From<ViolationCounts> for RichViolationCounts {
    fn from(c: ViolationCounts) -> Self {
        Self {
            naming: c.naming,
            placement: c.placement,
            sequence: c.sequence,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichCustomField {
    pub key: String,
    #[serde(flatten)]
    pub value: RichCustomValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum RichCustomValue {
    Text(String),
    Integer(i64),
}

impl From<CustomFieldValue> for RichCustomValue {
    fn from(v: CustomFieldValue) -> Self {
        match v {
            CustomFieldValue::Text(s) => RichCustomValue::Text(s),
            CustomFieldValue::Integer(n) => RichCustomValue::Integer(n),
        }
    }
}

impl From<CustomFieldEntry> for RichCustomField {
    fn from(e: CustomFieldEntry) -> Self {
        RichCustomField {
            key: e.key,
            value: e.value.into(),
        }
    }
}

impl From<RichRow> for RichSearchHit {
    fn from(r: RichRow) -> Self {
        RichSearchHit {
            file_id: r.file_id,
            path: r.path,
            name: r.name,
            kind: r.kind,
            ext: r.ext,
            tags: r.tags,
            violations: r.violations.into(),
            custom_fields: r.custom_fields.into_iter().map(Into::into).collect(),
        }
    }
}

impl ToSql for BindValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            BindValue::Text(s) => ToSqlOutput::Borrowed(s.as_str().into()),
            BindValue::Integer(n) => ToSqlOutput::Owned(SqlValue::Integer(*n)),
        })
    }
}

/// Run a planned query and collect hits in plan order.
///
/// The planner already appends `ORDER BY f.path ASC, f.file_id ASC`,
/// so the result is fully deterministic for a given (DB state,
/// query) pair.
///
/// Note: the planned SQL only `SELECT`s `f.file_id`. The executor
/// joins back to `files` for `path` because callers need it for
/// every UI surface. The cost is one extra index lookup per row.
pub fn execute(conn: &Connection, planned: &PlannedQuery) -> Result<Vec<SearchHit>, ExecuteError> {
    // Wrap the planner SQL in an outer SELECT that joins back to
    // files for the path column. Avoids forcing the planner to know
    // about projection.
    let wrapped = format!(
        "SELECT q.file_id, f.path FROM ({inner}) AS q \
         JOIN files f ON f.file_id = q.file_id \
         ORDER BY f.path ASC, f.file_id ASC",
        inner = planned.sql,
    );

    let params: Vec<&dyn ToSql> = planned.params.iter().map(|p| p as &dyn ToSql).collect();
    let mut stmt = conn.prepare(&wrapped)?;
    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok(SearchHit {
            file_id: row.get::<_, String>(0)?,
            path: row.get::<_, String>(1)?,
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Augment a list of [`SearchHit`]s with the rich projection (tags,
/// violation counts, custom fields) needed for the §8.1 JSON output.
///
/// Order is preserved (one [`RichSearchHit`] per input hit). Hits whose
/// `file_id` doesn't exist anymore (e.g. concurrent delete between
/// `execute` and `project_hits`) are silently dropped.
pub fn project_hits(
    index: &dyn Index,
    hits: &[SearchHit],
) -> Result<Vec<RichSearchHit>, ExecuteError> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }
    let file_ids: Vec<FileId> = hits
        .iter()
        .map(|h| {
            h.file_id
                .parse::<FileId>()
                .map_err(|_| ExecuteError::MalformedFileId(h.file_id.clone()))
        })
        .collect::<Result<_, _>>()?;
    let rich = index.rich_rows(&file_ids)?;
    Ok(rich.into_iter().map(RichSearchHit::from).collect())
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::migration::apply;
    use crate::search::{
        CustomFieldKind, CustomFields, parse, plan as plan_query, validate as validate_query,
    };
    use rusqlite::params;

    fn open() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        apply(&mut conn).unwrap();
        conn
    }

    fn schema_int(field: &str) -> CustomFields {
        let mut s = CustomFields::new();
        s.insert(field, CustomFieldKind::Int);
        s
    }

    /// Insert a minimal `files` row + optional populated columns.
    #[allow(clippy::too_many_arguments)]
    fn insert_file(
        conn: &Connection,
        file_id: &str,
        path: &str,
        name: &str,
        ext: Option<&str>,
        notes: Option<&str>,
        updated_at: Option<&str>,
        created_at: Option<&str>,
        is_orphan: bool,
        fingerprint: &str,
    ) {
        conn.execute(
            "INSERT INTO files (file_id, path, fingerprint, kind, status, name, ext, notes, \
             updated_at, created_at, is_orphan) \
             VALUES (?1, ?2, ?3, 'asset', 'active', ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                file_id,
                path,
                fingerprint,
                name,
                ext,
                notes,
                updated_at,
                created_at,
                i64::from(is_orphan),
            ],
        )
        .unwrap();
    }

    fn add_tag(conn: &Connection, file_id: &str, tag: &str) {
        conn.execute(
            "INSERT INTO tags (file_id, tag) VALUES (?1, ?2)",
            params![file_id, tag],
        )
        .unwrap();
    }

    fn add_violation(
        conn: &Connection,
        file_id: &str,
        category: &str,
        severity: &str,
        rule_id: &str,
    ) {
        conn.execute(
            "INSERT INTO violations (file_id, category, severity, rule_id) \
             VALUES (?1, ?2, ?3, ?4)",
            params![file_id, category, severity, rule_id],
        )
        .unwrap();
    }

    fn add_custom_int(conn: &Connection, file_id: &str, key: &str, value: i64) {
        conn.execute(
            "INSERT INTO custom_fields (file_id, key, value_int) VALUES (?1, ?2, ?3)",
            params![file_id, key, value],
        )
        .unwrap();
    }

    fn run(conn: &Connection, q: &str, schema: &CustomFields) -> Vec<SearchHit> {
        let parsed = parse(q).unwrap_or_else(|e| panic!("parse: {e}"));
        let validated = validate_query(&parsed, schema);
        let planned = plan_query(&validated);
        execute(conn, &planned).unwrap_or_else(|e| panic!("execute: {e}\nSQL: {}", planned.sql))
    }

    fn paths(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.path.as_str()).collect()
    }

    #[test]
    fn empty_db_returns_empty() {
        let conn = open();
        let hits = run(&conn, "tag:wip", &CustomFields::new());
        assert!(hits.is_empty());
    }

    #[test]
    fn tag_filter_matches() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        add_tag(&conn, "f1", "wip");

        let hits = run(&conn, "tag:wip", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn type_filter_normalizes_extension() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.tif",
            "b.tif",
            Some("tif"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "type:.PSD", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn implicit_and_intersects() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        insert_file(
            &conn,
            "f3",
            "./c.tif",
            "c.tif",
            Some("tif"),
            None,
            None,
            None,
            false,
            "fp3",
        );
        add_tag(&conn, "f1", "wip");
        add_tag(&conn, "f3", "wip");

        let hits = run(&conn, "tag:wip type:psd", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn negation_excludes() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        add_tag(&conn, "f1", "wip");

        let hits = run(&conn, "type:psd -tag:wip", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./b.psd"]);
    }

    #[test]
    fn or_unions() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        insert_file(
            &conn,
            "f3",
            "./c.psd",
            "c.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp3",
        );
        add_tag(&conn, "f1", "wip");
        add_tag(&conn, "f2", "review");

        let hits = run(&conn, "tag:wip OR tag:review", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd", "./b.psd"]);
    }

    #[test]
    fn freetext_uses_fts_trigram() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./forest_night.psd",
            "forest_night.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./mountain_dawn.psd",
            "mountain_dawn.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "forest", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./forest_night.psd"]);
    }

    #[test]
    fn freetext_matches_notes() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            Some("contains forest reference"),
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "forest", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn updated_at_range_inclusive() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            Some("2026-04-01T12:00:00Z"),
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            Some("2026-05-01T12:00:00Z"),
            None,
            false,
            "fp2",
        );

        let hits = run(
            &conn,
            "updated:2026-04-01..2026-04-30",
            &CustomFields::new(),
        );
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn is_violation_filters_by_severity() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        insert_file(
            &conn,
            "f3",
            "./c.psd",
            "c.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp3",
        );
        add_violation(&conn, "f1", "naming", "warn", "rule-1");
        add_violation(&conn, "f2", "naming", "hint", "rule-2"); // hint excluded
        // f3: no violation

        let hits = run(&conn, "is:violation", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn is_misplaced_filters_by_category() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        add_violation(&conn, "f1", "placement", "strict", "place-1");
        add_violation(&conn, "f2", "naming", "warn", "name-1");

        let hits = run(&conn, "is:misplaced", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn is_duplicate_via_fingerprint() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fpX",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fpX",
        );
        insert_file(
            &conn,
            "f3",
            "./c.psd",
            "c.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fpY",
        );

        let hits = run(&conn, "is:duplicate", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd", "./b.psd"]);
    }

    #[test]
    fn is_orphan_via_flag() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            true,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "is:orphan", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn name_glob_matches() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./shots/a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./shots/b.tif",
            "b.tif",
            Some("tif"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "name:*.psd", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./shots/a.psd"]);
    }

    #[test]
    fn path_glob_matches() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./assets/shots/a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./renders/b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "path:./assets/**", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./assets/shots/a.psd"]);
    }

    #[test]
    fn custom_int_field_matches() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        add_custom_int(&conn, "f1", "scene", 10);
        add_custom_int(&conn, "f2", "scene", 20);

        let hits = run(&conn, "scene:10", &schema_int("scene"));
        assert_eq!(paths(&hits), vec!["./a.psd"]);
    }

    #[test]
    fn custom_int_range_matches() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        insert_file(
            &conn,
            "f3",
            "./c.psd",
            "c.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp3",
        );
        add_custom_int(&conn, "f1", "shot", 5);
        add_custom_int(&conn, "f2", "shot", 50);
        add_custom_int(&conn, "f3", "shot", 100);

        let hits = run(&conn, "shot:1..50", &schema_int("shot"));
        assert_eq!(paths(&hits), vec!["./a.psd", "./b.psd"]);
    }

    #[test]
    fn unknown_key_short_circuits_to_empty() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        add_tag(&conn, "f1", "wip");

        // tag:wip alone matches f1, but adding unknown key shorts.
        let hits = run(&conn, "tag:wip foobar:hello", &CustomFields::new());
        assert!(hits.is_empty(), "got {hits:?}");
    }

    #[test]
    fn deterministic_order_by_path() {
        let conn = open();
        // Insert in non-alphabetical order.
        insert_file(
            &conn,
            "f3",
            "./c.psd",
            "c.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp3",
        );
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );

        let hits = run(&conn, "type:psd", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd", "./b.psd", "./c.psd"]);
    }

    #[test]
    fn complex_query_combines_predicates() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "a.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );
        insert_file(
            &conn,
            "f2",
            "./b.psd",
            "b.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp2",
        );
        insert_file(
            &conn,
            "f3",
            "./c.psd",
            "c.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp3",
        );
        add_tag(&conn, "f1", "wip");
        add_tag(&conn, "f2", "review");
        add_tag(&conn, "f3", "wip");
        add_violation(&conn, "f3", "placement", "warn", "place-1");

        // (wip OR review) AND -is:violation
        // f1 ✓, f2 ✓, f3 has placement violation → excluded
        let hits = run(
            &conn,
            "(tag:wip OR tag:review) -is:violation",
            &CustomFields::new(),
        );
        assert_eq!(paths(&hits), vec!["./a.psd", "./b.psd"]);
    }

    #[test]
    fn fts_triggers_fire_on_update() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "old_name.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );

        // Initial query against old name matches.
        let hits = run(&conn, "old_name", &CustomFields::new());
        assert_eq!(paths(&hits), vec!["./a.psd"]);

        // Rename `name` column. The AFTER UPDATE OF name trigger
        // should rewrite the FTS row.
        conn.execute(
            "UPDATE files SET name = 'new_name.psd' WHERE file_id = 'f1'",
            [],
        )
        .unwrap();

        let stale = run(&conn, "old_name", &CustomFields::new());
        assert!(stale.is_empty(), "expected old_name to no longer match");

        let fresh = run(&conn, "new_name", &CustomFields::new());
        assert_eq!(paths(&fresh), vec!["./a.psd"]);
    }

    #[test]
    fn fts_triggers_fire_on_delete() {
        let conn = open();
        insert_file(
            &conn,
            "f1",
            "./a.psd",
            "forest.psd",
            Some("psd"),
            None,
            None,
            None,
            false,
            "fp1",
        );

        let before = run(&conn, "forest", &CustomFields::new());
        assert_eq!(paths(&before), vec!["./a.psd"]);

        conn.execute("DELETE FROM files WHERE file_id = 'f1'", [])
            .unwrap();

        let after = run(&conn, "forest", &CustomFields::new());
        assert!(after.is_empty());
    }
}
