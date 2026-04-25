//! On-disk layout of `.progest/` and the [`initialize`] routine that
//! materializes it for a fresh project.
//!
//! Constants live here so that CLI, reconciler, and doctor all read the
//! same path names. [`initialize`] is idempotent for the content it writes
//! — re-running against an existing project returns
//! [`ProjectError::AlreadyInitialized`] rather than clobbering on-disk state.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::index::SqliteIndex;

use super::document::{ProjectDocument, ProjectError};
use super::root::ProjectRoot;

/// Name of the directory that marks a Progest project root.
pub const DOT_DIR: &str = ".progest";

/// Name of the shared project-level TOML file.
pub const PROJECT_TOML_FILENAME: &str = "project.toml";

/// Name of the naming-rules TOML file (optional; absent means "no
/// project-wide rules, only per-dir `.dirmeta.toml` `[[rules]]`").
pub const RULES_TOML_FILENAME: &str = "rules.toml";

/// Name of the alias / extension-compounds TOML file (optional;
/// absent means "use the builtin alias catalog only").
pub const SCHEMA_TOML_FILENAME: &str = "schema.toml";

/// Name of the saved-views TOML file (optional; absent means
/// "no project-shared views, only ad-hoc CLI / UI queries").
pub const VIEWS_TOML_FILENAME: &str = "views.toml";

/// Name of the user-editable ignore rules file (gitignore syntax).
pub const USER_IGNORE_FILENAME: &str = "ignore";

/// Name of the `SQLite` index database.
pub const INDEX_DB_FILENAME: &str = "index.db";

/// Name of the `SQLite` history database (lives under `LOCAL_DIR`).
pub const HISTORY_DB_FILENAME: &str = "history.db";

/// Name of the machine-local pending-writes / cache directory.
pub const LOCAL_DIR: &str = "local";

/// Name of the thumbnail cache directory.
pub const THUMBS_DIR: &str = "thumbs";

/// Seed content for `.progest/ignore` on a fresh project: a comment
/// explaining what the file is for and no rules beyond the defaults that
/// `IgnoreRules::load` already applies.
pub const IGNORE_TEMPLATE: &str = "\
# Progest user ignore rules (gitignore syntax).
# Built-in defaults (.git/, node_modules/, .DS_Store, etc.) are applied on
# top of whatever you add here — you don't need to repeat them.
";

/// Patterns that `progest init` ensures are present in the project's
/// top-level `.gitignore`. Kept as a slice so tests can assert them.
pub const GITIGNORE_ENTRIES: &[&str] =
    &[".progest/index.db", ".progest/thumbs/", ".progest/local/"];

/// Create the `.progest/` layout inside `root` and return a [`ProjectRoot`]
/// handle pointing at it.
///
/// The routine:
/// - refuses to overwrite an existing `.progest/`
/// - writes `project.toml` with a fresh [`ProjectId`] and the supplied `name`
/// - writes the `ignore` seed file
/// - creates `local/` and `thumbs/` subdirectories
/// - opens `index.db` once to run the initial migration so future `scan`
///   runs don't carry migration cost on the critical path
/// - appends missing entries to the project's top-level `.gitignore`
///   (creates the file if absent)
///
/// [`ProjectId`]: super::document::ProjectId
pub fn initialize(root: &Path, name: &str) -> Result<ProjectRoot, ProjectError> {
    let root = if root.exists() {
        fs::canonicalize(root)?
    } else {
        fs::create_dir_all(root)?;
        fs::canonicalize(root)?
    };
    let dot = root.join(DOT_DIR);
    if dot.exists() {
        return Err(ProjectError::AlreadyInitialized { root });
    }
    fs::create_dir_all(&dot)?;
    fs::create_dir_all(dot.join(LOCAL_DIR))?;
    fs::create_dir_all(dot.join(THUMBS_DIR))?;

    let doc = ProjectDocument::new(name.to_string());
    let project_toml_path = dot.join(PROJECT_TOML_FILENAME);
    fs::write(&project_toml_path, doc.to_toml_string()?)?;

    let ignore_path = dot.join(USER_IGNORE_FILENAME);
    fs::write(&ignore_path, IGNORE_TEMPLATE)?;

    // Open the index once so the first `scan` doesn't pay migration cost.
    // The handle is dropped immediately; the on-disk schema is what persists.
    let index_path = dot.join(INDEX_DB_FILENAME);
    SqliteIndex::open(&index_path).map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;

    ensure_gitignore(&root)?;

    Ok(ProjectRoot::from_path(root))
}

/// Ensure the project root's `.gitignore` contains every entry in
/// [`GITIGNORE_ENTRIES`]. Appends missing entries preserving whatever the
/// user already wrote; creates the file if it is not present.
fn ensure_gitignore(root: &Path) -> std::io::Result<()> {
    let path = root.join(".gitignore");
    let existing = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };

    // Normalize trailing slashes when comparing: `.progest/thumbs` and
    // `.progest/thumbs/` select the same set of files under gitignore
    // semantics, and treating them as distinct entries would duplicate
    // lines every time `progest init` re-runs against a project that
    // already has the un-slashed form.
    let have: std::collections::HashSet<&str> = existing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|l| l.trim_end_matches('/'))
        .collect();

    let missing: Vec<&&str> = GITIGNORE_ENTRIES
        .iter()
        .filter(|e| !have.contains(e.trim_end_matches('/')))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    // If the existing file doesn't end with a newline, add one so we don't
    // merge the last line with our entries.
    if !existing.is_empty() && !existing.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    if existing.is_empty() {
        file.write_all(b"# Progest\n")?;
    } else {
        file.write_all(b"\n# Progest\n")?;
    }
    for entry in missing {
        writeln!(file, "{entry}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn abs(path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap()
    }

    #[test]
    fn initialize_creates_expected_layout() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        let root = initialize(&project, "Demo").unwrap();

        let dot = root.dot_dir();
        assert!(dot.is_dir());
        assert!(dot.join(PROJECT_TOML_FILENAME).is_file());
        assert!(dot.join(USER_IGNORE_FILENAME).is_file());
        assert!(dot.join(INDEX_DB_FILENAME).is_file());
        assert!(dot.join(LOCAL_DIR).is_dir());
        assert!(dot.join(THUMBS_DIR).is_dir());

        let gitignore = abs(&project).join(".gitignore");
        let contents = fs::read_to_string(gitignore).unwrap();
        for entry in GITIGNORE_ENTRIES {
            assert!(contents.contains(entry), "missing {entry} in .gitignore");
        }
    }

    #[test]
    fn initialize_refuses_to_overwrite_existing_project() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        initialize(&project, "Demo").unwrap();
        let err = initialize(&project, "Demo Again").unwrap_err();
        assert!(matches!(err, ProjectError::AlreadyInitialized { .. }));
    }

    #[test]
    fn initialize_appends_to_existing_gitignore_without_duplicating() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join(".gitignore"), "*.log\n.progest/thumbs/\n").unwrap();

        initialize(&project, "Demo").unwrap();

        let gitignore = fs::read_to_string(project.join(".gitignore")).unwrap();
        // Pre-existing entry is kept exactly once.
        assert_eq!(gitignore.matches(".progest/thumbs/").count(), 1);
        // New entries appended.
        assert!(gitignore.contains(".progest/index.db"));
        assert!(gitignore.contains(".progest/local/"));
        // User entry preserved.
        assert!(gitignore.contains("*.log"));
    }

    #[test]
    fn initialize_treats_slash_variants_as_the_same_entry() {
        // The shipped patterns use trailing slashes, but a project that was
        // initialized by an older version of progest (or had its .gitignore
        // edited by hand) may use the un-slashed form. Appending again
        // would produce noisy duplicate lines.
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join(".gitignore"),
            ".progest/index.db\n.progest/thumbs\n.progest/local\n",
        )
        .unwrap();

        initialize(&project, "Demo").unwrap();

        let gitignore = fs::read_to_string(project.join(".gitignore")).unwrap();
        assert_eq!(gitignore.matches(".progest/thumbs").count(), 1);
        assert_eq!(gitignore.matches(".progest/local").count(), 1);
        assert_eq!(gitignore.matches(".progest/index.db").count(), 1);
    }

    #[test]
    fn project_toml_round_trips_from_disk() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        let root = initialize(&project, "Demo").unwrap();
        let text = fs::read_to_string(root.project_toml()).unwrap();
        let reloaded = ProjectDocument::from_toml_str(&text).unwrap();
        assert_eq!(reloaded.name, "Demo");
        assert_eq!(reloaded.progest_version, crate::VERSION);
    }
}
