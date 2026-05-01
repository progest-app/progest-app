//! Build an [`ImportPreview`] from a list of [`ImportRequest`]s,
//! detecting conflicts before any FS mutation happens.

use std::collections::HashSet;
use std::path::Path;

use crate::fs::{FileSystem, ProjectPath};

use super::types::{ImportConflict, ImportOp, ImportPreview, ImportRequest};

/// Build a preview for a batch of import requests.
///
/// Conflict detection:
/// - `SourceMissing`: source path doesn't exist on disk.
/// - `SourceIsProject`: source resolves to a path inside the project
///   (users should use `rename` instead).
/// - `DestExists`: destination already exists in the project FS.
/// - Duplicate destinations within the batch are flagged as `DestExists`.
pub fn build_preview(
    requests: &[ImportRequest],
    fs: &dyn FileSystem,
    project_root: &Path,
) -> ImportPreview {
    let mut seen_dests: HashSet<String> = HashSet::new();
    let mut ops = Vec::with_capacity(requests.len());

    for req in requests {
        let mut conflicts = Vec::new();

        if !req.source.exists() {
            conflicts.push(ImportConflict::SourceMissing {
                reason: "file not found".into(),
            });
        }

        if let Ok(canonical) = dunce::canonicalize(&req.source)
            && let Ok(root_canonical) = dunce::canonicalize(project_root)
            && canonical.starts_with(&root_canonical)
        {
            let rel = canonical
                .strip_prefix(&root_canonical)
                .ok()
                .and_then(|p| p.to_str())
                .and_then(|s| ProjectPath::new(s).ok());
            conflicts.push(ImportConflict::SourceIsProject {
                project_path: rel.unwrap_or_else(|| req.dest.clone()),
            });
        }

        if fs.exists(&req.dest) {
            conflicts.push(ImportConflict::DestExists {
                existing_path: req.dest.clone(),
            });
        }

        let dest_key = req.dest.as_str().to_owned();
        if !seen_dests.insert(dest_key) {
            conflicts.push(ImportConflict::DestExists {
                existing_path: req.dest.clone(),
            });
        }

        ops.push(ImportOp {
            source: req.source.to_string_lossy().into_owned(),
            dest: req.dest.clone(),
            mode: req.mode,
            group_id: req.group_id.clone(),
            conflicts,
        });
    }

    ImportPreview { ops }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;
    use crate::import::types::ImportMode;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn make_temp_file() -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "test content").unwrap();
        f
    }

    #[test]
    fn clean_import_has_no_conflicts() {
        let fs = MemFileSystem::new();
        let tmp = make_temp_file();
        let reqs = vec![ImportRequest {
            source: tmp.path().to_path_buf(),
            dest: p("assets/shot_010.psd"),
            mode: ImportMode::Copy,
            group_id: None,
        }];
        let preview = build_preview(&reqs, &fs, Path::new("/nonexistent-project"));
        assert!(preview.is_clean());
        assert_eq!(preview.ops.len(), 1);
    }

    #[test]
    fn missing_source_flags_conflict() {
        let fs = MemFileSystem::new();
        let reqs = vec![ImportRequest {
            source: "/tmp/definitely-does-not-exist-progest-test.psd".into(),
            dest: p("assets/shot_010.psd"),
            mode: ImportMode::Copy,
            group_id: None,
        }];
        let preview = build_preview(&reqs, &fs, Path::new("/nonexistent-project"));
        assert!(!preview.is_clean());
        assert!(matches!(
            &preview.ops[0].conflicts[0],
            ImportConflict::SourceMissing { .. }
        ));
    }

    #[test]
    fn dest_exists_flags_conflict() {
        let fs = MemFileSystem::new();
        fs.write_atomic(&p("assets/existing.psd"), b"data").unwrap();
        let tmp = make_temp_file();
        let reqs = vec![ImportRequest {
            source: tmp.path().to_path_buf(),
            dest: p("assets/existing.psd"),
            mode: ImportMode::Copy,
            group_id: None,
        }];
        let preview = build_preview(&reqs, &fs, Path::new("/nonexistent-project"));
        assert!(!preview.is_clean());
        assert!(matches!(
            &preview.ops[0].conflicts[0],
            ImportConflict::DestExists { .. }
        ));
    }

    #[test]
    fn duplicate_dest_in_batch_flags_conflict() {
        let fs = MemFileSystem::new();
        let tmp1 = make_temp_file();
        let tmp2 = make_temp_file();
        let reqs = vec![
            ImportRequest {
                source: tmp1.path().to_path_buf(),
                dest: p("assets/shot.psd"),
                mode: ImportMode::Copy,
                group_id: None,
            },
            ImportRequest {
                source: tmp2.path().to_path_buf(),
                dest: p("assets/shot.psd"),
                mode: ImportMode::Copy,
                group_id: None,
            },
        ];
        let preview = build_preview(&reqs, &fs, Path::new("/nonexistent-project"));
        // First op is clean, second has DestExists
        assert!(preview.ops[0].is_clean());
        assert!(!preview.ops[1].is_clean());
    }
}
