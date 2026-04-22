//! OS-level filesystem watcher feeding debounced [`crate::reconcile::ChangeSet`]s.
//!
//! The module wraps [`notify`] + [`notify_debouncer_full`] so that
//! downstream callers consume a plain `std::sync::mpsc::Receiver<ChangeSet>`
//! — no async runtime, no direct dependency on the notify crate, and no
//! knowledge of OS-specific event types.
//!
//! Events are filtered through [`crate::fs::IgnoreRules`] before being
//! emitted; sidecar (`.meta`) paths are passed through so the reconciler
//! can apply them via its existing companion-routing logic.
//!
//! The watcher is intentionally "dumb": it never writes to `.meta` or the
//! index. The three-tier FS sync contract (startup full scan + watch +
//! periodic reconcile) is orchestrated by a higher-level driver that lives
//! in the runtime (Tauri) or CLI layer; core only provides the primitives.

pub mod error;
pub mod watcher;

pub use error::WatchError;
pub use watcher::{DEFAULT_DEBOUNCE, Watcher};
