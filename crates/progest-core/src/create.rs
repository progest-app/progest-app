//! Create new files and directories inside a project.
//!
//! Thin wrappers around [`FileSystem`] that handle parent directory
//! creation and existence checks. The reconciler picks up new entries
//! on the next scan, generating `.meta` sidecars and index rows.

use serde::Serialize;
use thiserror::Error;

use crate::fs::{FileSystem, ProjectPath};

#[derive(Debug, Error)]
pub enum CreateError {
    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("filesystem error: {0}")]
    Fs(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateOutcome {
    pub path: ProjectPath,
    pub kind: &'static str,
}

/// Create an empty directory at `path` (relative to project root).
pub fn create_dir(fs: &dyn FileSystem, path: &ProjectPath) -> Result<CreateOutcome, CreateError> {
    if fs.exists(path) {
        return Err(CreateError::AlreadyExists(path.to_string()));
    }
    fs.create_dir_all(path)
        .map_err(|e| CreateError::Fs(e.to_string()))?;
    Ok(CreateOutcome {
        path: path.clone(),
        kind: "directory",
    })
}

/// Create an empty file at `path` (relative to project root).
/// Parent directories are created automatically.
/// The reconciler will generate the `.meta` sidecar and index entry.
pub fn create_file(fs: &dyn FileSystem, path: &ProjectPath) -> Result<CreateOutcome, CreateError> {
    if fs.exists(path) {
        return Err(CreateError::AlreadyExists(path.to_string()));
    }

    if let Some(parent) = path.parent()
        && !parent.is_root()
    {
        let _ = fs.create_dir_all(&parent);
    }

    fs.write_atomic(path, &[])
        .map_err(|e| CreateError::Fs(e.to_string()))?;

    Ok(CreateOutcome {
        path: path.clone(),
        kind: "file",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;

    #[test]
    fn create_dir_succeeds() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("new_dir").unwrap();
        let outcome = create_dir(&fs, &path).unwrap();
        assert_eq!(outcome.kind, "directory");
        assert!(fs.exists(&path));
    }

    #[test]
    fn create_dir_rejects_existing() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("existing").unwrap();
        fs.create_dir_all(&path).unwrap();
        let err = create_dir(&fs, &path).unwrap_err();
        assert!(matches!(err, CreateError::AlreadyExists(_)));
    }

    #[test]
    fn create_file_succeeds() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("test.txt").unwrap();
        let outcome = create_file(&fs, &path).unwrap();
        assert_eq!(outcome.kind, "file");
        assert!(fs.exists(&path));
    }

    #[test]
    fn create_file_creates_parents() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("deep/nested/file.txt").unwrap();
        create_file(&fs, &path).unwrap();
        assert!(fs.exists(&path));
    }

    #[test]
    fn create_file_rejects_existing() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("exists.txt").unwrap();
        fs.write_atomic(&path, b"content").unwrap();
        let err = create_file(&fs, &path).unwrap_err();
        assert!(matches!(err, CreateError::AlreadyExists(_)));
    }
}
