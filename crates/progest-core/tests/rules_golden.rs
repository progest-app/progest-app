//! Golden-fixture integration tests for `core::rules`.
//!
//! Each subdirectory under `tests/rules_golden/` is one scenario
//! derived 1:1 from `docs/NAMING_RULES_DSL.md` §10. A scenario has:
//!
//! ```text
//! <scenario>/
//! ├── rules.toml           # the project-wide rules.toml
//! └── cases/*.yaml         # one YAML file per expected outcome
//! ```
//!
//! Each `case_*.yaml` declares the evaluated path, optional
//! `.meta.custom` / `created_at`, optional inline dirmeta layers,
//! and the expected `violations` / `winner_rule_id`.
//!
//! The harness walks the directory, discovers fixtures dynamically,
//! and fails with a descriptive error naming the scenario + case so
//! CI output points directly at the broken fixture.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use progest_core::fs::ProjectPath;
use progest_core::identity::FileId;
use progest_core::meta::MetaDocument;
use progest_core::rules::{
    BUILTIN_COMPOUND_EXTS, Decision, RuleKind, RuleSetLayer, RuleSource, Severity, compile_ruleset,
    evaluate, load_document,
};

// --- Fixture types ---------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CaseDocument {
    #[allow(dead_code)]
    name: String,
    path: String,
    #[serde(default)]
    custom: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    dirmeta_layers: Vec<DirmetaLayer>,
    expected: ExpectedOutcome,
}

#[derive(Debug, Deserialize)]
struct DirmetaLayer {
    base_dir: String,
    rules_toml: String,
}

#[derive(Debug, Deserialize, Default)]
struct ExpectedOutcome {
    #[serde(default)]
    violations: Vec<ExpectedViolation>,
    #[serde(default)]
    winner_rule_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedViolation {
    rule_id: String,
    kind: String,     // "template" | "constraint"
    severity: String, // "strict" | "warn" | "hint" | "evaluation_error"
    #[serde(default)]
    reason_contains: Option<String>,
}

// --- Test entry point ------------------------------------------------------

#[test]
fn run_all_golden_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rules_golden");
    assert!(root.is_dir(), "expected fixture root at {}", root.display());

    let mut ran = 0;
    let mut failures: Vec<String> = Vec::new();

    for scenario in sorted_dirs(&root) {
        let rules_toml = scenario.join("rules.toml");
        assert!(
            rules_toml.is_file(),
            "scenario {} is missing rules.toml",
            scenario.display()
        );
        let cases_dir = scenario.join("cases");
        assert!(
            cases_dir.is_dir(),
            "scenario {} is missing cases/ directory",
            scenario.display()
        );

        for case_path in sorted_yaml_files(&cases_dir) {
            ran += 1;
            if let Err(msg) = run_case(&rules_toml, &case_path) {
                failures.push(format!(
                    "{}/{}: {msg}",
                    scenario.file_name().unwrap().to_string_lossy(),
                    case_path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    assert!(ran > 0, "no golden fixtures were discovered");
    assert!(
        failures.is_empty(),
        "{} / {} golden case(s) failed:\n{}",
        failures.len(),
        ran,
        failures.join("\n")
    );
}

fn run_case(rules_toml: &Path, case_path: &Path) -> Result<(), String> {
    let rules_src = fs::read_to_string(rules_toml)
        .map_err(|e| format!("read {}: {e}", rules_toml.display()))?;
    let project_doc =
        load_document(&rules_src).map_err(|e| format!("load project rules.toml: {e}"))?;

    let case_src = fs::read_to_string(case_path).map_err(|e| format!("read case: {e}"))?;
    let case: CaseDocument =
        serde_yaml::from_str(&case_src).map_err(|e| format!("parse case YAML: {e}"))?;

    // Layers: own dirmeta first (nearest wins per spec §7.1), project last.
    let mut layers = Vec::new();
    for dirmeta in &case.dirmeta_layers {
        let doc = load_document(&dirmeta.rules_toml)
            .map_err(|e| format!("load inline dirmeta `{}`: {e}", dirmeta.base_dir))?;
        layers.push(RuleSetLayer {
            source: RuleSource::Own,
            base_dir: ProjectPath::new(&dirmeta.base_dir)
                .map_err(|e| format!("invalid dirmeta base_dir `{}`: {e}", dirmeta.base_dir))?,
            rules: doc.rules,
        });
    }
    layers.push(RuleSetLayer {
        source: RuleSource::ProjectWide,
        base_dir: ProjectPath::root(),
        rules: project_doc.rules,
    });

    let ruleset = compile_ruleset(layers).map_err(|e| format!("compile_ruleset: {e}"))?;

    let path = ProjectPath::new(&case.path)
        .map_err(|e| format!("invalid case path `{}`: {e}", case.path))?;
    let meta = build_meta(&case).map_err(|e| format!("build meta: {e}"))?;

    let outcome = evaluate(&path, meta.as_ref(), &ruleset, BUILTIN_COMPOUND_EXTS);

    compare_outcome(&outcome, &case.expected)
}

// --- Helpers ---------------------------------------------------------------

fn build_meta(case: &CaseDocument) -> Result<Option<MetaDocument>, String> {
    if case.custom.is_empty() && case.created_at.is_none() {
        return Ok(None);
    }
    let mut doc = MetaDocument::new(
        FileId::new_v7(),
        "blake3:00112233445566778899aabbccddeeff"
            .parse()
            .map_err(|e| format!("fingerprint: {e}"))?,
    );
    for (k, v) in &case.custom {
        let value = yaml_to_toml(v.clone()).map_err(|e| format!("custom.{k} yaml → toml: {e}"))?;
        doc.custom.insert(k.clone(), value);
    }
    if let Some(dt) = &case.created_at {
        doc.created_at = Some(dt.parse().map_err(|e| format!("created_at `{dt}`: {e}"))?);
    }
    Ok(Some(doc))
}

fn yaml_to_toml(value: serde_yaml::Value) -> Result<toml::Value, String> {
    use serde_yaml::Value as Y;
    Ok(match value {
        Y::Null => toml::Value::String(String::new()),
        Y::Bool(b) => toml::Value::Boolean(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                return Err(format!("unsupported number {n:?}"));
            }
        }
        Y::String(s) => toml::Value::String(s),
        Y::Sequence(seq) => {
            let mut out = Vec::with_capacity(seq.len());
            for v in seq {
                out.push(yaml_to_toml(v)?);
            }
            toml::Value::Array(out)
        }
        Y::Mapping(map) => {
            let mut table = toml::map::Map::new();
            for (k, v) in map {
                let key = match k {
                    Y::String(s) => s,
                    other => return Err(format!("non-string map key: {other:?}")),
                };
                table.insert(key, yaml_to_toml(v)?);
            }
            toml::Value::Table(table)
        }
        Y::Tagged(tagged) => yaml_to_toml(tagged.value)?,
    })
}

fn compare_outcome(
    outcome: &progest_core::rules::EvaluationOutcome,
    expected: &ExpectedOutcome,
) -> Result<(), String> {
    // Violations
    if outcome.violations.len() != expected.violations.len() {
        return Err(format!(
            "expected {} violations, got {} ({})",
            expected.violations.len(),
            outcome.violations.len(),
            outcome
                .violations
                .iter()
                .map(|v| format!("{}:{}", v.rule_id, v.reason))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }

    // Match expected violations one-for-one on rule_id + kind; order-insensitive.
    let mut remaining: Vec<usize> = (0..outcome.violations.len()).collect();
    for exp in &expected.violations {
        let pos = remaining
            .iter()
            .position(|&i| {
                let v = &outcome.violations[i];
                v.rule_id.as_str() == exp.rule_id
                    && kind_str(v.kind) == exp.kind
                    && severity_str(v.severity) == exp.severity
                    && exp
                        .reason_contains
                        .as_deref()
                        .is_none_or(|needle| v.reason.contains(needle))
            })
            .ok_or_else(|| {
                format!(
                    "no violation matches rule_id={} kind={} severity={} reason_contains={:?}; actual = [{}]",
                    exp.rule_id,
                    exp.kind,
                    exp.severity,
                    exp.reason_contains,
                    outcome
                        .violations
                        .iter()
                        .map(|v| format!(
                            "{}:{}:{}:{}",
                            v.rule_id,
                            kind_str(v.kind),
                            severity_str(v.severity),
                            v.reason
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        remaining.remove(pos);
    }

    // Optional winner_rule_id cross-check.
    if let Some(expected_winner) = &expected.winner_rule_id {
        let actual_winner = outcome
            .trace
            .iter()
            .find(|h| {
                matches!(h.decision, Decision::Winner) && matches!(h.kind, RuleKind::Template)
            })
            .map(|h| h.rule_id.as_str());
        match actual_winner {
            Some(id) if id == expected_winner => {}
            Some(other) => {
                return Err(format!(
                    "expected winner `{expected_winner}`, got `{other}`"
                ));
            }
            None => {
                return Err(format!(
                    "expected winner `{expected_winner}`, but trace has no template winner"
                ));
            }
        }
    }

    Ok(())
}

fn kind_str(k: RuleKind) -> &'static str {
    match k {
        RuleKind::Template => "template",
        RuleKind::Constraint => "constraint",
    }
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Strict => "strict",
        Severity::Warn => "warn",
        Severity::Hint => "hint",
        Severity::EvaluationError => "evaluation_error",
    }
}

fn sorted_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn sorted_yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|s| s == "yaml" || s == "yml")
        })
        .collect();
    out.sort();
    out
}
