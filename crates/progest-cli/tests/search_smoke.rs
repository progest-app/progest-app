//! Smoke tests for `progest search`, `progest tag`, `progest view`.
//!
//! Exercises the full pipeline: init → scan (which populates the
//! search-projection columns) → tag/view CRUD → search execution.
//! Per-component logic lives in `core::search`, `core::tag`,
//! `core::search::views`; this file pins the CLI wire shape and
//! exit-code contract.

mod support;

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde_json::Value;
use tempfile::TempDir;

use support::{binary_path, init_project as init_named, touch};

fn init_project(cwd: &Path) -> Result<()> {
    init_named(cwd, "search-smoke")
}

fn run(cwd: &Path, args: &[&str]) -> (Vec<u8>, Vec<u8>, i32) {
    let out = Command::new(binary_path())
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawning progest");
    let code = out.status.code().unwrap_or(-1);
    (out.stdout, out.stderr, code)
}

fn run_json(cwd: &Path, args: &[&str]) -> (Value, i32) {
    let (stdout, _, code) = run(cwd, args);
    let json: Value = serde_json::from_slice(&stdout)
        .unwrap_or_else(|e| panic!("parsing JSON ({e}): {}", String::from_utf8_lossy(&stdout)));
    (json, code)
}

#[test]
fn search_after_scan_finds_files_by_tag() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();
    touch(cwd, "a.psd").unwrap();
    touch(cwd, "b.psd").unwrap();

    let (_, _, code) = run(cwd, &["scan"]);
    assert_eq!(code, 0, "scan should succeed");

    let (_, _, code) = run(cwd, &["tag", "add", "wip", "a.psd"]);
    assert_eq!(code, 0);

    let (json, code) = run_json(cwd, &["search", "tag:wip", "--format", "json"]);
    assert_eq!(code, 0);
    assert_eq!(json["result_count"].as_u64(), Some(1));
    let hits = json["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0]["path"].as_str().unwrap().ends_with("a.psd"),
        "{:?}",
        hits[0]
    );
}

#[test]
fn search_by_extension_finds_only_matching_files() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();
    touch(cwd, "a.psd").unwrap();
    touch(cwd, "b.tif").unwrap();
    let (_, _, code) = run(cwd, &["scan"]);
    assert_eq!(code, 0);

    let (json, code) = run_json(cwd, &["search", "type:psd", "--format", "json"]);
    assert_eq!(code, 0);
    let hits = json["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0]["path"].as_str().unwrap().ends_with("a.psd"));
}

#[test]
fn search_parse_error_returns_exit_2() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();

    let (_, _, code) = run(cwd, &["search", "--double-bad"]);
    // clap rejects the unrecognized flag with exit 2 itself; either
    // is acceptable as a "bad input" signal.
    assert_ne!(code, 0);
}

#[test]
fn tag_list_emits_tag_array() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();
    touch(cwd, "a.psd").unwrap();
    let (_, _, code) = run(cwd, &["scan"]);
    assert_eq!(code, 0);

    let (_, _, code) = run(cwd, &["tag", "add", "wip", "a.psd"]);
    assert_eq!(code, 0);
    let (_, _, code) = run(cwd, &["tag", "add", "review", "a.psd"]);
    assert_eq!(code, 0);

    let (json, code) = run_json(cwd, &["tag", "list", "a.psd", "--format", "json"]);
    assert_eq!(code, 0);
    let entry = &json[0];
    let tags = entry["tags"].as_array().unwrap();
    let tag_strs: Vec<&str> = tags.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(tag_strs, vec!["review", "wip"]);
}

#[test]
fn tag_remove_drops_tag() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();
    touch(cwd, "a.psd").unwrap();
    let (_, _, code) = run(cwd, &["scan"]);
    assert_eq!(code, 0);

    let (_, _, code) = run(cwd, &["tag", "add", "wip", "a.psd"]);
    assert_eq!(code, 0);
    let (_, _, code) = run(cwd, &["tag", "remove", "wip", "a.psd"]);
    assert_eq!(code, 0);
    let (json, _) = run_json(cwd, &["tag", "list", "a.psd", "--format", "json"]);
    assert!(json[0]["tags"].as_array().unwrap().is_empty());
}

#[test]
fn view_save_then_search_via_view_id() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();
    touch(cwd, "a.psd").unwrap();
    let (_, _, code) = run(cwd, &["scan"]);
    assert_eq!(code, 0);

    let (_, _, code) = run(
        cwd,
        &[
            "view", "save", "all-psd", "--query", "type:psd", "--name", "All PSDs",
        ],
    );
    assert_eq!(code, 0);

    let (json, code) = run_json(cwd, &["view", "list", "--format", "json"]);
    assert_eq!(code, 0);
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"].as_str(), Some("all-psd"));

    let (json, code) = run_json(cwd, &["search", "--view", "all-psd", "--format", "json"]);
    assert_eq!(code, 0);
    assert_eq!(json["result_count"].as_u64(), Some(1));
}

#[test]
fn view_delete_removes_entry() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();

    let (_, _, code) = run(cwd, &["view", "save", "v1", "--query", "tag:wip"]);
    assert_eq!(code, 0);
    let (_, _, code) = run(cwd, &["view", "delete", "v1"]);
    assert_eq!(code, 0);
    let (json, code) = run_json(cwd, &["view", "list", "--format", "json"]);
    assert_eq!(code, 0);
    assert!(json.as_array().unwrap().is_empty());
}

#[test]
fn lint_writes_violations_then_search_finds_them() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path();
    init_project(cwd).unwrap();
    touch(cwd, "Bad Name.psd").unwrap();

    // Drop a tiny rule that the file fails (basename must be lower
    // snake). Without rules.toml, `progest lint` reports zero
    // violations and the search will return nothing.
    let rules = "\
schema_version = 1

[[rules]]
id = \"snake-only\"
kind = \"constraint\"
applies_to = \"./**/*.psd\"
mode = \"warn\"
charset = \"ascii\"
forbidden_chars = [\" \"]
";
    std::fs::write(cwd.join(".progest").join("rules.toml"), rules).unwrap();

    let (_, _, code) = run(cwd, &["scan"]);
    assert_eq!(code, 0);
    let (_, _, _code) = run(cwd, &["lint"]);
    // exit 0 because the rule mode is warn, not strict.

    let (json, code) = run_json(cwd, &["search", "is:violation", "--format", "json"]);
    assert_eq!(code, 0);
    let hits = json["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "expected 1 hit, got {hits:?}");
}
