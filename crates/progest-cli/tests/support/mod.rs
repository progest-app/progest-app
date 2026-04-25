//! Shared scaffolding for `progest` CLI smoke tests.
//!
//! Every smoke test here drives the compiled binary against a
//! freshly-`init`-ed tempdir, drops a few files in, and asserts
//! against stdout / exit codes. The helpers below collapse the
//! boilerplate so individual tests focus on the scenario, not the
//! tempdir + `std::process::Command` dance.
//!
//! Cargo treats each file under `tests/` as a separate crate, so this
//! module is pulled in via `mod support;` from each smoke test file.
//! Items are flagged `#[allow(dead_code)]` because not every test
//! exercises every helper.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::Result;

/// Path to the freshly-compiled `progest` binary that Cargo wires in
/// for integration tests of binary crates.
pub fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_progest"))
}

/// Run `progest init --name <name>` in `cwd`, asserting success.
pub fn init_project(cwd: &Path, name: &str) -> Result<()> {
    let status = Command::new(binary_path())
        .current_dir(cwd)
        .args(["init", "--name", name])
        .status()?;
    assert!(status.success(), "progest init failed");
    Ok(())
}

/// Create an empty file at `cwd/rel`, creating intermediate
/// directories as needed.
pub fn touch(cwd: &Path, rel: &str) -> Result<()> {
    write_file(cwd, rel, "")
}

/// Write `body` to `cwd/rel`, creating intermediate directories.
pub fn write_file(cwd: &Path, rel: &str, body: impl AsRef<[u8]>) -> Result<()> {
    let path = cwd.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, body)?;
    Ok(())
}

/// Run `progest <args>` from `cwd` and return the captured `Output`.
/// Use this for tests that need to inspect stdout/stderr directly;
/// other tests should reach for [`run_json`] when the report is JSON.
pub fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(binary_path())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to invoke progest binary")
}
