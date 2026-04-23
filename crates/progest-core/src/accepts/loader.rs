//! Parse the `[accepts]` block out of a `.dirmeta.toml`'s raw
//! `extra` table into a typed [`RawAccepts`].
//!
//! This stops one step short of alias expansion: unknown-alias
//! detection needs the project's alias catalog, which lives in
//! `.progest/schema.toml` and is loaded separately.
//! [`extract_accepts`] therefore only enforces the local shape
//! (inherit bool, exts array, mode enum, per-token grammar) and
//! leaves alias-name resolution to the effective-accepts pass.
//!
//! Section references target REQUIREMENTS.md §3.13 and
//! `docs/ACCEPTS_ALIASES.md` §3.1.

use thiserror::Error;
use toml::{Table, Value};

use super::types::{AcceptsToken, RawAccepts, is_valid_alias_name, normalize_ext};
use crate::rules::Mode;

/// Non-fatal issue detected while parsing a single `[accepts]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptsWarning {
    /// An unknown key sat alongside `inherit` / `exts` / `mode`
    /// inside `[accepts]`. Forward-compat: kept as a warning, not an
    /// error, so a newer config still loads on older builds.
    UnknownKey { key: String },
}

/// Fatal error surfaced while parsing `[accepts]`.
#[derive(Debug, Error)]
pub enum AcceptsLoadError {
    /// `[accepts]` was present but was not a table.
    #[error("`accepts` must be a TOML table, got {ty}")]
    NotATable { ty: &'static str },

    /// A required field had the wrong TOML type.
    #[error("invalid value for `accepts.{field}`: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },

    /// A token inside `accepts.exts` didn't match any of the three
    /// allowed shapes: `":alias"`, `".ext"`, or `""`.
    #[error(
        "invalid entry in `accepts.exts`: `{raw}` — expected `.<ext>`, `:<alias>`, or empty string"
    )]
    InvalidExtToken { raw: String },

    /// A `:alias` reference used an identifier outside
    /// `^[a-z][a-z0-9_-]*$`.
    #[error("invalid alias name in `accepts.exts`: `:{name}`")]
    InvalidAliasName { name: String },
}

/// Result of a successful extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptsExtraction {
    pub accepts: RawAccepts,
    pub warnings: Vec<AcceptsWarning>,
}

/// Pull `[accepts]` out of the dirmeta document's raw `extra` table.
///
/// Returns `Ok(None)` when the dir has no `[accepts]` section at all
/// (REQUIREMENTS.md §3.13.1: "未設定 = 全拡張子を受け入れる"). Returns
/// `Ok(Some(_))` with the parsed payload when the section exists,
/// even if empty (`exts = []` with `inherit = false` is allowed and
/// means "this dir intentionally rejects every file"). Callers that
/// want placement lint to skip empty-but-declared dirs can check
/// `accepts.exts.is_empty()` themselves.
///
/// # Errors
///
/// [`AcceptsLoadError`] for structural problems in the `[accepts]`
/// table.
pub fn extract_accepts(extra: &Table) -> Result<Option<AcceptsExtraction>, AcceptsLoadError> {
    let Some(raw_value) = extra.get("accepts") else {
        return Ok(None);
    };
    let Value::Table(accepts_table) = raw_value else {
        return Err(AcceptsLoadError::NotATable {
            ty: raw_value.type_str(),
        });
    };

    let mut warnings = Vec::new();
    let mut inherit = false;
    let mut exts: Vec<AcceptsToken> = Vec::new();
    let mut mode = Mode::Warn;

    for (key, value) in accepts_table {
        match key.as_str() {
            "inherit" => {
                inherit = value
                    .as_bool()
                    .ok_or_else(|| AcceptsLoadError::InvalidField {
                        field: "inherit",
                        message: format!("expected boolean, got {}", value.type_str()),
                    })?;
            }
            "exts" => {
                let Value::Array(items) = value else {
                    return Err(AcceptsLoadError::InvalidField {
                        field: "exts",
                        message: format!("expected array, got {}", value.type_str()),
                    });
                };
                for item in items {
                    let Value::String(raw) = item else {
                        return Err(AcceptsLoadError::InvalidField {
                            field: "exts",
                            message: format!(
                                "array entry must be a string, got {}",
                                item.type_str()
                            ),
                        });
                    };
                    exts.push(parse_ext_token(raw)?);
                }
            }
            "mode" => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| AcceptsLoadError::InvalidField {
                        field: "mode",
                        message: format!("expected string, got {}", value.type_str()),
                    })?;
                mode = match raw {
                    "strict" => Mode::Strict,
                    "warn" => Mode::Warn,
                    "hint" => Mode::Hint,
                    "off" => Mode::Off,
                    other => {
                        return Err(AcceptsLoadError::InvalidField {
                            field: "mode",
                            message: format!(
                                "unknown mode `{other}` (expected strict/warn/hint/off)"
                            ),
                        });
                    }
                };
            }
            other => warnings.push(AcceptsWarning::UnknownKey {
                key: other.to_owned(),
            }),
        }
    }

    Ok(Some(AcceptsExtraction {
        accepts: RawAccepts {
            inherit,
            exts,
            mode,
        },
        warnings,
    }))
}

fn parse_ext_token(raw: &str) -> Result<AcceptsToken, AcceptsLoadError> {
    // REQUIREMENTS.md §3.13.1 + ACCEPTS_ALIASES.md §3.1:
    //  - `:alias`  → alias reference
    //  - `.ext`    → extension (leading dot mandatory)
    //  - ``       → the empty-string / no-extension sentinel
    // Anything else is rejected so typos like "psd" (no leading dot)
    // don't silently turn into an unmatched literal.
    if raw.is_empty() {
        return Ok(AcceptsToken::Ext(normalize_ext("")));
    }
    if let Some(alias) = raw.strip_prefix(':') {
        if !is_valid_alias_name(alias) {
            return Err(AcceptsLoadError::InvalidAliasName {
                name: alias.to_owned(),
            });
        }
        return Ok(AcceptsToken::Alias(alias.to_owned()));
    }
    if raw.starts_with('.') {
        return Ok(AcceptsToken::Ext(normalize_ext(raw)));
    }
    Err(AcceptsLoadError::InvalidExtToken {
        raw: raw.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    fn table(input: &str) -> Table {
        toml::from_str(input).expect("test TOML should parse")
    }

    // --- Happy path ---------------------------------------------------------

    #[test]
    fn absent_accepts_returns_none() {
        let t = table("other_key = 1\n");
        assert_eq!(extract_accepts(&t).unwrap(), None);
    }

    #[test]
    fn parses_full_shape() {
        let t = table(
            r#"
[accepts]
inherit = true
exts = [":image", ".psd", ".tar.gz", ""]
mode = "strict"
"#,
        );
        let got = extract_accepts(&t).unwrap().unwrap();
        assert!(got.accepts.inherit);
        assert_eq!(got.accepts.mode, Mode::Strict);
        assert_eq!(
            got.accepts.exts,
            vec![
                AcceptsToken::Alias("image".into()),
                AcceptsToken::Ext(normalize_ext(".psd")),
                AcceptsToken::Ext(normalize_ext(".tar.gz")),
                AcceptsToken::Ext(normalize_ext("")),
            ]
        );
        assert!(got.warnings.is_empty());
    }

    #[test]
    fn defaults_when_fields_omitted() {
        let t = table(
            r#"
[accepts]
exts = [".psd"]
"#,
        );
        let got = extract_accepts(&t).unwrap().unwrap();
        assert!(!got.accepts.inherit);
        assert_eq!(got.accepts.mode, Mode::Warn);
    }

    // --- Warnings -----------------------------------------------------------

    #[test]
    fn unknown_key_is_warning_not_error() {
        let t = table(
            r#"
[accepts]
exts = [".psd"]
future_knob = true
"#,
        );
        let got = extract_accepts(&t).unwrap().unwrap();
        assert_eq!(
            got.warnings,
            vec![AcceptsWarning::UnknownKey {
                key: "future_knob".into()
            }]
        );
    }

    // --- Errors -------------------------------------------------------------

    #[test]
    fn accepts_not_table_errors() {
        let mut t = Table::new();
        t.insert("accepts".into(), Value::String("not-a-table".into()));
        assert!(matches!(
            extract_accepts(&t).unwrap_err(),
            AcceptsLoadError::NotATable { .. }
        ));
    }

    #[test]
    fn ext_without_leading_dot_is_rejected() {
        // Would otherwise silently be treated as a weird literal.
        let t = table(
            r#"
[accepts]
exts = ["psd"]
"#,
        );
        assert!(matches!(
            extract_accepts(&t).unwrap_err(),
            AcceptsLoadError::InvalidExtToken { ref raw } if raw == "psd"
        ));
    }

    #[test]
    fn alias_with_invalid_name_is_rejected() {
        let t = table(
            r#"
[accepts]
exts = [":Foo"]
"#,
        );
        assert!(matches!(
            extract_accepts(&t).unwrap_err(),
            AcceptsLoadError::InvalidAliasName { ref name } if name == "Foo"
        ));
    }

    #[test]
    fn unknown_mode_is_rejected() {
        let t = table(
            r#"
[accepts]
exts = [".psd"]
mode = "maybe"
"#,
        );
        assert!(matches!(
            extract_accepts(&t).unwrap_err(),
            AcceptsLoadError::InvalidField { field: "mode", .. }
        ));
    }

    #[test]
    fn inherit_wrong_type_is_rejected() {
        let t = table(
            r#"
[accepts]
inherit = "yes"
exts = [".psd"]
"#,
        );
        assert!(matches!(
            extract_accepts(&t).unwrap_err(),
            AcceptsLoadError::InvalidField {
                field: "inherit",
                ..
            }
        ));
    }

    #[test]
    fn empty_exts_is_allowed() {
        // A declared dir with `exts = []` and `inherit = false`
        // intentionally rejects everything. Valid, just very strict.
        let t = table(
            r"
[accepts]
exts = []
",
        );
        let got = extract_accepts(&t).unwrap().unwrap();
        assert!(got.accepts.exts.is_empty());
    }
}
