use std::path::Path;

use crate::fs::ProjectPath;
use crate::index::Index;
use crate::meta::MetaStore;

use super::types::{AiConfig, AiContext, AiError, PrivacyFlags, SuggestionType};

const MAX_SIBLINGS: usize = 30;
const MAX_TAGS: usize = 100;
const MAX_DIRS: usize = 50;

/// Gather the context needed for an AI prompt about `file_path`.
///
/// Only collects the context relevant for `suggestion_type` to keep
/// prompt size (and API costs) down.
pub fn gather_context(
    file_path: &ProjectPath,
    suggestion_type: SuggestionType,
    privacy: &PrivacyFlags,
    project_root: &Path,
    index: &dyn Index,
    meta_store: &dyn MetaStore,
    config: &AiConfig,
) -> Result<AiContext, AiError> {
    let raw = file_path.as_str();

    let file_name = raw
        .rsplit_once('/')
        .map_or(raw, |(_, name)| name)
        .to_string();

    let file_extension = file_name.rsplit_once('.').map(|(_, ext)| ext.to_string());

    let parent_dir = raw.rsplit_once('/').map_or(".", |(dir, _)| dir).to_string();

    let mut ctx = AiContext {
        file_name,
        file_extension,
        parent_dir: parent_dir.clone(),
        glossary: config.glossary.clone(),
        ..AiContext::default()
    };

    // Sibling filenames (naming, notes, tags)
    if matches!(
        suggestion_type,
        SuggestionType::Naming | SuggestionType::Notes | SuggestionType::Tags
    ) {
        ctx.sibling_names = collect_siblings(project_root, &parent_dir);
    }

    // Tags on this file + project vocabulary (tags, notes)
    if matches!(
        suggestion_type,
        SuggestionType::Tags | SuggestionType::Notes | SuggestionType::Naming
    ) && let Some(row) = index
        .get_file_by_path(file_path)
        .map_err(|e| AiError::ContextError(e.to_string()))?
    {
        ctx.existing_tags = index.list_tags_for_file(&row.file_id).unwrap_or_default();
    }

    if matches!(suggestion_type, SuggestionType::Tags) {
        ctx.project_tag_vocabulary = index
            .list_all_tags()
            .unwrap_or_default()
            .into_iter()
            .take(MAX_TAGS)
            .collect();
    }

    // Notes body — only if privacy permits
    if matches!(suggestion_type, SuggestionType::Notes) && privacy.include_notes {
        let sidecar_path = format!("{raw}.meta");
        if let Ok(sidecar) = ProjectPath::new(&sidecar_path)
            && let Ok(doc) = meta_store.load(&sidecar)
        {
            ctx.notes_body = doc
                .notes
                .as_ref()
                .map(|n| n.body.clone())
                .filter(|b| !b.is_empty());
        }
    }

    // Directory structure + accepts (placement)
    if matches!(suggestion_type, SuggestionType::Placement) {
        ctx.project_dirs = collect_project_dirs(project_root);
        ctx.dir_accepts = collect_dir_accepts(project_root);
    }

    Ok(ctx)
}

fn collect_siblings(project_root: &Path, parent_dir: &str) -> Vec<String> {
    let abs = project_root.join(parent_dir);
    let Ok(entries) = std::fs::read_dir(abs) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_meta = name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("meta"));
            if name.starts_with('.') || is_meta || name == ".dirmeta.toml" {
                return None;
            }
            if e.file_type().ok()?.is_file() {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names.truncate(MAX_SIBLINGS);
    names
}

fn collect_project_dirs(project_root: &Path) -> Vec<String> {
    let mut dirs = Vec::new();
    collect_dirs_recursive(project_root, project_root, &mut dirs, 3);
    dirs.truncate(MAX_DIRS);
    dirs
}

fn collect_dirs_recursive(root: &Path, current: &Path, out: &mut Vec<String>, depth: u8) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
        }
        collect_dirs_recursive(root, &path, out, depth - 1);
    }
}

fn read_accepts_extensions(dirmeta: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(dirmeta).ok()?;
    let table = content.parse::<toml::Table>().ok()?;
    let accepts = table.get("accepts")?.as_table()?;
    let exts = accepts.get("extensions")?.as_array()?;
    let strs: Vec<String> = exts
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if strs.is_empty() { None } else { Some(strs) }
}

fn collect_dir_accepts(project_root: &Path) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    collect_accepts_recursive(project_root, project_root, &mut result, 3);
    result.truncate(MAX_DIRS);
    result
}

fn collect_accepts_recursive(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, Vec<String>)>,
    depth: u8,
) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };

    let dirmeta = current.join(".dirmeta.toml");
    if let Some(ext_strs) = read_accepts_extensions(&dirmeta) {
        let rel = current
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push((rel, ext_strs));
    }

    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_accepts_recursive(root, &path, out, depth - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::StdFileSystem;
    use crate::index::SqliteIndex;
    use crate::meta::StdMetaStore;

    #[test]
    fn gather_naming_context() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create files
        std::fs::create_dir_all(root.join("shots/sh010/comp")).unwrap();
        std::fs::write(root.join("shots/sh010/comp/hero_v01.psd"), b"").unwrap();
        std::fs::write(root.join("shots/sh010/comp/hero_v02.psd"), b"").unwrap();
        std::fs::write(root.join("shots/sh010/comp/hero_v03.psd"), b"").unwrap();

        let index = SqliteIndex::open_in_memory().unwrap();
        let meta_store = StdMetaStore::new(StdFileSystem::new(root.to_path_buf()));
        let config = AiConfig::default();
        let privacy = PrivacyFlags::default();
        let file_path = ProjectPath::new("shots/sh010/comp/hero_v03.psd").unwrap();

        let ctx = gather_context(
            &file_path,
            SuggestionType::Naming,
            &privacy,
            root,
            &index,
            &meta_store,
            &config,
        )
        .unwrap();

        assert_eq!(ctx.file_name, "hero_v03.psd");
        assert_eq!(ctx.file_extension.as_deref(), Some("psd"));
        assert_eq!(ctx.parent_dir, "shots/sh010/comp");
        assert!(ctx.sibling_names.contains(&"hero_v01.psd".to_string()));
        assert!(ctx.sibling_names.contains(&"hero_v02.psd".to_string()));
    }

    #[test]
    fn placement_collects_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("shots/sh010/comp")).unwrap();
        std::fs::create_dir_all(root.join("shots/sh010/plate")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("test.psd"), b"").unwrap();

        let index = SqliteIndex::open_in_memory().unwrap();
        let meta_store = StdMetaStore::new(StdFileSystem::new(root.to_path_buf()));
        let config = AiConfig::default();
        let privacy = PrivacyFlags::default();
        let file_path = ProjectPath::new("test.psd").unwrap();

        let ctx = gather_context(
            &file_path,
            SuggestionType::Placement,
            &privacy,
            root,
            &index,
            &meta_store,
            &config,
        )
        .unwrap();

        assert!(!ctx.project_dirs.is_empty());
        assert!(ctx.project_dirs.iter().any(|d| d.contains("assets")));
        // Siblings not collected for placement
        assert!(ctx.sibling_names.is_empty());
    }

    #[test]
    fn notes_excluded_without_privacy_flag() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(root.join("test.psd"), b"").unwrap();
        std::fs::write(
            root.join("test.psd.meta"),
            "schema_version = 1\nfile_id = \"01961234-5678-7000-8000-000000000001\"\ncontent_fingerprint = \"blake3:00000000000000000000000000000000\"\n\n[notes]\nbody = \"secret notes\"\n",
        )
        .unwrap();

        let index = SqliteIndex::open_in_memory().unwrap();
        let meta_store = StdMetaStore::new(StdFileSystem::new(root.to_path_buf()));
        let config = AiConfig::default();

        let privacy_off = PrivacyFlags {
            include_notes: false,
        };
        let ctx = gather_context(
            &ProjectPath::new("test.psd").unwrap(),
            SuggestionType::Notes,
            &privacy_off,
            root,
            &index,
            &meta_store,
            &config,
        )
        .unwrap();
        assert!(ctx.notes_body.is_none());

        let privacy_on = PrivacyFlags {
            include_notes: true,
        };
        let ctx2 = gather_context(
            &ProjectPath::new("test.psd").unwrap(),
            SuggestionType::Notes,
            &privacy_on,
            root,
            &index,
            &meta_store,
            &config,
        )
        .unwrap();
        assert_eq!(ctx2.notes_body.as_deref(), Some("secret notes"));
    }
}
