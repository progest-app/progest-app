//! Fault-injecting [`FileSystem`] decorator for tests.
//!
//! [`FaultyFileSystem`] wraps any [`FileSystem`] implementation and lets a
//! test schedule [`FsError`] returns at the *N*-th invocation of any I/O
//! method. The decorator checks its schedule **before** delegating to the
//! inner filesystem, so an injected fault models "the operation never
//! started" — the inner FS is left untouched.
//!
//! Designed for upcoming `core::rename` apply-path testing: scenarios like
//! "the third FS rename in a bulk apply fails with `PermissionDenied`, prove
//! that the first two are rolled back" are expressible without running on
//! disk.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use super::{FileSystem, FsError, Metadata, ProjectPath};

/// `FileSystem` operations whose calls can be counted and faulted.
///
/// `exists` is intentionally omitted: it is a pure query that returns `bool`
/// and does not surface [`FsError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Read,
    WriteAtomic,
    Rename,
    Metadata,
    RemoveFile,
    CreateDirAll,
}

/// The class of failure to inject.
///
/// Each kind maps to a specific [`FsError`] variant (or [`io::ErrorKind`])
/// chosen to mirror failures seen in real-world rename / `.meta` flows.
#[derive(Debug, Clone)]
pub enum FaultKind {
    /// Maps to [`FsError::NotFound`].
    NotFound,
    /// Maps to [`io::ErrorKind::PermissionDenied`].
    PermissionDenied,
    /// Maps to [`io::ErrorKind::AlreadyExists`].
    AlreadyExists,
    /// Arbitrary other I/O failure with a custom message.
    Other(String),
}

impl FaultKind {
    fn into_error(self, path: &ProjectPath) -> FsError {
        match self {
            FaultKind::NotFound => FsError::NotFound(path.to_string()),
            FaultKind::PermissionDenied => FsError::Io {
                path: path.to_string(),
                source: io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected fault: permission denied",
                ),
            },
            FaultKind::AlreadyExists => FsError::Io {
                path: path.to_string(),
                source: io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "injected fault: already exists",
                ),
            },
            FaultKind::Other(msg) => FsError::Io {
                path: path.to_string(),
                source: io::Error::other(format!("injected fault: {msg}")),
            },
        }
    }
}

#[derive(Default, Debug)]
struct Schedule {
    counts: HashMap<Op, u32>,
    faults: HashMap<(Op, u32), FaultKind>,
}

/// A [`FileSystem`] decorator that returns scheduled [`FsError`] values
/// instead of delegating to the inner filesystem on selected calls.
///
/// Construct with [`FaultyFileSystem::new`], schedule faults with
/// [`FaultyFileSystem::fail_at`], inspect call counts with
/// [`FaultyFileSystem::call_count`].
pub struct FaultyFileSystem<F: FileSystem> {
    inner: F,
    schedule: Mutex<Schedule>,
}

impl<F: FileSystem> FaultyFileSystem<F> {
    /// Wrap `inner` so that every I/O call passes through the fault schedule
    /// before reaching the underlying filesystem.
    pub fn new(inner: F) -> Self {
        Self {
            inner,
            schedule: Mutex::new(Schedule::default()),
        }
    }

    /// Inject `kind` so the `n`-th (1-indexed) call to `op` fails. Each
    /// scheduled fault fires once, then is removed; subsequent calls pass
    /// through.
    ///
    /// # Panics
    /// Panics if the internal schedule mutex was poisoned by a panicking
    /// caller in another thread.
    pub fn fail_at(&self, op: Op, n: u32, kind: FaultKind) {
        self.schedule
            .lock()
            .expect("faulty fs mutex poisoned")
            .faults
            .insert((op, n), kind);
    }

    /// Number of times `op` has been invoked since construction (counts both
    /// faulted and pass-through calls).
    ///
    /// # Panics
    /// Panics if the internal schedule mutex was poisoned by a panicking
    /// caller in another thread.
    pub fn call_count(&self, op: Op) -> u32 {
        self.schedule
            .lock()
            .expect("faulty fs mutex poisoned")
            .counts
            .get(&op)
            .copied()
            .unwrap_or(0)
    }

    /// Borrow the wrapped filesystem to assert state outside the trait
    /// (e.g. inspecting backing storage in tests).
    pub fn inner(&self) -> &F {
        &self.inner
    }

    fn maybe_fail(&self, op: Op, path: &ProjectPath) -> Result<(), FsError> {
        let mut schedule = self.schedule.lock().expect("faulty fs mutex poisoned");
        let count = schedule.counts.entry(op).or_insert(0);
        *count += 1;
        let n = *count;
        if let Some(kind) = schedule.faults.remove(&(op, n)) {
            return Err(kind.into_error(path));
        }
        Ok(())
    }
}

impl<F: FileSystem> FileSystem for FaultyFileSystem<F> {
    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn read(&self, path: &ProjectPath) -> Result<Vec<u8>, FsError> {
        self.maybe_fail(Op::Read, path)?;
        self.inner.read(path)
    }

    fn write_atomic(&self, path: &ProjectPath, bytes: &[u8]) -> Result<(), FsError> {
        self.maybe_fail(Op::WriteAtomic, path)?;
        self.inner.write_atomic(path, bytes)
    }

    fn rename(&self, from: &ProjectPath, to: &ProjectPath) -> Result<(), FsError> {
        self.maybe_fail(Op::Rename, from)?;
        self.inner.rename(from, to)
    }

    fn exists(&self, path: &ProjectPath) -> bool {
        self.inner.exists(path)
    }

    fn metadata(&self, path: &ProjectPath) -> Result<Metadata, FsError> {
        self.maybe_fail(Op::Metadata, path)?;
        self.inner.metadata(path)
    }

    fn remove_file(&self, path: &ProjectPath) -> Result<(), FsError> {
        self.maybe_fail(Op::RemoveFile, path)?;
        self.inner.remove_file(path)
    }

    fn create_dir_all(&self, path: &ProjectPath) -> Result<(), FsError> {
        self.maybe_fail(Op::CreateDirAll, path)?;
        self.inner.create_dir_all(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn setup() -> FaultyFileSystem<MemFileSystem> {
        let inner = MemFileSystem::new();
        inner.write_atomic(&p("a.txt"), b"a").unwrap();
        inner.write_atomic(&p("b.txt"), b"b").unwrap();
        FaultyFileSystem::new(inner)
    }

    #[test]
    fn passes_through_when_no_faults_scheduled() {
        let fs = setup();
        assert_eq!(fs.read(&p("a.txt")).unwrap(), b"a");
        fs.write_atomic(&p("c.txt"), b"c").unwrap();
        assert_eq!(fs.inner().read(&p("c.txt")).unwrap(), b"c");
    }

    #[test]
    fn fault_at_first_call_fires_and_skips_inner() {
        let fs = setup();
        fs.fail_at(Op::Rename, 1, FaultKind::PermissionDenied);
        let err = fs.rename(&p("a.txt"), &p("z.txt")).unwrap_err();
        assert!(matches!(err, FsError::Io { ref source, .. }
            if source.kind() == io::ErrorKind::PermissionDenied));
        // Inner FS untouched: source still present, target absent.
        assert!(fs.inner().exists(&p("a.txt")));
        assert!(!fs.inner().exists(&p("z.txt")));
    }

    #[test]
    fn fault_fires_only_at_scheduled_call() {
        let fs = setup();
        fs.fail_at(Op::Rename, 2, FaultKind::PermissionDenied);

        // 1st call: passes
        fs.rename(&p("a.txt"), &p("a2.txt")).unwrap();
        // 2nd call: fails
        let err = fs.rename(&p("b.txt"), &p("b2.txt")).unwrap_err();
        assert!(matches!(err, FsError::Io { .. }));
        assert!(fs.inner().exists(&p("b.txt")));
        // 3rd call: passes again (fault is consumed)
        fs.rename(&p("b.txt"), &p("b3.txt")).unwrap();
        assert!(!fs.inner().exists(&p("b.txt")));
    }

    #[test]
    fn call_count_tracks_both_pass_and_fail() {
        let fs = setup();
        fs.fail_at(Op::Read, 2, FaultKind::Other("disk error".into()));
        let _ = fs.read(&p("a.txt")).unwrap();
        let _ = fs.read(&p("b.txt")).unwrap_err();
        let _ = fs.read(&p("a.txt")).unwrap();
        assert_eq!(fs.call_count(Op::Read), 3);
        assert_eq!(fs.call_count(Op::Rename), 0);
    }

    #[test]
    fn each_op_has_independent_counter() {
        let fs = setup();
        fs.fail_at(Op::WriteAtomic, 1, FaultKind::AlreadyExists);
        // Reads should not consume the WriteAtomic fault counter.
        fs.read(&p("a.txt")).unwrap();
        fs.read(&p("b.txt")).unwrap();
        // First write triggers the fault.
        let err = fs.write_atomic(&p("c.txt"), b"c").unwrap_err();
        assert!(matches!(err, FsError::Io { ref source, .. }
            if source.kind() == io::ErrorKind::AlreadyExists));
    }

    #[test]
    fn not_found_kind_maps_to_not_found_variant() {
        let fs = setup();
        fs.fail_at(Op::Metadata, 1, FaultKind::NotFound);
        let err = fs.metadata(&p("a.txt")).unwrap_err();
        assert!(matches!(err, FsError::NotFound(_)));
    }
}
