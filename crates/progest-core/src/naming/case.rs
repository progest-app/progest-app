//! Case conversion backed by the `heck` crate.
//!
//! The private helpers in `core::rules::template` split words on any
//! non-alphanumeric boundary, which is fine for input like
//! `"Forest Night"` but collapses `"ForestNight"` to the single word
//! `"forestnight"` — the `PascalCase` → `snake_case` transform is
//! silently wrong. Delegating to `heck` gives us the documented
//! ICU-style boundary detection (`camelCase` / `PascalCase` / acronym
//! splits) without hand-rolling a tokenizer here.
//!
//! Digits cling to the word they touch: `"v01"` stays `"v01"` under
//! snake, `"V01"` becomes `"v01"` under snake — matching the
//! REQUIREMENTS §3.5.5 constraint that version tokens survive the
//! pipeline intact.

use heck::{
    ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToShoutySnakeCase as _,
    ToSnakeCase as _,
};
use thiserror::Error;

use super::types::CaseStyle;

/// Errors surfaced by [`convert_case`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CaseConvertError {
    /// `CaseStyle::Off` reached the convert function; callers should
    /// short-circuit before calling.
    #[error("convert_case called with CaseStyle::Off")]
    OffNotCallable,
}

/// Apply [`CaseStyle`] to a literal stem fragment.
///
/// The input is expected to be a single literal [`super::Segment::Literal`]
/// body — never a hole sentinel — because the pipeline must preserve
/// the identity of every hole across the case stage.
///
/// # Errors
///
/// Returns [`CaseConvertError::OffNotCallable`] when `style` is
/// [`CaseStyle::Off`].
pub fn convert_case(input: &str, style: CaseStyle) -> Result<String, CaseConvertError> {
    match style {
        CaseStyle::Off => Err(CaseConvertError::OffNotCallable),
        CaseStyle::Snake => Ok(input.to_snake_case()),
        CaseStyle::Kebab => Ok(input.to_kebab_case()),
        CaseStyle::Camel => Ok(input.to_lower_camel_case()),
        CaseStyle::Pascal => Ok(input.to_pascal_case()),
        CaseStyle::Lower => Ok(input.to_lowercase()),
        // Shouty snake would insert `_`; `Upper` is the spec-defined
        // "just shout it" variant that preserves existing separators.
        CaseStyle::Upper => Ok(input.to_uppercase()),
    }
}

/// Internal helper used by `core::rules::template` to render format
/// specifiers. Split out so the rules engine gets the same word-boundary
/// detection as the cleanup pipeline — see the function-level comment
/// for why this matters.
///
/// # Panics
///
/// Panics when `style` is `Shouty` — reserved for future use; none of
/// the DSL format specifiers map to it. Kept as a panic rather than
/// `Result` because `core::rules::template` already rejected it at
/// parse time.
#[must_use]
pub fn rules_format_spec(input: &str, style: RulesCase) -> String {
    match style {
        RulesCase::Snake => input.to_snake_case(),
        RulesCase::Kebab => input.to_kebab_case(),
        RulesCase::Camel => input.to_lower_camel_case(),
        RulesCase::Pascal => input.to_pascal_case(),
        RulesCase::Lower => input.to_lowercase(),
        RulesCase::Upper => input.to_uppercase(),
        RulesCase::Slug => to_slug(input),
        RulesCase::Shouty => input.to_shouty_snake_case(),
    }
}

/// Styles understood by [`rules_format_spec`]. Kept separate from
/// [`CaseStyle`] because the DSL includes `slug` (which the cleanup
/// pipeline does not expose) and does not include `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesCase {
    Snake,
    Kebab,
    Camel,
    Pascal,
    Lower,
    Upper,
    Slug,
    #[allow(dead_code)]
    Shouty,
}

fn to_slug(s: &str) -> String {
    // `heck` has no slug implementation that matches the spec-defined
    // behavior (collapse non-alphanumeric runs into single `-`, trim
    // edges). Keep the hand-rolled version here so rules::template
    // can retire its local copy.
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_handles_pascal_case_boundary() {
        // Regression: the old rules::template `word_chunks` splitter
        // would collapse `ForestNight` to `forestnight` because it
        // only broke on non-alphanumeric chars. heck handles this.
        assert_eq!(
            convert_case("ForestNight", CaseStyle::Snake).unwrap(),
            "forest_night"
        );
    }

    #[test]
    fn snake_handles_space_separated() {
        assert_eq!(
            convert_case("Forest Night", CaseStyle::Snake).unwrap(),
            "forest_night"
        );
    }

    #[test]
    fn kebab_matches_snake_modulo_separator() {
        assert_eq!(
            convert_case("ForestNight", CaseStyle::Kebab).unwrap(),
            "forest-night"
        );
    }

    #[test]
    fn camel_pascal_invert() {
        assert_eq!(
            convert_case("forest night", CaseStyle::Camel).unwrap(),
            "forestNight"
        );
        assert_eq!(
            convert_case("forest night", CaseStyle::Pascal).unwrap(),
            "ForestNight"
        );
    }

    #[test]
    fn lower_upper_preserve_separators() {
        assert_eq!(
            convert_case("Forest Night", CaseStyle::Lower).unwrap(),
            "forest night"
        );
        assert_eq!(
            convert_case("forest night", CaseStyle::Upper).unwrap(),
            "FOREST NIGHT"
        );
    }

    #[test]
    fn version_token_survives_snake_conversion() {
        // REQUIREMENTS §3.5.5: version tokens like `v01` must not be
        // split or mangled. heck keeps digit runs attached to their
        // preceding letter, which is the desired behavior.
        assert_eq!(
            convert_case("shotV01", CaseStyle::Snake).unwrap(),
            "shot_v01"
        );
        assert_eq!(
            convert_case("shot_v01", CaseStyle::Snake).unwrap(),
            "shot_v01"
        );
    }

    #[test]
    fn off_style_is_rejected() {
        assert!(matches!(
            convert_case("whatever", CaseStyle::Off),
            Err(CaseConvertError::OffNotCallable)
        ));
    }

    // --- rules_format_spec (used by core::rules::template) ---------------

    #[test]
    fn rules_slug_collapses_symbols_to_hyphens() {
        assert_eq!(
            rules_format_spec("Ch 10 / Sc 20", RulesCase::Slug),
            "ch-10-sc-20"
        );
        assert_eq!(
            rules_format_spec("  hello  world  ", RulesCase::Slug),
            "hello-world"
        );
    }

    #[test]
    fn rules_pascal_handles_pascal_input() {
        assert_eq!(
            rules_format_spec("ForestNight", RulesCase::Snake),
            "forest_night"
        );
    }
}
