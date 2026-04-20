//! In-memory [`FileSystem`] implementation for tests.
//!
//! [`MemFileSystem`] stores every write in a lock-protected map keyed by
//! [`ProjectPath`]. It lets later modules (`meta`, `index`, `reconcile`)
//! exercise their logic without touching disk — no `tempdir`, no ordering
//! hazards from other tests.
//!
//! The implementation intentionally keeps the surface small: just enough to
//! satisfy the [`FileSystem`] contract plus a few test-only helpers.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use super::{FileSystem, FsError, Metadata, ProjectPath};

/// A path under a [`MemFileSystem`] can be either a file or a directory.
#[derive(Debug, Clone)]
enum EntryData {
    File(Vec<u8>),
    Dir,
}

#[derive(Debug, Clone)]
struct Entry {
    data: EntryData,
    mtime: SystemTime,
}

/// In-memory filesystem used exclusively by tests.
pub struct MemFileSystem {
    root: PathBuf,
    entries: Mutex<BTreeMap<String, Entry>>,
}

impl MemFileSystem {
    /// Create an empty memory-backed filesystem rooted at a symbolic path.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: PathBuf::from("/mem"),
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    fn with_entries<R>(&self, f: impl FnOnce(&mut BTreeMap<String, Entry>) -> R) -> R {
        let mut guard = self.entries.lock().expect("mem fs mutex poisoned");
        f(&mut guard)
    }

    fn insert_dirs_leading_to(entries: &mut BTreeMap<String, Entry>, path: &ProjectPath) {
        let mut current = ProjectPath::root();
        let raw = path.as_str();
        if raw.is_empty() {
            return;
        }
        let segments: Vec<&str> = raw.split('/').collect();
        // For a path like "a/b/c.txt", create parents "a" and "a/b" (not "a/b/c.txt").
        for segment in segments.iter().take(segments.len().saturating_sub(1)) {
            current = current.join(*segment).expect("segment is valid");
            let key = current.as_str().to_string();
            entries.entry(key).or_insert(Entry {
                data: EntryData::Dir,
                mtime: SystemTime::now(),
            });
        }
    }
}

impl Default for MemFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for MemFileSystem {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read(&self, path: &ProjectPath) -> Result<Vec<u8>, FsError> {
        self.with_entries(|entries| match entries.get(path.as_str()) {
            Some(Entry {
                data: EntryData::File(bytes),
                ..
            }) => Ok(bytes.clone()),
            Some(Entry {
                data: EntryData::Dir,
                ..
            }) => Err(FsError::Io {
                path: path.to_string(),
                source: io::Error::new(io::ErrorKind::IsADirectory, "path is a directory"),
            }),
            None => Err(FsError::NotFound(path.to_string())),
        })
    }

    fn write_atomic(&self, path: &ProjectPath, bytes: &[u8]) -> Result<(), FsError> {
        if path.is_root() {
            return Err(FsError::Io {
                path: path.to_string(),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot write to the project root",
                ),
            });
        }
        self.with_entries(|entries| {
            Self::insert_dirs_leading_to(entries, path);
            entries.insert(
                path.as_str().to_string(),
                Entry {
                    data: EntryData::File(bytes.to_vec()),
                    mtime: SystemTime::now(),
                },
            );
        });
        Ok(())
    }

    fn rename(&self, from: &ProjectPath, to: &ProjectPath) -> Result<(), FsError> {
        self.with_entries(|entries| {
            let Some(source) = entries.remove(from.as_str()) else {
                return Err(FsError::NotFound(from.to_string()));
            };

            // If the source is a directory, relocate every descendant too.
            if matches!(source.data, EntryData::Dir) {
                let prefix = format!("{}/", from.as_str());
                let descendants: Vec<(String, Entry)> = entries
                    .iter()
                    .filter(|(k, _)| k.starts_with(&prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (k, _) in &descendants {
                    entries.remove(k);
                }
                for (k, v) in descendants {
                    let suffix = &k[prefix.len()..];
                    let new_key = format!("{}/{}", to.as_str(), suffix);
                    entries.insert(new_key, v);
                }
            }
            Self::insert_dirs_leading_to(entries, to);
            entries.insert(to.as_str().to_string(), source);
            Ok(())
        })
    }

    fn exists(&self, path: &ProjectPath) -> bool {
        if path.is_root() {
            return true;
        }
        self.with_entries(|entries| entries.contains_key(path.as_str()))
    }

    fn metadata(&self, path: &ProjectPath) -> Result<Metadata, FsError> {
        if path.is_root() {
            return Ok(Metadata {
                is_dir: true,
                size: 0,
                mtime: SystemTime::UNIX_EPOCH,
            });
        }
        self.with_entries(|entries| match entries.get(path.as_str()) {
            Some(entry) => Ok(match &entry.data {
                EntryData::File(bytes) => Metadata {
                    is_dir: false,
                    size: bytes.len() as u64,
                    mtime: entry.mtime,
                },
                EntryData::Dir => Metadata {
                    is_dir: true,
                    size: 0,
                    mtime: entry.mtime,
                },
            }),
            None => Err(FsError::NotFound(path.to_string())),
        })
    }

    fn remove_file(&self, path: &ProjectPath) -> Result<(), FsError> {
        self.with_entries(|entries| match entries.get(path.as_str()) {
            Some(Entry {
                data: EntryData::File(_),
                ..
            }) => {
                entries.remove(path.as_str());
                Ok(())
            }
            Some(Entry {
                data: EntryData::Dir,
                ..
            }) => Err(FsError::Io {
                path: path.to_string(),
                source: io::Error::new(
                    io::ErrorKind::IsADirectory,
                    "cannot remove_file a directory",
                ),
            }),
            None => Err(FsError::NotFound(path.to_string())),
        })
    }

    fn create_dir_all(&self, path: &ProjectPath) -> Result<(), FsError> {
        if path.is_root() {
            return Ok(());
        }
        self.with_entries(|entries| {
            let raw = path.as_str();
            let mut current = ProjectPath::root();
            for segment in raw.split('/') {
                current = current.join(segment).expect("segment is valid");
                let key = current.as_str().to_string();
                match entries.get(&key) {
                    Some(Entry {
                        data: EntryData::File(_),
                        ..
                    }) => {
                        return Err(FsError::Io {
                            path: current.to_string(),
                            source: io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                "a file exists at this path",
                            ),
                        });
                    }
                    Some(Entry {
                        data: EntryData::Dir,
                        ..
                    })
                    | None => {
                        entries.entry(key).or_insert(Entry {
                            data: EntryData::Dir,
                            mtime: SystemTime::now(),
                        });
                    }
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    #[test]
    fn read_after_write_returns_bytes() {
        let fs = MemFileSystem::new();
        fs.write_atomic(&p("notes.txt"), b"hello").unwrap();
        assert_eq!(fs.read(&p("notes.txt")).unwrap(), b"hello");
    }

    #[test]
    fn write_creates_parent_directories_implicitly() {
        let fs = MemFileSystem::new();
        fs.write_atomic(&p("a/b/c/leaf.txt"), b"x").unwrap();
        assert!(fs.exists(&p("a")));
        assert!(fs.exists(&p("a/b")));
        assert!(fs.exists(&p("a/b/c")));
        assert!(fs.metadata(&p("a/b")).unwrap().is_dir);
    }

    #[test]
    fn read_missing_returns_not_found() {
        let fs = MemFileSystem::new();
        let err = fs.read(&p("missing.txt")).unwrap_err();
        assert!(matches!(err, FsError::NotFound(_)));
    }

    #[test]
    fn rename_moves_file() {
        let fs = MemFileSystem::new();
        fs.write_atomic(&p("a.txt"), b"1").unwrap();
        fs.rename(&p("a.txt"), &p("b.txt")).unwrap();
        assert!(!fs.exists(&p("a.txt")));
        assert_eq!(fs.read(&p("b.txt")).unwrap(), b"1");
    }

    #[test]
    fn rename_moves_directory_and_descendants() {
        let fs = MemFileSystem::new();
        fs.write_atomic(&p("old/a.txt"), b"1").unwrap();
        fs.write_atomic(&p("old/nested/b.txt"), b"2").unwrap();
        fs.rename(&p("old"), &p("new")).unwrap();
        assert!(!fs.exists(&p("old/a.txt")));
        assert_eq!(fs.read(&p("new/a.txt")).unwrap(), b"1");
        assert_eq!(fs.read(&p("new/nested/b.txt")).unwrap(), b"2");
    }

    #[test]
    fn remove_file_rejects_directory() {
        let fs = MemFileSystem::new();
        fs.create_dir_all(&p("dir")).unwrap();
        assert!(fs.remove_file(&p("dir")).is_err());
    }

    #[test]
    fn metadata_on_root_reports_dir() {
        let fs = MemFileSystem::new();
        let meta = fs.metadata(&ProjectPath::root()).unwrap();
        assert!(meta.is_dir);
    }

    #[test]
    fn create_dir_all_over_file_errors() {
        let fs = MemFileSystem::new();
        fs.write_atomic(&p("conflict"), b"x").unwrap();
        let err = fs.create_dir_all(&p("conflict/child")).unwrap_err();
        assert!(matches!(err, FsError::Io { .. }));
    }
}
