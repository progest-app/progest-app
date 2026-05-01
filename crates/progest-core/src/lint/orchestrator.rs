//! Walk a batch of paths, produce [`LintReport`].

use std::collections::HashMap;
use std::sync::OnceLock;

use thiserror::Error;

use super::report::{LintReport, LintSummary};
use crate::accepts::{
    AcceptsLoadError, AliasCatalog, EffectiveAccepts, RawAccepts, ResolveError,
    compute_effective_accepts, evaluate_placement_for_file, extract_accepts,
};
use crate::fs::{FileSystem, FsError, ProjectPath};
use crate::identity::FileId;
use crate::meta::{DIRMETA_FILENAME, MetaDocument, MetaStore, MetaStoreError, sidecar_path};
use crate::naming::{CleanupConfig, fill_suggested_names};
use crate::rules::{
    Category, CompiledRuleSet, Decision, RuleId, RuleKind, Severity, Violation, evaluate,
};
use crate::sequence::{DriftReason, DriftViolation, detect_drift, detect_sequences};

/// Stable `rule_id` stamped on every sequence-drift violation. The
/// CLI, lint UI, and saved-search filters key off this id so it must
/// not change across releases.
pub const SEQUENCE_DRIFT_RULE_ID: &str = "sequence-drift";

/// Inputs the orchestrator needs from the caller. Compiled / parsed
/// artifacts rather than raw TOML so the orchestrator stays IO-light
/// and testable.
pub struct LintOptions<'a> {
    pub ruleset: &'a CompiledRuleSet,
    pub alias_catalog: &'a AliasCatalog,
    pub compound_exts: &'a [&'a str],
    pub cleanup_cfg: &'a CleanupConfig,
    /// Retain rule traces for every file, not just violating ones
    /// (DSL §9.3). `false` keeps the report compact.
    pub explain: bool,
}

/// Error surface of [`lint_paths`].
#[derive(Debug, Error)]
pub enum LintError {
    #[error(transparent)]
    Meta(#[from] MetaStoreError),
    #[error(transparent)]
    Accepts(#[from] AcceptsLoadError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("failed to parse `{path}`: {source}")]
    Toml {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("`{0}` is not valid UTF-8")]
    InvalidUtf8(String),
}

/// Run lint across `paths`, returning the grouped report.
///
/// # Errors
///
/// Returns [`LintError`] when a `.meta` or `.dirmeta.toml` read fails,
/// or when `[accepts]` parsing rejects a dirmeta block. Individual
/// evaluation errors (missing `{field:}` references, etc.) are folded
/// into the report as [`Severity::EvaluationError`] violations and do
/// not bubble up.
pub fn lint_paths<F: FileSystem, M: MetaStore>(
    fs: &F,
    meta_store: &M,
    paths: &[ProjectPath],
    opts: &LintOptions<'_>,
) -> Result<LintReport, LintError> {
    lint_paths_with_progress(fs, meta_store, paths, opts, &|_, _, _| {})
}

/// Run lint across `paths` with per-file progress reporting.
pub fn lint_paths_with_progress<F: FileSystem, M: MetaStore>(
    fs: &F,
    meta_store: &M,
    paths: &[ProjectPath],
    opts: &LintOptions<'_>,
    on_progress: &dyn Fn(u64, u64, &str),
) -> Result<LintReport, LintError> {
    let mut naming: Vec<Violation> = Vec::new();
    let mut placement: Vec<Violation> = Vec::new();

    let total = paths.len() as u64;
    let throttle = progress_throttle(total);

    let mut effective_cache: HashMap<ProjectPath, Option<EffectiveAccepts>> = HashMap::new();
    let mut raw_cache: HashMap<ProjectPath, Option<RawAccepts>> = HashMap::new();
    let mut file_ids: HashMap<ProjectPath, FileId> = HashMap::new();

    for (i, p) in paths.iter().enumerate() {
        let current = (i + 1) as u64;
        if should_report(current, total, throttle) {
            on_progress(current, total, "Checking files\u{2026}");
        }
        let meta = try_load_meta(meta_store, p)?;
        if let Some(m) = &meta {
            file_ids.insert(p.clone(), m.file_id);
        }

        let parent = p.parent().unwrap_or_else(ProjectPath::root);
        let effective = ensure_effective(
            fs,
            &parent,
            opts.alias_catalog,
            &mut raw_cache,
            &mut effective_cache,
        )?;

        let outcome = evaluate(p, meta.as_ref(), opts.ruleset, opts.compound_exts);
        let mut naming_hits = outcome.violations;
        if !opts.explain {
            // DSL §9.3: without --explain, keep only the Winner trace
            // rows — Shadowed / NotApplicable rows are only useful
            // when the user is debugging rule authorship.
            for v in &mut naming_hits {
                v.trace.retain(|h| matches!(h.decision, Decision::Winner));
            }
        }
        naming.append(&mut naming_hits);

        if let Some(v) = evaluate_placement_for_file(
            p,
            meta.as_ref().map(|m| m.file_id),
            effective,
            opts.compound_exts,
        ) {
            placement.push(v);
        }
    }

    // Drift runs over the whole batch: the detector groups sequences
    // per-parent internally, so one pass is enough.
    let detection = detect_sequences(paths);
    let drift = detect_drift(&detection);
    let mut sequence: Vec<Violation> = drift
        .into_iter()
        .map(|d| drift_to_violation(d, &file_ids))
        .collect();

    // Naming-only mechanical suggestions (placement has its own
    // `suggested_destinations` path, sequence has `suggested_name`
    // baked in by the detector).
    fill_suggested_names(&mut naming, opts.cleanup_cfg, opts.compound_exts);

    sort_by_path_then_rule(&mut naming);
    sort_by_path_then_rule(&mut placement);
    sort_by_path_then_rule(&mut sequence);

    let summary = build_summary(paths.len(), &naming, &placement, &sequence);

    Ok(LintReport {
        naming,
        placement,
        sequence,
        summary,
    })
}

// --- helpers ---------------------------------------------------------------

fn try_load_meta<M: MetaStore>(
    store: &M,
    file: &ProjectPath,
) -> Result<Option<MetaDocument>, MetaStoreError> {
    if file.is_root() {
        return Ok(None);
    }
    let sidecar = sidecar_path(file)?;
    if !store.exists(&sidecar) {
        return Ok(None);
    }
    match store.load(&sidecar) {
        Ok(doc) => Ok(Some(doc)),
        Err(MetaStoreError::Fs(FsError::NotFound(_))) => Ok(None),
        Err(e) => Err(e),
    }
}

fn ensure_effective<'a, F: FileSystem>(
    fs: &F,
    dir: &ProjectPath,
    catalog: &AliasCatalog,
    raw_cache: &mut HashMap<ProjectPath, Option<RawAccepts>>,
    eff_cache: &'a mut HashMap<ProjectPath, Option<EffectiveAccepts>>,
) -> Result<Option<&'a EffectiveAccepts>, LintError> {
    if !eff_cache.contains_key(dir) {
        let own = ensure_raw(fs, dir, raw_cache)?;
        let mut chain_raws: Vec<RawAccepts> = Vec::new();
        let mut cursor = dir.parent();
        while let Some(ancestor) = cursor {
            if let Some(raw) = ensure_raw(fs, &ancestor, raw_cache)? {
                chain_raws.push(raw);
            }
            cursor = ancestor.parent();
        }
        let chain_refs: Vec<&RawAccepts> = chain_raws.iter().collect();
        let effective = compute_effective_accepts(own.as_ref(), &chain_refs, catalog)?;
        eff_cache.insert(dir.clone(), effective);
    }
    Ok(eff_cache.get(dir).and_then(Option::as_ref))
}

fn ensure_raw<F: FileSystem>(
    fs: &F,
    dir: &ProjectPath,
    cache: &mut HashMap<ProjectPath, Option<RawAccepts>>,
) -> Result<Option<RawAccepts>, LintError> {
    if !cache.contains_key(dir) {
        let raw = load_raw_accepts(fs, dir)?;
        cache.insert(dir.clone(), raw);
    }
    Ok(cache.get(dir).cloned().flatten())
}

fn load_raw_accepts<F: FileSystem>(
    fs: &F,
    dir: &ProjectPath,
) -> Result<Option<RawAccepts>, LintError> {
    let dirmeta_path = if dir.is_root() {
        ProjectPath::new(DIRMETA_FILENAME).expect("DIRMETA_FILENAME is valid")
    } else {
        dir.join(DIRMETA_FILENAME)
            .expect("DIRMETA_FILENAME is a valid segment")
    };
    if !fs.exists(&dirmeta_path) {
        return Ok(None);
    }
    let bytes = match fs.read(&dirmeta_path) {
        Ok(b) => b,
        Err(FsError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(LintError::Fs(e)),
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| LintError::InvalidUtf8(dirmeta_path.as_str().to_owned()))?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| LintError::Toml {
        path: dirmeta_path.as_str().to_owned(),
        source: e,
    })?;
    Ok(extract_accepts(&table)?.map(|ext| ext.accepts))
}

fn drift_to_violation(d: DriftViolation, file_ids: &HashMap<ProjectPath, FileId>) -> Violation {
    Violation {
        file_id: file_ids.get(&d.path).copied(),
        path: d.path.clone(),
        rule_id: sequence_drift_rule_id(),
        category: Category::Sequence,
        kind: RuleKind::Constraint,
        // Drift is advisory-by-default; a future [sequence] config
        // block can let teams upgrade it to strict.
        severity: Severity::Warn,
        reason: format!(
            "sequence drift ({}): actual `{}{}{:0>w1$}.` vs canonical `{}{}{:0>w2$}.` (use canonical to keep the group coherent)",
            drift_reason_tag(d.reason),
            d.actual.stem_prefix,
            d.actual.separator,
            0,
            d.canonical.stem_prefix,
            d.canonical.separator,
            0,
            w1 = d.actual.padding,
            w2 = d.canonical.padding,
        ),
        trace: Vec::new(),
        suggested_names: vec![d.suggested_name],
        placement_details: None,
    }
}

fn drift_reason_tag(reason: DriftReason) -> &'static str {
    match reason {
        DriftReason::Separator => "separator",
        DriftReason::Padding => "padding",
        DriftReason::StemCase => "stem-case",
        DriftReason::Combined => "combined",
    }
}

fn sequence_drift_rule_id() -> RuleId {
    static CELL: OnceLock<RuleId> = OnceLock::new();
    CELL.get_or_init(|| {
        SEQUENCE_DRIFT_RULE_ID
            .parse()
            .expect("SEQUENCE_DRIFT_RULE_ID is a valid RuleId")
    })
    .clone()
}

fn sort_by_path_then_rule(vs: &mut [Violation]) {
    vs.sort_by(|a, b| {
        a.path
            .as_str()
            .cmp(b.path.as_str())
            .then_with(|| a.rule_id.as_str().cmp(b.rule_id.as_str()))
    });
}

fn progress_throttle(total: u64) -> u64 {
    if total <= 200 { 1 } else { total / 100 }
}

fn should_report(current: u64, total: u64, throttle: u64) -> bool {
    current == total || current.is_multiple_of(throttle)
}

fn build_summary(
    scanned: usize,
    naming: &[Violation],
    placement: &[Violation],
    sequence: &[Violation],
) -> LintSummary {
    let all = naming.iter().chain(placement).chain(sequence);
    let mut strict = 0;
    let mut eval = 0;
    let mut warn = 0;
    let mut hint = 0;
    for v in all {
        match v.severity {
            Severity::Strict => strict += 1,
            Severity::EvaluationError => eval += 1,
            Severity::Warn => warn += 1,
            Severity::Hint => hint += 1,
        }
    }
    LintSummary {
        scanned,
        naming_count: naming.len(),
        placement_count: placement.len(),
        sequence_count: sequence.len(),
        strict_count: strict,
        evaluation_error_count: eval,
        warn_count: warn,
        hint_count: hint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;
    use crate::meta::StdMetaStore;
    use crate::naming::CaseStyle;
    use crate::rules::{compile_ruleset, load_document};

    fn opts<'a>(
        ruleset: &'a CompiledRuleSet,
        catalog: &'a AliasCatalog,
        cleanup: &'a CleanupConfig,
    ) -> LintOptions<'a> {
        LintOptions {
            ruleset,
            alias_catalog: catalog,
            compound_exts: &[],
            cleanup_cfg: cleanup,
            explain: false,
        }
    }

    fn empty_ruleset() -> CompiledRuleSet {
        compile_ruleset(vec![]).unwrap()
    }

    fn cleanup_off() -> CleanupConfig {
        CleanupConfig {
            remove_copy_suffix: false,
            remove_cjk: false,
            convert_case: CaseStyle::Off,
        }
    }

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn store_with(files: &[(&str, &str)]) -> StdMetaStore<MemFileSystem> {
        let fs = MemFileSystem::new();
        for (path, content) in files {
            fs.write_atomic(&p(path), content.as_bytes()).unwrap();
        }
        StdMetaStore::new(fs)
    }

    #[test]
    fn empty_input_produces_empty_report() {
        let rules = empty_ruleset();
        let catalog = AliasCatalog::builtin();
        let cleanup = cleanup_off();
        let store = store_with(&[]);
        let report = lint_paths(
            store.filesystem(),
            &store,
            &[],
            &opts(&rules, &catalog, &cleanup),
        )
        .unwrap();
        assert_eq!(report.total(), 0);
        assert_eq!(report.summary.scanned, 0);
        assert!(!report.fails_ci());
    }

    #[test]
    fn placement_violation_from_dirmeta_accepts() {
        let rules = empty_ruleset();
        let catalog = AliasCatalog::builtin();
        let cleanup = cleanup_off();

        // `images/` declares `exts = [".png"]`. A `.mp4` landing there
        // must be flagged.
        let store = store_with(&[
            (
                "images/.dirmeta.toml",
                r#"
schema_version = 1
[accepts]
inherit = false
exts = [".png"]
mode = "warn"
"#,
            ),
            ("images/foo.png", ""),
            ("images/bad.mp4", ""),
        ]);
        let report = lint_paths(
            store.filesystem(),
            &store,
            &[p("images/foo.png"), p("images/bad.mp4")],
            &opts(&rules, &catalog, &cleanup),
        )
        .unwrap();

        assert_eq!(report.naming.len(), 0);
        assert_eq!(report.placement.len(), 1);
        assert_eq!(report.placement[0].path, p("images/bad.mp4"));
        assert_eq!(report.placement[0].category, Category::Placement);
        assert_eq!(report.summary.placement_count, 1);
        assert_eq!(report.summary.warn_count, 1);
        assert!(!report.fails_ci(), "warn does not fail CI");
    }

    #[test]
    fn sequence_drift_flows_through_as_sequence_category() {
        let rules = empty_ruleset();
        let catalog = AliasCatalog::builtin();
        let cleanup = cleanup_off();
        let store = store_with(&[]);
        let paths = vec![
            p("assets/shot_0001.png"),
            p("assets/shot_0002.png"),
            p("assets/shot_0003.png"),
            p("assets/shot_001.png"),
            p("assets/shot_002.png"),
        ];
        let report = lint_paths(
            store.filesystem(),
            &store,
            &paths,
            &opts(&rules, &catalog, &cleanup),
        )
        .unwrap();
        assert_eq!(report.sequence.len(), 2);
        for v in &report.sequence {
            assert_eq!(v.category, Category::Sequence);
            assert_eq!(v.rule_id.as_str(), SEQUENCE_DRIFT_RULE_ID);
            assert_eq!(v.severity, Severity::Warn);
            assert_eq!(v.suggested_names.len(), 1);
        }
    }

    #[test]
    fn strict_naming_violation_fails_ci() {
        let catalog = AliasCatalog::builtin();
        let cleanup = cleanup_off();

        // Rule: assets/**/*.psd basenames must be ASCII.
        let rules_toml = r#"
schema_version = 1

[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = "./assets/**/*.psd"
mode = "strict"
charset = "ascii"
"#;
        let doc = load_document(rules_toml).unwrap();
        let layer = crate::rules::inheritance::RuleSetLayer {
            source: crate::rules::RuleSource::ProjectWide,
            base_dir: ProjectPath::root(),
            rules: doc.rules,
        };
        let ruleset = compile_ruleset(vec![layer]).unwrap();

        let store = store_with(&[("assets/日本語.psd", "")]);
        let report = lint_paths(
            store.filesystem(),
            &store,
            &[p("assets/日本語.psd")],
            &opts(&ruleset, &catalog, &cleanup),
        )
        .unwrap();

        assert!(
            report.naming.iter().any(|v| v.severity == Severity::Strict),
            "strict severity should propagate"
        );
        assert!(report.fails_ci());
        assert!(report.summary.strict_count >= 1);
    }
}
