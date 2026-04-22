//! Retry queue for `.meta` sidecar writes that failed mid-save.
//!
//! Atomic-write failures (EPERM during a checkout, a transient file-system
//! glitch, a user holding the file open in an editor) shouldn't discard
//! metadata. Instead, [`StdMetaStore::save`] enqueues the in-memory TOML
//! bytes into `.progest/local/pending/`, and every subsequent `MetaStore`
//! operation transparently drains the queue before doing its own work.
//!
//! The queue is a flat directory of TOML envelope files. One file per
//! target sidecar: re-enqueuing for the same sidecar simply overwrites the
//! earlier envelope, because only the latest content was ever meant to
//! land. On a successful retry the envelope file is removed; on another
//! failure the `attempts` counter is bumped and the envelope is rewritten.
//!
//! The directory itself is created on demand, so fresh projects and
//! test fixtures that have never failed a write don't carry a stale empty
//! directory on disk.

use std::io;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fs::{FileSystem, FsError, ProjectPath, ProjectPathError};

use super::document::MetaError;

/// Project-relative path of the pending queue directory.
pub const PENDING_DIR: &str = ".progest/local/pending";

/// Suffix used for queued envelope files inside [`PENDING_DIR`].
pub const PENDING_SUFFIX: &str = ".toml";

/// Errors surfaced by [`PendingQueue`] operations.
#[derive(Debug, Error)]
pub enum PendingError {
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Path(#[from] ProjectPathError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("pending envelope contains invalid UTF-8: {0}")]
    InvalidUtf8(String),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    Meta(#[from] MetaError),
}

/// On-disk representation of a queued write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEntry {
    /// Project-relative path of the sidecar this write targets.
    pub target: String,

    /// The would-be `.meta` TOML body. Stored as a string rather than a
    /// nested TOML table so we sidestep round-trip concerns for any unknown
    /// sections inside the document.
    pub document_toml: String,

    /// Number of retry attempts so far; zero when the entry is first
    /// enqueued.
    #[serde(default)]
    pub attempts: u32,

    /// Human-readable description of the most recent failure, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl PendingEntry {
    #[must_use]
    pub fn new(target: String, document_toml: String, error: Option<String>) -> Self {
        Self {
            target,
            document_toml,
            attempts: 0,
            last_error: error,
        }
    }

    /// Serialize this envelope to TOML for writing to the pending dir.
    pub fn to_toml(&self) -> Result<String, PendingError> {
        Ok(toml::to_string(self)?)
    }

    /// Parse an envelope from the TOML bytes on disk.
    pub fn from_toml(text: &str) -> Result<Self, PendingError> {
        Ok(toml::from_str(text)?)
    }
}

/// Per-target stable filename inside [`PENDING_DIR`].
///
/// Uses the blake3 hash of the target path so reenqueues collide on the
/// same file (latest content wins) and the name stays ASCII even when the
/// sidecar path contains non-ASCII characters.
#[must_use]
pub fn envelope_filename(target: &str) -> String {
    let hash = blake3::hash(target.as_bytes());
    let hex = hash.to_hex();
    // 24 hex chars = 96 bits, far beyond any realistic project's target set.
    let short = &hex.as_str()[..24];
    format!("{short}{PENDING_SUFFIX}")
}

fn envelope_project_path(target: &str) -> Result<ProjectPath, ProjectPathError> {
    ProjectPath::new(format!("{PENDING_DIR}/{}", envelope_filename(target)))
}

/// Handle to the pending queue for a single project.
///
/// Holds a [`FileSystem`] by reference so it composes with stores that do
/// not want to hand out ownership of their filesystem handle (notably
/// [`crate::fs::MemFileSystem`], which is intentionally not `Clone`).
pub struct PendingQueue<'a, F: FileSystem> {
    fs: &'a F,
}

/// Summary returned by [`PendingQueue::flush`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlushReport {
    /// Number of entries that were successfully retried and removed.
    pub drained: usize,
    /// Entries that failed again; their attempt counters were bumped and
    /// the envelopes remain on disk.
    pub retained: usize,
}

impl<'a, F: FileSystem> PendingQueue<'a, F> {
    #[must_use]
    pub fn new(fs: &'a F) -> Self {
        Self { fs }
    }

    /// Add or replace the envelope for `target`. Any previous pending write
    /// for the same target is overwritten — only the most recent body was
    /// ever meant to land.
    pub fn enqueue(
        &self,
        target: &ProjectPath,
        document_toml: String,
        last_error: Option<String>,
    ) -> Result<(), PendingError> {
        self.ensure_dir()?;
        let entry = PendingEntry::new(target.as_str().to_string(), document_toml, last_error);
        let path = envelope_project_path(target.as_str())?;
        self.fs.write_atomic(&path, entry.to_toml()?.as_bytes())?;
        Ok(())
    }

    /// List every envelope in the queue. Returns an empty vec when the
    /// pending directory does not yet exist.
    pub fn list(&self) -> Result<Vec<PendingEntry>, PendingError> {
        let dir = ProjectPath::new(PENDING_DIR)?;
        if !self.fs.exists(&dir) {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for name in self.list_filenames(&dir)? {
            let path = ProjectPath::new(format!("{PENDING_DIR}/{name}"))?;
            let bytes = match self.fs.read(&path) {
                Ok(b) => b,
                Err(FsError::NotFound(_)) => continue, // raced with a flush
                Err(e) => return Err(e.into()),
            };
            let text = String::from_utf8(bytes)
                .map_err(|_| PendingError::InvalidUtf8(path.to_string()))?;
            entries.push(PendingEntry::from_toml(&text)?);
        }
        // Stable order for deterministic flush + test assertions.
        entries.sort_by(|a, b| a.target.cmp(&b.target));
        Ok(entries)
    }

    /// Drop the on-disk envelope for `entry`.
    pub fn remove(&self, entry: &PendingEntry) -> Result<(), PendingError> {
        let path = envelope_project_path(&entry.target)?;
        match self.fs.remove_file(&path) {
            Ok(()) | Err(FsError::NotFound(_)) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Overwrite the envelope for `entry` with its current state — used
    /// after a retry fails to persist an updated `attempts` counter and
    /// `last_error` field.
    pub fn update(&self, entry: &PendingEntry) -> Result<(), PendingError> {
        let path = envelope_project_path(&entry.target)?;
        self.fs.write_atomic(&path, entry.to_toml()?.as_bytes())?;
        Ok(())
    }

    /// Try every queued write. `retry_fn` is invoked with the target path
    /// and the TOML body for each entry; returning `Ok` drops the envelope,
    /// returning `Err` bumps the attempt counter and keeps it.
    ///
    /// Errors from `retry_fn` never propagate out of `flush` — they're
    /// recorded on the retained envelope instead. An error from persisting
    /// that update does surface, because it usually points at a busted
    /// project state that the caller should react to.
    pub fn flush<RetryFn>(&self, mut retry_fn: RetryFn) -> Result<FlushReport, PendingError>
    where
        RetryFn: FnMut(&ProjectPath, &str) -> Result<(), FsError>,
    {
        let mut report = FlushReport::default();
        for mut entry in self.list()? {
            let Ok(target_path) = ProjectPath::new(entry.target.clone()) else {
                // Corrupt envelope — drop it rather than hang the queue.
                self.remove(&entry)?;
                continue;
            };
            match retry_fn(&target_path, &entry.document_toml) {
                Ok(()) => {
                    self.remove(&entry)?;
                    report.drained += 1;
                }
                Err(err) => {
                    entry.attempts = entry.attempts.saturating_add(1);
                    entry.last_error = Some(err.to_string());
                    self.update(&entry)?;
                    report.retained += 1;
                }
            }
        }
        Ok(report)
    }

    fn ensure_dir(&self) -> Result<(), PendingError> {
        let dir = ProjectPath::new(PENDING_DIR)?;
        self.fs.create_dir_all(&dir)?;
        Ok(())
    }

    fn list_filenames(&self, dir: &ProjectPath) -> Result<Vec<String>, PendingError> {
        // We intentionally avoid adding a list-directory method to the
        // FileSystem trait for a single consumer; instead peek at the
        // absolute path via `root()`. Consumers that want a pure-trait
        // experience can wrap this in a future `MemFileSystem` helper.
        let absolute = self.fs.root().join(dir.as_str());
        let mut out = Vec::new();
        let read = match std::fs::read_dir(&absolute) {
            Ok(iter) => iter,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in read {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(PENDING_SUFFIX) {
                out.push(name);
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::StdFileSystem;
    use tempfile::TempDir;

    struct Harness {
        _tmp: TempDir,
        fs: StdFileSystem,
    }

    impl Harness {
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let fs = StdFileSystem::new(tmp.path().to_path_buf());
            Self { _tmp: tmp, fs }
        }

        fn queue(&self) -> PendingQueue<'_, StdFileSystem> {
            PendingQueue::new(&self.fs)
        }
    }

    #[test]
    fn enqueue_creates_dir_and_persists_the_entry() {
        let h = Harness::new();
        let queue = h.queue();
        let fs = &h.fs;
        let target = ProjectPath::new("assets/hero.psd.meta").unwrap();
        queue
            .enqueue(&target, "body".into(), Some("io err".into()))
            .unwrap();

        assert!(fs.exists(&ProjectPath::new(PENDING_DIR).unwrap()));
        let entries = queue.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].target, "assets/hero.psd.meta");
        assert_eq!(entries[0].document_toml, "body");
        assert_eq!(entries[0].attempts, 0);
    }

    #[test]
    fn enqueue_same_target_overwrites_previous_body() {
        let h = Harness::new();
        let queue = h.queue();
        let target = ProjectPath::new("hero.psd.meta").unwrap();
        queue.enqueue(&target, "old".into(), None).unwrap();
        queue.enqueue(&target, "new".into(), None).unwrap();
        let entries = queue.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].document_toml, "new");
    }

    #[test]
    fn flush_drains_successful_retries_and_retains_failures() {
        let h = Harness::new();
        let queue = h.queue();
        let ok = ProjectPath::new("ok.psd.meta").unwrap();
        let err = ProjectPath::new("err.psd.meta").unwrap();
        queue.enqueue(&ok, "ok-body".into(), None).unwrap();
        queue.enqueue(&err, "err-body".into(), None).unwrap();

        let err_path = err.clone();
        let report = queue
            .flush(|target, _body| {
                if target.as_str() == err_path.as_str() {
                    Err(FsError::Io {
                        path: target.to_string(),
                        source: io::Error::other("synthetic"),
                    })
                } else {
                    Ok(())
                }
            })
            .unwrap();

        assert_eq!(report.drained, 1);
        assert_eq!(report.retained, 1);

        let remaining = queue.list().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].target, "err.psd.meta");
        assert_eq!(remaining[0].attempts, 1);
        assert!(remaining[0].last_error.is_some());
    }

    #[test]
    fn list_on_missing_queue_returns_empty_vec() {
        let h = Harness::new();
        let queue = h.queue();
        assert!(queue.list().unwrap().is_empty());
    }

    #[test]
    fn flush_on_missing_queue_is_a_noop() {
        let h = Harness::new();
        let queue = h.queue();
        let report = queue.flush(|_, _| Ok(())).unwrap();
        assert_eq!(report.drained, 0);
        assert_eq!(report.retained, 0);
    }
}
