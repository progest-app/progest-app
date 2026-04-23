//! Constraint-rule compiler and evaluator (§5).
//!
//! [`compile_constraint`] turns a [`RawConstraintBody`] into a
//! [`CompiledConstraint`] — regex / forbidden-char / casing pre-
//! compilation happen once here so the evaluator hot path stays
//! allocation-free.
//!
//! [`evaluate_constraint`] AND-composes every field in one compiled
//! constraint against a basename and returns zero or more
//! [`ConstraintFailure`] entries. Each failure maps 1:1 to a
//! `Violation` in the eval pass; `core::rules::eval` can concatenate
//! failures across multiple constraint rules without any additional
//! logic, which is exactly what spec §8.1 asks for.
//!
//! Section references below target `docs/NAMING_RULES_DSL.md`.

use std::fmt;

use regex::Regex;
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

use super::loader::RawConstraintBody;
use super::types::{Casing, Charset};

// --- Public types ----------------------------------------------------------

/// A compiled constraint ready for repeated evaluation.
#[derive(Debug)]
pub struct CompiledConstraint {
    pub charset: Charset,
    pub casing: Casing,
    casing_regex: Option<Regex>,
    pub forbidden_chars: Vec<char>,
    pub forbidden_patterns: Vec<Regex>,
    /// Lowercased for case-insensitive comparison at evaluation time.
    pub reserved_words: Vec<String>,
    pub max_length: u32,
    pub min_length: u32,
    pub required_prefix: String,
    pub required_suffix: String,
}

/// Builtin compound extension tokens recognized by Progest v1 (§4.8).
///
/// Users may extend this via `.progest/schema.toml [extension_compounds]`
/// in a later commit — the evaluator simply takes the list as input.
pub const BUILTIN_COMPOUND_EXTS: &[&str] = &["tar.gz", "blend1"];

/// One atomic constraint violation. Many of these can come from a
/// single `evaluate_constraint` call (all fields AND-compose).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintFailure {
    Charset { kind: Charset, offending: Vec<char> },
    Casing { expected: Casing, actual: String },
    ForbiddenChar { ch: char },
    ForbiddenPattern { pattern: String, hit: String },
    ReservedWord { word: String },
    TooLong { length: u32, max: u32 },
    TooShort { length: u32, min: u32 },
    MissingPrefix { required: String },
    MissingSuffix { required: String },
}

impl fmt::Display for ConstraintFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Charset { kind, offending } => {
                let rendered = offending
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "charset {kind:?} violated by: {rendered}")
            }
            Self::Casing { expected, actual } => {
                write!(f, "expected {expected:?} casing, got `{actual}`")
            }
            Self::ForbiddenChar { ch } => write!(f, "forbidden character `{ch}` in basename"),
            Self::ForbiddenPattern { pattern, hit } => {
                write!(f, "forbidden pattern `{pattern}` matched `{hit}`")
            }
            Self::ReservedWord { word } => write!(f, "reserved word `{word}` in basename"),
            Self::TooLong { length, max } => {
                write!(f, "basename has {length} graphemes, exceeds max {max}")
            }
            Self::TooShort { length, min } => {
                write!(f, "basename has {length} graphemes, below min {min}")
            }
            Self::MissingPrefix { required } => write!(f, "required prefix `{required}` missing"),
            Self::MissingSuffix { required } => write!(f, "required suffix `{required}` missing"),
        }
    }
}

/// Errors returned while compiling a constraint body.
#[derive(Debug, Error)]
pub enum ConstraintCompileError {
    #[error("forbidden_chars entry must be exactly one character, got `{0}`")]
    MultiCharForbiddenChar(String),
    #[error("forbidden_chars entry is empty")]
    EmptyForbiddenChar,
    #[error("failed to compile forbidden_patterns regex `{pattern}`: {source}")]
    Regex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("min_length ({min}) is greater than max_length ({max})")]
    InvalidLengthRange { min: u32, max: u32 },
}

// --- Compile ---------------------------------------------------------------

/// Compile a raw constraint body into a shape the evaluator can reuse.
///
/// # Errors
///
/// Returns [`ConstraintCompileError`] if:
/// - `forbidden_chars` contains an empty or multi-codepoint string
///   (each entry must be exactly one `char`);
/// - any `forbidden_patterns` regex fails to compile;
/// - `min_length > max_length`.
pub fn compile_constraint(
    raw: &RawConstraintBody,
) -> Result<CompiledConstraint, ConstraintCompileError> {
    if raw.min_length > raw.max_length {
        return Err(ConstraintCompileError::InvalidLengthRange {
            min: raw.min_length,
            max: raw.max_length,
        });
    }

    let mut forbidden_chars = Vec::with_capacity(raw.forbidden_chars.len());
    for entry in &raw.forbidden_chars {
        let mut chars = entry.chars();
        let first = chars
            .next()
            .ok_or(ConstraintCompileError::EmptyForbiddenChar)?;
        if chars.next().is_some() {
            return Err(ConstraintCompileError::MultiCharForbiddenChar(
                entry.clone(),
            ));
        }
        forbidden_chars.push(first);
    }

    let mut forbidden_patterns = Vec::with_capacity(raw.forbidden_patterns.len());
    for p in &raw.forbidden_patterns {
        let re = Regex::new(p).map_err(|source| ConstraintCompileError::Regex {
            pattern: p.clone(),
            source,
        })?;
        forbidden_patterns.push(re);
    }

    let reserved_words = raw
        .reserved_words
        .iter()
        .map(|w| w.to_lowercase())
        .collect();

    let casing_regex = casing_regex(raw.casing);

    Ok(CompiledConstraint {
        charset: raw.charset,
        casing: raw.casing,
        casing_regex,
        forbidden_chars,
        forbidden_patterns,
        reserved_words,
        max_length: raw.max_length,
        min_length: raw.min_length,
        required_prefix: raw.required_prefix.clone(),
        required_suffix: raw.required_suffix.clone(),
    })
}

fn casing_regex(casing: Casing) -> Option<Regex> {
    // The unwraps below compile static regex literals we authored
    // ourselves — a panic here means a programmer mistake in this
    // file, not a user-supplied input, so it's appropriate.
    match casing {
        Casing::Any => None,
        Casing::Snake => Some(Regex::new(r"^[a-z0-9]+(_[a-z0-9]+)*$").unwrap()),
        Casing::Kebab => Some(Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap()),
        Casing::Camel => Some(Regex::new(r"^[a-z][a-zA-Z0-9]*$").unwrap()),
        Casing::Pascal => Some(Regex::new(r"^[A-Z][a-zA-Z0-9]*$").unwrap()),
    }
}

// --- Evaluate --------------------------------------------------------------

/// AND-compose every constraint field against a basename.
///
/// `compound_exts` controls how the basename is split into stem and
/// extension: the longest matching suffix wins (spec §4.8). Pass
/// [`BUILTIN_COMPOUND_EXTS`] for the v1 default set.
#[must_use]
pub fn evaluate_constraint(
    c: &CompiledConstraint,
    basename: &str,
    compound_exts: &[&str],
) -> Vec<ConstraintFailure> {
    let mut failures = Vec::new();
    let (stem, _ext) = split_basename(basename, compound_exts);

    // charset (basename whole)
    match c.charset {
        Charset::Ascii => {
            let offending: Vec<char> = basename
                .chars()
                .filter(|ch| !is_printable_ascii(*ch))
                .collect();
            if !offending.is_empty() {
                failures.push(ConstraintFailure::Charset {
                    kind: Charset::Ascii,
                    offending,
                });
            }
        }
        Charset::Utf8 => {
            // Native `str` is already guaranteed valid UTF-8; nothing
            // to enforce beyond what Rust's type system already does.
        }
        Charset::NoCjk => {
            let offending: Vec<char> = basename.chars().filter(|ch| is_cjk_blocked(*ch)).collect();
            if !offending.is_empty() {
                failures.push(ConstraintFailure::Charset {
                    kind: Charset::NoCjk,
                    offending,
                });
            }
        }
    }

    // casing (stem)
    if let Some(re) = &c.casing_regex
        && !re.is_match(stem)
    {
        failures.push(ConstraintFailure::Casing {
            expected: c.casing,
            actual: stem.to_owned(),
        });
    }

    // forbidden_chars (stem)
    for &ch in &c.forbidden_chars {
        if stem.contains(ch) {
            failures.push(ConstraintFailure::ForbiddenChar { ch });
        }
    }

    // forbidden_patterns (stem)
    for re in &c.forbidden_patterns {
        if let Some(m) = re.find(stem) {
            failures.push(ConstraintFailure::ForbiddenPattern {
                pattern: re.as_str().to_owned(),
                hit: m.as_str().to_owned(),
            });
        }
    }

    // reserved_words (tokens of stem)
    for token in basename_tokens(stem) {
        let lower = token.to_lowercase();
        if c.reserved_words.iter().any(|w| w == &lower) {
            failures.push(ConstraintFailure::ReservedWord {
                word: token.to_owned(),
            });
        }
    }

    // max/min length (basename whole; NFC-equivalent grapheme counts)
    let len = grapheme_count(basename);
    if len > c.max_length {
        failures.push(ConstraintFailure::TooLong {
            length: len,
            max: c.max_length,
        });
    }
    if len < c.min_length {
        failures.push(ConstraintFailure::TooShort {
            length: len,
            min: c.min_length,
        });
    }

    // required_prefix / required_suffix
    if !c.required_prefix.is_empty() && !basename.starts_with(&c.required_prefix) {
        failures.push(ConstraintFailure::MissingPrefix {
            required: c.required_prefix.clone(),
        });
    }
    if !c.required_suffix.is_empty() && !stem.ends_with(&c.required_suffix) {
        failures.push(ConstraintFailure::MissingSuffix {
            required: c.required_suffix.clone(),
        });
    }

    failures
}

// --- Helpers ---------------------------------------------------------------

/// Split `basename` at the longest matching compound or simple extension.
///
/// Returns `(stem, ext)` where `ext` excludes the leading dot, or
/// `(basename, None)` when the basename has no recognizable extension.
///
/// Compound extensions from `compound_exts` win over the last-dot fallback
/// whenever their entire token matches the suffix; otherwise we fall
/// back to "everything after the final dot" — matching what `.psd`,
/// `.tif`, etc. produce in practice.
#[must_use]
pub fn split_basename<'a>(basename: &'a str, compound_exts: &[&str]) -> (&'a str, Option<&'a str>) {
    let lower = basename.to_ascii_lowercase();

    // Prefer the longest compound match.
    let mut best: Option<(&str, usize)> = None;
    for &compound in compound_exts {
        let with_dot = format!(".{compound}");
        if lower.ends_with(&with_dot.to_ascii_lowercase())
            && let Some(idx) = basename.len().checked_sub(with_dot.len())
            && best.is_none_or(|(_, l)| compound.len() > l)
        {
            best = Some((compound, compound.len()));
            let stem = &basename[..idx];
            let ext = &basename[idx + 1..];
            // Return early with this compound only once we've confirmed
            // nothing longer is available — we check all compounds first
            // because the list is expected to be tiny (v1 has 2 entries).
            // We update `best` instead of returning so the loop keeps
            // looking for longer alternatives.
            let _ = (stem, ext);
        }
    }
    if let Some((compound, _)) = best {
        let idx = basename.len() - compound.len() - 1; // minus dot
        return (&basename[..idx], Some(&basename[idx + 1..]));
    }

    // Fallback: simple "last dot" split, but skip leading-dot files
    // (e.g. `.gitignore`) per ProjectPath::extension's behavior.
    match basename.rfind('.') {
        Some(0) | None => (basename, None),
        Some(idx) => (&basename[..idx], Some(&basename[idx + 1..])),
    }
}

/// Tokenize a stem by the separators spec §5.6 lists (`[_\-. ]+`).
fn basename_tokens(stem: &str) -> Vec<&str> {
    stem.split(['_', '-', '.', ' '])
        .filter(|s| !s.is_empty())
        .collect()
}

fn grapheme_count(s: &str) -> u32 {
    u32::try_from(s.graphemes(true).count()).unwrap_or(u32::MAX)
}

fn is_printable_ascii(c: char) -> bool {
    let cp = u32::from(c);
    (0x20..=0x7E).contains(&cp)
}

/// Spec §5.3: "CJK" block list kept in sync with `NAMING_RULES_DSL.md`.
/// The list is deliberately a bit wider than strict "Han" to include
/// kana, Hangul, CJK symbols and half/full-width forms.
fn is_cjk_blocked(c: char) -> bool {
    let cp = u32::from(c);
    matches!(
        cp,
        0x3000..=0x303F       // CJK Symbols and Punctuation
        | 0x3040..=0x309F     // Hiragana
        | 0x30A0..=0x30FF     // Katakana
        | 0x31F0..=0x31FF     // Katakana Phonetic Extensions
        | 0x3400..=0x4DBF     // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF     // CJK Unified Ideographs
        | 0xA960..=0xA97F     // Hangul Jamo Extended A
        | 0xAC00..=0xD7AF     // Hangul Syllables
        | 0xD7B0..=0xD7FF     // Hangul Jamo Extended B
        | 0xFF00..=0xFFEF     // Halfwidth and Fullwidth Forms
        | 0x1100..=0x11FF     // Hangul Jamo
        | 0x20000..=0x2FA1F   // CJK Unified Ideographs Extension B+
    )
}

// --- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Tests rebuild the raw body in a handful of shapes — a single
    // variadic helper stays more readable than eight ad-hoc shapes,
    // even at 9 params. Clippy's default is 7; this is a test-only
    // helper.
    #[allow(clippy::too_many_arguments)]
    fn raw(
        charset: Charset,
        casing: Casing,
        forbidden_chars: Vec<&str>,
        forbidden_patterns: Vec<&str>,
        reserved_words: Vec<&str>,
        max_length: u32,
        min_length: u32,
        required_prefix: &str,
        required_suffix: &str,
    ) -> RawConstraintBody {
        RawConstraintBody {
            charset,
            casing,
            forbidden_chars: forbidden_chars.into_iter().map(String::from).collect(),
            forbidden_patterns: forbidden_patterns.into_iter().map(String::from).collect(),
            reserved_words: reserved_words.into_iter().map(String::from).collect(),
            max_length,
            min_length,
            required_prefix: required_prefix.into(),
            required_suffix: required_suffix.into(),
        }
    }

    fn default_raw() -> RawConstraintBody {
        raw(
            Charset::Utf8,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        )
    }

    // --- split_basename ---------------------------------------------------

    #[test]
    fn splits_simple_extension() {
        assert_eq!(split_basename("foo.psd", &[]), ("foo", Some("psd")));
    }

    #[test]
    fn splits_compound_extension() {
        assert_eq!(
            split_basename("archive.tar.gz", BUILTIN_COMPOUND_EXTS),
            ("archive", Some("tar.gz"))
        );
    }

    #[test]
    fn compound_beats_trailing_segment() {
        // Without BUILTIN_COMPOUND_EXTS we only peel ".gz".
        assert_eq!(
            split_basename("archive.tar.gz", &[]),
            ("archive.tar", Some("gz"))
        );
    }

    #[test]
    fn leading_dot_file_has_no_extension() {
        assert_eq!(split_basename(".gitignore", &[]), (".gitignore", None));
    }

    #[test]
    fn no_dot_returns_none() {
        assert_eq!(split_basename("README", &[]), ("README", None));
    }

    // --- compile errors ---------------------------------------------------

    #[test]
    fn rejects_multi_char_forbidden_chars() {
        let mut r = default_raw();
        r.forbidden_chars.push("ab".into());
        let err = compile_constraint(&r).unwrap_err();
        assert!(matches!(
            err,
            ConstraintCompileError::MultiCharForbiddenChar(ref s) if s == "ab"
        ));
    }

    #[test]
    fn rejects_invalid_regex() {
        let mut r = default_raw();
        r.forbidden_patterns.push("[oops".into());
        let err = compile_constraint(&r).unwrap_err();
        assert!(matches!(err, ConstraintCompileError::Regex { .. }));
    }

    #[test]
    fn rejects_inverted_length_range() {
        let mut r = default_raw();
        r.min_length = 10;
        r.max_length = 5;
        let err = compile_constraint(&r).unwrap_err();
        assert!(matches!(
            err,
            ConstraintCompileError::InvalidLengthRange { min: 10, max: 5 }
        ));
    }

    // --- charset ----------------------------------------------------------

    #[test]
    fn ascii_charset_flags_non_ascii_bytes() {
        let c = compile_constraint(&raw(
            Charset::Ascii,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        let f = evaluate_constraint(&c, "forêt_night.psd", &[]);
        assert!(matches!(
            &f[..],
            [ConstraintFailure::Charset {
                kind: Charset::Ascii,
                ..
            }]
        ));
    }

    #[test]
    fn ascii_charset_passes_printable_ascii() {
        let c = compile_constraint(&raw(
            Charset::Ascii,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        assert!(evaluate_constraint(&c, "forest_night.psd", &[]).is_empty());
    }

    #[test]
    fn no_cjk_flags_japanese_chars() {
        let c = compile_constraint(&raw(
            Charset::NoCjk,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        let f = evaluate_constraint(&c, "日本語メモ.pdf", &[]);
        assert!(f.iter().any(|v| matches!(
            v,
            ConstraintFailure::Charset {
                kind: Charset::NoCjk,
                ..
            }
        )));
    }

    #[test]
    fn no_cjk_passes_emoji_and_latin() {
        let c = compile_constraint(&raw(
            Charset::NoCjk,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        assert!(evaluate_constraint(&c, "rocket_🚀.png", &[]).is_empty());
    }

    // --- casing -----------------------------------------------------------

    #[test]
    fn snake_casing_passes_and_fails_as_expected() {
        let c = compile_constraint(&raw(
            Charset::Utf8,
            Casing::Snake,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        assert!(evaluate_constraint(&c, "forest_night.psd", &[]).is_empty());
        let f = evaluate_constraint(&c, "ForestNight.psd", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::Casing { .. }))
        );
    }

    #[test]
    fn any_casing_never_complains() {
        let c = compile_constraint(&default_raw()).unwrap();
        assert!(evaluate_constraint(&c, "ForestNight.psd", &[]).is_empty());
    }

    // --- forbidden chars / patterns / reserved words ----------------------

    #[test]
    fn forbidden_chars_trigger_on_stem() {
        let c = compile_constraint(&raw(
            Charset::Utf8,
            Casing::Any,
            vec![" "],
            vec![],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        let f = evaluate_constraint(&c, "hello world.psd", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::ForbiddenChar { ch } if *ch == ' '))
        );
    }

    #[test]
    fn forbidden_patterns_report_match_text() {
        let c = compile_constraint(&raw(
            Charset::Utf8,
            Casing::Any,
            vec![],
            vec!["^_"],
            vec![],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        let f = evaluate_constraint(&c, "_hidden.psd", &[]);
        assert!(f.iter().any(|v| matches!(
            v,
            ConstraintFailure::ForbiddenPattern { pattern, hit } if pattern == "^_" && hit == "_"
        )));
    }

    #[test]
    fn reserved_words_match_tokens_case_insensitive() {
        let c = compile_constraint(&raw(
            Charset::Utf8,
            Casing::Any,
            vec![],
            vec![],
            vec!["final", "copy"],
            255,
            1,
            "",
            "",
        ))
        .unwrap();
        let f = evaluate_constraint(&c, "shot_Final_v02.psd", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::ReservedWord { word } if word == "Final"))
        );

        // A bare substring shouldn't trigger — `finale` contains `final`
        // but is a different token.
        assert!(
            evaluate_constraint(&c, "finale_v02.psd", &[])
                .iter()
                .all(|v| !matches!(v, ConstraintFailure::ReservedWord { .. }))
        );
    }

    // --- length -----------------------------------------------------------

    #[test]
    fn too_long_reports_grapheme_count() {
        let mut r = default_raw();
        r.max_length = 5;
        let c = compile_constraint(&r).unwrap();
        let f = evaluate_constraint(&c, "abcdef.psd", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::TooLong { .. }))
        );
    }

    #[test]
    fn too_short_reports_grapheme_count() {
        let mut r = default_raw();
        r.min_length = 3;
        let c = compile_constraint(&r).unwrap();
        let f = evaluate_constraint(&c, "a", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::TooShort { .. }))
        );
    }

    #[test]
    fn length_counts_graphemes_not_bytes() {
        // Japanese text: 3 graphemes / 9 bytes.
        let mut r = default_raw();
        r.max_length = 3;
        let c = compile_constraint(&r).unwrap();
        assert!(evaluate_constraint(&c, "日本語", &[]).is_empty());
    }

    // --- prefix / suffix ---------------------------------------------------

    #[test]
    fn missing_prefix_reports() {
        let c = compile_constraint(&raw(
            Charset::Utf8,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "ch",
            "",
        ))
        .unwrap();
        assert!(evaluate_constraint(&c, "ch010_v01.psd", &[]).is_empty());
        let f = evaluate_constraint(&c, "sc010_v01.psd", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::MissingPrefix { .. }))
        );
    }

    #[test]
    fn missing_suffix_applies_to_stem() {
        let c = compile_constraint(&raw(
            Charset::Utf8,
            Casing::Any,
            vec![],
            vec![],
            vec![],
            255,
            1,
            "",
            "_final",
        ))
        .unwrap();
        assert!(evaluate_constraint(&c, "scene_final.psd", &[]).is_empty());
        // The `.psd` is on the extension side, so suffix is measured
        // against the stem → `scene` does not end with `_final`.
        let f = evaluate_constraint(&c, "scene.psd", &[]);
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::MissingSuffix { .. }))
        );
    }

    // --- AND composition ---------------------------------------------------

    #[test]
    fn accumulates_multiple_failures_from_one_rule() {
        let c = compile_constraint(&raw(
            Charset::Ascii,
            Casing::Snake,
            vec![" "],
            vec![],
            vec![],
            10,
            1,
            "",
            "",
        ))
        .unwrap();
        let f = evaluate_constraint(&c, "Forest Nightlong.psd", &[]);
        // Expect casing + forbidden_char + too_long; ascii passes.
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::Casing { .. }))
        );
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::ForbiddenChar { .. }))
        );
        assert!(
            f.iter()
                .any(|v| matches!(v, ConstraintFailure::TooLong { .. }))
        );
    }
}
