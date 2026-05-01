use std::path::Path;

use anyhow::{Context, Result};

use progest_core::ai::{
    self, AiConfig, AiProvider, PrivacyFlags, SuggestionType, extract_ai_config,
};
use progest_core::fs::{ProjectPath, StdFileSystem};
use progest_core::meta::StdMetaStore;
use progest_core::project::ProjectDocument;

use crate::context;
use crate::output::{self, OutputFormat};

pub struct SuggestArgs {
    pub path: String,
    pub suggestion_type: SuggestionType,
    pub include_notes: bool,
    pub format: OutputFormat,
}

pub fn run_suggest(cwd: &Path, args: &SuggestArgs) -> Result<i32> {
    let root = context::discover_root(cwd)?;
    let index = context::open_index(&root)?;
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let meta_store = StdMetaStore::new(fs);
    let config = load_ai_config(&root)?;
    let privacy = PrivacyFlags {
        include_notes: args.include_notes,
    };
    let file_path = resolve_path(&root, &args.path)?;
    let local = root.dot_dir().join("local");

    let response = ai::suggest(
        &file_path,
        args.suggestion_type,
        &privacy,
        root.root(),
        &index,
        &meta_store,
        &config,
        &local,
    )
    .with_context(|| format!("AI suggest failed for `{}`", args.path))?;

    match args.format {
        OutputFormat::Json => {
            output::emit_json(&response, "ai-suggest")?;
        }
        OutputFormat::Text => {
            println!(
                "{} suggestions for {} ({}ms, {}/{}):",
                args.suggestion_type,
                args.path,
                response.elapsed_ms,
                response.provider,
                response.model,
            );
            for (i, s) in response.suggestions.iter().enumerate() {
                print!("  {}. {}", i + 1, s.value);
                if let Some(expl) = &s.explanation {
                    print!("  — {expl}");
                }
                println!();
            }
        }
    }
    Ok(0)
}

pub fn run_set_key(provider: AiProvider, key: &str) -> Result<i32> {
    ai::store_api_key(provider, key)
        .with_context(|| format!("failed to store API key for `{provider}`"))?;
    println!("API key stored for {provider}");
    Ok(0)
}

#[derive(serde::Serialize)]
struct StatusReport {
    provider: String,
    model: String,
    audit_log: bool,
    has_key: bool,
    glossary: Vec<String>,
}

pub fn run_status(cwd: &Path, format: OutputFormat) -> Result<i32> {
    let root = context::discover_root(cwd)?;
    let config = load_ai_config(&root)?;
    let has_key = ai::has_api_key(config.provider);

    let report = StatusReport {
        provider: config.provider.to_string(),
        model: config.model.clone(),
        audit_log: config.audit_log,
        has_key,
        glossary: config.glossary.clone(),
    };

    match format {
        OutputFormat::Json => output::emit_json(&report, "ai-status")?,
        OutputFormat::Text => {
            println!("Provider:  {}", config.provider);
            println!("Model:     {}", config.model);
            println!("Audit log: {}", if config.audit_log { "on" } else { "off" });
            println!("API key:   {}", if has_key { "stored" } else { "not set" });
            if !config.glossary.is_empty() {
                println!("Glossary:  {}", config.glossary.join(", "));
            }
        }
    }
    Ok(0)
}

fn load_ai_config(root: &progest_core::project::ProjectRoot) -> Result<AiConfig> {
    let toml_path = root.project_toml();
    let text = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("reading {}", toml_path.display()))?;
    let doc = ProjectDocument::from_toml_str(&text)?;
    let (config, warnings) = extract_ai_config(&doc.extra)?;
    for w in &warnings {
        tracing::warn!("project.toml [ai]: {w:?}");
    }
    Ok(config)
}

fn resolve_path(root: &progest_core::project::ProjectRoot, raw: &str) -> Result<ProjectPath> {
    let abs = std::path::Path::new(raw);
    let rel = if abs.is_absolute() {
        abs.strip_prefix(root.root())
            .with_context(|| format!("`{raw}` is not inside the project root"))?
            .to_string_lossy()
            .to_string()
    } else {
        raw.to_string()
    };
    ProjectPath::new(&rel).with_context(|| format!("invalid project path `{rel}`"))
}
