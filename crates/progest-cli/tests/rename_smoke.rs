//! Smoke tests for `progest rename`.
//!
//! Build a tempdir project, drop a file with a name the cleanup
//! pipeline rewrites, and exercise the path-based + stdin-based
//! input modes against both `--preview` and `--apply`.

mod support;

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde_json::Value;
use tempfile::TempDir;

use support::{binary_path, init_project as init_named, touch};

fn init_project(cwd: &Path) -> Result<()> {
    init_named(cwd, "rename-smoke")
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
fn sequence_stem_renames_every_member_preserving_index_and_padding() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    // Three members of one frame sequence + one decoy file.
    touch(cwd, "shots/sc01/frame_0001.exr")?;
    touch(cwd, "shots/sc01/frame_0002.exr")?;
    touch(cwd, "shots/sc01/frame_0003.exr")?;
    touch(cwd, "shots/sc01/notes.md")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "rename",
            "shots/sc01",
            "--sequence-stem",
            "shot",
            "--mode",
            "apply",
            "--format",
            "json",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "rename --sequence-stem exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // All three members renamed; index + padding preserved.
    assert!(!cwd.join("shots/sc01/frame_0001.exr").exists());
    assert!(cwd.join("shots/sc01/shot_0001.exr").exists());
    assert!(cwd.join("shots/sc01/shot_0002.exr").exists());
    assert!(cwd.join("shots/sc01/shot_0003.exr").exists());
    // The decoy singleton is left alone.
    assert!(cwd.join("shots/sc01/notes.md").exists());

    // Outcome JSON reports a shared group_id (sequence members are
    // tied together for undo).
    let report: Value = serde_json::from_slice(&output.stdout)?;
    let group = report
        .get("group_id")
        .and_then(Value::as_str)
        .expect("sequence rename should populate group_id");
    assert!(
        group.starts_with("seq-"),
        "expected sequence-prefixed group_id, got {group}"
    );
    Ok(())
}

#[test]
fn sequence_stem_with_no_sequences_errors_out() -> Result<()> {
    let tmp = TempDir::new()?;
    let cwd = tmp.path();
    init_project(cwd)?;
    touch(cwd, "assets/foo.psd")?;
    touch(cwd, "assets/bar.psd")?;

    let output = Command::new(binary_path())
        .current_dir(cwd)
        .args([
            "rename",
            "assets",
            "--sequence-stem",
            "shot",
            "--mode",
            "preview",
        ])
        .output()?;
    assert!(
        !output.status.success(),
        "expected non-zero exit when no sequences are detected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no sequences detected"),
        "expected `no sequences detected` in stderr: {stderr}"
    );
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
