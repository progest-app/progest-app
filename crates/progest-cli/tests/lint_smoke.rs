//! Smoke tests for `progest lint`.
//!
//! Anchors the JSON wire shape (`{naming, placement, sequence,
//! summary}`), the exit-code contract (DSL §8.2), and the three
//! Violation sources wired end-to-end. Per-component unit tests live
//! alongside the respective `core::` modules; this file is just the
//! integration layer.

mod support;

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde_json::Value;
use tempfile::TempDir;

use support::{binary_path, init_project as init_named, touch, write_file};

fn init_project(cwd: &Path) -> Result<()> {
    init_named(cwd, "lint-smoke")
}

fn run_lint_json(cwd: &Path) -> (Value, i32) {
    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args(["lint", "--format", "json"])
        .output()
        .expect("running progest lint");
    let code = output.status.code().unwrap_or(-1);
    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "parsing lint JSON ({e}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    (json, code)
}

#[test]
fn clean_tree_returns_empty_groups_and_exit_zero() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/ok.png")?;

    let (report, code) = run_lint_json(cwd);
    assert_eq!(code, 0, "clean tree must exit 0");
    assert_eq!(report["naming"].as_array().unwrap().len(), 0);
    assert_eq!(report["placement"].as_array().unwrap().len(), 0);
    assert_eq!(report["sequence"].as_array().unwrap().len(), 0);
    Ok(())
}

#[test]
fn placement_violation_surfaces_under_placement_group() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;

    // `images/` accepts only `.png`; `.mp4` must trip placement.
    write_file(
        cwd,
        "images/.dirmeta.toml",
        r#"schema_version = 1
[accepts]
inherit = false
exts = [".png"]
mode = "warn"
"#,
    )?;
    touch(cwd, "images/ok.png")?;
    touch(cwd, "images/bad.mp4")?;

    let (report, code) = run_lint_json(cwd);
    assert_eq!(code, 0, "warn severity must not fail CI");
    let placement = report["placement"].as_array().unwrap();
    assert!(
        placement.iter().any(|v| {
            v["path"].as_str() == Some("images/bad.mp4")
                && v["rule_id"].as_str() == Some("placement")
                && v["category"].as_str() == Some("placement")
        }),
        "expected placement violation for images/bad.mp4, got {placement:?}"
    );
    Ok(())
}

#[test]
fn sequence_drift_surfaces_under_sequence_group() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;

    // Majority (3 × pad=4) wins; the two pad=3 files drift.
    for n in 1..=3 {
        touch(cwd, &format!("assets/shot_{n:04}.png"))?;
    }
    touch(cwd, "assets/shot_001.png")?;
    touch(cwd, "assets/shot_002.png")?;

    let (report, code) = run_lint_json(cwd);
    assert_eq!(code, 0, "drift defaults to warn severity");
    let seq = report["sequence"].as_array().unwrap();
    assert_eq!(seq.len(), 2, "expected 2 drift rows, got {seq:?}");
    for v in seq {
        assert_eq!(v["category"].as_str(), Some("sequence"));
        assert_eq!(v["rule_id"].as_str(), Some("sequence-drift"));
        let suggested = v["suggested_names"].as_array().unwrap();
        assert_eq!(suggested.len(), 1);
        let s = suggested[0].as_str().unwrap();
        assert!(
            s.starts_with("shot_000"),
            "suggestion should use pad=4: {s}"
        );
    }
    Ok(())
}

#[test]
fn strict_naming_violation_fails_ci_exit_one() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;

    // A strict ASCII-only rule on /assets/**/*.psd.
    write_file(
        cwd,
        ".progest/rules.toml",
        r#"schema_version = 1

[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = "./assets/**/*.psd"
mode = "strict"
charset = "ascii"
"#,
    )?;
    touch(cwd, "assets/日本語.psd")?;

    let (report, code) = run_lint_json(cwd);
    assert_eq!(code, 1, "strict violation must exit 1");
    let naming = report["naming"].as_array().unwrap();
    assert!(
        naming
            .iter()
            .any(|v| v["severity"].as_str() == Some("strict")),
        "expected at least one strict naming violation, got {naming:?}"
    );
    Ok(())
}

#[test]
fn explain_includes_traces_for_all_evaluated_files() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    // A warn-level rule that a `.psd` will violate so we get both a
    // winner-trace (violating file) and a non-violating trace to
    // compare against.
    write_file(
        cwd,
        ".progest/rules.toml",
        r#"schema_version = 1

[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = "./assets/**/*.psd"
mode = "warn"
charset = "ascii"
"#,
    )?;
    touch(cwd, "assets/日本語.psd")?;

    // With --explain the trace field on each violation should be
    // populated with at least one RuleHit; without it the orchestrator
    // trims non-Winner rows. Under warn mode the violation row always
    // has its Winner hit either way, so compare naming trace length.
    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args(["lint", "--format", "json", "--explain"])
        .output()?;
    assert!(output.status.success() || output.status.code() == Some(0));
    let report: Value = serde_json::from_slice(&output.stdout)?;
    let naming = report["naming"].as_array().unwrap();
    assert!(
        !naming.is_empty(),
        "expected a naming violation to carry the trace"
    );
    let trace = naming[0]["trace"].as_array().unwrap();
    assert!(!trace.is_empty(), "trace must not be empty under --explain");
    Ok(())
}

#[test]
fn text_format_prints_grouped_sections() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    write_file(
        cwd,
        "images/.dirmeta.toml",
        r#"schema_version = 1
[accepts]
inherit = false
exts = [".png"]
mode = "warn"
"#,
    )?;
    touch(cwd, "images/bad.mp4")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args(["lint", "--format", "text"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[placement]"), "missing header: {stdout}");
    assert!(stdout.contains("images/bad.mp4"), "missing path: {stdout}");
    assert!(stdout.contains("Summary:"), "missing summary: {stdout}");
    Ok(())
}
