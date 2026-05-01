use std::fs;
use std::path::Path;

use crate::project::layout::{
    DOT_DIR, RULES_TOML_FILENAME, SCHEMA_TOML_FILENAME, VIEWS_TOML_FILENAME,
};

use super::types::{ApplyReport, TemplateDocument, TemplateError};

pub fn apply_template(
    project_root: &Path,
    template: &TemplateDocument,
) -> Result<ApplyReport, TemplateError> {
    apply_template_with_progress(project_root, template, &|_, _, _| {})
}

pub fn apply_template_with_progress(
    project_root: &Path,
    template: &TemplateDocument,
    on_progress: &dyn Fn(u64, u64, &str),
) -> Result<ApplyReport, TemplateError> {
    let mut report = ApplyReport::default();

    let config_count = [
        &template.include.rules_toml,
        &template.include.schema_toml,
        &template.include.views_toml,
    ]
    .iter()
    .filter(|c| c.is_some())
    .count();
    let total = (template.directories.len() + config_count + template.dirmeta.len()) as u64;
    let mut step: u64 = 0;

    for dir in &template.directories {
        step += 1;
        on_progress(step, total, "Creating directories\u{2026}");
        let abs = project_root.join(dir);
        if !abs.exists() {
            fs::create_dir_all(&abs)?;
            report.directories_created += 1;
        }
    }

    let dot_dir = project_root.join(DOT_DIR);

    if let Some(content) = &template.include.rules_toml {
        step += 1;
        on_progress(step, total, "Writing rules.toml\u{2026}");
        fs::write(dot_dir.join(RULES_TOML_FILENAME), content)?;
        report.configs_written.push("rules.toml".to_owned());
    }
    if let Some(content) = &template.include.schema_toml {
        step += 1;
        on_progress(step, total, "Writing schema.toml\u{2026}");
        fs::write(dot_dir.join(SCHEMA_TOML_FILENAME), content)?;
        report.configs_written.push("schema.toml".to_owned());
    }
    if let Some(content) = &template.include.views_toml {
        step += 1;
        on_progress(step, total, "Writing views.toml\u{2026}");
        fs::write(dot_dir.join(VIEWS_TOML_FILENAME), content)?;
        report.configs_written.push("views.toml".to_owned());
    }

    for entry in &template.dirmeta {
        step += 1;
        on_progress(step, total, "Writing dirmeta\u{2026}");
        let target_dir = if entry.path == "." {
            project_root.to_path_buf()
        } else {
            let dir = project_root.join(&entry.path);
            if !dir.exists() {
                fs::create_dir_all(&dir)?;
            }
            dir
        };
        fs::write(target_dir.join(".dirmeta.toml"), &entry.content)?;
        report.dirmeta_written += 1;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::initialize;
    use crate::template::types::{DirmetaEntry, IncludeSection, TemplateDocument, TemplateMeta};
    use tempfile::TempDir;

    fn sample_template() -> TemplateDocument {
        TemplateDocument {
            meta: TemplateMeta {
                id: "test".to_owned(),
                name: "Test".to_owned(),
                version: "1.0.0".to_owned(),
                author: String::new(),
                description: String::new(),
                progest_version: "0.1.0".to_owned(),
                created_at: "2026-04-30T00:00:00Z".to_owned(),
            },
            directories: vec![
                "assets/shots".to_owned(),
                "assets/scenes".to_owned(),
                "output".to_owned(),
            ],
            include: IncludeSection {
                rules_toml: Some("schema_version = 1\n".to_owned()),
                schema_toml: None,
                views_toml: Some("schema_version = 1\n".to_owned()),
            },
            dirmeta: vec![DirmetaEntry {
                path: "assets/shots".to_owned(),
                content: "[accepts]\nexts = [\":image\"]\n".to_owned(),
            }],
        }
    }

    #[test]
    fn apply_creates_directories() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        initialize(&project, "Demo").unwrap();

        let template = sample_template();
        let report = apply_template(&project, &template).unwrap();

        assert_eq!(report.directories_created, 3);
        assert!(project.join("assets/shots").is_dir());
        assert!(project.join("assets/scenes").is_dir());
        assert!(project.join("output").is_dir());
    }

    #[test]
    fn apply_writes_config_files() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        initialize(&project, "Demo").unwrap();

        let template = sample_template();
        let report = apply_template(&project, &template).unwrap();

        assert!(report.configs_written.contains(&"rules.toml".to_owned()));
        assert!(report.configs_written.contains(&"views.toml".to_owned()));
        assert!(!report.configs_written.contains(&"schema.toml".to_owned()));

        let rules = std::fs::read_to_string(project.join(".progest/rules.toml")).unwrap();
        assert_eq!(rules, "schema_version = 1\n");
    }

    #[test]
    fn apply_writes_dirmeta_files() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        initialize(&project, "Demo").unwrap();

        let template = sample_template();
        let report = apply_template(&project, &template).unwrap();

        assert_eq!(report.dirmeta_written, 1);
        let dirmeta = std::fs::read_to_string(project.join("assets/shots/.dirmeta.toml")).unwrap();
        assert!(dirmeta.contains("[accepts]"));
    }

    #[test]
    fn apply_is_idempotent_for_existing_dirs() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        initialize(&project, "Demo").unwrap();
        std::fs::create_dir_all(project.join("assets/shots")).unwrap();

        let template = sample_template();
        let report = apply_template(&project, &template).unwrap();
        // assets/shots already existed, so only 2 new dirs
        assert_eq!(report.directories_created, 2);
    }

    #[test]
    fn export_then_apply_round_trips() {
        use crate::template::{export::export_template, types::ExportOptions};

        let tmp = TempDir::new().unwrap();

        // Source project
        let src = tmp.path().join("source");
        let src_root = initialize(&src, "Source").unwrap();
        std::fs::create_dir_all(src_root.root().join("assets/shots")).unwrap();
        std::fs::create_dir_all(src_root.root().join("output")).unwrap();
        std::fs::write(src_root.rules_toml(), "schema_version = 1\n").unwrap();
        std::fs::write(
            src_root.root().join("assets/shots/.dirmeta.toml"),
            "[accepts]\nexts = [\":image\"]\n",
        )
        .unwrap();

        let exported = export_template(&src_root, "Source", &ExportOptions::all()).unwrap();

        // Target project
        let dst = tmp.path().join("target");
        initialize(&dst, "Target").unwrap();
        let report = apply_template(&dst, &exported).unwrap();

        assert!(dst.join("assets/shots").is_dir());
        assert!(dst.join("output").is_dir());
        assert!(dst.join(".progest/rules.toml").is_file());
        assert!(dst.join("assets/shots/.dirmeta.toml").is_file());
        assert!(report.directories_created > 0);
    }
}
