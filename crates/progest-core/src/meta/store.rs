//! On-disk I/O for sidecar `.meta` files.
//!
//! Callers work with this layer rather than [`crate::fs::FileSystem`] directly
//! so that (a) the TOML serialization boundary stays in one place and (b) the
//! atomic-write contract of `.meta` is never bypassed. The trait also gives
//! tests a single seam to stub out — a later `MemMetaStore` can slot in the
//! same way [`crate::fs::MemFileSystem`] does for scans.
//!
//! The actual atomicity guarantee comes from [`crate::fs::FileSystem::write_atomic`]:
//! save writes to `foo.psd.meta.tmp` first and renames into place, so a crash
//! mid-write leaves the previous version intact rather than a truncated file.
//! See the requirements doc §4.2.

use thiserror::Error;

use crate::fs::{FileSystem, FsError, ProjectPath, ProjectPathError};

use super::document::{MetaDocument, MetaError};

/// File-extension suffix identifying sidecar metadata files.
pub const SIDECAR_SUFFIX: &str = ".meta";

/// Errors surfaced by [`MetaStore`] operations.
#[derive(Debug, Error)]
pub enum MetaStoreError {
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Meta(#[from] MetaError),
    #[error(".meta file contains invalid UTF-8: {0}")]
    InvalidUtf8(String),
    #[error(transparent)]
    Path(#[from] ProjectPathError),
}

/// Compute the sidecar `.meta` path for a tracked file.
///
/// For `assets/foo.psd` the sidecar is `assets/foo.psd.meta`. Returns an
/// error when `file` refers to the project root, since the root has no
/// sidecar (per-directory metadata lives in `.dirmeta.toml`, handled by a
/// separate loader).
pub fn sidecar_path(file: &ProjectPath) -> Result<ProjectPath, MetaStoreError> {
    if file.is_root() {
        return Err(MetaStoreError::Path(ProjectPathError::InvalidComponent(
            "cannot derive a .meta sidecar for the project root".into(),
        )));
    }
    Ok(ProjectPath::new(format!(
        "{}{SIDECAR_SUFFIX}",
        file.as_str()
    ))?)
}

/// Storage seam for sidecar `.meta` files.
///
/// Paths passed in must already be sidecar paths (use [`sidecar_path`] to
/// derive them from the tracked file's path). This keeps the trait focused
/// on I/O and leaves naming policy in one place.
pub trait MetaStore: Send + Sync {
    /// Load the sidecar at `sidecar`, parsing its TOML body.
    fn load(&self, sidecar: &ProjectPath) -> Result<MetaDocument, MetaStoreError>;

    /// Serialize `doc` to TOML and write it to `sidecar` atomically
    /// (temp file + rename). Any preexisting sidecar is replaced.
    fn save(&self, sidecar: &ProjectPath, doc: &MetaDocument) -> Result<(), MetaStoreError>;

    /// Return `true` if a sidecar exists at `sidecar`.
    fn exists(&self, sidecar: &ProjectPath) -> bool;

    /// Remove the sidecar at `sidecar`. Missing files surface as
    /// [`FsError::NotFound`].
    fn delete(&self, sidecar: &ProjectPath) -> Result<(), MetaStoreError>;
}

/// `MetaStore` backed by a [`FileSystem`] — the real implementation used in
/// production. Generic over the filesystem so that tests can swap in
/// `MemFileSystem`.
#[derive(Debug, Clone)]
pub struct StdMetaStore<F: FileSystem> {
    fs: F,
}

impl<F: FileSystem> StdMetaStore<F> {
    /// Construct a new store that reads and writes through `fs`.
    pub fn new(fs: F) -> Self {
        Self { fs }
    }

    /// Access the underlying filesystem — handy when callers need to compose
    /// meta-store and filesystem operations inside a single reconcile pass.
    pub fn filesystem(&self) -> &F {
        &self.fs
    }
}

impl<F: FileSystem> MetaStore for StdMetaStore<F> {
    fn load(&self, sidecar: &ProjectPath) -> Result<MetaDocument, MetaStoreError> {
        let bytes = self.fs.read(sidecar)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| MetaStoreError::InvalidUtf8(sidecar.to_string()))?;
        Ok(MetaDocument::from_toml_str(&text)?)
    }

    fn save(&self, sidecar: &ProjectPath, doc: &MetaDocument) -> Result<(), MetaStoreError> {
        let text = doc.to_toml_string()?;
        self.fs.write_atomic(sidecar, text.as_bytes())?;
        Ok(())
    }

    fn exists(&self, sidecar: &ProjectPath) -> bool {
        self.fs.exists(sidecar)
    }

    fn delete(&self, sidecar: &ProjectPath) -> Result<(), MetaStoreError> {
        self.fs.remove_file(sidecar)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;
    use crate::identity::{FileId, Fingerprint};

    fn sample_doc() -> MetaDocument {
        MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff"
                .parse::<Fingerprint>()
                .unwrap(),
        )
    }

    #[test]
    fn sidecar_path_appends_meta_suffix() {
        let file = ProjectPath::new("assets/foo.psd").unwrap();
        let sidecar = sidecar_path(&file).unwrap();
        assert_eq!(sidecar.as_str(), "assets/foo.psd.meta");
    }

    #[test]
    fn sidecar_path_rejects_project_root() {
        let err = sidecar_path(&ProjectPath::root()).unwrap_err();
        assert!(matches!(err, MetaStoreError::Path(_)));
    }

    #[test]
    fn save_then_load_round_trips() {
        let fs = MemFileSystem::new();
        let store = StdMetaStore::new(fs);

        let sidecar = ProjectPath::new("foo.psd.meta").unwrap();
        let doc = sample_doc();
        store.save(&sidecar, &doc).unwrap();
        let loaded = store.load(&sidecar).unwrap();
        assert_eq!(loaded, doc);
    }

    #[test]
    fn save_replaces_existing_sidecar() {
        let fs = MemFileSystem::new();
        let store = StdMetaStore::new(fs);

        let sidecar = ProjectPath::new("foo.psd.meta").unwrap();
        let first = sample_doc();
        store.save(&sidecar, &first).unwrap();

        let second = sample_doc();
        assert_ne!(first.file_id, second.file_id);
        store.save(&sidecar, &second).unwrap();

        let loaded = store.load(&sidecar).unwrap();
        assert_eq!(loaded.file_id, second.file_id);
    }

    #[test]
    fn load_missing_surfaces_fs_not_found() {
        let fs = MemFileSystem::new();
        let store = StdMetaStore::new(fs);
        let sidecar = ProjectPath::new("missing.psd.meta").unwrap();
        let err = store.load(&sidecar).unwrap_err();
        assert!(matches!(err, MetaStoreError::Fs(FsError::NotFound(_))));
    }

    #[test]
    fn exists_reflects_save_and_delete() {
        let fs = MemFileSystem::new();
        let store = StdMetaStore::new(fs);
        let sidecar = ProjectPath::new("foo.psd.meta").unwrap();
        assert!(!store.exists(&sidecar));

        store.save(&sidecar, &sample_doc()).unwrap();
        assert!(store.exists(&sidecar));

        store.delete(&sidecar).unwrap();
        assert!(!store.exists(&sidecar));
    }

    #[test]
    fn load_rejects_non_utf8_contents() {
        // Craft a filesystem with raw non-UTF-8 bytes at the sidecar path.
        // `write_atomic` accepts raw bytes, so we can seed invalid UTF-8 directly.
        let fs = MemFileSystem::new();
        let sidecar = ProjectPath::new("bad.psd.meta").unwrap();
        fs.write_atomic(&sidecar, &[0xff, 0xfe, 0xfd]).unwrap();
        let store = StdMetaStore::new(fs);
        let err = store.load(&sidecar).unwrap_err();
        assert!(matches!(err, MetaStoreError::InvalidUtf8(_)));
    }
}
