//! Shared CLI context: project discovery + config loaders.
//!
//! Every reporting subcommand walks the same opener:
//!
//! 1. Find the project root from `cwd`.
//! 2. Load whichever subset of `rules.toml` / `schema.toml` /
//!    `project.toml [cleanup]` the command needs.
//! 3. Surface loader warnings on stderr.
//!
//! This module owns those steps so `lint`, `clean`, and `rename` stop
//! re-implementing them with subtly drifting error messages. The
//! cleanup loader optionally folds in CLI overrides
//! ([`CleanupOverrides`]) since `clean` and `rename` both let the user
//! override `convert_case` / `remove_cjk` / `remove_copy_suffix` from
//! the command line.

use std::path::Path;

use anyhow::{Context, Result};
use progest_core::accepts::{AliasCatalog, SchemaLoad, load_alias_catalog};
use progest_core::history::SqliteStore as HistoryStore;
use progest_core::index::SqliteIndex;
use progest_core::naming::{CaseStyle, CleanupConfig, extract_cleanup_config};
use progest_core::project::{ProjectDocument, ProjectRoot};
use progest_core::rules::{
    CompiledRuleSet, RuleSetLayer, RuleSource, compile_ruleset, load_document,
};

/// Discover the [`ProjectRoot`] containing `cwd`, surfacing the same
/// "no Progest project found" message every command was hand-rolling.
pub fn discover_root(cwd: &Path) -> Result<ProjectRoot> {
    ProjectRoot::discover(cwd).with_context(|| {
        format!(
            "could not find a Progest project at or above `{}`",
            cwd.display()
        )
    })
}

/// Load `rules.toml` (if present) into a [`CompiledRuleSet`]. An
/// absent file yields the empty ruleset — projects without explicit
/// rules still benefit from accepts / sequence checks.
pub fn load_ruleset(root: &ProjectRoot) -> Result<CompiledRuleSet> {
    let path = root.rules_toml();
    if !path.exists() {
        return compile_ruleset(vec![]).context("compiling empty ruleset (should be infallible)");
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading `{}`", path.display()))?;
    let doc = load_document(&raw).with_context(|| format!("parsing `{}`", path.display()))?;
    // Surface loader warnings so rule authors see typos early.
    for w in &doc.warnings {
        eprintln!("warning: rules.toml: {w:?}");
    }
    let layer = RuleSetLayer {
        source: RuleSource::ProjectWide,
        base_dir: progest_core::fs::ProjectPath::root(),
        rules: doc.rules,
    };
    compile_ruleset(vec![layer]).with_context(|| format!("compiling `{}`", path.display()))
}

/// Load `schema.toml` (if present) into an [`AliasCatalog`]. Missing
/// file falls back to the builtin catalog.
pub fn load_alias_catalog_from_root(root: &ProjectRoot) -> Result<AliasCatalog> {
    let path = root.schema_toml();
    if !path.exists() {
        return Ok(AliasCatalog::builtin());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading `{}`", path.display()))?;
    let SchemaLoad { catalog, warnings } =
        load_alias_catalog(&raw).with_context(|| format!("parsing `{}`", path.display()))?;
    for w in &warnings {
        eprintln!("warning: schema.toml: {w:?}");
    }
    Ok(catalog)
}

/// CLI flag overrides folded into `[cleanup]` after the TOML loader
/// returns. `force_*` flags can only flip a setting on — there's no
/// command-line way to force a setting off; users who want that should
/// edit `project.toml`.
#[derive(Debug, Default, Clone)]
pub struct CleanupOverrides {
    pub case: Option<CaseStyle>,
    pub force_remove_cjk: bool,
    pub force_remove_copy_suffix: bool,
}

/// Load `project.toml [cleanup]`, then layer [`CleanupOverrides`] from
/// CLI flags. Warnings (unknown keys, bad types) print to stderr.
pub fn load_cleanup_config(
    root: &ProjectRoot,
    overrides: &CleanupOverrides,
) -> Result<CleanupConfig> {
    let raw = std::fs::read_to_string(root.project_toml())
        .with_context(|| format!("reading `{}`", root.project_toml().display()))?;
    let doc = ProjectDocument::from_toml_str(&raw)
        .with_context(|| format!("parsing `{}`", root.project_toml().display()))?;
    let (mut cfg, warns) = extract_cleanup_config(&doc.extra)
        .with_context(|| format!("reading [cleanup] from `{}`", root.project_toml().display()))?;
    for w in warns {
        eprintln!("warning: project.toml [cleanup]: {w:?}");
    }
    if let Some(case) = overrides.case {
        cfg.convert_case = case;
    }
    if overrides.force_remove_cjk {
        cfg.remove_cjk = true;
    }
    if overrides.force_remove_copy_suffix {
        cfg.remove_copy_suffix = true;
    }
    Ok(cfg)
}

/// Open the project's `SQLite` index. Wraps the recurring "opening
/// index `<path>`" `with_context` call.
pub fn open_index(root: &ProjectRoot) -> Result<SqliteIndex> {
    SqliteIndex::open(&root.index_db())
        .with_context(|| format!("opening index `{}`", root.index_db().display()))
}

/// Open the project's history log. Wraps the recurring "opening
/// history `<path>`" `with_context` call.
pub fn open_history(root: &ProjectRoot) -> Result<HistoryStore> {
    HistoryStore::open(&root.history_db())
        .with_context(|| format!("opening history `{}`", root.history_db().display()))
}
