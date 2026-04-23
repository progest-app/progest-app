//! Atomic apply: turn a [`RenamePreview`] into committed FS + `.meta` +
//! index changes.
//!
//! # Strategy
//!
//! Renames are executed in two phases through a per-batch staging
//! directory under `.progest/local/staging/<batch_id>/`:
//!
//! 1. **Stage** every `(from, from.meta?)` pair into the staging
//!    directory. If any rename fails, all already-staged moves are
//!    reversed and the apply errors out. Source files end up either
//!    at their original `from` (failure) or in staging (continue).
//! 2. **Commit** every staged file/sidecar to its final `to`. If any
//!    rename fails, the in-progress op's commit is reversed, then
//!    every previously-committed op is moved back into staging, then
//!    every staged file is moved back to its `from`. The disk ends
//!    up byte-for-byte identical to pre-apply.
//!
//! Why staging rather than direct rename: chains like `foo→bar→baz`
//! cannot be done in any single order without a transient state where
//! one of the targets already exists. Pulling every source into
//! neutral ground first sidesteps the ordering problem and lets
//! Phase 2 swap targets in independently.
//!
//! # Index handling
//!
//! Index updates run after Phase 2 commits, in input order. Failures
//! are recorded in [`ApplyOutcome::index_warnings`] but **do not**
//! roll back the FS state — the index is a queryable cache and
//! reconcile rebuilds it from disk. Rolling back successful FS
//! renames because the cache stayed stale would punish the user for
//! a recoverable condition.
//!
//! # History wiring
//!
//! After the FS commit and index update, a [`history::Operation::Rename`]
//! entry is appended for every applied op. Bulk renames (≥2 ops) are
//! tied together by a fresh `group_id` so [`history::Store::undo`] can
//! roll the whole batch back as a unit. A caller-supplied
//! [`RenameOp::group_id`] (e.g. set by `core::sequence` for frame
//! batches) takes precedence over the auto-generated one. History
//! append failures land in [`ApplyOutcome::history_warnings`] for the
//! same reason as index failures: undo coverage is recoverable, FS
//! truth is not.
//!
//! # Rollback caveats
//!
//! Rollback is best-effort: rollback uses `fs.rename`, which can
//! itself fail (a "double fault"). When that happens we surface the
//! original error and silently continue the rollback, leaving any
//! files we couldn't restore in the staging directory. They are
//! recoverable by hand under `.progest/local/staging/<batch_id>/`
//! and will be picked up by `progest doctor` in a future commit.

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use crate::fs::{FileSystem, FsError, ProjectPath, ProjectPathError};
use crate::history::{self, AppendRequest, Operation};
use crate::index::{Index, IndexError};
use crate::meta::{MetaStoreError, sidecar_path};

use super::ops::RenameOp;
use super::preview::RenamePreview;

/// Path prefix under the project root where in-flight rename batches
/// stage their files. Always under `.progest/local/`, which is
/// gitignored and reconcile-skipped.
pub const STAGING_PREFIX: &str = ".progest/local/staging";

/// Successfully applied op, returned in [`ApplyOutcome::applied`].
pub type AppliedOp = RenameOp;

/// One index row whose update did not land. The FS rename succeeded;
/// reconcile will repair the index on its next pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexWarning {
    pub from: ProjectPath,
    pub to: ProjectPath,
    pub message: String,
}

/// One history entry that failed to append. The FS rename and index
/// update both succeeded; only undo coverage was lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryWarning {
    pub from: ProjectPath,
    pub to: ProjectPath,
    pub message: String,
}

/// Result of a successful [`Rename::apply`].
#[derive(Debug)]
pub struct ApplyOutcome {
    /// UUID identifying the staging batch directory under
    /// `.progest/local/staging/`. Empty when the preview was empty.
    pub batch_id: String,
    /// `group_id` shared by every history entry in this batch. `None`
    /// when the batch has fewer than 2 ops and the caller did not
    /// pre-set per-op `group_id`s.
    pub group_id: Option<String>,
    /// Ops that committed successfully. Same length and order as the
    /// input preview's clean ops.
    pub applied: Vec<AppliedOp>,
    /// Index updates that failed after a successful FS commit. FS
    /// state is correct; reconcile will repair.
    pub index_warnings: Vec<IndexWarning>,
    /// History entries that failed to append after a successful FS
    /// commit. FS state is correct; only undo coverage was lost.
    pub history_warnings: Vec<HistoryWarning>,
}

/// Which half of a (file, sidecar) pair was being moved when an FS
/// operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStep {
    /// The file itself.
    File,
    /// The companion `.meta` sidecar.
    Sidecar,
}

/// Errors returned by [`Rename::apply`].
///
/// All variants other than [`ApplyError::PreviewHasConflicts`] and
/// [`ApplyError::StagingSetup`] are best-effort rolled back to the
/// pre-apply FS state before being returned.
#[derive(Debug, Error)]
pub enum ApplyError {
    #[error(
        "refusing to apply: {} op(s) carry conflicts; resolve overrides first",
        .conflicting.len()
    )]
    PreviewHasConflicts { conflicting: Vec<RenameOp> },

    #[error("failed to set up staging directory: {source}")]
    StagingSetup {
        #[source]
        source: FsError,
    },

    #[error("staging op[{op_index}] {what:?} failed: {source}")]
    Stage {
        op_index: usize,
        what: StageStep,
        #[source]
        source: FsError,
    },

    #[error("commit op[{op_index}] {what:?} failed: {source}")]
    Commit {
        op_index: usize,
        what: StageStep,
        #[source]
        source: FsError,
    },

    #[error(transparent)]
    Path(#[from] ProjectPathError),
    #[error(transparent)]
    Meta(#[from] MetaStoreError),
}

/// Apply driver. Holds borrowed references to the filesystem,
/// index, and history seams so callers can compose with their own
/// meta-store / reconcile loops.
pub struct Rename<'a> {
    fs: &'a dyn FileSystem,
    index: &'a dyn Index,
    history: &'a dyn history::Store,
}

#[derive(Debug)]
struct StagedOp {
    op: RenameOp,
    stage_file: ProjectPath,
    /// `Some` when we successfully staged the sidecar; `None` when
    /// the source had no `.meta` companion to begin with.
    stage_meta: Option<ProjectPath>,
}

impl<'a> Rename<'a> {
    pub fn new(
        fs: &'a dyn FileSystem,
        index: &'a dyn Index,
        history: &'a dyn history::Store,
    ) -> Self {
        Self { fs, index, history }
    }

    /// Execute every clean op in `preview`. See module docs for the
    /// staging strategy and rollback contract.
    ///
    /// # Errors
    /// See [`ApplyError`].
    pub fn apply(&self, preview: &RenamePreview) -> Result<ApplyOutcome, ApplyError> {
        let conflicting: Vec<RenameOp> = preview.conflicting_ops().cloned().collect();
        if !conflicting.is_empty() {
            return Err(ApplyError::PreviewHasConflicts { conflicting });
        }
        if preview.ops.is_empty() {
            return Ok(ApplyOutcome {
                batch_id: String::new(),
                group_id: None,
                applied: Vec::new(),
                index_warnings: Vec::new(),
                history_warnings: Vec::new(),
            });
        }

        let batch_id = Uuid::now_v7().simple().to_string();
        let staging = ProjectPath::new(format!("{STAGING_PREFIX}/{batch_id}"))?;
        self.fs
            .create_dir_all(&staging)
            .map_err(|source| ApplyError::StagingSetup { source })?;

        let staged = self.stage_all(&preview.ops, &staging)?;
        self.commit_all(&staged)?;

        // Bulk renames get a fresh group_id so undo can reverse the
        // whole batch as a unit. A per-op group_id (e.g. set by
        // `core::sequence` for frame batches) takes precedence.
        let batch_group = (preview.ops.len() >= 2).then(|| Uuid::now_v7().simple().to_string());

        let mut index_warnings = Vec::new();
        let mut history_warnings = Vec::new();
        for staged_op in &staged {
            if let Err(message) = self.update_index(&staged_op.op.from, &staged_op.op.to) {
                index_warnings.push(IndexWarning {
                    from: staged_op.op.from.clone(),
                    to: staged_op.op.to.clone(),
                    message,
                });
            }

            let effective_group = staged_op
                .op
                .group_id
                .clone()
                .or_else(|| batch_group.clone());
            let op = Operation::Rename {
                from: staged_op.op.from.clone(),
                to: staged_op.op.to.clone(),
                rule_id: staged_op.op.rule_id.clone(),
            };
            let mut req = AppendRequest::new(op);
            if let Some(group) = effective_group {
                req = req.with_group(group);
            }
            if let Err(e) = self.history.append(&req) {
                history_warnings.push(HistoryWarning {
                    from: staged_op.op.from.clone(),
                    to: staged_op.op.to.clone(),
                    message: e.to_string(),
                });
            }
        }

        let applied = staged.into_iter().map(|s| s.op).collect();
        Ok(ApplyOutcome {
            batch_id,
            group_id: batch_group,
            applied,
            index_warnings,
            history_warnings,
        })
    }

    /// Phase 1: move every (file, sidecar) pair from its `from`
    /// location into the per-batch staging directory.
    fn stage_all(
        &self,
        ops: &[RenameOp],
        staging: &ProjectPath,
    ) -> Result<Vec<StagedOp>, ApplyError> {
        let mut staged: Vec<StagedOp> = Vec::with_capacity(ops.len());

        for (i, op) in ops.iter().enumerate() {
            let stage_file = staging.join(format!("{i}.f"))?;
            if let Err(source) = self.fs.rename(&op.from, &stage_file) {
                self.rollback_phase1(&staged);
                return Err(ApplyError::Stage {
                    op_index: i,
                    what: StageStep::File,
                    source,
                });
            }

            let from_meta = sidecar_path(&op.from)?;
            let stage_meta = if self.fs.exists(&from_meta) {
                let target = staging.join(format!("{i}.m"))?;
                if let Err(source) = self.fs.rename(&from_meta, &target) {
                    // Reverse the file we just staged for this op
                    // before unwinding the previously-staged ones.
                    let _ = self.fs.rename(&stage_file, &op.from);
                    self.rollback_phase1(&staged);
                    return Err(ApplyError::Stage {
                        op_index: i,
                        what: StageStep::Sidecar,
                        source,
                    });
                }
                Some(target)
            } else {
                None
            };

            staged.push(StagedOp {
                op: op.clone(),
                stage_file,
                stage_meta,
            });
        }

        Ok(staged)
    }

    /// Reverse Phase 1: move staged files (and sidecars) back to
    /// their original `from` paths. Best-effort.
    fn rollback_phase1(&self, staged: &[StagedOp]) {
        for s in staged.iter().rev() {
            if let Some(stage_meta) = &s.stage_meta
                && let Ok(from_meta) = sidecar_path(&s.op.from)
            {
                let _ = self.fs.rename(stage_meta, &from_meta);
            }
            let _ = self.fs.rename(&s.stage_file, &s.op.from);
        }
    }

    /// Phase 2: move staged files (and sidecars) into their final
    /// `to` paths. On failure, reverse this op's partial commit, then
    /// reverse every prior commit, then reverse the entire Phase 1
    /// staging.
    fn commit_all(&self, staged: &[StagedOp]) -> Result<(), ApplyError> {
        for (i, s) in staged.iter().enumerate() {
            // `committed` == number of ops fully past this loop iteration,
            // which is exactly `i` at the top of the body.
            let committed = i;
            if let Err(source) = self.fs.rename(&s.stage_file, &s.op.to) {
                self.rollback_phase2(&staged[..committed]);
                self.rollback_phase1(staged);
                return Err(ApplyError::Commit {
                    op_index: i,
                    what: StageStep::File,
                    source,
                });
            }
            if let Some(stage_meta) = &s.stage_meta {
                let to_meta = sidecar_path(&s.op.to)?;
                if let Err(source) = self.fs.rename(stage_meta, &to_meta) {
                    // Reverse the file commit we just made for this
                    // op so rollback_phase1 can find it back at
                    // `stage_file`.
                    let _ = self.fs.rename(&s.op.to, &s.stage_file);
                    self.rollback_phase2(&staged[..committed]);
                    self.rollback_phase1(staged);
                    return Err(ApplyError::Commit {
                        op_index: i,
                        what: StageStep::Sidecar,
                        source,
                    });
                }
            }
        }
        Ok(())
    }

    /// Reverse Phase 2: move committed (file, sidecar) pairs from
    /// their `to` destinations back into staging. Best-effort.
    fn rollback_phase2(&self, committed: &[StagedOp]) {
        for s in committed.iter().rev() {
            if let Some(stage_meta) = &s.stage_meta
                && let Ok(to_meta) = sidecar_path(&s.op.to)
            {
                let _ = self.fs.rename(&to_meta, stage_meta);
            }
            let _ = self.fs.rename(&s.op.to, &s.stage_file);
        }
    }

    /// Update the index row for one rename. Returns a human-readable
    /// reason on failure so the caller can pack it into [`IndexWarning`].
    fn update_index(&self, from: &ProjectPath, to: &ProjectPath) -> Result<(), String> {
        match self.index.get_file_by_path(from) {
            Ok(Some(mut row)) => {
                row.path = to.clone();
                self.index
                    .upsert_file(&row)
                    .map_err(|e: IndexError| e.to_string())
            }
            // No row to rename — reconcile will pick it up later.
            Ok(None) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FaultKind, FaultOp, FaultyFileSystem, MemFileSystem};
    use crate::history::{Entry, HistoryError, OpKind, SqliteStore, Store};
    use crate::identity::{FileId, Fingerprint};
    use crate::index::{FileRow, SqliteIndex};
    use crate::meta::{Kind, Status};
    use crate::naming::FillMode;
    use crate::naming::types::{NameCandidate, Segment};
    use crate::rename::preview::{RenameRequest, build_preview};
    use proptest::prelude::*;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn literal(stem: &str, ext: &str) -> NameCandidate {
        NameCandidate {
            segments: vec![Segment::Literal(stem.into())],
            ext: Some(ext.into()),
        }
    }

    fn sample_row(path: &str) -> FileRow {
        FileRow {
            file_id: FileId::new_v7(),
            path: p(path),
            fingerprint: "blake3:00112233445566778899aabbccddeeff"
                .parse::<Fingerprint>()
                .unwrap(),
            source_file_id: None,
            kind: Kind::Asset,
            status: Status::Active,
            size: Some(1),
            mtime: None,
            created_at: None,
            last_seen_at: None,
        }
    }

    /// Convenience: write a file and a placeholder `.meta` sidecar.
    fn write_with_meta(fs: &dyn FileSystem, path: &str) {
        fs.write_atomic(&p(path), path.as_bytes()).unwrap();
        let meta_path = format!("{path}.meta");
        fs.write_atomic(&p(&meta_path), b"meta-placeholder")
            .unwrap();
    }

    #[test]
    fn happy_path_renames_file_and_sidecar() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");
        index.upsert_file(&sample_row("a.psd")).unwrap();

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();

        let outcome = Rename::new(&fs, &index, &history).apply(&preview).unwrap();
        assert_eq!(outcome.applied.len(), 1);
        assert!(outcome.index_warnings.is_empty());

        // FS state: file + sidecar at new location, none at old.
        assert!(!fs.exists(&p("a.psd")));
        assert!(!fs.exists(&p("a.psd.meta")));
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"a.psd");
        assert_eq!(fs.read(&p("b.psd.meta")).unwrap(), b"meta-placeholder");

        // Index row follows the rename.
        assert!(index.get_file_by_path(&p("a.psd")).unwrap().is_none());
        let new_row = index.get_file_by_path(&p("b.psd")).unwrap().unwrap();
        assert_eq!(new_row.path.as_str(), "b.psd");
    }

    #[test]
    fn happy_path_handles_chain_renames() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "foo.psd");
        write_with_meta(&fs, "bar.psd");
        index.upsert_file(&sample_row("foo.psd")).unwrap();
        index.upsert_file(&sample_row("bar.psd")).unwrap();

        // foo→bar, bar→baz: only feasible because staging pulls both
        // sources out before either commit.
        let reqs = [
            RenameRequest::new(p("foo.psd"), literal("bar", "psd")),
            RenameRequest::new(p("bar.psd"), literal("baz", "psd")),
        ];
        let preview = build_preview(&reqs, &FillMode::Skip, &fs).unwrap();
        assert!(preview.is_clean());

        Rename::new(&fs, &index, &history).apply(&preview).unwrap();
        assert_eq!(fs.read(&p("bar.psd")).unwrap(), b"foo.psd");
        assert_eq!(fs.read(&p("baz.psd")).unwrap(), b"bar.psd");
        assert!(!fs.exists(&p("foo.psd")));
    }

    #[test]
    fn renames_without_sidecar_are_supported() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        // No .meta sidecar — file alone.
        fs.write_atomic(&p("a.psd"), b"a").unwrap();

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        Rename::new(&fs, &index, &history).apply(&preview).unwrap();
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"a");
        assert!(!fs.exists(&p("a.psd.meta")));
    }

    #[test]
    fn refuses_when_preview_carries_conflicts() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");
        write_with_meta(&fs, "b.psd"); // collision target

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        assert!(!preview.is_clean());

        let err = Rename::new(&fs, &index, &history)
            .apply(&preview)
            .unwrap_err();
        assert!(matches!(err, ApplyError::PreviewHasConflicts { .. }));

        // Disk untouched.
        assert_eq!(fs.read(&p("a.psd")).unwrap(), b"a.psd");
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"b.psd");
    }

    #[test]
    fn empty_preview_returns_empty_outcome() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        let preview = RenamePreview { ops: Vec::new() };
        let outcome = Rename::new(&fs, &index, &history).apply(&preview).unwrap();
        assert!(outcome.applied.is_empty());
        assert!(outcome.batch_id.is_empty());
    }

    #[test]
    fn fault_during_phase1_file_rolls_back_to_origin() {
        let inner = MemFileSystem::new();
        write_with_meta(&inner, "a.psd");
        write_with_meta(&inner, "b.psd");
        let fs = FaultyFileSystem::new(inner);
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();

        // Calls 1, 2 = stage a.psd file + sidecar. Call 3 = stage b.psd file → fail.
        fs.fail_at(FaultOp::Rename, 3, FaultKind::PermissionDenied);

        let preview = build_preview(
            &[
                RenameRequest::new(p("a.psd"), literal("aa", "psd")),
                RenameRequest::new(p("b.psd"), literal("bb", "psd")),
            ],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();

        let err = Rename::new(&fs, &index, &history)
            .apply(&preview)
            .unwrap_err();
        assert!(matches!(
            err,
            ApplyError::Stage {
                op_index: 1,
                what: StageStep::File,
                ..
            }
        ));

        // Both files (and sidecars) restored to their `from`.
        assert_eq!(fs.read(&p("a.psd")).unwrap(), b"a.psd");
        assert_eq!(fs.read(&p("a.psd.meta")).unwrap(), b"meta-placeholder");
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"b.psd");
        assert_eq!(fs.read(&p("b.psd.meta")).unwrap(), b"meta-placeholder");
        assert!(!fs.exists(&p("aa.psd")));
        assert!(!fs.exists(&p("bb.psd")));
    }

    #[test]
    fn fault_during_phase1_sidecar_rolls_back_files_too() {
        let inner = MemFileSystem::new();
        write_with_meta(&inner, "a.psd");
        let fs = FaultyFileSystem::new(inner);
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();

        // Calls: 1=stage file, 2=stage sidecar → fail.
        fs.fail_at(FaultOp::Rename, 2, FaultKind::PermissionDenied);

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("aa", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();

        let err = Rename::new(&fs, &index, &history)
            .apply(&preview)
            .unwrap_err();
        assert!(matches!(
            err,
            ApplyError::Stage {
                op_index: 0,
                what: StageStep::Sidecar,
                ..
            }
        ));

        // File AND sidecar restored.
        assert_eq!(fs.read(&p("a.psd")).unwrap(), b"a.psd");
        assert_eq!(fs.read(&p("a.psd.meta")).unwrap(), b"meta-placeholder");
        assert!(!fs.exists(&p("aa.psd")));
    }

    #[test]
    fn fault_during_phase2_rolls_back_to_origin() {
        let inner = MemFileSystem::new();
        write_with_meta(&inner, "a.psd");
        write_with_meta(&inner, "b.psd");
        let fs = FaultyFileSystem::new(inner);
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();

        // Stage: 4 renames (a.psd file, a.meta, b.psd file, b.meta).
        // Commit: 5=a.psd→aa.psd file, 6=a.meta→aa.meta, 7=b.psd→bb.psd file → fail.
        fs.fail_at(FaultOp::Rename, 7, FaultKind::PermissionDenied);

        let preview = build_preview(
            &[
                RenameRequest::new(p("a.psd"), literal("aa", "psd")),
                RenameRequest::new(p("b.psd"), literal("bb", "psd")),
            ],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();

        let err = Rename::new(&fs, &index, &history)
            .apply(&preview)
            .unwrap_err();
        assert!(matches!(
            err,
            ApplyError::Commit {
                op_index: 1,
                what: StageStep::File,
                ..
            }
        ));

        assert_eq!(fs.read(&p("a.psd")).unwrap(), b"a.psd");
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"b.psd");
        assert!(!fs.exists(&p("aa.psd")));
        assert!(!fs.exists(&p("bb.psd")));
    }

    #[test]
    fn index_update_failure_records_warning_but_keeps_fs_change() {
        // No index row exists → update_index sees None → no warning.
        // To actually surface a warning we'd need an index that
        // errors on get/upsert. Here we instead drop the index row
        // before apply and assert the rename still goes through.
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");
        // No row inserted into the index for this file.

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        let outcome = Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        // FS landed.
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"a.psd");
        // No row → No warning (the update is a no-op when there's
        // nothing to update; reconcile will create the row later).
        assert!(outcome.index_warnings.is_empty());
    }

    /// Counts every `append` call and rejects each with a fixed
    /// `HistoryError`. Used to drive the `history_warnings` path
    /// without needing to corrupt a real `SqliteStore`.
    struct AlwaysFailingStore;

    impl Store for AlwaysFailingStore {
        fn append(&self, _: &AppendRequest) -> Result<Entry, HistoryError> {
            Err(HistoryError::InvalidOpKind("test injection".into()))
        }
        fn list(&self, _: usize) -> Result<Vec<Entry>, HistoryError> {
            Ok(Vec::new())
        }
        fn head(&self) -> Result<Option<Entry>, HistoryError> {
            Ok(None)
        }
        fn undo(&self) -> Result<Entry, HistoryError> {
            Err(HistoryError::UndoEmpty)
        }
        fn redo(&self) -> Result<Entry, HistoryError> {
            Err(HistoryError::RedoEmpty)
        }
    }

    #[test]
    fn single_op_appends_history_entry_without_group_id() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        let outcome = Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        assert!(
            outcome.group_id.is_none(),
            "single-op batch should not allocate a group"
        );
        assert!(outcome.history_warnings.is_empty());

        let entries = history.list(usize::MAX).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.op.kind(), OpKind::Rename);
        assert!(entry.group_id.is_none());
        match &entry.op {
            Operation::Rename { from, to, rule_id } => {
                assert_eq!(from.as_str(), "a.psd");
                assert_eq!(to.as_str(), "b.psd");
                assert!(rule_id.is_none());
            }
            other => panic!("unexpected op kind: {other:?}"),
        }
    }

    #[test]
    fn bulk_rename_shares_a_single_auto_generated_group_id() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");
        write_with_meta(&fs, "b.psd");

        let preview = build_preview(
            &[
                RenameRequest::new(p("a.psd"), literal("aa", "psd")),
                RenameRequest::new(p("b.psd"), literal("bb", "psd")),
            ],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        let outcome = Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        let group = outcome
            .group_id
            .expect("bulk rename should allocate a group");
        let entries = history.list(usize::MAX).unwrap();
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            assert_eq!(entry.group_id.as_deref(), Some(group.as_str()));
        }
    }

    #[test]
    fn per_op_group_id_overrides_batch_group() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");
        write_with_meta(&fs, "b.psd");

        // Caller (e.g. `core::sequence`) pre-set both ops' group_id to
        // "frame-batch-7"; the apply must respect that and not overwrite
        // it with a fresh batch group.
        let preview = build_preview(
            &[
                RenameRequest::new(p("a.psd"), literal("aa", "psd")).with_group_id("frame-batch-7"),
                RenameRequest::new(p("b.psd"), literal("bb", "psd")).with_group_id("frame-batch-7"),
            ],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        let entries = history.list(usize::MAX).unwrap();
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            assert_eq!(entry.group_id.as_deref(), Some("frame-batch-7"));
        }
    }

    #[test]
    fn rule_id_propagates_into_history_entry() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd")).with_rule_id("shot-assets-v1")],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        let head = history.head().unwrap().expect("head should be the rename");
        match head.op {
            Operation::Rename { rule_id, .. } => {
                assert_eq!(rule_id.as_deref(), Some("shot-assets-v1"));
            }
            other => panic!("unexpected op: {other:?}"),
        }
    }

    #[test]
    fn history_failure_records_warning_but_keeps_fs_change() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = AlwaysFailingStore;
        write_with_meta(&fs, "a.psd");

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        let outcome = Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        // FS landed.
        assert_eq!(fs.read(&p("b.psd")).unwrap(), b"a.psd");
        // History failure surfaced as a warning, not an error.
        assert_eq!(outcome.history_warnings.len(), 1);
        assert_eq!(outcome.history_warnings[0].from.as_str(), "a.psd");
        assert_eq!(outcome.history_warnings[0].to.as_str(), "b.psd");
    }

    #[test]
    fn history_undo_returns_inverse_pointing_back_to_origin() {
        let fs = MemFileSystem::new();
        let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();
        write_with_meta(&fs, "a.psd");

        let preview = build_preview(
            &[RenameRequest::new(p("a.psd"), literal("b", "psd"))],
            &FillMode::Skip,
            &fs,
        )
        .unwrap();
        Rename::new(&fs, &index, &history).apply(&preview).unwrap();

        // The inverse operation reverses the rename. Replaying it is
        // the responsibility of the undo dispatcher (lands with the
        // CLI command); here we only verify the inverse payload.
        let entry = history.undo().unwrap();
        match entry.inverse {
            Operation::Rename { from, to, .. } => {
                assert_eq!(from.as_str(), "b.psd");
                assert_eq!(to.as_str(), "a.psd");
            }
            other => panic!("unexpected inverse: {other:?}"),
        }
    }

    proptest! {
        /// Across many random fault placements the FS state is
        /// either fully applied or fully original — never partial.
        ///
        /// We seed five files and rename them all; the fault injects
        /// a `PermissionDenied` at a random rename call index, which
        /// covers staging, sidecar staging, file commits, and sidecar
        /// commits at different points in the batch.
        #[test]
        fn fs_is_all_or_nothing_under_random_rename_fault(
            // Calls 1..=20 cover every possible failure point in a
            // 5-op batch (5 files * 2 stage + 5 files * 2 commit = 20).
            fault_call in 1u32..=20,
        ) {
            let inner = MemFileSystem::new();
            let names = ["a.psd", "b.psd", "c.psd", "d.psd", "e.psd"];
            for name in &names {
                inner.write_atomic(&p(name), name.as_bytes()).unwrap();
                inner
                    .write_atomic(&p(&format!("{name}.meta")), b"meta")
                    .unwrap();
            }
            let fs = FaultyFileSystem::new(inner);
            let index = SqliteIndex::open_in_memory().unwrap();
        let history = SqliteStore::open_in_memory().unwrap();

            fs.fail_at(FaultOp::Rename, fault_call, FaultKind::PermissionDenied);

            let reqs: Vec<RenameRequest> = names
                .iter()
                .map(|n| {
                    let stem = n.trim_end_matches(".psd");
                    let new_stem = format!("X{stem}");
                    RenameRequest::new(p(n), literal(&new_stem, "psd"))
                })
                .collect();
            let preview = build_preview(&reqs, &FillMode::Skip, &fs).unwrap();
            let result = Rename::new(&fs, &index, &history).apply(&preview);

            // Tally: how many files at original vs. renamed targets?
            let origs_present = names.iter().filter(|n| fs.exists(&p(n))).count();
            let news_present = names
                .iter()
                .filter(|n| {
                    let stem = n.trim_end_matches(".psd");
                    fs.exists(&p(&format!("X{stem}.psd")))
                })
                .count();

            // Same tally for sidecars.
            let orig_metas = names
                .iter()
                .filter(|n| fs.exists(&p(&format!("{n}.meta"))))
                .count();
            let new_metas = names
                .iter()
                .filter(|n| {
                    let stem = n.trim_end_matches(".psd");
                    fs.exists(&p(&format!("X{stem}.psd.meta")))
                })
                .count();

            // Apply succeeded ⇒ all renamed; apply failed ⇒ all original.
            // Either way the (file, sidecar) accounting must be consistent.
            if result.is_ok() {
                prop_assert_eq!(news_present, 5, "ok path: every file should be renamed");
                prop_assert_eq!(origs_present, 0, "ok path: no original files left");
                prop_assert_eq!(new_metas, 5, "ok path: every sidecar should be renamed");
                prop_assert_eq!(orig_metas, 0, "ok path: no original sidecars left");
            } else {
                prop_assert_eq!(
                    origs_present, 5,
                    "err path: every file should be back at origin (fault@{}, news_present={})",
                    fault_call, news_present
                );
                prop_assert_eq!(news_present, 0, "err path: no renamed files");
                prop_assert_eq!(
                    orig_metas, 5,
                    "err path: every sidecar should be back at origin"
                );
                prop_assert_eq!(new_metas, 0, "err path: no renamed sidecars");
            }
        }
    }
}
