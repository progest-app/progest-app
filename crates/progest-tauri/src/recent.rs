//! Machine-global "recent projects" log used by the open-project flow.
//!
//! Lives at `<data_local_dir>/Progest/recent-projects.json` so it
//! survives reopening any individual project (this is the analogue of
//! IDE-style "Recent Projects" lists). Each entry records the absolute
//! root path, the project's display name, and the timestamp of the
//! most recent open.
//!
//! Retention is fixed at [`MAX_ENTRIES`]; the older entries are
//! dropped on every [`record`]. Duplicate paths are de-duplicated by
//! moving the existing entry to the front.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FILENAME: &str = "recent-projects.json";
const APP_DIRNAME: &str = "Progest";
const SCHEMA_VERSION: u32 = 1;

/// Hard cap on retained entries.
pub const MAX_ENTRIES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub root: String,
    pub name: String,
    pub last_opened: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecentDocument {
    schema_version: u32,
    #[serde(default)]
    entries: Vec<RecentProject>,
}

impl Default for RecentDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

/// Resolve the OS-specific recent-projects path. Returns `None` when
/// the platform doesn't expose a data-local directory (very rare —
/// effectively just very old Linux setups without `XDG_DATA_HOME` or
/// `$HOME`).
pub fn recent_path() -> Option<PathBuf> {
    let base = dirs::data_local_dir()?;
    Some(base.join(APP_DIRNAME).join(FILENAME))
}

/// Load the list. Missing file or schema mismatch falls back to
/// empty so the UI can still render — this is a UX log, not data we
/// need to be strict about.
pub fn load() -> Vec<RecentProject> {
    let Some(path) = recent_path() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(doc) = serde_json::from_slice::<RecentDocument>(&bytes) else {
        return Vec::new();
    };
    if doc.schema_version != SCHEMA_VERSION {
        return Vec::new();
    }
    doc.entries
}

/// Persist `entries` atomically (write to a tmp sibling, then rename).
pub fn save(entries: &[RecentProject]) -> std::io::Result<()> {
    let Some(path) = recent_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = RecentDocument {
        schema_version: SCHEMA_VERSION,
        entries: entries.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&doc).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    #[cfg(windows)]
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Record `root`/`name` as the most-recent entry. Existing entries
/// matching the same canonical root are removed first so the
/// timestamp tracks the latest open.
pub fn record(root: &Path, name: &str, ts: DateTime<Utc>) -> std::io::Result<Vec<RecentProject>> {
    let canonical = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical_str = canonical.display().to_string();
    let mut entries = load();
    entries.retain(|e| e.root != canonical_str);
    entries.insert(
        0,
        RecentProject {
            root: canonical_str,
            name: name.to_string(),
            last_opened: ts,
        },
    );
    if entries.len() > MAX_ENTRIES {
        entries.truncate(MAX_ENTRIES);
    }
    save(&entries)?;
    Ok(entries)
}

/// Drop every recorded entry on disk.
pub fn clear() -> std::io::Result<()> {
    save(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn record_dedups_and_orders_by_recency() {
        // Operate on a synthetic in-memory list to avoid touching the
        // real OS data dir from CI / dev machines.
        let mut entries: Vec<RecentProject> = Vec::new();
        let push = |entries: &mut Vec<RecentProject>, root: &str, name: &str, t: DateTime<Utc>| {
            entries.retain(|e| e.root != root);
            entries.insert(
                0,
                RecentProject {
                    root: root.to_string(),
                    name: name.to_string(),
                    last_opened: t,
                },
            );
            if entries.len() > MAX_ENTRIES {
                entries.truncate(MAX_ENTRIES);
            }
        };
        push(&mut entries, "/a", "A", ts(0));
        push(&mut entries, "/b", "B", ts(1));
        push(&mut entries, "/a", "A", ts(2));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].root, "/a");
        assert_eq!(entries[0].last_opened, ts(2));
        assert_eq!(entries[1].root, "/b");
    }
}
