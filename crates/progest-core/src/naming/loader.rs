//! `[cleanup]` TOML loader.
//!
//! Extracts a [`super::types::CleanupConfig`] from
//! `.progest/project.toml`'s top-level `extra` table (where
//! `ProjectDocument` parks unknown keys). Unknown fields inside
//! `[cleanup]` become warnings rather than errors so that a newer
//! Progest version can add stages without breaking old installations.

use thiserror::Error;
use toml::Value;

use super::types::{CaseStyle, CleanupConfig};

const SECTION: &str = "cleanup";
const KEY_COPY_SUFFIX: &str = "remove_copy_suffix";
const KEY_CJK: &str = "remove_cjk";
const KEY_CASE: &str = "convert_case";

/// Errors that block loading `[cleanup]`. Any recoverable situation
/// (unknown keys, wrong types on non-critical fields) is surfaced via
/// [`CleanupConfigWarning`] instead.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CleanupConfigError {
    #[error("[cleanup] must be a TOML table; got `{got}`")]
    WrongSectionType { got: &'static str },
    #[error("[cleanup].{key} must be a boolean; got `{got}`")]
    BoolExpected {
        key: &'static str,
        got: &'static str,
    },
    #[error("[cleanup].convert_case must be a string; got `{got}`")]
    ConvertCaseType { got: &'static str },
    #[error(
        "[cleanup].convert_case = `{0}` is not a known case style \
         (expected: off|snake|kebab|camel|pascal|lower|upper)"
    )]
    UnknownConvertCase(String),
}

/// Non-fatal surprises surfaced to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupConfigWarning {
    /// An unrecognized key inside `[cleanup]`. Carries the dotted path
    /// so UI can round-trip it in rule-linter output.
    UnknownKey(String),
}

/// Parse `.progest/project.toml`'s top-level `extra` table and pull
/// out `[cleanup]`. Absent section → returns the default config.
///
/// # Errors
///
/// Returns [`CleanupConfigError`] when a required value has the wrong
/// TOML type. Unknown keys are returned as warnings, not errors.
pub fn extract_cleanup_config(
    extra: &toml::Table,
) -> Result<(CleanupConfig, Vec<CleanupConfigWarning>), CleanupConfigError> {
    let mut cfg = CleanupConfig::default();
    let mut warnings = Vec::new();

    let Some(raw) = extra.get(SECTION) else {
        return Ok((cfg, warnings));
    };
    let table = match raw {
        Value::Table(t) => t,
        other => {
            return Err(CleanupConfigError::WrongSectionType {
                got: other.type_str(),
            });
        }
    };

    for (key, val) in table {
        match key.as_str() {
            KEY_COPY_SUFFIX => {
                cfg.remove_copy_suffix = parse_bool(val, KEY_COPY_SUFFIX)?;
            }
            KEY_CJK => {
                cfg.remove_cjk = parse_bool(val, KEY_CJK)?;
            }
            KEY_CASE => {
                cfg.convert_case = parse_case(val)?;
            }
            other => {
                warnings.push(CleanupConfigWarning::UnknownKey(format!(
                    "{SECTION}.{other}"
                )));
            }
        }
    }

    Ok((cfg, warnings))
}

fn parse_bool(val: &Value, key: &'static str) -> Result<bool, CleanupConfigError> {
    val.as_bool().ok_or(CleanupConfigError::BoolExpected {
        key,
        got: val.type_str(),
    })
}

fn parse_case(val: &Value) -> Result<CaseStyle, CleanupConfigError> {
    let s = val.as_str().ok_or(CleanupConfigError::ConvertCaseType {
        got: val.type_str(),
    })?;
    match s {
        "off" => Ok(CaseStyle::Off),
        "snake" => Ok(CaseStyle::Snake),
        "kebab" => Ok(CaseStyle::Kebab),
        "camel" => Ok(CaseStyle::Camel),
        "pascal" => Ok(CaseStyle::Pascal),
        "lower" => Ok(CaseStyle::Lower),
        "upper" => Ok(CaseStyle::Upper),
        other => Err(CleanupConfigError::UnknownConvertCase(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Table;

    fn parse(text: &str) -> Result<(CleanupConfig, Vec<CleanupConfigWarning>), CleanupConfigError> {
        let tbl: Table = toml::from_str(text).expect("valid TOML");
        extract_cleanup_config(&tbl)
    }

    #[test]
    fn absent_section_yields_defaults() {
        let (cfg, warns) = parse("other = true").unwrap();
        assert_eq!(cfg, CleanupConfig::default());
        assert!(warns.is_empty());
    }

    #[test]
    fn full_section_round_trips() {
        let (cfg, warns) = parse(
            r#"
[cleanup]
remove_copy_suffix = true
remove_cjk = true
convert_case = "kebab"
"#,
        )
        .unwrap();
        assert!(cfg.remove_copy_suffix);
        assert!(cfg.remove_cjk);
        assert_eq!(cfg.convert_case, CaseStyle::Kebab);
        assert!(warns.is_empty());
    }

    #[test]
    fn convert_case_off_is_accepted() {
        let (cfg, _) = parse(
            r#"
[cleanup]
convert_case = "off"
"#,
        )
        .unwrap();
        assert_eq!(cfg.convert_case, CaseStyle::Off);
    }

    #[test]
    fn unknown_key_becomes_warning_not_error() {
        let (cfg, warns) = parse(
            r#"
[cleanup]
convert_case = "snake"
future_stage = true
"#,
        )
        .unwrap();
        assert_eq!(cfg.convert_case, CaseStyle::Snake);
        assert_eq!(warns.len(), 1);
        match &warns[0] {
            CleanupConfigWarning::UnknownKey(k) => {
                assert_eq!(k, "cleanup.future_stage");
            }
        }
    }

    #[test]
    fn wrong_bool_type_is_error() {
        let err = parse(
            r#"
[cleanup]
remove_cjk = "yes"
"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CleanupConfigError::BoolExpected {
                key: "remove_cjk",
                ..
            }
        ));
    }

    #[test]
    fn unknown_case_style_is_error() {
        let err = parse(
            r#"
[cleanup]
convert_case = "shouty"
"#,
        )
        .unwrap_err();
        assert!(matches!(err, CleanupConfigError::UnknownConvertCase(s) if s == "shouty"));
    }

    #[test]
    fn section_must_be_a_table() {
        let err = parse(r#"cleanup = "off""#).unwrap_err();
        assert!(matches!(err, CleanupConfigError::WrongSectionType { .. }));
    }
}
