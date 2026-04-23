//! `applies_to` glob compilation and specificity scoring (§3 / §7.4).
//!
//! Two independent jobs live here:
//!
//! 1. Take a raw [`AppliesToRaw`] and turn each pattern into a compiled
//!    [`globset::GlobMatcher`], normalized against the base directory
//!    of the defining file so downstream evaluation always sees
//!    project-root-relative globs (spec §3.1).
//! 2. Compute the specificity score used to pick a template winner
//!    and to explain ties in the `rule_id` trace (§7.4, §9.2).
//!
//! The specificity score intentionally stops at
//! `(literal_segments, literal_chars)` — further tie-breakers (source
//! hierarchy and lexicographic `rule_id`) are applied by the
//! evaluator with information this module doesn't have access to.

use globset::{Glob, GlobMatcher};
use thiserror::Error;

use super::loader::AppliesToRaw;
use super::types::SpecificityScore;
use crate::fs::ProjectPath;

/// The `./` prefix required by spec §3.1 on every pattern.
const LEADING_DOT_SLASH: &str = "./";

/// Glob metacharacters, used for the literal-segment check.
const METACHARS: &[char] = &['*', '?', '[', ']', '\\', '{', '}'];

/// A compiled `applies_to` value: one or more globs plus per-pattern
/// specificity scores.
#[derive(Debug, Clone)]
pub struct CompiledAppliesTo {
    patterns: Vec<CompiledPattern>,
}

/// One normalized pattern after compilation.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    /// The original pattern as written in TOML, kept for tracing.
    raw: String,
    /// The pattern after base-directory normalization, also kept for
    /// tracing so users can see what the engine actually matched
    /// against.
    normalized: String,
    matcher: GlobMatcher,
    specificity: SpecificityScore,
}

impl CompiledPattern {
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    #[must_use]
    pub fn specificity(&self) -> SpecificityScore {
        self.specificity
    }

    /// Test whether a project-root-relative path matches this pattern.
    #[must_use]
    pub fn matches(&self, path: &ProjectPath) -> bool {
        self.matcher.is_match(path.as_str())
    }
}

impl CompiledAppliesTo {
    /// Compile every pattern in `raw` against the given base directory.
    ///
    /// For `.progest/rules.toml` pass [`ProjectPath::root`]; for a
    /// `.dirmeta.toml` at `<dir>` pass that directory so the loader
    /// can honor spec §3.1's "self-relative" semantics.
    ///
    /// # Errors
    ///
    /// Returns [`AppliesToError`] on any malformed pattern: missing
    /// leading `./`, empty body after normalization, unsupported
    /// brace expansion, or any other globset compilation failure.
    pub fn compile(raw: &AppliesToRaw, base: &ProjectPath) -> Result<Self, AppliesToError> {
        let mut patterns = Vec::new();
        for pattern in raw.patterns() {
            patterns.push(compile_single(pattern, base)?);
        }
        if patterns.is_empty() {
            return Err(AppliesToError::Empty);
        }
        Ok(Self { patterns })
    }

    /// Return the first pattern that matches `path`, along with its
    /// specificity. The spec (§3.3) says "any one of the globs is
    /// enough", so we short-circuit on first hit — callers that want
    /// every matching pattern for an `--explain` view can use
    /// [`Self::all_matches`] instead.
    #[must_use]
    pub fn match_best(&self, path: &ProjectPath) -> Option<&CompiledPattern> {
        // Spec §7.4 ranks by specificity within a single rule when the
        // rule has multiple patterns, so pick the highest-specificity
        // pattern that hits rather than the first written.
        self.patterns
            .iter()
            .filter(|p| p.matches(path))
            .max_by_key(|p| p.specificity)
    }

    /// Return every pattern that matches `path` — useful for
    /// `--explain` output where we want to show everything that
    /// contributed.
    pub fn all_matches<'a>(
        &'a self,
        path: &'a ProjectPath,
    ) -> impl Iterator<Item = &'a CompiledPattern> + 'a {
        self.patterns.iter().filter(|p| p.matches(path))
    }

    /// All compiled patterns in original order.
    #[must_use]
    pub fn patterns(&self) -> &[CompiledPattern] {
        &self.patterns
    }
}

fn compile_single(raw: &str, base: &ProjectPath) -> Result<CompiledPattern, AppliesToError> {
    if raw.is_empty() {
        return Err(AppliesToError::Empty);
    }
    if !raw.starts_with(LEADING_DOT_SLASH) {
        return Err(AppliesToError::MissingLeadingDotSlash(raw.to_owned()));
    }

    // Cheap early check — globset would error anyway, but a dedicated
    // message is easier on the user.
    if raw.contains('{') || raw.contains('}') {
        return Err(AppliesToError::BraceExpansion(raw.to_owned()));
    }

    let body = &raw[LEADING_DOT_SLASH.len()..];
    let normalized = if base.is_root() {
        body.to_owned()
    } else if body.is_empty() {
        base.as_str().to_owned()
    } else {
        format!("{}/{body}", base.as_str())
    };

    if normalized.is_empty() {
        return Err(AppliesToError::Empty);
    }

    let glob = Glob::new(&normalized).map_err(|source| AppliesToError::Glob {
        pattern: raw.to_owned(),
        source,
    })?;
    let matcher = glob.compile_matcher();
    let specificity = compute_specificity(&normalized);

    Ok(CompiledPattern {
        raw: raw.to_owned(),
        normalized,
        matcher,
        specificity,
    })
}

/// Compute the specificity score for a normalized pattern (§7.4).
///
/// The input is expected to be project-root-relative, `/`-separated,
/// with no leading `./`. Each `/`-separated segment contributes to the
/// score iff it contains no glob metacharacter.
#[must_use]
pub fn compute_specificity(normalized: &str) -> SpecificityScore {
    let mut literal_segments = 0u32;
    let mut literal_chars = 0u32;
    for seg in normalized.split('/') {
        if seg.is_empty() {
            // `foo//bar` shouldn't reach here (ProjectPath rejects it),
            // but ignore defensively rather than crashing on unusual
            // inputs like a trailing slash.
            continue;
        }
        if seg.chars().any(|c| METACHARS.contains(&c)) {
            continue;
        }
        literal_segments += 1;
        literal_chars =
            literal_chars.saturating_add(u32::try_from(seg.chars().count()).unwrap_or(u32::MAX));
    }
    SpecificityScore {
        literal_segments,
        literal_chars,
    }
}

/// Errors returned while compiling `applies_to`.
#[derive(Debug, Error)]
pub enum AppliesToError {
    #[error("applies_to pattern is empty")]
    Empty,
    #[error("applies_to pattern must start with `./`, got `{0}`")]
    MissingLeadingDotSlash(String),
    #[error("brace expansion is not supported in v1 applies_to, got `{0}`")]
    BraceExpansion(String),
    #[error("failed to compile glob `{pattern}`: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: globset::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn raw_single(s: &str) -> AppliesToRaw {
        AppliesToRaw::Single(s.to_owned())
    }

    fn raw_multi(xs: &[&str]) -> AppliesToRaw {
        AppliesToRaw::Multi(xs.iter().map(|s| (*s).to_owned()).collect())
    }

    // --- specificity computation ------------------------------------------

    #[test]
    fn specificity_counts_literal_segments_only() {
        assert_eq!(
            compute_specificity("assets/shots/ch010/**"),
            SpecificityScore {
                literal_segments: 3,
                literal_chars: 6 + 5 + 5,
            }
        );
        assert_eq!(
            compute_specificity("assets/**"),
            SpecificityScore {
                literal_segments: 1,
                literal_chars: 6,
            }
        );
        assert_eq!(
            compute_specificity("**/*.psd"),
            SpecificityScore {
                literal_segments: 0,
                literal_chars: 0,
            }
        );
    }

    #[test]
    fn specificity_handles_unicode_segments() {
        // `レファレンス` = 6 graphemes; chars().count() gives 6 too.
        let score = compute_specificity("assets/レファレンス/**");
        assert_eq!(
            score,
            SpecificityScore {
                literal_segments: 2,
                literal_chars: 6 + 6,
            }
        );
    }

    // --- compile: base = project root -------------------------------------

    #[test]
    fn compiles_single_pattern_against_project_root() {
        let compiled = CompiledAppliesTo::compile(
            &raw_single("./assets/shots/**/*.psd"),
            &ProjectPath::root(),
        )
        .unwrap();
        assert_eq!(compiled.patterns().len(), 1);
        let pattern = &compiled.patterns()[0];
        assert_eq!(pattern.raw(), "./assets/shots/**/*.psd");
        assert_eq!(pattern.normalized(), "assets/shots/**/*.psd");
    }

    #[test]
    fn matches_project_root_relative_paths() {
        let compiled = CompiledAppliesTo::compile(
            &raw_single("./assets/shots/**/*.psd"),
            &ProjectPath::root(),
        )
        .unwrap();

        let hit = pp("assets/shots/ch010/ch010_001_bg_v03.psd");
        assert!(compiled.match_best(&hit).is_some());

        let miss_dir = pp("assets/misc/foo.psd");
        assert!(compiled.match_best(&miss_dir).is_none());

        let miss_ext = pp("assets/shots/ch010/ch010_001.tif");
        assert!(compiled.match_best(&miss_ext).is_none());
    }

    // --- compile: base = dirmeta directory --------------------------------

    #[test]
    fn rebases_patterns_against_dirmeta_location() {
        let base = pp("references");
        let compiled = CompiledAppliesTo::compile(&raw_single("./**"), &base).unwrap();
        let pattern = &compiled.patterns()[0];
        assert_eq!(pattern.normalized(), "references/**");

        assert!(compiled.match_best(&pp("references/doc.pdf")).is_some());
        assert!(compiled.match_best(&pp("assets/foo.psd")).is_none());
    }

    #[test]
    fn rebase_handles_bare_dot_slash() {
        // `./` against a dirmeta base should mean "this directory".
        let base = pp("assets");
        let compiled = CompiledAppliesTo::compile(&raw_single("./"), &base).unwrap();
        assert_eq!(compiled.patterns()[0].normalized(), "assets");
    }

    // --- arrays / multi-match ---------------------------------------------

    #[test]
    fn compiles_and_matches_array_form() {
        let compiled = CompiledAppliesTo::compile(
            &raw_multi(&["./assets/**", "./references/**"]),
            &ProjectPath::root(),
        )
        .unwrap();
        assert_eq!(compiled.patterns().len(), 2);

        assert!(compiled.match_best(&pp("assets/foo.psd")).is_some());
        assert!(compiled.match_best(&pp("references/doc.pdf")).is_some());
        assert!(compiled.match_best(&pp("docs/memo.md")).is_none());
    }

    #[test]
    fn match_best_picks_most_specific_among_multi() {
        // Both patterns match, but the literal-segment count differs.
        let compiled = CompiledAppliesTo::compile(
            &raw_multi(&["./assets/**", "./assets/shots/**"]),
            &ProjectPath::root(),
        )
        .unwrap();
        let winner = compiled
            .match_best(&pp("assets/shots/ch010/foo.psd"))
            .expect("expected a match");
        assert_eq!(winner.normalized(), "assets/shots/**");
        assert_eq!(
            winner.specificity(),
            SpecificityScore {
                literal_segments: 2,
                literal_chars: 6 + 5,
            }
        );

        let iter_count = compiled
            .all_matches(&pp("assets/shots/ch010/foo.psd"))
            .count();
        assert_eq!(iter_count, 2);
    }

    // --- error cases ------------------------------------------------------

    #[test]
    fn rejects_missing_leading_dot_slash() {
        let err =
            CompiledAppliesTo::compile(&raw_single("assets/**"), &ProjectPath::root()).unwrap_err();
        assert!(matches!(err, AppliesToError::MissingLeadingDotSlash(_)));
    }

    #[test]
    fn rejects_brace_expansion() {
        let err = CompiledAppliesTo::compile(
            &raw_single("./{assets,references}/**"),
            &ProjectPath::root(),
        )
        .unwrap_err();
        assert!(matches!(err, AppliesToError::BraceExpansion(_)));
    }

    #[test]
    fn reports_glob_compile_errors() {
        // An unterminated character class should bubble up as Glob.
        let err = CompiledAppliesTo::compile(&raw_single("./assets/[abc"), &ProjectPath::root())
            .unwrap_err();
        assert!(matches!(err, AppliesToError::Glob { .. }));
    }

    #[test]
    fn rejects_empty_applies_to() {
        let err = CompiledAppliesTo::compile(&raw_multi(&[]), &ProjectPath::root()).unwrap_err();
        assert!(matches!(err, AppliesToError::Empty));

        let err2 = CompiledAppliesTo::compile(&raw_single(""), &ProjectPath::root()).unwrap_err();
        assert!(matches!(err2, AppliesToError::Empty));
    }

    // --- regression: exact path match -------------------------------------

    #[test]
    fn exact_literal_path_matches_only_that_path() {
        let compiled = CompiledAppliesTo::compile(
            &raw_single("./docs/meeting-notes.md"),
            &ProjectPath::root(),
        )
        .unwrap();
        assert!(compiled.match_best(&pp("docs/meeting-notes.md")).is_some());
        assert!(compiled.match_best(&pp("docs/other.md")).is_none());
    }
}
