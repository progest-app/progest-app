//! OS-level filesystem watcher feeding debounced [`ChangeSet`]s to reconcile.
//!
//! The watcher is a thin wrapper over [`notify_debouncer_full`]. It owns an
//! OS-specific backend (`FSEvents` on macOS, inotify on Linux) and a worker
//! thread that translates notify events into [`FsEvent`]s, filters them
//! through [`IgnoreRules`] and the `.meta` sidecar suffix, bundles the
//! remainder into a [`ChangeSet`], and forwards each batch to the caller
//! via a plain [`std::sync::mpsc::Receiver`].
//!
//! Two design choices worth calling out:
//!
//! - **Receiver, not callback.** A channel keeps the watcher detached from
//!   what consumes events, so the CLI's blocking loop and the Tauri runtime
//!   can both subscribe the same way without forcing core to pick an async
//!   runtime.
//! - **One batch per notify tick.** Every time the debouncer flushes, the
//!   worker emits at most one `ChangeSet`. Empty batches (after filtering)
//!   are dropped so consumers can `recv()` without polling for content.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ignore::Match;
use ignore::gitignore::Gitignore;
use notify::{EventKind, RecursiveMode, event::ModifyKind, event::RenameMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use tracing::warn;

use crate::fs::{IgnoreRules, ProjectPath, StdFileSystem};
use crate::reconcile::{ChangeSet, FsEvent};

use super::error::WatchError;

/// Default debounce window applied to raw notify events.
///
/// Rationale: 500 ms is long enough to coalesce the "save storm" emitted by
/// DCC tools like Photoshop (a single save fires ~5 writes) and short enough
/// that the UI feels live. Tune via [`Watcher::start_with_debounce`] when
/// benchmarking.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(500);

/// Handle to a running [`notify`] watcher and its translation worker.
///
/// Dropping the value tears both down. The manual [`Drop`] impl is careful
/// to drop the debouncer **before** joining the worker — the worker is
/// blocked on a `recv` that only unblocks when the debouncer's sender is
/// dropped, so the naive "join then drop" order deadlocks.
pub struct Watcher {
    debouncer: Option<Debouncer<notify::RecommendedWatcher, RecommendedCache>>,
    worker: Option<JoinHandle<()>>,
}

impl Watcher {
    /// Start watching `root` recursively, returning the handle and a receiver
    /// of filtered [`ChangeSet`]s. Uses [`DEFAULT_DEBOUNCE`].
    ///
    /// The watcher canonicalizes `root` internally (important on macOS, where
    /// `TempDir` returns a `/var/folders` path whose canonical form lives
    /// under `/private/var/folders` and `FSEvents` reports events using the
    /// canonical path) and loads [`IgnoreRules`] against the canonical root
    /// so the matcher and incoming event paths share the same prefix.
    pub fn start(root: PathBuf) -> Result<(Self, Receiver<ChangeSet>), WatchError> {
        Self::start_with_debounce(root, DEFAULT_DEBOUNCE)
    }

    /// Start watching `root` with a custom debounce window.
    pub fn start_with_debounce(
        root: PathBuf,
        debounce: Duration,
    ) -> Result<(Self, Receiver<ChangeSet>), WatchError> {
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        let fs = StdFileSystem::new(root.clone());
        let ignore = IgnoreRules::load(&fs).map_err(WatchError::Ignore)?;
        let matcher = ignore.matcher().clone();

        let (raw_tx, raw_rx) = channel::<DebounceEventResult>();
        let mut debouncer = new_debouncer(debounce, None, raw_tx)?;
        debouncer.watch(&root, RecursiveMode::Recursive)?;

        let (out_tx, out_rx) = channel::<ChangeSet>();
        let worker = thread::spawn(move || run_worker(&raw_rx, &out_tx, &root, &matcher));

        Ok((
            Self {
                debouncer: Some(debouncer),
                worker: Some(worker),
            },
            out_rx,
        ))
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // Drop the debouncer FIRST: it owns the sender side of the raw
        // event channel, and only once that sender is gone will the worker
        // wake from its blocking `recv` and exit. Joining before this point
        // deadlocks the current thread against the worker.
        self.debouncer.take();
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

/// Worker loop: read debounced batches until the debouncer drops its sender.
///
/// Errors from the backend are logged through `tracing` rather than surfaced
/// — they're usually recoverable (transient permission glitches during
/// checkout, for instance) and reconcile's periodic full scan covers any
/// gap they leave.
fn run_worker(
    raw_rx: &Receiver<DebounceEventResult>,
    out_tx: &Sender<ChangeSet>,
    root: &Path,
    matcher: &Gitignore,
) {
    while let Ok(result) = raw_rx.recv() {
        let events = match result {
            Ok(events) => events,
            Err(errors) => {
                for err in errors {
                    warn!(target: "progest::watch", error = %err, "notify backend error");
                }
                continue;
            }
        };
        let change_set = translate_events(&events, root, matcher);
        if change_set.is_empty() {
            continue;
        }
        if out_tx.send(change_set).is_err() {
            // Consumer dropped the receiver; shut down quietly.
            break;
        }
    }
}

/// Convert a batch of raw debounced events into a filtered [`ChangeSet`].
fn translate_events(
    events: &[notify_debouncer_full::DebouncedEvent],
    root: &Path,
    matcher: &Gitignore,
) -> ChangeSet {
    let mut out = ChangeSet::new();
    for debounced in events {
        if let Some(event) = translate_single(&debounced.event, root, matcher) {
            out.push(event);
        }
    }
    out
}

/// Translate one notify [`Event`](notify::Event) into at most one
/// [`FsEvent`]. Paths outside the project root or filtered by the ignore
/// matcher are discarded.
fn translate_single(event: &notify::Event, root: &Path, matcher: &Gitignore) -> Option<FsEvent> {
    match event.kind {
        EventKind::Create(_) => first_project_path(event, root, matcher).map(FsEvent::Added),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            let from_raw = event.paths.first()?;
            let to_raw = event.paths.get(1)?;
            let from = to_project_path(from_raw, root, matcher)?;
            let to = to_project_path(to_raw, root, matcher)?;
            Some(FsEvent::Renamed { from, to })
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            first_project_path(event, root, matcher).map(FsEvent::Added)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            first_project_path(event, root, matcher).map(FsEvent::Removed)
        }
        EventKind::Modify(_) => first_project_path(event, root, matcher).map(FsEvent::Modified),
        EventKind::Remove(_) => first_project_path(event, root, matcher).map(FsEvent::Removed),
        // Access / Other / Any: nothing reconcile-worthy.
        _ => None,
    }
}

/// Try to translate the event's first path into a keep-worthy [`ProjectPath`].
fn first_project_path(
    event: &notify::Event,
    root: &Path,
    matcher: &Gitignore,
) -> Option<ProjectPath> {
    event
        .paths
        .first()
        .and_then(|p| to_project_path(p, root, matcher))
}

/// Translate an absolute filesystem path into a project-relative
/// [`ProjectPath`], applying the ignore matcher. Returns `None` when the
/// path is outside `root`, fails validation, or matches an ignore rule.
///
/// Sidecar (`.meta`) paths are kept: the reconciler's `apply_changes` handles
/// them by routing to the companion file. Dropping them here would hide
/// legitimate changes (a teammate checking out a new `.meta`, for example).
fn to_project_path(path: &Path, root: &Path, matcher: &Gitignore) -> Option<ProjectPath> {
    let relative = path.strip_prefix(root).ok()?;
    // notify occasionally reports the root itself after the watcher attaches;
    // filter that out so we don't emit a FsEvent for the project directory.
    if relative.as_os_str().is_empty() {
        return None;
    }
    // is_dir is best-effort here — the path may already be gone (remove event),
    // in which case we conservatively treat it as a file for the matcher.
    let is_dir = path.is_dir();
    if matches!(
        matcher.matched_path_or_any_parents(path, is_dir),
        Match::Ignore(_)
    ) {
        return None;
    }
    // Sidecar (`.meta`) paths intentionally flow through; the reconciler's
    // apply_changes handles them by routing to the companion file.
    ProjectPath::from_absolute(root, path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn prep() -> (TempDir, Gitignore) {
        let tmp = TempDir::new().unwrap();
        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let matcher = rules.matcher().clone();
        (tmp, matcher)
    }

    #[test]
    fn to_project_path_drops_paths_outside_root() {
        let (tmp, matcher) = prep();
        let other = std::env::temp_dir().join("unrelated.txt");
        assert!(to_project_path(&other, tmp.path(), &matcher).is_none());
    }

    #[test]
    fn to_project_path_drops_the_root_itself() {
        let (tmp, matcher) = prep();
        assert!(to_project_path(tmp.path(), tmp.path(), &matcher).is_none());
    }

    #[test]
    fn to_project_path_filters_ignored_paths() {
        let (tmp, matcher) = prep();
        let ignored = tmp.path().join(".progest").join("index.db");
        assert!(to_project_path(&ignored, tmp.path(), &matcher).is_none());
    }

    #[test]
    fn to_project_path_accepts_sidecar_paths() {
        // The reconciler's apply_changes routes `.meta` events to the
        // companion file — we must not silently drop them here.
        let (tmp, matcher) = prep();
        let sidecar = tmp.path().join("foo.psd.meta");
        let project = to_project_path(&sidecar, tmp.path(), &matcher).unwrap();
        assert_eq!(project.as_str(), "foo.psd.meta");
    }
}
