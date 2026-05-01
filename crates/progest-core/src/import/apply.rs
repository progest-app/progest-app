//! Atomic apply: turn a clean [`ImportPreview`] into committed FS +
//! `.meta` + index changes.
//!
//! # Strategy
//!
//! Import uses the same staging directory as rename
//! (`.progest/local/staging/<batch_id>/`) but with a simpler flow:
//!
//! 1. **Copy/move** each source into a staging slot.
//! 2. **Commit** each staged file to its final destination, creating
//!    parent directories as needed.
//! 3. **Post-commit**: generate `.meta` sidecar, register in index,
//!    append history entry. Failures here are warnings, not errors.
//!
//! Rollback on failure: if commit fails for any op, all previously
//! committed files are removed and staging is cleaned up.

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use crate::fs::{FileSystem, FsError, ProjectPath, ProjectPathError};
use crate::history::{self, AppendRequest, Operation};
use crate::identity::{FileId, compute_fingerprint};
use crate::index::{FileRow, Index};
use crate::meta::{Kind, MetaDocument, MetaStore, Status, sidecar_path};
use crate::rename::apply::STAGING_PREFIX;

use super::types::{ImportMode, ImportOp, ImportPreview};

/// Successfully imported file.
#[derive(Debug, Clone, Serialize)]
pub struct ImportedFile {
    pub source: String,
    pub dest: ProjectPath,
    pub file_id: FileId,
    pub mode: ImportMode,
}

/// Non-fatal post-commit anomaly (same philosophy as rename's
/// `ApplyWarning`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportWarning {
    MetaCreate { dest: ProjectPath, message: String },
    IndexInsert { dest: ProjectPath, message: String },
    HistoryAppend { dest: ProjectPath, message: String },
    FingerprintFailed { dest: ProjectPath, message: String },
}

/// Result of a successful import apply.
#[derive(Debug)]
pub struct ImportOutcome {
    pub batch_id: String,
    pub group_id: Option<String>,
    pub imported: Vec<ImportedFile>,
    pub warnings: Vec<ImportWarning>,
}

/// Errors returned by [`Import::apply`].
#[derive(Debug, Error)]
pub enum ImportApplyError {
    #[error(
        "refusing to apply: {} op(s) carry conflicts",
        .count
    )]
    HasConflicts { count: usize },

    #[error("failed to set up staging directory: {source}")]
    StagingSetup {
        #[source]
        source: FsError,
    },

    #[error("staging op[{op_index}] failed: {message}")]
    Stage { op_index: usize, message: String },

    #[error("commit op[{op_index}] failed: {message}")]
    Commit { op_index: usize, message: String },

    #[error(transparent)]
    Path(#[from] ProjectPathError),
}

/// Apply driver for import operations.
pub struct Import<'a> {
    fs: &'a dyn FileSystem,
    meta_store: &'a dyn MetaStore,
    index: &'a dyn Index,
    history: Option<&'a dyn history::Store>,
    project_root: &'a std::path::Path,
}

struct StagedImport {
    op: ImportOp,
    stage_path: ProjectPath,
}

impl<'a> Import<'a> {
    pub fn new(
        fs: &'a dyn FileSystem,
        meta_store: &'a dyn MetaStore,
        index: &'a dyn Index,
        history: &'a dyn history::Store,
        project_root: &'a std::path::Path,
    ) -> Self {
        Self {
            fs,
            meta_store,
            index,
            history: Some(history),
            project_root,
        }
    }

    /// Apply all clean ops in the preview.
    pub fn apply(&self, preview: &ImportPreview) -> Result<ImportOutcome, ImportApplyError> {
        self.apply_with_progress(preview, &|_, _, _| {})
    }

    /// Apply all clean ops with per-file progress reporting.
    pub fn apply_with_progress(
        &self,
        preview: &ImportPreview,
        on_progress: &dyn Fn(u64, u64, &str),
    ) -> Result<ImportOutcome, ImportApplyError> {
        let conflict_count = preview.conflicting_ops().count();
        if conflict_count > 0 {
            return Err(ImportApplyError::HasConflicts {
                count: conflict_count,
            });
        }

        let clean_ops: Vec<&ImportOp> = preview.clean_ops().collect();
        if clean_ops.is_empty() {
            return Ok(ImportOutcome {
                batch_id: String::new(),
                group_id: None,
                imported: Vec::new(),
                warnings: Vec::new(),
            });
        }

        let batch_id = Uuid::now_v7().simple().to_string();
        let staging = ProjectPath::new(format!("{STAGING_PREFIX}/{batch_id}"))?;
        self.fs
            .create_dir_all(&staging)
            .map_err(|source| ImportApplyError::StagingSetup { source })?;

        let staged = self.stage_all(&clean_ops, &staging, on_progress)?;
        self.commit_all(&staged)?;

        let auto_group = (staged.len() >= 2).then(|| Uuid::now_v7().simple().to_string());
        let unified = unified_caller_group(&staged);
        let outcome_group = unified.or(auto_group.clone());

        let (imported, warnings) = self.post_commit(&staged, auto_group.as_deref(), on_progress);

        let _ = self.fs.remove_file(&staging);

        Ok(ImportOutcome {
            batch_id,
            group_id: outcome_group,
            imported,
            warnings,
        })
    }

    fn post_commit(
        &self,
        staged: &[StagedImport],
        auto_group: Option<&str>,
        on_progress: &dyn Fn(u64, u64, &str),
    ) -> (Vec<ImportedFile>, Vec<ImportWarning>) {
        let total = staged.len() as u64;
        let mut warnings = Vec::new();
        let mut imported = Vec::new();

        for (i, s) in staged.iter().enumerate() {
            on_progress((i + 1) as u64, total, "Indexing files\u{2026}");
            let file_id = FileId::new_v7();
            let abs_dest = self.project_root.join(s.op.dest.as_str());

            let fingerprint = match std::fs::File::open(&abs_dest)
                .map_err(|e| e.to_string())
                .and_then(|f| compute_fingerprint(f).map_err(|e| e.to_string()))
            {
                Ok(fp) => Some(fp),
                Err(msg) => {
                    warnings.push(ImportWarning::FingerprintFailed {
                        dest: s.op.dest.clone(),
                        message: msg,
                    });
                    None
                }
            };

            if let Some(fp) = fingerprint {
                self.register_meta(s, file_id, fp, &mut warnings);
                self.register_index(s, file_id, fp, &abs_dest, &mut warnings);
            }

            self.register_history(s, auto_group, &mut warnings);

            imported.push(ImportedFile {
                source: s.op.source.clone(),
                dest: s.op.dest.clone(),
                file_id,
                mode: s.op.mode,
            });
        }

        (imported, warnings)
    }

    fn register_meta(
        &self,
        s: &StagedImport,
        file_id: FileId,
        fp: crate::identity::Fingerprint,
        warnings: &mut Vec<ImportWarning>,
    ) {
        let doc = MetaDocument::new(file_id, fp);
        let sidecar = match sidecar_path(&s.op.dest) {
            Ok(sp) => sp,
            Err(e) => {
                warnings.push(ImportWarning::MetaCreate {
                    dest: s.op.dest.clone(),
                    message: e.to_string(),
                });
                return;
            }
        };
        if let Err(e) = self.meta_store.save(&sidecar, &doc) {
            warnings.push(ImportWarning::MetaCreate {
                dest: s.op.dest.clone(),
                message: e.to_string(),
            });
        }
    }

    fn register_index(
        &self,
        s: &StagedImport,
        file_id: FileId,
        fp: crate::identity::Fingerprint,
        abs_dest: &std::path::Path,
        warnings: &mut Vec<ImportWarning>,
    ) {
        let row = FileRow {
            file_id,
            path: s.op.dest.clone(),
            fingerprint: fp,
            source_file_id: None,
            kind: Kind::Asset,
            status: Status::Active,
            size: std::fs::metadata(abs_dest).ok().map(|m| m.len()),
            mtime: None,
            created_at: None,
            last_seen_at: None,
        };
        if let Err(e) = self.index.upsert_file(&row) {
            warnings.push(ImportWarning::IndexInsert {
                dest: s.op.dest.clone(),
                message: e.to_string(),
            });
        }
    }

    fn register_history(
        &self,
        s: &StagedImport,
        auto_group: Option<&str>,
        warnings: &mut Vec<ImportWarning>,
    ) {
        let Some(history) = self.history else {
            return;
        };
        let effective_group =
            s.op.group_id
                .clone()
                .or_else(|| auto_group.map(String::from));
        let op = Operation::Import {
            path: s.op.dest.clone(),
            is_inverse: false,
        };
        let mut req = AppendRequest::new(op);
        if let Some(group) = effective_group {
            req = req.with_group(group);
        }
        if let Err(e) = history.append(&req) {
            warnings.push(ImportWarning::HistoryAppend {
                dest: s.op.dest.clone(),
                message: e.to_string(),
            });
        }
    }

    fn stage_all(
        &self,
        ops: &[&ImportOp],
        staging: &ProjectPath,
        on_progress: &dyn Fn(u64, u64, &str),
    ) -> Result<Vec<StagedImport>, ImportApplyError> {
        let total = ops.len() as u64;
        let mut staged = Vec::with_capacity(ops.len());

        for (i, op) in ops.iter().enumerate() {
            let label = match op.mode {
                ImportMode::Copy => "Copying files\u{2026}",
                ImportMode::Move => "Moving files\u{2026}",
            };
            on_progress((i + 1) as u64, total, label);

            let stage_path = staging.join(format!("{i}"))?;
            let src = std::path::Path::new(&op.source);

            let result = match op.mode {
                ImportMode::Copy => std::fs::copy(src, self.project_root.join(stage_path.as_str()))
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                ImportMode::Move => {
                    std::fs::rename(src, self.project_root.join(stage_path.as_str()))
                        .map_err(|e| e.to_string())
                }
            };

            if let Err(msg) = result {
                // Rollback previously staged
                self.rollback_stage(&staged);
                return Err(ImportApplyError::Stage {
                    op_index: i,
                    message: msg,
                });
            }

            staged.push(StagedImport {
                op: (*op).clone(),
                stage_path,
            });
        }

        Ok(staged)
    }

    fn commit_all(&self, staged: &[StagedImport]) -> Result<(), ImportApplyError> {
        for (i, s) in staged.iter().enumerate() {
            // Ensure parent dir exists
            if let Some(parent_path) = s.op.dest.parent() {
                let _ = self.fs.create_dir_all(&parent_path);
            }

            if let Err(source) = self.fs.rename(&s.stage_path, &s.op.dest) {
                // Rollback committed files
                self.rollback_commit(&staged[..i]);
                self.rollback_stage(staged);
                return Err(ImportApplyError::Commit {
                    op_index: i,
                    message: source.to_string(),
                });
            }
        }
        Ok(())
    }

    fn rollback_stage(&self, staged: &[StagedImport]) {
        for s in staged.iter().rev() {
            match s.op.mode {
                ImportMode::Move => {
                    // Try to move back to original location
                    let src = std::path::Path::new(&s.op.source);
                    let staged_abs = self.project_root.join(s.stage_path.as_str());
                    let _ = std::fs::rename(staged_abs, src);
                }
                ImportMode::Copy => {
                    // Just remove the staged copy
                    let staged_abs = self.project_root.join(s.stage_path.as_str());
                    let _ = std::fs::remove_file(staged_abs);
                }
            }
        }
    }

    fn rollback_commit(&self, committed: &[StagedImport]) {
        for s in committed.iter().rev() {
            let _ = self.fs.rename(&s.op.dest, &s.stage_path);
        }
    }
}

fn unified_caller_group(staged: &[StagedImport]) -> Option<String> {
    let first = staged.first()?.op.group_id.clone()?;
    if staged
        .iter()
        .all(|s| s.op.group_id.as_deref() == Some(first.as_str()))
    {
        Some(first)
    } else {
        None
    }
}
