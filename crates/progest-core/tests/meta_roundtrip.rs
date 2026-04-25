//! End-to-end checks for `progest_core::meta` exercised against a real
//! tempdir rather than `MemFileSystem`, so that the atomic-write guarantees
//! (temp file + rename) and the `.meta` naming convention are verified the
//! same way downstream consumers (`cli`, `tauri`) will use them.
//!
//! Unit tests inside the crate cover the TOML schema and the `MetaStore`
//! trait against the in-memory fake; this file is the integration-level
//! sanity net.

mod support;

use std::fs;

use progest_core::fs::{ProjectPath, StdFileSystem};
use progest_core::identity::FileId;
use progest_core::meta::{
    MetaDocument, MetaStore, NotesSection, SIDECAR_SUFFIX, StdMetaStore, TagsSection, sidecar_path,
};
use tempfile::TempDir;
use toml::Table;

use support::sample_fingerprint;

#[test]
fn save_and_load_round_trip_through_real_filesystem() {
    let tmp = TempDir::new().unwrap();
    let store = StdMetaStore::new(StdFileSystem::new(tmp.path().to_path_buf()));

    let file = ProjectPath::new("assets/hero.psd").unwrap();
    let sidecar = sidecar_path(&file).unwrap();
    assert_eq!(sidecar.as_str(), "assets/hero.psd.meta");

    let mut doc = MetaDocument::new(FileId::new_v7(), sample_fingerprint());
    doc.tags = Some(TagsSection {
        list: vec!["forest".into(), "approved".into()],
        extra: Table::new(),
    });
    doc.notes = Some(NotesSection {
        body: "replacement candidate exists".into(),
        extra: Table::new(),
    });

    store.save(&sidecar, &doc).unwrap();
    let reloaded = store.load(&sidecar).unwrap();
    assert_eq!(reloaded, doc);

    // The sidecar lands on disk at exactly the sibling path callers expect.
    assert!(tmp.path().join("assets/hero.psd.meta").exists());
}

#[test]
fn atomic_save_leaves_no_tmp_file_behind() {
    let tmp = TempDir::new().unwrap();
    let store = StdMetaStore::new(StdFileSystem::new(tmp.path().to_path_buf()));

    let sidecar = ProjectPath::new("foo.psd.meta").unwrap();
    let doc = MetaDocument::new(FileId::new_v7(), sample_fingerprint());
    store.save(&sidecar, &doc).unwrap();

    let stray = tmp.path().join(format!("foo.psd{SIDECAR_SUFFIX}.tmp"));
    assert!(
        !stray.exists(),
        "stale {} left behind after atomic save",
        stray.display()
    );
}

#[test]
fn preserves_hand_edited_unknown_fields_across_a_save_cycle() {
    // Simulate a future Progest version (or a sibling tool) having written a
    // field this build doesn't know about. Loading here, modifying a known
    // field, then saving must not lose the unknown key.
    let tmp = TempDir::new().unwrap();
    let sidecar_on_disk = tmp.path().join("assets/hero.psd.meta");
    fs::create_dir_all(sidecar_on_disk.parent().unwrap()).unwrap();
    fs::write(
        &sidecar_on_disk,
        r#"
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"
source_file_id = ""
future_top_level = "hello"

[core]
kind = "asset"
status = "active"
future_core_field = 42

[future_section]
note = "from tomorrow"
"#,
    )
    .unwrap();

    let store = StdMetaStore::new(StdFileSystem::new(tmp.path().to_path_buf()));
    let sidecar = ProjectPath::new("assets/hero.psd.meta").unwrap();

    let mut doc = store.load(&sidecar).unwrap();
    // Modify a known field — the interesting question is whether the unknown
    // data survives the write.
    doc.tags = Some(TagsSection {
        list: vec!["approved".into()],
        extra: Table::new(),
    });
    store.save(&sidecar, &doc).unwrap();

    let raw = fs::read_to_string(&sidecar_on_disk).unwrap();
    assert!(
        raw.contains("future_top_level = \"hello\""),
        "lost unknown top-level key; got:\n{raw}"
    );
    assert!(
        raw.contains("future_core_field = 42"),
        "lost unknown key inside a known section; got:\n{raw}"
    );
    assert!(
        raw.contains("[future_section]") && raw.contains("note = \"from tomorrow\""),
        "lost unknown section; got:\n{raw}"
    );
    assert!(
        raw.contains("approved"),
        "lost the modification we made; got:\n{raw}"
    );

    // And the reloaded document matches what we saved.
    let reloaded = store.load(&sidecar).unwrap();
    assert_eq!(reloaded.tags.as_ref().unwrap().list, vec!["approved"]);
    assert_eq!(
        reloaded
            .extra
            .get("future_top_level")
            .and_then(toml::Value::as_str),
        Some("hello")
    );
}

#[test]
fn save_replaces_existing_sidecar_atomically() {
    // A second save with different content must leave the sidecar at the new
    // content, not a merged or truncated file.
    let tmp = TempDir::new().unwrap();
    let store = StdMetaStore::new(StdFileSystem::new(tmp.path().to_path_buf()));
    let sidecar = ProjectPath::new("foo.psd.meta").unwrap();

    let first_id = FileId::new_v7();
    let mut first = MetaDocument::new(first_id, sample_fingerprint());
    first.notes = Some(NotesSection {
        body: "first".into(),
        extra: Table::new(),
    });
    store.save(&sidecar, &first).unwrap();

    let second_id = FileId::new_v7();
    assert_ne!(first_id, second_id);
    let mut second = MetaDocument::new(second_id, sample_fingerprint());
    second.notes = Some(NotesSection {
        body: "second".into(),
        extra: Table::new(),
    });
    store.save(&sidecar, &second).unwrap();

    let raw = fs::read_to_string(tmp.path().join("foo.psd.meta")).unwrap();
    assert!(raw.contains(&second_id.to_string()));
    assert!(!raw.contains(&first_id.to_string()));
    assert!(raw.contains("second"));
    assert!(!raw.contains("first"));
}
