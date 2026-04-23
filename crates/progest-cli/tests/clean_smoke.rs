//! Smoke tests for `progest clean`.
//!
//! Builds a tempdir project, populates a few files that hit each
//! pipeline stage, and asserts the CLI's JSON output round-trips
//! the expected candidates. Text-mode output is not asserted on in
//! detail — the JSON surface is stable enough to anchor regressions,
//! and the text renderer is layered on top.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use serde_json::Value;
use tempfile::TempDir;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_progest"))
}

fn init_project(cwd: &Path) -> Result<()> {
    let status = Command::new(binary_path())
        .current_dir(cwd)
        .args(["init", "--name", "clean-smoke"])
        .status()?;
    assert!(status.success(), "progest init failed");
    Ok(())
}

fn touch(cwd: &Path, rel: &str) -> Result<()> {
    let path = cwd.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, b"")?;
    Ok(())
}

fn run_clean_json(cwd: &Path) -> Result<Value> {
    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args(["clean", "--strip-cjk", "--strip-suffix", "--format", "json"])
        .output()?;
    assert!(
        output.status.success(),
        "progest clean exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn find_candidate<'a>(report: &'a Value, path: &str) -> Option<&'a Value> {
    report
        .get("candidates")?
        .as_array()?
        .iter()
        .find(|c| c.get("path").and_then(Value::as_str) == Some(path))
}

#[test]
fn reports_cleaned_name_for_pascal_and_copy_suffix() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/MainRole_v01 (1).png")?;

    let report = run_clean_json(cwd)?;
    let c = find_candidate(&report, "assets/MainRole_v01 (1).png")
        .expect("expected candidate for copy-suffix file");
    assert_eq!(
        c.get("resolved").and_then(Value::as_str),
        Some("main_role_v01.png")
    );
    assert_eq!(c.get("changed").and_then(Value::as_bool), Some(true));
    Ok(())
}

#[test]
fn reports_skipped_when_cjk_run_leaves_hole_under_skip_mode() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/カット_v01.png")?;

    let report = run_clean_json(cwd)?;
    let c =
        find_candidate(&report, "assets/カット_v01.png").expect("expected candidate for CJK file");
    assert!(
        c.get("resolved").is_none() || c.get("resolved").and_then(Value::as_str).is_none(),
        "expected `resolved` to be omitted under skip mode, got {c:?}"
    );
    let holes = c.get("holes").and_then(Value::as_array).expect("holes[]");
    assert_eq!(holes.len(), 1);
    assert_eq!(
        holes[0].get("origin").and_then(Value::as_str),
        Some("カット")
    );
    assert_eq!(holes[0].get("kind").and_then(Value::as_str), Some("cjk"));
    Ok(())
}

#[test]
fn placeholder_fill_mode_resolves_holes_to_underscore() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/カット_v01.png")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "clean",
            "--strip-cjk",
            "--strip-suffix",
            "--fill-mode",
            "placeholder",
            "--format",
            "json",
        ])
        .output()?;
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout)?;
    let c =
        find_candidate(&report, "assets/カット_v01.png").expect("expected candidate for CJK file");
    assert_eq!(
        c.get("resolved").and_then(Value::as_str),
        Some("_v01.png"),
        "expected placeholder `_` substituted for the hole (leading underscore \
         consumed by heck snake-case boundary)"
    );
    Ok(())
}

#[test]
fn apply_renames_changed_candidates_on_disk() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/MainRole_v01.png")?;
    touch(cwd, "assets/already_clean.psd")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "clean",
            "assets/MainRole_v01.png",
            "--strip-cjk",
            "--strip-suffix",
            "--apply",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "progest clean --apply exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Changed candidate moved to its cleaned name.
    assert!(!cwd.join("assets/MainRole_v01.png").exists());
    assert!(cwd.join("assets/main_role_v01.png").exists());
    // Already-clean candidate untouched.
    assert!(cwd.join("assets/already_clean.psd").exists());
    // History recorded.
    assert!(cwd.join(".progest/local/history.db").exists());
    Ok(())
}

#[test]
fn apply_with_no_changes_exits_zero_and_says_nothing_to_apply() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/already_clean.psd")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "clean",
            "assets/already_clean.psd",
            "--strip-cjk",
            "--strip-suffix",
            "--apply",
        ])
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("nothing to apply"),
        "expected `nothing to apply` in stdout: {stdout}"
    );
    Ok(())
}

#[test]
fn summary_counts_match_candidates() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/MainRole_v01 (1).png")?;
    touch(cwd, "assets/already_clean.psd")?;
    touch(cwd, "assets/カット_v01.png")?;

    let report = run_clean_json(cwd)?;
    let summary = &report["summary"];
    assert!(summary["scanned"].as_u64().unwrap() >= 3);
    // `would_rename` and `unchanged` counts include project-init
    // artifacts that the walk surfaces (notably `.gitignore`, which
    // snake-cases to `gitignore`), so only lower-bound check here.
    assert!(summary["would_rename"].as_u64().unwrap() >= 1);
    assert_eq!(summary["skipped_due_to_holes"].as_u64().unwrap(), 1);
    assert!(summary["unchanged"].as_u64().unwrap() >= 1);

    // The specific candidates we care about must be present and have
    // the expected change status regardless of ambient files.
    let main = find_candidate(&report, "assets/MainRole_v01 (1).png").unwrap();
    assert_eq!(main["changed"].as_bool(), Some(true));
    let clean = find_candidate(&report, "assets/already_clean.psd").unwrap();
    assert_eq!(clean["changed"].as_bool(), Some(false));
    Ok(())
}
