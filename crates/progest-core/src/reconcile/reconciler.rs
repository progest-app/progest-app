//! The [`Reconciler`] — three-way reconciliation between scan output,
//! sidecar `.meta` files, and the `SQLite` index.
//!
//! A single instance is reusable across calls because every method takes
//! `&self` and rebuilds the per-scan state (ignore rules, walker, diff maps)
//! locally. The reconciler holds only immutable borrows of the collaborators
//! so the same value can be shared across CLI commands and watch-driven
//! reconcile passes.

use std::collections::HashMap;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fs::{EntryKind, FileSystem, IgnoreRules, Metadata, ProjectPath, ScanEntry, Scanner};
use crate::identity::{FileId, Fingerprint, compute_fingerprint};
use crate::index::{FileRow, Index, SearchProjection};
use crate::meta::{Kind, MetaDocument, MetaStore, SIDECAR_SUFFIX, Status, sidecar_path};

use super::change_set::{ChangeSet, FsEvent};
use super::error::ReconcileError;
use super::report::{ApplyReport, ReconcileOutcome, ScanReport};

/// Orchestrates reconciliation passes against an FS, meta store, and index.
///
/// The reconciler is a plain borrow-holder; it is cheap to construct, holds
/// no per-pass state, and is `Send + Sync` whenever its collaborators are.
pub struct Reconciler<'a> {
    fs: &'a dyn FileSystem,
    meta: &'a dyn MetaStore,
    index: &'a dyn Index,
}

impl<'a> Reconciler<'a> {
    /// Construct a reconciler that will read and write through the supplied
    /// collaborators. Borrows are kept for the reconciler's lifetime so that
    /// callers can reuse existing trait objects without cloning.
    #[must_use]
    pub fn new(fs: &'a dyn FileSystem, meta: &'a dyn MetaStore, index: &'a dyn Index) -> Self {
        Self { fs, meta, index }
    }

    /// Walk the project from the root, reconciling every non-ignored file
    /// with the index and `.meta` sidecars.
    ///
    /// Behavior:
    /// - new file, no sidecar → mint a fresh `FileId`, write `.meta`, insert index row
    /// - new file, existing sidecar → load `.meta` (trust its `file_id`),
    ///   verify fingerprint, insert index row; `.meta` is rewritten only if
    ///   its recorded fingerprint disagrees with the bytes on disk
    /// - indexed file, cheap compare match (size + mtime) → untouched
    /// - indexed file, cheap compare miss → recompute fingerprint, update
    ///   `.meta` (if one exists and its fingerprint drifted), upsert index row
    /// - indexed row with no corresponding file → row deleted; `.meta` left
    ///   in place so doctor / the user can decide whether to clean it up
    /// - sidecar whose companion file is missing → recorded in
    ///   [`ScanReport::orphan_metas`] without any side effects
    pub fn full_scan(&self) -> Result<ScanReport, ReconcileError> {
        let rules = IgnoreRules::load(self.fs)?;
        let scanner = Scanner::new(self.fs.root().to_path_buf(), rules);

        let mut files: HashMap<ProjectPath, ScanEntry> = HashMap::new();
        let mut metas: Vec<ProjectPath> = Vec::new();

        for entry in scanner {
            let entry = entry?;
            if entry.kind != EntryKind::File {
                continue;
            }
            if is_sidecar(&entry.path) {
                metas.push(entry.path);
            } else {
                files.insert(entry.path.clone(), entry);
            }
        }

        let mut existing_by_path: HashMap<ProjectPath, FileRow> = self
            .index
            .list_files()?
            .into_iter()
            .map(|row| (row.path.clone(), row))
            .collect();

        // Sort for deterministic outcome order; `ignore::Walk` yields in
        // filesystem order which differs across platforms and makes the scan
        // report hard to assert against in tests.
        let mut file_entries: Vec<ScanEntry> = files.into_values().collect();
        file_entries.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));

        let mut outcomes = Vec::with_capacity(file_entries.len());
        for entry in &file_entries {
            let existing = existing_by_path.remove(&entry.path);
            let outcome = self.reconcile_present_file(entry, existing)?;
            outcomes.push(outcome);
        }

        // Any row still in `existing_by_path` had no matching file on disk.
        for (path, row) in existing_by_path {
            self.index.delete_file(&row.file_id)?;
            outcomes.push(ReconcileOutcome::Removed {
                file_id: row.file_id,
                path,
            });
        }

        let orphan_metas = metas
            .into_iter()
            .filter(|meta_path| match companion_of(meta_path) {
                Some(companion) => !self.fs.exists(&companion),
                None => false,
            })
            .collect();

        Ok(ScanReport {
            outcomes,
            orphan_metas,
        })
    }

    /// Apply the events in `changes` incrementally. Each event is reconciled
    /// against the index and `.meta` the same way [`Self::full_scan`] would
    /// have handled it — the helpers are shared so the two paths cannot drift.
    pub fn apply_changes(&self, changes: &ChangeSet) -> Result<ApplyReport, ReconcileError> {
        let mut outcomes = Vec::with_capacity(changes.len());
        for event in changes.iter() {
            match event {
                FsEvent::Added(path) | FsEvent::Modified(path) => {
                    if is_sidecar(path) {
                        // Sidecar churn is handled lazily — the reconciler
                        // acts on the companion file, not on `.meta` itself.
                        continue;
                    }
                    let Some(entry) = self.load_entry(path)? else {
                        // The FS event arrived but the file is already gone.
                        // Treat it like a Removed event so the index reflects
                        // on-disk reality.
                        if let Some(row) = self.index.get_file_by_path(path)? {
                            self.index.delete_file(&row.file_id)?;
                            outcomes.push(ReconcileOutcome::Removed {
                                file_id: row.file_id,
                                path: path.clone(),
                            });
                        }
                        continue;
                    };
                    let existing = self.index.get_file_by_path(path)?;
                    outcomes.push(self.reconcile_present_file(&entry, existing)?);
                }
                FsEvent::Removed(path) => {
                    if is_sidecar(path) {
                        continue;
                    }
                    if let Some(row) = self.index.get_file_by_path(path)? {
                        self.index.delete_file(&row.file_id)?;
                        outcomes.push(ReconcileOutcome::Removed {
                            file_id: row.file_id,
                            path: path.clone(),
                        });
                    }
                }
                FsEvent::Renamed { from, to } => {
                    if is_sidecar(from) || is_sidecar(to) {
                        continue;
                    }
                    // A rename preserves identity. If the index already knew
                    // the `from` path, keep its `file_id` and move the row to
                    // `to`; otherwise fall back to the normal add path.
                    let Some(entry) = self.load_entry(to)? else {
                        continue;
                    };
                    let existing_from = self.index.get_file_by_path(from)?;
                    if let Some(mut row) = existing_from {
                        let old_file_id = row.file_id;
                        row.path = to.clone();
                        row.size = Some(entry.size);
                        row.mtime = Some(system_time_to_unix(entry.mtime));
                        self.index.upsert_file(&row)?;
                        self.write_search_projection(&row, None)?;
                        outcomes.push(ReconcileOutcome::Updated {
                            file_id: old_file_id,
                            path: to.clone(),
                        });
                    } else {
                        let existing_to = self.index.get_file_by_path(to)?;
                        outcomes.push(self.reconcile_present_file(&entry, existing_to)?);
                    }
                }
            }
        }
        Ok(ApplyReport { outcomes })
    }

    /// Core merge routine shared by `full_scan` and `apply_changes`. Given a
    /// fresh [`ScanEntry`] and whatever row (if any) the index currently has
    /// for the same path, bring both the index and the `.meta` sidecar into
    /// agreement with the file on disk.
    fn reconcile_present_file(
        &self,
        entry: &ScanEntry,
        existing: Option<FileRow>,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        let path = &entry.path;
        let entry_mtime = system_time_to_unix(entry.mtime);

        if let Some(row) = existing {
            // Cheap compare: unchanged when both size and mtime match.
            if row.size == Some(entry.size) && row.mtime == Some(entry_mtime) {
                return Ok(ReconcileOutcome::Unchanged {
                    file_id: row.file_id,
                    path: path.clone(),
                });
            }
            // Something moved; recompute fingerprint and update everything.
            let fingerprint = self.fingerprint_of(path)?;
            self.sync_sidecar_fingerprint(path, row.file_id, fingerprint)?;
            let mut updated = row.clone();
            updated.fingerprint = fingerprint;
            updated.size = Some(entry.size);
            updated.mtime = Some(entry_mtime);
            self.index.upsert_file(&updated)?;
            self.write_search_projection(&updated, None)?;
            return Ok(ReconcileOutcome::Updated {
                file_id: updated.file_id,
                path: path.clone(),
            });
        }

        // The index does not yet know this path. Either the sidecar already
        // carries an identity we should respect, or we mint a fresh one.
        let sidecar = sidecar_path(path)?;
        let fingerprint = self.fingerprint_of(path)?;
        let file_id = if self.meta.exists(&sidecar) {
            let existing_doc = self.meta.load(&sidecar)?;
            if existing_doc.content_fingerprint != fingerprint {
                let mut updated = existing_doc.clone();
                updated.content_fingerprint = fingerprint;
                self.meta.save(&sidecar, &updated)?;
            }
            existing_doc.file_id
        } else {
            let fresh = FileId::new_v7();
            let doc = MetaDocument::new(fresh, fingerprint);
            self.meta.save(&sidecar, &doc)?;
            fresh
        };

        let row = FileRow {
            file_id,
            path: path.clone(),
            fingerprint,
            source_file_id: None,
            kind: Kind::Asset,
            status: Status::Active,
            size: Some(entry.size),
            mtime: Some(entry_mtime),
            created_at: None,
            last_seen_at: None,
        };
        self.index.upsert_file(&row)?;
        // Try to load the freshly-saved (or pre-existing) sidecar so
        // we can mirror `notes` into the search projection. Failures
        // are non-fatal — the row is already inserted.
        let notes = self
            .meta
            .load(&sidecar)
            .ok()
            .and_then(|d| d.notes)
            .map(|n| n.body);
        self.write_search_projection(&row, notes)?;
        Ok(ReconcileOutcome::Added {
            file_id,
            path: path.clone(),
        })
    }

    /// Compute and persist the search-projection columns
    /// (`name` / `ext` / `notes` / `updated_at` / `is_orphan`) for
    /// `row`. `notes` is supplied by the caller when it has the
    /// sidecar in hand; otherwise it stays untouched (None →
    /// previous value is overwritten with NULL, which is fine for
    /// search since the FTS5 trigger collapses NULL → '').
    ///
    /// `is_orphan` is always `false` here — orphans are tracked via
    /// `ScanReport::orphan_metas` and don't reach this code path.
    /// A future PR will surface them in `files` as orphan rows.
    fn write_search_projection(
        &self,
        row: &FileRow,
        notes: Option<String>,
    ) -> Result<(), ReconcileError> {
        let proj = build_search_projection(row, notes);
        self.index.set_search_projection(&row.file_id, &proj)?;
        Ok(())
    }

    /// Resolve a `ProjectPath` into a [`ScanEntry`] by reading filesystem
    /// metadata directly. Returns `Ok(None)` when the file has vanished
    /// between event emission and reconciliation.
    fn load_entry(&self, path: &ProjectPath) -> Result<Option<ScanEntry>, ReconcileError> {
        match self.fs.metadata(path) {
            Ok(Metadata {
                is_dir: false,
                size,
                mtime,
            }) => Ok(Some(ScanEntry {
                path: path.clone(),
                kind: EntryKind::File,
                size,
                mtime,
            })),
            // Ignore directories and already-vanished paths identically:
            // neither maps to a reconcile outcome.
            Ok(_) | Err(crate::fs::FsError::NotFound(_)) => Ok(None),
            Err(other) => Err(other.into()),
        }
    }

    /// Stream the file at `path` through blake3 to produce its fingerprint.
    fn fingerprint_of(&self, path: &ProjectPath) -> Result<Fingerprint, ReconcileError> {
        let bytes = self.fs.read(path)?;
        Ok(compute_fingerprint(Cursor::new(bytes))?)
    }

    /// Keep the sidecar's fingerprint aligned with the index when the index
    /// already owns the identity. Missing sidecars are left alone: the
    /// reconciler only forcefully writes sidecars for brand-new files.
    fn sync_sidecar_fingerprint(
        &self,
        path: &ProjectPath,
        file_id: FileId,
        fingerprint: Fingerprint,
    ) -> Result<(), ReconcileError> {
        let sidecar = sidecar_path(path)?;
        if !self.meta.exists(&sidecar) {
            // An index row existed without a sidecar — write one so future
            // reconciles treat this file as anchored.
            let doc = MetaDocument::new(file_id, fingerprint);
            self.meta.save(&sidecar, &doc)?;
            return Ok(());
        }
        let mut doc = self.meta.load(&sidecar)?;
        if doc.file_id != file_id {
            // Sidecar drifted to a different identity — trust the index for
            // now (it owns the live row) and correct the sidecar. A future
            // PR can surface this as an IdentityConflict via doctor.
            doc.file_id = file_id;
        }
        if doc.content_fingerprint != fingerprint {
            doc.content_fingerprint = fingerprint;
            self.meta.save(&sidecar, &doc)?;
        }
        Ok(())
    }
}

/// Return `true` when `path` looks like a `.meta` sidecar.
fn is_sidecar(path: &ProjectPath) -> bool {
    path.as_str().ends_with(SIDECAR_SUFFIX)
}

/// Build the search-projection columns from a freshly-upserted
/// [`FileRow`]. `name` and `ext` come from the path basename;
/// `updated_at` is the row's mtime formatted as RFC 3339 UTC;
/// `notes` is whatever the caller could load from the sidecar (or
/// `None`). `is_orphan` is always `false` here — orphan tracking
/// for the search projection is a future PR.
fn build_search_projection(row: &FileRow, notes: Option<String>) -> SearchProjection {
    let name = row.path.file_name().map(str::to_string);
    let ext = row.path.extension().map(str::to_ascii_lowercase);
    let updated_at = row.mtime.and_then(unix_to_iso);
    SearchProjection {
        name,
        ext,
        notes,
        updated_at,
        is_orphan: false,
    }
}

/// Format a Unix-second timestamp as `YYYY-MM-DDTHH:MM:SSZ` in UTC.
fn unix_to_iso(seconds: i64) -> Option<String> {
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)?;
    Some(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// Strip the `.meta` suffix from a sidecar path, yielding the companion file
/// path. Returns `None` when `path` is not a sidecar.
fn companion_of(path: &ProjectPath) -> Option<ProjectPath> {
    let raw = path.as_str();
    let base = raw.strip_suffix(SIDECAR_SUFFIX)?;
    ProjectPath::new(base).ok()
}

/// Convert a [`SystemTime`] into Unix seconds, defaulting to zero for times
/// before the epoch so `i64` never wraps.
fn system_time_to_unix(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
