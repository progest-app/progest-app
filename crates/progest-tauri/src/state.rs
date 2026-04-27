//! Process-wide app state held by the Tauri runtime.
//!
//! On startup the shell tries to attach to a Progest project by:
//! 1. honoring `PROGEST_PROJECT` if set (treated as the desired root),
//! 2. otherwise walking up from the current working directory.
//!
//! When no project is found, [`AppState::project`] stays [`None`] and
//! the IPC commands surface a structured "no project" error so the
//! React shell can render the empty state. Re-attaching to a different
//! project at runtime (via a project switcher) is a v1.x story —
//! switching directories without a process restart is out of scope
//! for the M3 #7 command palette landing.

use std::path::PathBuf;
use std::sync::Mutex;

use progest_core::fs::StdFileSystem;
use progest_core::index::SqliteIndex;
use progest_core::project::ProjectRoot;
use serde::Serialize;

/// Resolved project + the long-lived handles that IPC commands reuse.
///
/// `index` is held open for the life of the process — `SqliteIndex`
/// guards its connection with an internal `Mutex`, so this struct is
/// `Sync` and can sit behind `tauri::State<AppState>` without extra
/// locking.
pub struct ProjectContext {
    pub root: ProjectRoot,
    pub fs: StdFileSystem,
    pub index: SqliteIndex,
}

impl ProjectContext {
    /// Open the project at `root`, materializing the long-lived
    /// handles. Errors if `index.db` cannot be opened.
    pub fn open(root: ProjectRoot) -> Result<Self, String> {
        let fs = StdFileSystem::new(root.root().to_path_buf());
        let index = SqliteIndex::open(&root.index_db())
            .map_err(|e| format!("opening index `{}`: {e}", root.index_db().display()))?;
        Ok(Self { root, fs, index })
    }
}

/// Process-wide state shared by every IPC command.
///
/// `project` is `Mutex<Option<_>>` so future "switch project" plumbing
/// can swap the slot without redesigning the surface; today the slot
/// is populated once at startup and never mutated.
#[derive(Default)]
pub struct AppState {
    pub project: Mutex<Option<ProjectContext>>,
}

/// Lightweight snapshot of the current project for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    pub root: String,
    pub name: String,
}

impl ProjectInfo {
    pub fn from_context(ctx: &ProjectContext) -> Self {
        let name = ctx
            .root
            .root()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Self {
            root: ctx.root.root().display().to_string(),
            name,
        }
    }
}

/// Attempt to discover a project to attach to at startup. Returns
/// `Ok(None)` when no project is reachable — the shell still launches
/// so the empty state can render.
pub fn discover_initial_project() -> Result<Option<ProjectContext>, String> {
    let candidate = match std::env::var_os("PROGEST_PROJECT") {
        Some(p) => Some(PathBuf::from(p)),
        None => std::env::current_dir().ok(),
    };
    let Some(start) = candidate else {
        return Ok(None);
    };
    match ProjectRoot::discover(&start) {
        Ok(root) => Ok(Some(ProjectContext::open(root)?)),
        Err(_) => Ok(None),
    }
}
