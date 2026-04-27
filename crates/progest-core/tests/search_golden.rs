//! Golden-fixture integration tests for `core::search`.
//!
//! Each scenario file under `tests/search_golden/<id>.toml` mirrors
//! one of the `docs/SEARCH_DSL.md` §10 worked examples. The harness
//! parses + validates + plans the query, then compares structural
//! invariants (no SQL string match — the executor lives in a later
//! PR). This locks the intended behavior of the §10 examples
//! without coupling tests to incidental SQL formatting.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use progest_core::search::{
    BindValue, CustomFieldKind, CustomFields, Warning, parse, plan, validate,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Scenario {
    #[allow(dead_code)]
    name: String,
    query: String,
    #[serde(default)]
    schema: BTreeMap<String, ScenarioField>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ScenarioField {
    String,
    Int,
    Enum { values: Vec<String> },
}

impl From<&ScenarioField> for CustomFieldKind {
    fn from(f: &ScenarioField) -> Self {
        match f {
            ScenarioField::String => CustomFieldKind::String,
            ScenarioField::Int => CustomFieldKind::Int,
            ScenarioField::Enum { values } => CustomFieldKind::Enum {
                values: values.clone(),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct Expected {
    /// Substrings that must all appear in the planned SQL.
    #[serde(default)]
    sql_contains: Vec<String>,
    /// Substrings that must not appear in the planned SQL.
    #[serde(default)]
    sql_absent: Vec<String>,
    /// Expected text bind parameters (in order).
    #[serde(default)]
    text_params: Vec<String>,
    /// Expected integer bind parameters (in order).
    #[serde(default)]
    int_params: Vec<i64>,
    /// Validator warning kinds (`snake_case` enum tag) that must be
    /// present.
    #[serde(default)]
    warning_kinds: Vec<String>,
    /// Number of warnings (exact match).
    #[serde(default)]
    warning_count: Option<usize>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("search_golden")
}

fn discover() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(fixtures_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "toml") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn warning_kind(w: &Warning) -> &'static str {
    match w {
        Warning::UnknownKey { .. } => "unknown_key",
        Warning::TypeMismatch { .. } => "type_mismatch",
        Warning::TypeAndMulti => "type_and_multi",
        Warning::ReservedValueInvalid { .. } => "reserved_value_invalid",
        Warning::RangeOnNonRangeKey { .. } => "range_on_non_range_key",
        Warning::InvalidDatetime { .. } => "invalid_datetime",
        Warning::InvalidGlob { .. } => "invalid_glob",
        Warning::DuplicateInstantClause { .. } => "duplicate_instant_clause",
        Warning::UnknownAlias { .. } => "unknown_alias",
        Warning::UnsupportedAlias { .. } => "unsupported_alias",
        Warning::EmptyListItem { .. } => "empty_list_item",
    }
}

fn run(path: &PathBuf) {
    let label = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("(unknown)");
    let body = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {label}: {e}"));
    let scenario: Scenario =
        toml::from_str(&body).unwrap_or_else(|e| panic!("parse fixture {label}: {e}"));

    let mut schema = CustomFields::new();
    for (k, v) in &scenario.schema {
        schema.insert(k, CustomFieldKind::from(v));
    }

    let parsed = parse(&scenario.query)
        .unwrap_or_else(|e| panic!("[{label}] parse failed for {:?}: {e}", scenario.query));
    let validated = validate(&parsed, &schema);
    let planned = plan(&validated);

    for needle in &scenario.expected.sql_contains {
        assert!(
            planned.sql.contains(needle),
            "[{label}] expected SQL to contain {needle:?}\n--- SQL ---\n{}",
            planned.sql
        );
    }
    for needle in &scenario.expected.sql_absent {
        assert!(
            !planned.sql.contains(needle),
            "[{label}] expected SQL to NOT contain {needle:?}\n--- SQL ---\n{}",
            planned.sql
        );
    }
    let actual_text: Vec<&str> = planned
        .params
        .iter()
        .filter_map(|b| match b {
            BindValue::Text(s) => Some(s.as_str()),
            BindValue::Integer(_) => None,
        })
        .collect();
    let actual_int: Vec<i64> = planned
        .params
        .iter()
        .filter_map(|b| match b {
            BindValue::Integer(n) => Some(*n),
            BindValue::Text(_) => None,
        })
        .collect();
    if !scenario.expected.text_params.is_empty() {
        assert_eq!(
            actual_text, scenario.expected.text_params,
            "[{label}] text params"
        );
    }
    if !scenario.expected.int_params.is_empty() {
        assert_eq!(
            actual_int, scenario.expected.int_params,
            "[{label}] int params"
        );
    }
    if let Some(want_count) = scenario.expected.warning_count {
        assert_eq!(
            validated.warnings.len(),
            want_count,
            "[{label}] warning count, got {:?}",
            validated.warnings
        );
    }
    for kind in &scenario.expected.warning_kinds {
        assert!(
            validated.warnings.iter().any(|w| warning_kind(w) == kind),
            "[{label}] expected warning kind {kind}, got {:?}",
            validated.warnings
        );
    }
}

#[test]
fn run_all_search_goldens() {
    let scenarios = discover();
    assert!(
        !scenarios.is_empty(),
        "no scenario fixtures found in {}",
        fixtures_dir().display()
    );
    for path in scenarios {
        run(&path);
    }
}
