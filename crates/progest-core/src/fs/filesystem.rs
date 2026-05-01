//! Pluggable filesystem abstraction.
//!
//! All filesystem I/O in core flows through the [`FileSystem`] trait so that
//! tests can substitute an in-memory fake and future platforms (Windows,
//! networked stores) can slot in without touching callers.
//!
//! The real implementation, [`StdFileSystem`], wraps [`std::fs`] and is
//! anchored to a single project root. Paths are always supplied as
//! [`ProjectPath`] values — never raw [`Path`] — so callers cannot accidentally
//! escape the root.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use thiserror::Error;

use super::ProjectPath;

/// File or directory metadata surfaced by [`FileSystem::metadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub is_dir: bool,
    pub size: u64,
    pub mtime: SystemTime,
}

/// Errors returned by [`FileSystem`] implementations.
#[derive(Debug, Error)]
pub enum FsError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("path not found: {0}")]
    NotFound(String),
}

impl FsError {
    fn from_io(path: &ProjectPath, source: io::Error) -> Self {
        if source.kind() == io::ErrorKind::NotFound {
            Self::NotFound(path.to_string())
        } else {
            Self::Io {
                path: path.to_string(),
                source,
            }
        }
    }
}

/// Abstraction over read/write filesystem operations rooted at a project.
pub trait FileSystem: Send + Sync {
    /// Absolute path of the project root on the host filesystem.
    fn root(&self) -> &Path;

    /// Read the entire contents of `path` as bytes.
    fn read(&self, path: &ProjectPath) -> Result<Vec<u8>, FsError>;

    /// Write `bytes` to `path` atomically: data is first written to a temp
    /// file in the same directory, then renamed into place. On success the
    /// destination either reflects the new contents or is untouched.
    fn write_atomic(&self, path: &ProjectPath, bytes: &[u8]) -> Result<(), FsError>;

    /// Rename `from` to `to`, replacing `to` if it exists.
    fn rename(&self, from: &ProjectPath, to: &ProjectPath) -> Result<(), FsError>;

    /// Return `true` if `path` exists (file or directory).
    fn exists(&self, path: &ProjectPath) -> bool;

    /// Fetch metadata for `path`.
    fn metadata(&self, path: &ProjectPath) -> Result<Metadata, FsError>;

    /// Remove the file at `path`. Fails if `path` is a directory.
    fn remove_file(&self, path: &ProjectPath) -> Result<(), FsError>;

    /// Create `path` and every missing parent directory.
    fn create_dir_all(&self, path: &ProjectPath) -> Result<(), FsError>;
}

/// Retry an I/O operation when the OS reports a sharing violation.
///
/// On Windows, DCC apps like Photoshop and Maya hold file locks during
/// saves, producing `ERROR_SHARING_VIOLATION` (raw OS error 32). A
/// short exponential backoff (100→200→400→800→1600 ms ≈ 3 s total)
/// avoids failing the whole operation for a transient lock.
///
/// On non-Windows platforms the closure runs exactly once.
fn retry_sharing_violation<F, T>(f: F) -> io::Result<T>
where
    F: Fn() -> io::Result<T>,
{
    #[cfg(not(windows))]
    {
        f()
    }

    #[cfg(windows)]
    {
        let mut attempts: u32 = 0;
        loop {
            match f() {
                Ok(v) => return Ok(v),
                Err(e) if attempts < 5 && e.raw_os_error() == Some(32) => {
                    attempts += 1;
                    tracing::warn!(
                        attempts,
                        "sharing violation, retrying in {}ms",
                        100 * 2u64.pow(attempts - 1)
                    );
                    std::thread::sleep(std::time::Duration::from_millis(
                        100 * 2u64.pow(attempts - 1),
                    ));
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Filesystem implementation backed by [`std::fs`], scoped to a project root.
#[derive(Debug, Clone)]
pub struct StdFileSystem {
    root: PathBuf,
}

impl StdFileSystem {
    /// Create a new `StdFileSystem` rooted at `root`. The root must be an
    /// existing directory; callers are expected to verify this before
    /// constructing the filesystem.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn resolve(&self, path: &ProjectPath) -> PathBuf {
        path.to_absolute(&self.root)
    }
}

impl FileSystem for StdFileSystem {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read(&self, path: &ProjectPath) -> Result<Vec<u8>, FsError> {
        let resolved = self.resolve(path);
        retry_sharing_violation(|| fs::read(&resolved)).map_err(|e| FsError::from_io(path, e))
    }

    fn write_atomic(&self, path: &ProjectPath, bytes: &[u8]) -> Result<(), FsError> {
        let target = self.resolve(path);
        let parent = target
            .parent()
            .ok_or_else(|| FsError::Io {
                path: path.to_string(),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "atomic write requires a parent directory",
                ),
            })?
            .to_path_buf();
        fs::create_dir_all(&parent).map_err(|e| FsError::from_io(path, e))?;

        let file_name = target
            .file_name()
            .ok_or_else(|| FsError::Io {
                path: path.to_string(),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "atomic write requires a file name",
                ),
            })?
            .to_os_string();
        let mut tmp_name = file_name.clone();
        tmp_name.push(".tmp");
        let tmp_path = parent.join(tmp_name);

        {
            let mut tmp = fs::File::create(&tmp_path).map_err(|e| FsError::from_io(path, e))?;
            tmp.write_all(bytes)
                .map_err(|e| FsError::from_io(path, e))?;
            tmp.sync_all().map_err(|e| FsError::from_io(path, e))?;
        }

        #[cfg(windows)]
        if target.exists() {
            let _ = fs::remove_file(&target);
        }

        match retry_sharing_violation(|| fs::rename(&tmp_path, &target)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(FsError::from_io(path, e))
            }
        }
    }

    fn rename(&self, from: &ProjectPath, to: &ProjectPath) -> Result<(), FsError> {
        let from_abs = self.resolve(from);
        let to_abs = self.resolve(to);
        #[cfg(windows)]
        if to_abs.exists() {
            let _ = fs::remove_file(&to_abs);
        }
        retry_sharing_violation(|| fs::rename(&from_abs, &to_abs))
            .map_err(|e| FsError::from_io(from, e))
    }

    fn exists(&self, path: &ProjectPath) -> bool {
        self.resolve(path).exists()
    }

    fn metadata(&self, path: &ProjectPath) -> Result<Metadata, FsError> {
        let meta = fs::metadata(self.resolve(path)).map_err(|e| FsError::from_io(path, e))?;
        let mtime = meta
            .modified()
            .map_err(|e| FsError::from_io(path, e))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        Ok(Metadata {
            is_dir: meta.is_dir(),
            size: meta.len(),
            mtime,
        })
    }

    fn remove_file(&self, path: &ProjectPath) -> Result<(), FsError> {
        let resolved = self.resolve(path);
        retry_sharing_violation(|| fs::remove_file(&resolved))
            .map_err(|e| FsError::from_io(path, e))
    }

    fn create_dir_all(&self, path: &ProjectPath) -> Result<(), FsError> {
        fs::create_dir_all(self.resolve(path)).map_err(|e| FsError::from_io(path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, StdFileSystem) {
        let dir = TempDir::new().unwrap();
        let fs = StdFileSystem::new(dir.path().to_path_buf());
        (dir, fs)
    }

    #[test]
    fn read_after_write_atomic_returns_bytes() {
        let (_dir, fs) = setup();
        let path = ProjectPath::new("notes.txt").unwrap();
        fs.write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs.read(&path).unwrap(), b"hello");
    }

    #[test]
    fn write_atomic_creates_parent_directories() {
        let (_dir, fs) = setup();
        let path = ProjectPath::new("deep/nested/note.txt").unwrap();
        fs.write_atomic(&path, b"x").unwrap();
        assert!(fs.exists(&path));
    }

    #[test]
    fn write_atomic_leaves_no_tmp_on_success() {
        let (dir, fs) = setup();
        let path = ProjectPath::new("ok.txt").unwrap();
        fs.write_atomic(&path, b"ok").unwrap();
        let tmp = dir.path().join("ok.txt.tmp");
        assert!(
            !tmp.exists(),
            "stale tmp file left behind: {}",
            tmp.display()
        );
    }

    #[test]
    fn read_missing_returns_not_found() {
        let (_dir, fs) = setup();
        let path = ProjectPath::new("missing.txt").unwrap();
        let err = fs.read(&path).unwrap_err();
        assert!(matches!(err, FsError::NotFound(_)));
    }

    #[test]
    fn rename_moves_file() {
        let (_dir, fs) = setup();
        let a = ProjectPath::new("a.txt").unwrap();
        let b = ProjectPath::new("b.txt").unwrap();
        fs.write_atomic(&a, b"data").unwrap();
        fs.rename(&a, &b).unwrap();
        assert!(!fs.exists(&a));
        assert_eq!(fs.read(&b).unwrap(), b"data");
    }

    #[test]
    fn metadata_reports_size_and_dir_flag() {
        let (_dir, fs) = setup();
        let file = ProjectPath::new("file.txt").unwrap();
        fs.write_atomic(&file, b"12345").unwrap();
        let meta = fs.metadata(&file).unwrap();
        assert!(!meta.is_dir);
        assert_eq!(meta.size, 5);

        let sub = ProjectPath::new("sub").unwrap();
        fs.create_dir_all(&sub).unwrap();
        let sub_meta = fs.metadata(&sub).unwrap();
        assert!(sub_meta.is_dir);
    }

    #[test]
    fn remove_file_removes_entry() {
        let (_dir, fs) = setup();
        let path = ProjectPath::new("gone.txt").unwrap();
        fs.write_atomic(&path, b"bye").unwrap();
        fs.remove_file(&path).unwrap();
        assert!(!fs.exists(&path));
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let (_dir, fs) = setup();
        let path = ProjectPath::new("overwrite.txt").unwrap();
        fs.write_atomic(&path, b"first").unwrap();
        fs.write_atomic(&path, b"second").unwrap();
        assert_eq!(fs.read(&path).unwrap(), b"second");
    }

    #[test]
    fn rename_replaces_existing_destination() {
        let (_dir, fs) = setup();
        let a = ProjectPath::new("src.txt").unwrap();
        let b = ProjectPath::new("dst.txt").unwrap();
        fs.write_atomic(&a, b"new").unwrap();
        fs.write_atomic(&b, b"old").unwrap();
        fs.rename(&a, &b).unwrap();
        assert!(!fs.exists(&a));
        assert_eq!(fs.read(&b).unwrap(), b"new");
    }

    #[test]
    fn root_is_project_root_when_path_is_root() {
        let (dir, fs) = setup();
        let root = ProjectPath::root();
        let meta = fs.metadata(&root).unwrap();
        assert!(meta.is_dir);
        assert_eq!(fs.root(), dir.path());
    }
}
