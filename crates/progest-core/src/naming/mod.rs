//! Mechanical, AI-independent name cleanup (`core::naming`).
//!
//! Implements the Phase 2.5 cleanup pipeline scoped in
//! [`docs/REQUIREMENTS.md` §3.5.5][req]: a fixed ordering of
//! `remove_copy_suffix → remove_cjk → convert_case`, each stage toggled
//! via `.progest/project.toml [cleanup]`.
//!
//! The pipeline does **not** silently delete content: runs of CJK
//! characters become typed holes in the resulting [`NameCandidate`]
//! instead of vanishing. Holes must be resolved via a
//! [`fill::FillMode`] before the name is allowed to touch disk.
//!
//! Entry points:
//!
//! - [`pipeline::clean_basename`] — run the pipeline over a single
//!   basename.
//! - [`fill::resolve`] — collapse a candidate's holes into a final
//!   string (or refuse, depending on mode).
//! - [`loader::extract_cleanup_config`] — parse the `[cleanup]` TOML
//!   block inside a `ProjectDocument.extra` table.
//!
//! [req]: ../../../../docs/REQUIREMENTS.md

pub mod case;
pub mod fill;
pub mod loader;
pub mod pipeline;
pub mod suggest;
pub mod types;

pub use case::{CaseConvertError, convert_case};
pub use fill::{
    FillMode, FillResolution, HolePrompter, PromptError, UnresolvedHoleError, resolve,
    resolve_with_prompter,
};
pub use loader::{CleanupConfigError, CleanupConfigWarning, extract_cleanup_config};
pub use pipeline::clean_basename;
pub use suggest::fill_suggested_names;
pub use types::{CaseStyle, CleanupConfig, Hole, HoleKind, NameCandidate, Segment};
