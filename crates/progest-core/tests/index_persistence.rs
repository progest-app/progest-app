//! End-to-end checks for `progest_core::index` exercised against a real
//! on-disk `SQLite` database. The unit tests inside the crate run against
//! `:memory:`; this suite closes and reopens the database to verify that
//! migrations and data survive across sessions the same way downstream
//! consumers (CLI `scan`, reconcile worker) will use them.

use progest_core::fs::ProjectPath;
use progest_core::identity::{FileId, Fingerprint};
use progest_core::index::{FileRow, Index, SqliteIndex};
use progest_core::meta::{Kind, Status};
use tempfile::TempDir;

fn sample_row(path: &str) -> FileRow {
    FileRow {
        file_id: FileId::new_v7(),
        path: ProjectPath::new(path).unwrap(),
        fingerprint: "blake3:00112233445566778899aabbccddeeff"
            .parse::<Fingerprint>()
            .unwrap(),
        source_file_id: None,
        kind: Kind::Asset,
        status: Status::Active,
        size: Some(4096),
        mtime: Some(1_713_600_000),
        created_at: Some("2026-04-21T10:00:00Z".into()),
        last_seen_at: Some("2026-04-22T08:00:00Z".into()),
    }
}

#[test]
fn data_persists_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("index.db");

    let row = sample_row("assets/hero.psd");
    let file_id = row.file_id;

    {
        let idx = SqliteIndex::open(&db_path).unwrap();
        idx.upsert_file(&row).unwrap();
        idx.tag_add(&file_id, "approved").unwrap();
        idx.tag_add(&file_id, "forest").unwrap();
    } // index dropped here — forces the connection closed before we reopen.

    let idx = SqliteIndex::open(&db_path).unwrap();
    let reloaded = idx.get_file(&file_id).unwrap().unwrap();
    assert_eq!(reloaded, row);

    let tags = idx.list_tags_for_file(&file_id).unwrap();
    assert_eq!(tags, vec!["approved", "forest"]);
}

#[test]
fn opening_an_existing_database_does_not_reapply_migrations() {
    // A brand-new database applies migration 1; reopening the same file must
    // not double-apply it. If the runner ever re-ran a migration, the
    // `CREATE TABLE files` inside 0001_initial.sql would fail on the second
    // open (the statement has no `IF NOT EXISTS`), and this loop would
    // panic.
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("index.db");

    let row = sample_row("assets/hero.psd");
    {
        let idx = SqliteIndex::open(&db_path).unwrap();
        idx.upsert_file(&row).unwrap();
    }

    // Reopen twice more; the data must survive and opening must stay
    // side-effect free.
    for _ in 0..2 {
        let idx = SqliteIndex::open(&db_path).unwrap();
        assert!(idx.get_file(&row.file_id).unwrap().is_some());
    }
}

#[test]
fn deleting_a_file_cascades_tags_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("index.db");

    let row = sample_row("assets/hero.psd");
    let file_id = row.file_id;

    {
        let idx = SqliteIndex::open(&db_path).unwrap();
        idx.upsert_file(&row).unwrap();
        idx.tag_add(&file_id, "approved").unwrap();
        idx.tag_add(&file_id, "night").unwrap();
        idx.delete_file(&file_id).unwrap();
    }

    let idx = SqliteIndex::open(&db_path).unwrap();
    assert!(idx.get_file(&file_id).unwrap().is_none());
    let tags = idx.list_tags_for_file(&file_id).unwrap();
    assert!(tags.is_empty(), "tags should have cascaded, got {tags:?}");
}

#[test]
fn many_rows_survive_round_trip_ordered_by_path() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("index.db");

    let paths = [
        "shots/s020/c001.mov",
        "assets/backgrounds/bg01.png",
        "notes.md",
        "assets/characters/hero.psd",
    ];

    {
        let idx = SqliteIndex::open(&db_path).unwrap();
        for p in paths {
            idx.upsert_file(&sample_row(p)).unwrap();
        }
    }

    let idx = SqliteIndex::open(&db_path).unwrap();
    let all = idx.list_files().unwrap();
    let ordered: Vec<&str> = all.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        ordered,
        vec![
            "assets/backgrounds/bg01.png",
            "assets/characters/hero.psd",
            "notes.md",
            "shots/s020/c001.mov",
        ]
    );
}
