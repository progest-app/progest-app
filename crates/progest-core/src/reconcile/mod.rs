//! Three-way reconciliation between the filesystem, `.meta` sidecars, and the
//! `SQLite` index.
//!
//! The reconciler is the bridge that turns on-disk state into index rows and
//! keeps the two in agreement. Progest operates in a "trust nothing, verify
//! everything" model:
//!
//! 1. A **startup full scan** walks the project and reconciles every tracked
//!    file. `.meta` sidecars are auto-generated for any file that doesn't
//!    already carry one so the identity chain is always anchored on disk.
//! 2. A **change-set driven apply** handles incremental updates from the
//!    watch layer (PR 後続) without revisiting untouched files.
//! 3. A periodic timer re-runs the full scan to recover from dropped watch
//!    events; the driver lives in the runtime crate, not here.
//!
//! The 5-second M1 performance budget targets (2); (1) is expected to take
//! longer on first scan because it writes a `.meta` for every file — see the
//! note attached to the M1 milestone in `docs/IMPLEMENTATION_PLAN.md`.
//!
//! The module is split into:
//!
//! - [`change_set`] — [`FsEvent`] / [`ChangeSet`] types exchanged with watch.
//! - [`report`] — [`ScanReport`] / [`ApplyReport`] return values.
//! - [`error`] — the aggregate [`ReconcileError`].
//! - [`reconciler`] — the [`Reconciler`] itself.

pub mod change_set;
pub mod error;
pub mod reconciler;
pub mod report;

pub use change_set::{ChangeSet, FsEvent};
pub use error::ReconcileError;
pub use reconciler::Reconciler;
pub use report::{ApplyReport, ReconcileOutcome, ScanReport};
