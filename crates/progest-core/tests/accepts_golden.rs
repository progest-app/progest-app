//! Golden fixtures for `core::accepts` (placement lint).
//!
//! Each scenario under `tests/accepts_golden/<scenario>/` has:
//!
//! - `project.toml` — optional `[alias.*]` entries (layered into the
//!   alias catalog) plus `[[dirs]]` entries describing per-dir
//!   `[accepts]` declarations. The `[[dirs]]` list is ordered deepest
//!   first (leaf dir first, root last) to match the inheritance
//!   walk's expectation.
//! - `cases/*.yaml` — per-file expectations. Each YAML specifies the
//!   test path (relative to project root) and the expected
//!   violations.
//!
//! The harness runs every case in every scenario, compares the
//! emitted violations against the YAML, and fails once with a
//! collected report.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use progest_core::accepts::{
    AliasCatalog, BUILTIN_ALIASES, EffectiveAccepts, RawAccepts, compute_effective_accepts,
    evaluate_placement_for_file, extract_accepts, load_alias_catalog_from_table, normalize_ext,
};
use progest_core::fs::ProjectPath;
use progest_core::rules::{AcceptsSource, BUILTIN_COMPOUND_EXTS, Category, RuleKind, Severity};
use serde::Deserialize;
use toml::{Table, Value};

// --- Case YAML schema ------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CaseDocument {
    #[allow(dead_code)] // used by humans reading failures; not asserted on
    name: String,
    path: String,
    expected: Expected,
}

#[derive(Debug, Deserialize, Default)]
struct Expected {
    #[serde(default)]
    violations: Vec<ExpectedViolation>,
}

#[derive(Debug, Deserialize)]
struct ExpectedViolation {
    severity: String,
    #[serde(default)]
    expected_exts: Option<Vec<String>>,
    #[serde(default)]
    winning_rule_source: Option<String>,
    #[serde(default)]
    reason_contains: Option<String>,
}

// --- Harness ---------------------------------------------------------------

#[test]
fn run_all_accepts_golden_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/accepts_golden");
    assert!(root.is_dir(), "expected fixture root at {}", root.display());

    let mut ran = 0;
    let mut failures: Vec<String> = Vec::new();

    for scenario in sorted_dirs(&root) {
        let project_toml = scenario.join("project.toml");
        assert!(
            project_toml.is_file(),
            "scenario {} missing project.toml",
            scenario.display()
        );
        let cases_dir = scenario.join("cases");
        assert!(
            cases_dir.is_dir(),
            "scenario {} missing cases/",
            scenario.display()
        );

        for case_path in sorted_yaml_files(&cases_dir) {
            ran += 1;
            if let Err(msg) = run_case(&project_toml, &case_path) {
                failures.push(format!(
                    "{}/{}: {msg}",
                    scenario.file_name().unwrap().to_string_lossy(),
                    case_path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    assert!(ran > 0, "no accepts fixtures were discovered");
    assert!(
        failures.is_empty(),
        "{} / {} accepts case(s) failed:\n{}",
        failures.len(),
        ran,
        failures.join("\n")
    );
}

fn run_case(project_toml: &Path, case_path: &Path) -> Result<(), String> {
    let project_src = fs::read_to_string(project_toml)
        .map_err(|e| format!("read {}: {e}", project_toml.display()))?;
    let project: ProjectFixture =
        toml::from_str(&project_src).map_err(|e| format!("parse project.toml: {e}"))?;

    // Build the alias catalog (builtin + project aliases).
    let catalog = if let Some(alias_block) = project.alias {
        let mut root = Table::new();
        root.insert("alias".into(), Value::Table(alias_block));
        load_alias_catalog_from_table(&root)
            .map_err(|e| format!("load alias catalog: {e}"))?
            .catalog
    } else {
        AliasCatalog::builtin()
    };

    // Build the dir → RawAccepts map.
    let mut dir_accepts: BTreeMap<String, RawAccepts> = BTreeMap::new();
    for d in project.dirs {
        let mut wrapper = Table::new();
        wrapper.insert("accepts".into(), Value::Table(d.accepts));
        let extracted = extract_accepts(&wrapper)
            .map_err(|e| format!("extract accepts for dir `{}`: {e}", d.path))?;
        if let Some(ex) = extracted {
            dir_accepts.insert(d.path, ex.accepts);
        }
    }

    let case_src = fs::read_to_string(case_path).map_err(|e| format!("read case: {e}"))?;
    let case: CaseDocument =
        serde_yaml::from_str(&case_src).map_err(|e| format!("parse case YAML: {e}"))?;

    let path = ProjectPath::new(&case.path)
        .map_err(|e| format!("invalid case path `{}`: {e}", case.path))?;
    let parent_path = path
        .parent()
        .ok_or_else(|| format!("case path `{}` has no parent", case.path))?;

    // Walk the ancestor chain from the parent upward, collecting each
    // dir's RawAccepts in parent-first order.
    let own = dir_accepts.get(parent_path.as_str());
    let chain = ancestor_chain(&parent_path, &dir_accepts);

    let effective = compute_effective_accepts(own, &chain.iter().collect::<Vec<_>>(), &catalog)
        .map_err(|e| format!("compute effective accepts: {e}"))?;

    let violation =
        evaluate_placement_for_file(&path, None, effective.as_ref(), BUILTIN_COMPOUND_EXTS);

    compare(&case.expected, violation.as_ref(), effective.as_ref())
}

fn ancestor_chain(
    parent: &ProjectPath,
    dir_accepts: &BTreeMap<String, RawAccepts>,
) -> Vec<RawAccepts> {
    // The spec walks up from parent's parent, skipping the parent
    // itself (which is the `own` in compute_effective_accepts).
    let mut out = Vec::new();
    let mut cursor = parent.parent();
    while let Some(dir) = cursor {
        if let Some(raw) = dir_accepts.get(dir.as_str()) {
            out.push(raw.clone());
        }
        cursor = dir.parent();
    }
    out
}

fn compare(
    expected: &Expected,
    actual: Option<&progest_core::rules::Violation>,
    _effective: Option<&EffectiveAccepts>,
) -> Result<(), String> {
    if expected.violations.is_empty() {
        return match actual {
            None => Ok(()),
            Some(v) => Err(format!(
                "expected no violations but got one: rule_id={} severity={:?} reason={}",
                v.rule_id.as_str(),
                v.severity,
                v.reason,
            )),
        };
    }
    let Some(actual) = actual else {
        return Err("expected a placement violation but got none".into());
    };
    let expected = &expected.violations[0];

    let want_severity = match expected.severity.as_str() {
        "strict" => Severity::Strict,
        "warn" => Severity::Warn,
        "hint" => Severity::Hint,
        "evaluation_error" => Severity::EvaluationError,
        other => return Err(format!("unknown expected severity `{other}`")),
    };
    if actual.severity != want_severity {
        return Err(format!(
            "severity mismatch: expected {want_severity:?}, got {:?}",
            actual.severity
        ));
    }
    if actual.category != Category::Placement {
        return Err(format!(
            "expected category=placement, got {:?}",
            actual.category
        ));
    }
    if actual.kind != RuleKind::Constraint {
        return Err(format!("expected kind=constraint, got {:?}", actual.kind));
    }
    let details = actual
        .placement_details
        .as_ref()
        .ok_or("placement_details must be populated on placement violations")?;

    if let Some(want_exts) = &expected.expected_exts
        && details.expected_exts != *want_exts
    {
        return Err(format!(
            "expected_exts mismatch: expected {want_exts:?}, got {:?}",
            details.expected_exts
        ));
    }
    if let Some(want_source) = &expected.winning_rule_source {
        let got = match details.winning_rule_source {
            AcceptsSource::Own => "own",
            AcceptsSource::Inherited => "inherited",
        };
        if got != want_source {
            return Err(format!(
                "winning_rule_source mismatch: expected `{want_source}`, got `{got}`"
            ));
        }
    }
    if let Some(needle) = &expected.reason_contains
        && !actual.reason.contains(needle)
    {
        return Err(format!(
            "reason does not contain `{needle}`: actual `{}`",
            actual.reason
        ));
    }
    Ok(())
}

// --- Fixture helpers --------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ProjectFixture {
    #[serde(default)]
    alias: Option<Table>,
    #[serde(default)]
    dirs: Vec<DirFixture>,
}

#[derive(Debug, Deserialize)]
struct DirFixture {
    path: String,
    accepts: Table,
}

fn sorted_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", root.display()))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

fn sorted_yaml_files(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", root.display()))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

// Explicit reference so unused-import lints do not bite the harness.
#[allow(dead_code)]
fn _unused_anchors() {
    let _ = BUILTIN_ALIASES;
    let _ = normalize_ext;
}
