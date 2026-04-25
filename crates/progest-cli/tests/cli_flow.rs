//! End-to-end tests for the `progest` binary.
//!
//! We shell out to the compiled binary rather than calling Rust APIs
//! because the whole point is to validate exit codes, stdout, and the
//! filesystem effects a real user would see. `std::process::Command`
//! keeps the dependency footprint small; richer ergonomics (snapshot
//! assertions, etc.) can arrive with `assert_cmd` later if needed.

mod support;

use std::process::Output;

use tempfile::TempDir;

use support::run;

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn init_materializes_the_project_layout() {
    let tmp = TempDir::new().unwrap();
    let out = run(tmp.path(), &["init", "--name", "Demo"]);
    assert!(out.status.success(), "init failed: {out:?}");

    let root = std::fs::canonicalize(tmp.path()).unwrap();
    assert!(root.join(".progest/project.toml").is_file());
    assert!(root.join(".progest/ignore").is_file());
    assert!(root.join(".progest/index.db").is_file());
    assert!(root.join(".progest/local").is_dir());
    assert!(root.join(".progest/thumbs").is_dir());
    assert!(root.join(".gitignore").is_file());

    let stdout = stdout_of(&out);
    assert!(stdout.contains("Demo"), "unexpected init output: {stdout}");
    assert!(stdout.contains("project.toml"));
}

#[test]
fn scan_reports_added_files_then_unchanged_on_repeat() {
    let tmp = TempDir::new().unwrap();
    run(tmp.path(), &["init"]);

    std::fs::write(tmp.path().join("hero.psd"), b"bytes").unwrap();

    let first = run(tmp.path(), &["scan"]);
    assert!(first.status.success());
    let first_stdout = stdout_of(&first);
    assert!(
        first_stdout.contains("added"),
        "expected `added` summary, got {first_stdout}"
    );

    let second = run(tmp.path(), &["scan"]);
    assert!(second.status.success());
    let second_stdout = stdout_of(&second);
    assert!(
        second_stdout.contains("unchanged"),
        "expected `unchanged` summary, got {second_stdout}"
    );
}

#[test]
fn doctor_surfaces_orphan_sidecars_with_exit_code_two() {
    let tmp = TempDir::new().unwrap();
    run(tmp.path(), &["init"]);
    std::fs::write(tmp.path().join("hero.psd"), b"bytes").unwrap();
    run(tmp.path(), &["scan"]);

    // Clean doctor: no orphans yet.
    let clean = run(tmp.path(), &["doctor"]);
    assert!(clean.status.success());
    assert_eq!(clean.status.code(), Some(0));

    // Delete the tracked file, leaving the sidecar behind.
    std::fs::remove_file(tmp.path().join("hero.psd")).unwrap();

    let dirty = run(tmp.path(), &["doctor"]);
    assert_eq!(
        dirty.status.code(),
        Some(2),
        "stdout: {}",
        stdout_of(&dirty)
    );
    let stdout = stdout_of(&dirty);
    assert!(stdout.contains("hero.psd.meta"));
}

#[test]
fn scan_outside_a_project_errors() {
    let tmp = TempDir::new().unwrap();
    let out = run(tmp.path(), &["scan"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("could not find a Progest project"),
        "unexpected stderr: {stderr}"
    );
}
