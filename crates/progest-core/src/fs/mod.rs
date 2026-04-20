//! Filesystem abstraction layer.
//!
//! Progest keeps all filesystem access behind traits so that domain logic can
//! be exercised against an in-memory fake in tests. Paths inside a project are
//! modeled as [`ProjectPath`] — a project-root-relative, forward-slash
//! separated string — so that core logic never touches platform-specific
//! representations directly.
//!
//! The real-world implementation lives in [`StdFileSystem`]. Future modules
//! (`core::meta`, `core::scanner`, etc.) depend on this trait rather than on
//! [`std::fs`] or [`std::path`] directly.

pub mod filesystem;
pub mod ignore;
pub mod mem;
pub mod path;
pub mod scanner;

pub use filesystem::{FileSystem, FsError, Metadata, StdFileSystem};
pub use ignore::{DEFAULT_PATTERNS, IgnoreError, IgnoreRules, USER_IGNORE_PATH};
pub use mem::MemFileSystem;
pub use path::{ProjectPath, ProjectPathError};
pub use scanner::{EntryKind, ScanEntry, ScanError, ScanIter, Scanner};
