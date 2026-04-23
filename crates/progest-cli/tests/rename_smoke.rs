//! Smoke tests for `progest rename`.
//!
//! Build a tempdir project, drop a file with a name the cleanup
//! pipeline rewrites, and exercise the path-based + stdin-based
//! input modes against both `--preview` and `--apply`.

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
        .args(["init", "--name", "rename-smoke"])
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

#[test]
fn preview_path_mode_emits_clean_op_for_pascal_basename() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/MainRole_v01.png")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "rename",
            "assets/MainRole_v01.png",
            "--mode",
            "preview",
            "--format",
            "json",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "rename --preview exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout)?;
    let ops = report.get("ops").and_then(Value::as_array).expect("ops[]");
    let op = ops
        .iter()
        .find(|o| o.get("from").and_then(Value::as_str) == Some("assets/MainRole_v01.png"))
        .expect("op for MainRole_v01.png");
    assert_eq!(
        op.get("to").and_then(Value::as_str),
        Some("assets/main_role_v01.png")
    );
    assert!(
        op.get("conflicts").is_none()
            || op
                .get("conflicts")
                .and_then(Value::as_array)
                .unwrap()
                .is_empty()
    );
    Ok(())
}

#[test]
fn apply_path_mode_renames_file_on_disk_and_records_history() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/MainRole_v01.png")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "rename",
            "assets/MainRole_v01.png",
            "--mode",
            "apply",
            "--format",
            "json",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "rename --apply exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Disk reflects the rename.
    assert!(!cwd.join("assets/MainRole_v01.png").exists());
    assert!(cwd.join("assets/main_role_v01.png").exists());

    // JSON outcome reports a single applied op.
    let report: Value = serde_json::from_slice(&output.stdout)?;
    let paths = report
        .get("paths")
        .and_then(Value::as_array)
        .expect("paths[]");
    let target = paths
        .iter()
        .find(|p| p.get("from").and_then(Value::as_str) == Some("assets/MainRole_v01.png"))
        .expect("applied op for MainRole_v01.png");
    assert_eq!(
        target.get("to").and_then(Value::as_str),
        Some("assets/main_role_v01.png")
    );

    // History DB exists (proxy for "history was wired"). Detailed
    // entry-level assertions live in core::rename's apply tests.
    assert!(cwd.join(".progest/local/history.db").exists());
    Ok(())
}

#[test]
fn from_stdin_mode_applies_a_handcrafted_renameop() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/foo.psd")?;

    // Hand-craft a RenameOp[] payload — what `--preview --format=json
    // | jq '.ops'` would produce in a real pipeline.
    let stdin_payload = serde_json::json!([
        {
            "from": "assets/foo.psd",
            "to": "assets/bar.psd"
        }
    ])
    .to_string();

    let mut child = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "rename",
            "--from-stdin",
            "--mode",
            "apply",
            "--format",
            "json",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin_payload.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "rename --from-stdin --apply exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!cwd.join("assets/foo.psd").exists());
    assert!(cwd.join("assets/bar.psd").exists());
    Ok(())
}
