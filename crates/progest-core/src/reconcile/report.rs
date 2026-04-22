//! Summary values returned by reconcile operations.
//!
//! Reports are plain data, not builders: the reconciler fully populates one
//! and hands it back so CLI / UI layers can render without carrying the
//! reconciler's borrow lifetime around.

use crate::fs::ProjectPath;
use crate::identity::FileId;

/// Per-file outcome of a reconcile operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// The file was new to the index; a fresh [`FileId`] was minted and a
    /// `.meta` sidecar was written.
    Added { file_id: FileId, path: ProjectPath },

    /// An existing row was kept (cheap compare hit; nothing to do).
    Unchanged { file_id: FileId, path: ProjectPath },

    /// An existing row was updated — either its fingerprint changed, its
    /// mtime/size drifted, or a rename moved it to a new path.
    Updated { file_id: FileId, path: ProjectPath },

    /// The file no longer exists on disk; the row was removed from the index.
    /// The corresponding `.meta` sidecar is left alone so that `progest
    /// doctor` (or the user) can decide whether to delete or restore it.
    Removed { file_id: FileId, path: ProjectPath },
}

/// Summary of a [`super::Reconciler::full_scan`] pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanReport {
    /// Outcome per tracked file, in scan order.
    pub outcomes: Vec<ReconcileOutcome>,

    /// Sidecar `.meta` paths whose companion file was not observed during
    /// the walk. The reconciler does not delete these — doctor does, after
    /// the user confirms.
    pub orphan_metas: Vec<ProjectPath>,
}

impl ScanReport {
    /// Count of [`ReconcileOutcome::Added`] entries.
    #[must_use]
    pub fn added(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ReconcileOutcome::Added { .. }))
            .count()
    }

    /// Count of [`ReconcileOutcome::Updated`] entries.
    #[must_use]
    pub fn updated(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ReconcileOutcome::Updated { .. }))
            .count()
    }

    /// Count of [`ReconcileOutcome::Removed`] entries.
    #[must_use]
    pub fn removed(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ReconcileOutcome::Removed { .. }))
            .count()
    }

    /// Count of [`ReconcileOutcome::Unchanged`] entries.
    #[must_use]
    pub fn unchanged(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ReconcileOutcome::Unchanged { .. }))
            .count()
    }
}

/// Summary of a [`super::Reconciler::apply_changes`] pass.
///
/// Carries only outcomes: change-set driven reconcile does not walk the whole
/// tree, so it cannot report on orphans. `progest doctor` / a full scan
/// remains the authoritative orphan detector.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub outcomes: Vec<ReconcileOutcome>,
}
