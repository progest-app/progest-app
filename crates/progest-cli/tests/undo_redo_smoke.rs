//! Smoke tests for `progest undo` / `progest redo`.
//!
//! Builds a project, applies a rename, then exercises the two
//! subcommands end-to-end:
//!
//! - `progest undo` with no prior history is a no-op (exit 0, empty
//!   report)
//! - after a single rename, undo restores the original path
//! - redo re-applies the rename
//! - after a bulk `progest clean --apply`, a single `progest undo`
//!   reverses every member of the shared sequence group in one hop
//! - `--entry` limits the action to the head entry only

mod support;

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde_json::Value;
use tempfile::TempDir;

use support::{binary_path, init_project as init_named, touch};

fn init_project(cwd: &Path) -> Result<()> {
    init_named(cwd, "undo-smoke")
}

fn run_undo(cwd: &Path, extra: &[&str]) -> (Value, i32) {
    let mut args = vec!["undo", "--format", "json"];
    args.extend_from_slice(extra);
    let out = Command::new(binary_path())
        .current_dir(cwd)
        .args(&args)
        .output()
        .expect("running progest undo");
    let code = out.status.code().unwrap_or(-1);
    let json: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parsing undo JSON ({e}): stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )
    });
    (json, code)
}

fn run_redo(cwd: &Path, extra: &[&str]) -> (Value, i32) {
    let mut args = vec!["redo", "--format", "json"];
    args.extend_from_slice(extra);
    let out = Command::new(binary_path())
        .current_dir(cwd)
        .args(&args)
        .output()
        .expect("running progest redo");
    let code = out.status.code().unwrap_or(-1);
    let json: Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    (json, code)
}

fn apply_rename(cwd: &Path, from: &str, to: &str) -> Result<()> {
    // Use `progest rename` with the stdin JSON shape to drive a single
    // rename op without going through clean.
    let op = serde_json::json!([{
        "from": from,
        "to": to,
    }]);
    let mut cmd = Command::new(binary_path());
    cmd.current_dir(cwd).args([
        "rename",
        "--from-stdin",
        "--mode",
        "apply",
        "--format",
        "json",
    ]);
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write as _;
        let stdin = child.stdin.as_mut().expect("stdin piped");
        stdin.write_all(op.to_string().as_bytes())?;
    }
    let out = child.wait_with_output()?;
    assert!(
        out.status.success(),
        "rename apply failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[test]
fn undo_on_empty_history_is_noop() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;

    let (report, code) = run_undo(cwd, &[]);
    assert_eq!(code, 0);
    assert_eq!(report.as_array().map(Vec::len), Some(0));
    Ok(())
}

#[test]
fn undo_reverses_a_single_rename_on_disk() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/original.png")?;
    apply_rename(cwd, "assets/original.png", "assets/renamed.png")?;

    assert!(cwd.join("assets/renamed.png").exists());
    assert!(!cwd.join("assets/original.png").exists());

    let (report, code) = run_undo(cwd, &[]);
    assert_eq!(code, 0);
    let rows = report.as_array().unwrap();
    assert_eq!(rows.len(), 1, "expected one row, got {rows:?}");
    assert_eq!(rows[0]["op_kind"].as_str(), Some("rename"));

    assert!(cwd.join("assets/original.png").exists());
    assert!(!cwd.join("assets/renamed.png").exists());
    Ok(())
}

#[test]
fn redo_replays_the_rename_after_undo() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/a.png")?;
    apply_rename(cwd, "assets/a.png", "assets/b.png")?;

    run_undo(cwd, &[]);
    assert!(cwd.join("assets/a.png").exists());

    let (report, code) = run_redo(cwd, &[]);
    assert_eq!(code, 0);
    let rows = report.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(cwd.join("assets/b.png").exists());
    assert!(!cwd.join("assets/a.png").exists());
    Ok(())
}

#[test]
fn undo_of_bulk_clean_reverses_the_whole_sequence() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;

    // A 3-member sequence, with CamelCase stems so cleanup would
    // rename them. clean --apply writes them as one group; one undo
    // call must reverse all three.
    for n in 1..=3 {
        touch(cwd, &format!("assets/FrameShot_{n:04}.png"))?;
    }

    let out = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "clean",
            "assets",
            "--case",
            "snake",
            "--strip-suffix",
            "--apply",
        ])
        .output()?;
    assert!(
        out.status.success(),
        "clean --apply failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // All three renamed.
    for n in 1..=3 {
        assert!(
            cwd.join(format!("assets/frame_shot_{n:04}.png")).exists(),
            "expected snake-cased member {n}"
        );
    }

    let (report, code) = run_undo(cwd, &[]);
    assert_eq!(code, 0, "undo must exit 0");
    let rows = report.as_array().unwrap();
    assert_eq!(
        rows.len(),
        3,
        "expected 3 rows for a 3-member sequence undo, got {rows:?}"
    );
    // All three should share the same group_id.
    let groups: Vec<Option<&str>> = rows.iter().map(|r| r["group_id"].as_str()).collect();
    let first = groups[0].expect("group_id present");
    assert!(first.starts_with("seq-"));
    for g in &groups {
        assert_eq!(*g, Some(first));
    }

    // Disk reverted.
    for n in 1..=3 {
        assert!(cwd.join(format!("assets/FrameShot_{n:04}.png")).exists());
        assert!(!cwd.join(format!("assets/frame_shot_{n:04}.png")).exists());
    }
    Ok(())
}

#[test]
fn entry_flag_undoes_only_the_head_of_a_group() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    for n in 1..=3 {
        touch(cwd, &format!("assets/FrameShot_{n:04}.png"))?;
    }
    Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "clean",
            "assets",
            "--case",
            "snake",
            "--strip-suffix",
            "--apply",
        ])
        .output()?;

    let (report, code) = run_undo(cwd, &["--entry"]);
    assert_eq!(code, 0);
    let rows = report.as_array().unwrap();
    assert_eq!(rows.len(), 1, "--entry must touch exactly one row");

    // Two of three should still be in their snake-cased form; exactly
    // one (the head) should be back to the original.
    let mut original_count = 0;
    let mut snake_count = 0;
    for n in 1..=3 {
        if cwd.join(format!("assets/FrameShot_{n:04}.png")).exists() {
            original_count += 1;
        }
        if cwd.join(format!("assets/frame_shot_{n:04}.png")).exists() {
            snake_count += 1;
        }
    }
    assert_eq!(original_count, 1);
    assert_eq!(snake_count, 2);
    Ok(())
}
