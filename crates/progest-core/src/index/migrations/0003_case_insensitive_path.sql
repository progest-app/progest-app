-- Case-insensitive path uniqueness for Windows and default macOS.
--
-- On case-insensitive filesystems (Windows NTFS, default macOS HFS+)
-- the same file can be indexed with different casing, producing
-- duplicate rows. This migration:
--
--  1. Deduplicates existing rows (keeps the first inserted per
--     case-folded path).
--  2. Rebuilds `files` with `COLLATE NOCASE` on the `path` column
--     so future INSERTs and lookups are case-insensitive.
--  3. Recreates indexes and FTS5 triggers dropped by the table
--     rebuild.

-- 1. Deduplicate: keep first row per case-folded path.
DELETE FROM files WHERE rowid NOT IN (
    SELECT MIN(rowid) FROM files GROUP BY path COLLATE NOCASE
);

-- 2. Rebuild table with COLLATE NOCASE on path.
CREATE TABLE files_new (
    file_id          TEXT PRIMARY KEY,
    path             TEXT NOT NULL UNIQUE COLLATE NOCASE,
    fingerprint      TEXT NOT NULL,
    source_file_id   TEXT,
    kind             TEXT NOT NULL,
    status           TEXT NOT NULL,
    size             INTEGER,
    mtime            INTEGER,
    created_at       TEXT,
    last_seen_at     TEXT,
    name             TEXT,
    ext              TEXT,
    notes            TEXT,
    updated_at       TEXT,
    is_orphan        INTEGER NOT NULL DEFAULT 0
);
INSERT INTO files_new SELECT * FROM files;
DROP TABLE files;
ALTER TABLE files_new RENAME TO files;

-- 3. Recreate indexes (from 0001 + 0002).
CREATE INDEX idx_files_path        ON files(path COLLATE NOCASE);
CREATE INDEX idx_files_fingerprint ON files(fingerprint);
CREATE INDEX idx_files_ext         ON files(ext);
CREATE INDEX idx_files_updated_at  ON files(updated_at);

-- 4. Recreate FTS5 triggers (from 0002).
--    The virtual table `files_fts` survives (it has its own storage)
--    but the triggers reference `files` which was rebuilt.
CREATE TRIGGER files_fts_after_insert
AFTER INSERT ON files
BEGIN
    INSERT INTO files_fts (file_id, name, notes)
    VALUES (NEW.file_id, COALESCE(NEW.name, ''), COALESCE(NEW.notes, ''));
END;

CREATE TRIGGER files_fts_after_update
AFTER UPDATE OF name, notes ON files
BEGIN
    DELETE FROM files_fts WHERE file_id = OLD.file_id;
    INSERT INTO files_fts (file_id, name, notes)
    VALUES (NEW.file_id, COALESCE(NEW.name, ''), COALESCE(NEW.notes, ''));
END;

CREATE TRIGGER files_fts_after_delete
AFTER DELETE ON files
BEGIN
    DELETE FROM files_fts WHERE file_id = OLD.file_id;
END;
