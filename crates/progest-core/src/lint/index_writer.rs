//! Persist a [`LintReport`] into the search index's `violations`
//! table.
//!
//! Lives in [`crate::lint`] rather than [`crate::index`] because the
//! grouping logic (`path` → `file_id` resolution, severity
//! normalization) is lint-shaped, not index-shaped.
//!
//! Scoping rule: only `visited` `file_id`s are touched. Files outside
//! the lint scope keep their previous violation rows. Within scope,
//! every visited file's violations are replaced wholesale (clear +
//! insert), so re-running lint on a file that now passes will purge
//! its stale rows.

use std::collections::BTreeMap;

use crate::identity::FileId;
use crate::index::{Index, IndexError, ViolationRecord};
use crate::rules::{Severity, Violation};

use super::report::LintReport;

/// Persist `report` into `index.violations`, scoped to `visited`.
///
/// `visited` is the set of `file_id`s that the lint pass considered
/// (typically derived from the walker's output by the CLI driver).
/// Each visited `file_id` gets its violation rows replaced, even if
/// the new set is empty — so a fixed-up file stops matching
/// `is:violation` immediately. Files outside `visited` are
/// untouched.
///
/// Violations whose `file_id` is `None` and whose `path` cannot be
/// resolved against the index are silently dropped (lint can be
/// invoked over paths that aren't yet scanned).
pub fn write_to_index(
    index: &dyn Index,
    visited: &[FileId],
    report: &LintReport,
) -> Result<usize, IndexError> {
    let mut by_file: BTreeMap<FileId, Vec<ViolationRecord>> = BTreeMap::new();
    for v in report.iter_all() {
        let Some(file_id) = resolve_file_id(index, v)? else {
            continue;
        };
        if !visited.contains(&file_id) {
            // Out-of-scope hit (e.g. path-derived violation that
            // resolves to a file outside the requested subset). Skip.
            continue;
        }
        by_file.entry(file_id).or_default().push(record_from(v));
    }

    let mut written = 0;
    for file_id in visited {
        let recs = by_file.remove(file_id).unwrap_or_default();
        index.replace_violations(file_id, &recs)?;
        written += recs.len();
    }
    Ok(written)
}

fn resolve_file_id(index: &dyn Index, v: &Violation) -> Result<Option<FileId>, IndexError> {
    if let Some(file_id) = v.file_id {
        return Ok(Some(file_id));
    }
    Ok(index.get_file_by_path(&v.path)?.map(|row| row.file_id))
}

fn record_from(v: &Violation) -> ViolationRecord {
    ViolationRecord {
        category: category_str(v.category).to_string(),
        severity: severity_str(v.severity).to_string(),
        rule_id: v.rule_id.as_str().to_string(),
        message: Some(v.reason.clone()),
    }
}

fn category_str(c: crate::rules::Category) -> &'static str {
    match c {
        crate::rules::Category::Naming => "naming",
        crate::rules::Category::Placement => "placement",
        crate::rules::Category::Sequence => "sequence",
    }
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        // Evaluation errors fail CI like strict; map them onto the
        // same severity bucket so `is:violation` catches them.
        Severity::Strict | Severity::EvaluationError => "strict",
        Severity::Warn => "warn",
        Severity::Hint => "hint",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::ProjectPath;
    use crate::identity::Fingerprint;
    use crate::index::{FileRow, SqliteIndex};
    use crate::lint::report::{LintReport, LintSummary};
    use crate::meta::{Kind, Status};
    use crate::rules::{Category, RuleId, RuleKind};

    fn fresh_index() -> SqliteIndex {
        SqliteIndex::open_in_memory().unwrap()
    }

    fn insert_file(idx: &SqliteIndex, file_id: FileId, raw: &str) {
        let row = FileRow {
            file_id,
            path: ProjectPath::new(raw).unwrap(),
            fingerprint: Fingerprint::from_bytes([0u8; 16]),
            source_file_id: None,
            kind: Kind::Asset,
            status: Status::Active,
            size: None,
            mtime: None,
            created_at: None,
            last_seen_at: None,
        };
        idx.upsert_file(&row).unwrap();
    }

    fn naming_warn(file_id: FileId, raw: &str, rule: &str) -> Violation {
        Violation {
            file_id: Some(file_id),
            path: ProjectPath::new(raw).unwrap(),
            rule_id: rule.parse::<RuleId>().unwrap(),
            category: Category::Naming,
            kind: RuleKind::Template,
            severity: Severity::Warn,
            reason: "naming reason".into(),
            trace: vec![],
            suggested_names: vec![],
            placement_details: None,
        }
    }

    fn empty_report() -> LintReport {
        LintReport {
            naming: vec![],
            placement: vec![],
            sequence: vec![],
            summary: LintSummary::default(),
        }
    }

    #[test]
    fn write_inserts_violations() {
        let idx = fresh_index();
        let fid = FileId::new_v7();
        insert_file(&idx, fid, "a.psd");

        let v = naming_warn(fid, "a.psd", "rule-x");
        let mut report = empty_report();
        report.naming.push(v);

        let written = write_to_index(&idx, &[fid], &report).unwrap();
        assert_eq!(written, 1);
    }

    #[test]
    fn write_clears_stale_for_visited_files() {
        let idx = fresh_index();
        let fid = FileId::new_v7();
        insert_file(&idx, fid, "a.psd");

        // First run: violation present.
        let mut r1 = empty_report();
        r1.naming.push(naming_warn(fid, "a.psd", "rule-x"));
        write_to_index(&idx, &[fid], &r1).unwrap();

        // Second run: violation gone (file got fixed).
        let r2 = empty_report();
        let written = write_to_index(&idx, &[fid], &r2).unwrap();
        assert_eq!(written, 0);

        // Direct DB check: violations table is empty for this file.
        let rich = idx.rich_rows(&[fid]).unwrap();
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].violations.naming, 0);
    }

    #[test]
    fn out_of_scope_files_untouched() {
        let idx = fresh_index();
        let in_scope = FileId::new_v7();
        let out_of_scope = FileId::new_v7();
        insert_file(&idx, in_scope, "a.psd");
        insert_file(&idx, out_of_scope, "b.psd");

        // Seed an existing violation on the out-of-scope file.
        idx.replace_violations(
            &out_of_scope,
            &[ViolationRecord {
                category: "naming".into(),
                severity: "warn".into(),
                rule_id: "stale".into(),
                message: None,
            }],
        )
        .unwrap();

        // Run lint over only `in_scope`.
        let r = empty_report();
        write_to_index(&idx, &[in_scope], &r).unwrap();

        // out_of_scope's violation is preserved.
        let rich = idx.rich_rows(&[out_of_scope]).unwrap();
        assert_eq!(rich[0].violations.naming, 1);
    }

    #[test]
    fn violation_without_file_id_resolves_via_path() {
        let idx = fresh_index();
        let fid = FileId::new_v7();
        insert_file(&idx, fid, "a.psd");

        let mut v = naming_warn(fid, "a.psd", "rule-x");
        v.file_id = None; // path-only

        let mut report = empty_report();
        report.naming.push(v);
        let written = write_to_index(&idx, &[fid], &report).unwrap();
        assert_eq!(written, 1);
    }

    #[test]
    fn violation_for_unknown_path_silently_dropped() {
        let idx = fresh_index();
        let fid = FileId::new_v7();
        insert_file(&idx, fid, "a.psd");

        let mut v = naming_warn(fid, "ghost.psd", "rule-x");
        v.file_id = None;

        let mut report = empty_report();
        report.naming.push(v);
        let written = write_to_index(&idx, &[fid], &report).unwrap();
        assert_eq!(written, 0);
    }
}
