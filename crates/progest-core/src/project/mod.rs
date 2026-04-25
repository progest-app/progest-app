//! Project-root discovery and `.progest/` layout initialization.
//!
//! A Progest project is a directory that contains a `.progest/` subdirectory
//! — the presence of that directory is what distinguishes a project from an
//! ordinary folder. This module owns the logic for:
//!
//! - walking upward from the current working directory to find an existing
//!   project root, so every subcommand can be run from anywhere inside the
//!   tree (like `git`);
//! - laying down a fresh project, including the shared TOML files, the
//!   user-editable ignore file, the `SQLite` index database, and a set of
//!   entries in the project's `.gitignore` so the machine-local artifacts
//!   never get committed.
//!
//! The code is intentionally free of opinions about `rules.toml`,
//! `schema.toml`, and `views.toml`: those gain real shape in M2/M3, so for
//! now `init` does not create stub files whose schemas are still up in the
//! air.

pub mod document;
pub mod layout;
pub mod root;

pub use document::{ProjectDocument, ProjectError, ProjectId};
pub use layout::{
    DOT_DIR, GITIGNORE_ENTRIES, HISTORY_DB_FILENAME, IGNORE_TEMPLATE, INDEX_DB_FILENAME, LOCAL_DIR,
    PROJECT_TOML_FILENAME, RULES_TOML_FILENAME, SCHEMA_TOML_FILENAME, THUMBS_DIR,
    USER_IGNORE_FILENAME, VIEWS_TOML_FILENAME, initialize,
};
pub use root::ProjectRoot;
