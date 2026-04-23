-- Initial schema for progest-core::history.
--
-- Single `entries` table carries one row per completed operation.
-- payload_json / inverse_json are opaque JSON blobs keyed by op_kind;
-- schema evolution inside a variant doesn't require a migration.
--
-- `meta` stores the pointer to the most recently applied (not
-- undone) entry. An empty log has no `pointer` row.

CREATE TABLE entries (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    ts             TEXT    NOT NULL,
    op_kind        TEXT    NOT NULL,
    payload_json   TEXT    NOT NULL,
    inverse_json   TEXT    NOT NULL,
    consumed       INTEGER NOT NULL DEFAULT 0,
    group_id       TEXT
);

CREATE INDEX idx_entries_ts       ON entries(ts);
CREATE INDEX idx_entries_consumed ON entries(consumed);
CREATE INDEX idx_entries_group    ON entries(group_id);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
