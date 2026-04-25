//! Search DSL planner (`docs/SEARCH_DSL.md` §7).
//!
//! Lowers a [`ValidatedQuery`] into a SQL `WHERE` expression + bound
//! parameters. Pure function: same `ValidatedQuery` → same SQL byte
//! string. The planner does NOT execute the query — that's the
//! `executor` module (next PR, after FTS5 schema migration).
//!
//! Output target schema (created by `core::index` migration in M3
//! #4):
//!
//! ```sql
//! files(file_id PK, path, name, ext, kind, notes, created_at,
//!       updated_at, has_naming_violation, has_placement_violation,
//!       has_sequence_violation, has_orphan, has_duplicate)
//! tags(file_id, name)
//! custom_fields(file_id, key, value_text, value_int)
//! files_fts(file_id UNINDEXED, name, notes, tokenize='trigram')
//! ```

use serde::{Deserialize, Serialize};

use super::validate::{
    CustomClause, CustomMatcher, FreeTextTerm, IsValue, KindValue, ReservedClause, ValidAtom,
    ValidExpr, ValidatedQuery,
};

/// Bind parameter for a planned query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindValue {
    Text(String),
    Integer(i64),
}

/// Planned SQL artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedQuery {
    /// Full SELECT statement; the executor runs this verbatim.
    pub sql: String,
    /// Positional bind parameters in `?` order.
    pub params: Vec<BindValue>,
}

/// Lower a validated query to SQL. Returns a SELECT that produces
/// `file_id` rows in deterministic order (`path ASC, file_id ASC`).
pub fn plan(query: &ValidatedQuery) -> PlannedQuery {
    let mut builder = SqlBuilder::default();
    let where_sql = builder.build_expr(&query.expr);
    let sql = format!(
        "SELECT f.file_id\n  FROM files f\n WHERE {where_sql}\n ORDER BY f.path ASC, f.file_id ASC",
    );
    PlannedQuery {
        sql,
        params: builder.params,
    }
}

#[derive(Default)]
struct SqlBuilder {
    params: Vec<BindValue>,
}

impl SqlBuilder {
    fn push_text(&mut self, s: impl Into<String>) {
        self.params.push(BindValue::Text(s.into()));
    }
    fn push_int(&mut self, n: i64) {
        self.params.push(BindValue::Integer(n));
    }

    fn build_expr(&mut self, expr: &ValidExpr) -> String {
        match expr {
            ValidExpr::Or(branches) => {
                let parts: Vec<_> = branches.iter().map(|b| self.build_expr(b)).collect();
                format!("({})", parts.join(" OR "))
            }
            ValidExpr::And(branches) => {
                let parts: Vec<_> = branches.iter().map(|b| self.build_expr(b)).collect();
                format!("({})", parts.join(" AND "))
            }
            ValidExpr::Not(inner) => {
                let s = self.build_expr(inner);
                format!("NOT {s}")
            }
            ValidExpr::Atom(atom) => self.build_atom(atom),
        }
    }

    fn build_atom(&mut self, atom: &ValidAtom) -> String {
        match atom {
            ValidAtom::Reserved(c) => self.build_reserved(c),
            ValidAtom::Custom(c) => self.build_custom(c),
            ValidAtom::FreeText(t) => self.build_freetext(t),
            ValidAtom::AlwaysFalse(_) => "0 = 1".into(),
        }
    }

    fn build_reserved(&mut self, c: &ReservedClause) -> String {
        match c {
            ReservedClause::Tag(name) => {
                self.push_text(name);
                "EXISTS (SELECT 1 FROM tags t WHERE t.file_id = f.file_id AND t.name = ?)".into()
            }
            ReservedClause::Type(ext) => {
                self.push_text(ext);
                "f.ext = ?".into()
            }
            ReservedClause::Kind(k) => {
                self.push_text(kind_str(*k));
                "f.kind = ?".into()
            }
            ReservedClause::Is(v) => is_predicate(*v).into(),
            ReservedClause::Name(glob) => {
                self.push_text(glob);
                "f.name GLOB ?".into()
            }
            ReservedClause::Path(glob) => {
                self.push_text(glob);
                "f.path GLOB ?".into()
            }
            ReservedClause::Created(range) => self.build_instant("f.created_at", range),
            ReservedClause::Updated(range) => self.build_instant("f.updated_at", range),
        }
    }

    fn build_instant(&mut self, col: &str, range: &super::validate::InstantRange) -> String {
        let mut conds = Vec::with_capacity(2);
        if let Some(lo) = range.lo.as_ref() {
            self.push_text(lo);
            conds.push(format!("{col} >= ?"));
        }
        if let Some(hi) = range.hi.as_ref() {
            self.push_text(hi);
            conds.push(format!("{col} <= ?"));
        }
        if conds.is_empty() {
            "1 = 1".into()
        } else {
            format!("({})", conds.join(" AND "))
        }
    }

    fn build_custom(&mut self, c: &CustomClause) -> String {
        match &c.matcher {
            CustomMatcher::String(v) | CustomMatcher::Enum(v) => {
                self.push_text(&c.key);
                self.push_text(v);
                "EXISTS (SELECT 1 FROM custom_fields cf WHERE cf.file_id = f.file_id \
                 AND cf.key = ? AND cf.value_text = ?)"
                    .into()
            }
            CustomMatcher::Int(n) => {
                self.push_text(&c.key);
                self.push_int(*n);
                "EXISTS (SELECT 1 FROM custom_fields cf WHERE cf.file_id = f.file_id \
                 AND cf.key = ? AND cf.value_int = ?)"
                    .into()
            }
            CustomMatcher::IntRange { lo, hi } => {
                self.push_text(&c.key);
                let mut sub = String::from(
                    "EXISTS (SELECT 1 FROM custom_fields cf WHERE cf.file_id = f.file_id \
                     AND cf.key = ?",
                );
                if let Some(lo) = lo {
                    self.push_int(*lo);
                    sub.push_str(" AND cf.value_int >= ?");
                }
                if let Some(hi) = hi {
                    self.push_int(*hi);
                    sub.push_str(" AND cf.value_int <= ?");
                }
                sub.push(')');
                sub
            }
        }
    }

    fn build_freetext(&mut self, term: &FreeTextTerm) -> String {
        let q = match term {
            FreeTextTerm::Bareword(s) => fts_escape_bareword(s),
            FreeTextTerm::Phrase(s) => fts_escape_phrase(s),
        };
        self.push_text(q);
        "EXISTS (SELECT 1 FROM files_fts fts WHERE fts.file_id = f.file_id \
         AND fts MATCH ?)"
            .into()
    }
}

fn kind_str(k: KindValue) -> &'static str {
    match k {
        KindValue::Asset => "asset",
        KindValue::Directory => "directory",
        KindValue::Derived => "derived",
    }
}

fn is_predicate(v: IsValue) -> &'static str {
    match v {
        IsValue::Violation => {
            "(f.has_naming_violation = 1 OR f.has_placement_violation = 1 \
             OR f.has_sequence_violation = 1)"
        }
        IsValue::Orphan => "f.has_orphan = 1",
        IsValue::Duplicate => "f.has_duplicate = 1",
        IsValue::Misplaced => "f.has_placement_violation = 1",
    }
}

/// FTS5 query escaping for bareword: wrap in double quotes (FTS5
/// phrase syntax); inner double quotes are doubled per FTS5 rules.
fn fts_escape_bareword(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' {
            out.push('"');
            out.push('"');
        } else {
            out.push(ch);
        }
    }
    out.push('"');
    out
}

fn fts_escape_phrase(s: &str) -> String {
    fts_escape_bareword(s)
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::parse::parse;
    use crate::search::validate::{CustomFieldKind, CustomFields, validate};

    fn schema() -> CustomFields {
        let mut s = CustomFields::new();
        s.insert("scene", CustomFieldKind::Int);
        s.insert("shot", CustomFieldKind::Int);
        s
    }

    fn planned(q: &str) -> PlannedQuery {
        let parsed = parse(q).unwrap();
        let validated = validate(&parsed, &schema());
        plan(&validated)
    }

    fn texts(p: &PlannedQuery) -> Vec<&str> {
        p.params
            .iter()
            .filter_map(|b| match b {
                BindValue::Text(s) => Some(s.as_str()),
                BindValue::Integer(_) => None,
            })
            .collect()
    }

    fn ints(p: &PlannedQuery) -> Vec<i64> {
        p.params
            .iter()
            .filter_map(|b| match b {
                BindValue::Integer(n) => Some(*n),
                BindValue::Text(_) => None,
            })
            .collect()
    }

    #[test]
    fn tag_clause() {
        let p = planned("tag:wip");
        assert!(p.sql.contains("EXISTS"));
        assert!(p.sql.contains("tags"));
        assert_eq!(texts(&p), vec!["wip"]);
    }

    #[test]
    fn type_clause_normalizes_extension() {
        let p = planned("type:.PSD");
        assert_eq!(texts(&p), vec!["psd"]);
    }

    #[test]
    fn implicit_and_groups() {
        let p = planned("tag:wip type:psd");
        assert!(p.sql.contains(" AND "), "expected AND, got: {}", p.sql);
        assert_eq!(texts(&p), vec!["wip", "psd"]);
    }

    #[test]
    fn or_then_and_grouping() {
        let p = planned("tag:a OR tag:b type:psd");
        // tag:a OR (tag:b AND type:psd)
        // Parens visible in SQL: Or( a, And(b, psd))
        let sql = &p.sql;
        assert!(sql.contains(" OR "), "{sql}");
        assert!(sql.contains(" AND "), "{sql}");
    }

    #[test]
    fn negation_emits_not() {
        let p = planned("-tag:foo");
        assert!(p.sql.contains("NOT "), "{}", p.sql);
        assert_eq!(texts(&p), vec!["foo"]);
    }

    #[test]
    fn unknown_key_short_circuits_to_zero() {
        let p = planned("foo:bar");
        assert!(p.sql.contains("0 = 1"), "{}", p.sql);
        assert!(texts(&p).is_empty());
    }

    #[test]
    fn name_glob() {
        let p = planned("name:*.psd");
        assert!(p.sql.contains("f.name GLOB ?"));
        assert_eq!(texts(&p), vec!["*.psd"]);
    }

    #[test]
    fn path_glob() {
        let p = planned("path:./assets/**");
        assert!(p.sql.contains("f.path GLOB ?"));
    }

    #[test]
    fn date_range_emits_two_bounds() {
        let p = planned("created:2026-01-01..2026-04-30");
        assert!(p.sql.contains("f.created_at >= ?"));
        assert!(p.sql.contains("f.created_at <= ?"));
        assert_eq!(texts(&p).len(), 2);
    }

    #[test]
    fn date_half_open_emits_one_bound() {
        let p = planned("updated:2026-04-01..");
        assert!(p.sql.contains("f.updated_at >= ?"));
        assert!(!p.sql.contains("<="));
        assert_eq!(texts(&p).len(), 1);
    }

    #[test]
    fn is_violation() {
        let p = planned("is:violation");
        assert!(p.sql.contains("has_naming_violation"), "{}", p.sql);
    }

    #[test]
    fn is_misplaced() {
        let p = planned("is:misplaced");
        assert!(p.sql.contains("has_placement_violation"));
    }

    #[test]
    fn custom_int_field_has_int_param() {
        let p = planned("scene:10");
        assert_eq!(texts(&p), vec!["scene"]);
        assert_eq!(ints(&p), vec![10]);
    }

    #[test]
    fn custom_int_range_has_three_params() {
        let p = planned("shot:1..50");
        assert_eq!(texts(&p), vec!["shot"]);
        assert_eq!(ints(&p), vec![1, 50]);
    }

    #[test]
    fn freetext_uses_fts5_match() {
        let p = planned("forest");
        assert!(p.sql.contains("files_fts"));
        assert!(p.sql.contains("MATCH ?"));
        assert_eq!(texts(&p), vec![r#""forest""#]);
    }

    #[test]
    fn freetext_phrase_doublequotes_inside_get_doubled() {
        let p = planned(r#""he said \"hi\"""#);
        // Expected: open `"` + content + doubled inner `"` + close `"`
        // = `"he said ""hi"""` (16 chars, 7 quotes).
        assert_eq!(texts(&p), vec!["\"he said \"\"hi\"\"\""]);
    }

    #[test]
    fn deterministic_order_clause_present() {
        let p = planned("tag:foo");
        assert!(p.sql.contains("ORDER BY f.path ASC, f.file_id ASC"));
    }

    #[test]
    fn complex_query_structure_lowers() {
        let p = planned("(tag:wip OR tag:review) -is:violation");
        // Should produce: ((EXISTS tags wip) OR (EXISTS tags review)) AND NOT (...)
        assert!(p.sql.contains(" OR "));
        assert!(p.sql.contains(" AND "));
        assert!(p.sql.contains("NOT "));
        assert_eq!(texts(&p), vec!["wip", "review"]);
    }
}
