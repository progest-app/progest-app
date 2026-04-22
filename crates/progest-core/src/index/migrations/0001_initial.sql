-- Initial schema for progest-core::index.
--
-- Mirrors docs/IMPLEMENTATION_PLAN.md §3 for the M1 core data layer:
--
--   * files           — one row per tracked path, keyed by file_id.
--   * tags            — many-to-one tags per file, cascade-deleted with the file.
--
-- FTS5 and custom_fields land in later milestones; this migration keeps the
-- schema small enough that reconcile + doctor can be written on top of it
-- without pulling in search concerns.

CREATE TABLE files (
    file_id          TEXT PRIMARY KEY,
    path             TEXT NOT NULL UNIQUE,
    fingerprint      TEXT NOT NULL,
    source_file_id   TEXT,
    kind             TEXT NOT NULL,
    status           TEXT NOT NULL,
    size             INTEGER,
    mtime            INTEGER,
    created_at       TEXT,
    last_seen_at     TEXT
);

CREATE INDEX idx_files_path        ON files(path);
CREATE INDEX idx_files_fingerprint ON files(fingerprint);

CREATE TABLE tags (
    file_id TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    PRIMARY KEY (file_id, tag)
);

CREATE INDEX idx_tags_tag ON tags(tag);
