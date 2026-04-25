-- M3 search schema additions.
--
-- Adds search-relevant columns to `files`, the FTS5 trigram virtual
-- table for free-text matching, the `custom_fields` typed-clause
-- index, and the `violations` table that backs `is:violation` /
-- `is:misplaced` clauses. All columns / tables start empty;
-- reconcile and lint will populate them in subsequent PRs.
--
-- Mirrors `docs/SEARCH_DSL.md` §14.2 ("SQLite / FTS5 設定") and
-- planner expectations in `core::search::plan`.

-- 1. Search-derivable columns on `files`.
--    `name` and `ext` are derivable from `path`; reconcile will
--    populate them. `notes` mirrors `.meta.notes`. `updated_at`
--    is the human-facing modification timestamp (mirrors mtime
--    but stored as ISO 8601 for FTS5 / range comparisons).
ALTER TABLE files ADD COLUMN name        TEXT;
ALTER TABLE files ADD COLUMN ext         TEXT;
ALTER TABLE files ADD COLUMN notes       TEXT;
ALTER TABLE files ADD COLUMN updated_at  TEXT;
ALTER TABLE files ADD COLUMN is_orphan   INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_files_ext         ON files(ext);
CREATE INDEX idx_files_updated_at  ON files(updated_at);

-- 2. Custom-field typed index.
--    Exactly one of value_text / value_int is non-NULL per row.
--    The (key, value_int) and (key, value_text) indexes back the
--    range and equality forms of the planner's custom_fields
--    EXISTS subqueries.
CREATE TABLE custom_fields (
    file_id    TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    value_text TEXT,
    value_int  INTEGER,
    PRIMARY KEY (file_id, key)
);

CREATE INDEX idx_custom_fields_key_int  ON custom_fields(key, value_int);
CREATE INDEX idx_custom_fields_key_text ON custom_fields(key, value_text);

-- 3. Lint violations index.
--    Populated by `core::lint::lint_paths` (M3 #5 wires the writer).
--    `severity` mirrors DSL §8.2 mode → severity mapping
--    ('strict' / 'warn' / 'hint'). `is:violation` matches strict + warn.
CREATE TABLE violations (
    file_id  TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    rule_id  TEXT NOT NULL,
    message  TEXT
);

CREATE INDEX idx_violations_file_id     ON violations(file_id);
CREATE INDEX idx_violations_category    ON violations(category);
CREATE INDEX idx_violations_severity    ON violations(severity);

-- 4. FTS5 trigram virtual table.
--    Indexes `name` and `notes` for free-text matching with
--    CJK-friendly trigram tokenization. `file_id` is unindexed
--    (used only as a join key).
CREATE VIRTUAL TABLE files_fts USING fts5(
    name,
    notes,
    file_id UNINDEXED,
    tokenize = 'trigram'
);

-- 5. FTS5 sync triggers.
--    Keep `files_fts` mirrored against `files.name` / `files.notes`.
--    Each trigger nulls collapse to '' so MATCH never sees NULL.
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
