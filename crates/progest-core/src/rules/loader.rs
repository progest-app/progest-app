//! TOML loader for `.progest/rules.toml` and the `[[rules]]` section of
//! `.dirmeta.toml`.
//!
//! This pass keeps the output intentionally raw: `applies_to` stays as
//! strings (glob compilation lives in `applies_to.rs`), `template`
//! bodies are untouched (parsed by `template.rs`), and constraint
//! regexes are not compiled here. The goal is a faithful AST that
//! downstream commits can turn into compiled, evaluable rules without
//! re-touching TOML.
//!
//! Section references below target `docs/NAMING_RULES_DSL.md`.

use std::collections::BTreeSet;

use serde::Deserialize;
use thiserror::Error;
use toml::{Table, Value};

use super::types::{Casing, Charset, Mode, RuleId, RuleIdError, RuleKind};

/// Schema version understood by this build of the rules loader.
///
/// See §1.3: on load we accept `schema_version == RULES_SCHEMA_VERSION`
/// (known shape, unknown keys become warnings) and `schema_version >
/// RULES_SCHEMA_VERSION` (newer shape, unknown keys kept verbatim in
/// `extra` without warning). Older versions are rejected so migration
/// gets written when it lands.
pub const RULES_SCHEMA_VERSION: u32 = 1;

/// Upper bound on `max_length` default value. Spec §5.2 sets the
/// "no explicit cap" default at 255 graphemes, which dominates the
/// typical filesystem basename limit.
const DEFAULT_MAX_LENGTH: u32 = 255;

/// Spec §5.2: `min_length` defaults to 1 (an empty basename cannot exist
/// anyway, so this just keeps the lower-bound check well-defined).
const DEFAULT_MIN_LENGTH: u32 = 1;

/// Top-level document for `.progest/rules.toml`.
///
/// `warnings` collects non-fatal issues the loader detected (typo
/// candidates under a known schema, duplicate-but-surviving ids, etc.)
/// so callers can surface them in CLI `--format json` or as stderr
/// messages.
#[derive(Debug, Clone, PartialEq)]
pub struct RulesDocument {
    pub schema_version: u32,
    pub rules: Vec<RawRule>,
    pub warnings: Vec<LoadWarning>,
    /// Unknown top-level keys preserved verbatim. Non-empty only when
    /// `schema_version > RULES_SCHEMA_VERSION` (forward-compat) — under
    /// the known schema, unknowns turn into warnings instead.
    pub extra: Table,
}

/// A raw rule entry, independent of the evaluation machinery.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRule {
    pub id: RuleId,
    pub kind: RuleKind,
    pub applies_to: AppliesToRaw,
    pub mode: Mode,
    pub description: Option<String>,
    /// Parsed from the optional `override` TOML key. Interpretation
    /// (§7.2: "must be true when changing kind") happens in the
    /// inheritance pass, not here.
    pub override_flag: bool,
    pub body: RawRuleBody,
    /// Unknown keys inside this `[[rules]]` entry, kept for forward
    /// compat when `schema_version > RULES_SCHEMA_VERSION`.
    pub extra: Table,
}

/// Raw `applies_to` value: either a single glob string or an array of them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum AppliesToRaw {
    Single(String),
    Multi(Vec<String>),
}

impl AppliesToRaw {
    /// Return a non-empty slice view of the globs. Panics are not
    /// possible — emptiness is rejected at parse time.
    #[must_use]
    pub fn patterns(&self) -> Vec<&str> {
        match self {
            Self::Single(s) => vec![s.as_str()],
            Self::Multi(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// Kind-specific body. Keeps the common fields (`id`, `mode`, ...) in
/// [`RawRule`] and the kind-exclusive ones here so invariants can't
/// drift between the two.
#[derive(Debug, Clone, PartialEq)]
pub enum RawRuleBody {
    Template(RawTemplateBody),
    Constraint(RawConstraintBody),
}

/// Template-rule body (§4).
#[derive(Debug, Clone, PartialEq)]
pub struct RawTemplateBody {
    pub template: String,
}

/// Constraint-rule body (§5).
///
/// All fields are resolved to their effective values at load time:
/// omitted knobs collapse to the spec's defaults so the evaluator
/// sees a uniform shape.
#[derive(Debug, Clone, PartialEq)]
pub struct RawConstraintBody {
    pub charset: Charset,
    pub casing: Casing,
    pub forbidden_chars: Vec<String>,
    pub forbidden_patterns: Vec<String>,
    pub reserved_words: Vec<String>,
    pub max_length: u32,
    pub min_length: u32,
    pub required_prefix: String,
    pub required_suffix: String,
}

/// Non-fatal issue detected during load.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadWarning {
    /// Unknown top-level key under `schema_version == RULES_SCHEMA_VERSION`.
    UnknownTopLevelKey { key: String },
    /// Unknown key inside a `[[rules]]` entry under the known schema.
    UnknownRuleKey { rule_id: String, key: String },
    /// Reserved for the inheritance pass — not emitted by the loader.
    /// Kept here so callers can pattern-match against a single enum.
    OverrideWithoutExplicitFlag { rule_id: String },
}

/// Fatal parse / validation error.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("failed to parse rules TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to decode rules TOML: {0}")]
    Decode(String),
    #[error("missing required field `{0}`")]
    MissingField(&'static str),
    #[error("invalid value for field `{field}`: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },
    #[error("invalid rule id `{raw}`: {source}")]
    InvalidRuleId {
        raw: String,
        #[source]
        source: RuleIdError,
    },
    #[error(
        "unsupported schema_version {found}; this build expects {expected} (or newer for forward compat)"
    )]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("duplicate rule id `{0}` within the same file")]
    DuplicateId(String),
    #[error("rule `{0}` has an empty `applies_to`")]
    EmptyAppliesTo(String),
    #[error("template rule `{0}` requires a non-empty `template` field")]
    EmptyTemplate(String),
    #[error("rule `{rule_id}` (kind = constraint) must not define `template`")]
    UnexpectedTemplateField { rule_id: String },
    #[error("rule `{rule_id}` (kind = template) must not define constraint field `{field}`")]
    UnexpectedConstraintField {
        rule_id: String,
        field: &'static str,
    },
}

/// Parse `rules.toml` content into a [`RulesDocument`].
///
/// # Errors
///
/// Returns [`LoadError`] for malformed TOML, unsupported schema
/// versions, missing required fields, invalid rule ids, duplicate ids,
/// and cross-kind field misuse. Non-fatal issues (typo candidates,
/// unknown keys under the known schema) come back in
/// [`RulesDocument::warnings`].
pub fn load_document(raw: &str) -> Result<RulesDocument, LoadError> {
    let mut root: Table = toml::from_str(raw)?;

    let schema_version = extract_schema_version(&mut root)?;

    if schema_version < RULES_SCHEMA_VERSION {
        return Err(LoadError::UnsupportedVersion {
            found: schema_version,
            expected: RULES_SCHEMA_VERSION,
        });
    }

    // `schema_version > known` is forward-compat: we still parse every
    // `[[rules]]` entry against the v1 shape and honor its semantics,
    // but unknown keys are preserved verbatim in `extra` instead of
    // becoming warnings — a v1 binary that quietly dropped rules from
    // a v2 project would hide real violations from the lint.
    let newer_schema = schema_version > RULES_SCHEMA_VERSION;

    let rules_value = root
        .remove("rules")
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let Value::Array(rules_array) = rules_value else {
        return Err(LoadError::InvalidField {
            field: "rules",
            message: "expected an array of tables".into(),
        });
    };

    let mut warnings = Vec::new();
    let mut rules = Vec::with_capacity(rules_array.len());
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    for entry in rules_array {
        let table = entry.try_into::<Table>().map_err(|e| decode_err(&e))?;
        let rule = parse_rule(table, &mut seen_ids, &mut warnings, newer_schema)?;
        rules.push(rule);
    }

    // Under the known schema, leftover top-level keys are typos →
    // warn but carry on. Under a newer schema, they're future shape →
    // keep them in `extra`, suppress warnings.
    if newer_schema {
        Ok(RulesDocument {
            schema_version,
            rules,
            warnings: Vec::new(),
            extra: root,
        })
    } else {
        for key in root.keys() {
            warnings.push(LoadWarning::UnknownTopLevelKey { key: key.clone() });
        }
        Ok(RulesDocument {
            schema_version,
            rules,
            warnings,
            extra: Table::new(),
        })
    }
}

fn extract_schema_version(root: &mut Table) -> Result<u32, LoadError> {
    let value = root
        .remove("schema_version")
        .ok_or(LoadError::MissingField("schema_version"))?;
    let int = value.as_integer().ok_or_else(|| LoadError::InvalidField {
        field: "schema_version",
        message: format!("expected integer, got {}", value.type_str()),
    })?;
    u32::try_from(int).map_err(|_| LoadError::InvalidField {
        field: "schema_version",
        message: format!("must fit in u32, got {int}"),
    })
}

fn parse_rule(
    mut table: Table,
    seen_ids: &mut BTreeSet<String>,
    warnings: &mut Vec<LoadWarning>,
    newer_schema: bool,
) -> Result<RawRule, LoadError> {
    let id = extract_rule_id(&mut table)?;
    if !seen_ids.insert(id.as_str().to_owned()) {
        return Err(LoadError::DuplicateId(id.as_str().to_owned()));
    }

    let kind = extract_kind(&mut table)?;
    let applies_to = extract_applies_to(&mut table, &id)?;
    let mode = extract_mode(&mut table)?;
    let description = extract_optional_string(&mut table, "description")?;
    let override_flag = extract_optional_bool(&mut table, "override")?.unwrap_or(false);

    let body = match kind {
        RuleKind::Template => RawRuleBody::Template(extract_template_body(&mut table, &id)?),
        RuleKind::Constraint => RawRuleBody::Constraint(extract_constraint_body(&mut table, &id)?),
    };

    // Whatever remains is unknown. Under the known schema these are
    // typos (warn so `casign` surfaces to the user); under a newer
    // schema they're future fields (keep them in `extra` without
    // warnings so the v1 binary still parses the rest correctly).
    if !newer_schema {
        for key in table.keys() {
            warnings.push(LoadWarning::UnknownRuleKey {
                rule_id: id.as_str().to_owned(),
                key: key.clone(),
            });
        }
    }
    let extra = if newer_schema { table } else { Table::new() };

    Ok(RawRule {
        id,
        kind,
        applies_to,
        mode,
        description,
        override_flag,
        body,
        extra,
    })
}

fn extract_rule_id(table: &mut Table) -> Result<RuleId, LoadError> {
    let value = table.remove("id").ok_or(LoadError::MissingField("id"))?;
    let raw = value.as_str().ok_or_else(|| LoadError::InvalidField {
        field: "id",
        message: format!("expected string, got {}", value.type_str()),
    })?;
    raw.parse::<RuleId>()
        .map_err(|source| LoadError::InvalidRuleId {
            raw: raw.to_owned(),
            source,
        })
}

fn extract_kind(table: &mut Table) -> Result<RuleKind, LoadError> {
    let value = table
        .remove("kind")
        .ok_or(LoadError::MissingField("kind"))?;
    value
        .try_into::<RuleKind>()
        .map_err(|e| LoadError::InvalidField {
            field: "kind",
            message: e.to_string(),
        })
}

fn extract_applies_to(table: &mut Table, id: &RuleId) -> Result<AppliesToRaw, LoadError> {
    let value = table
        .remove("applies_to")
        .ok_or(LoadError::MissingField("applies_to"))?;
    let raw: AppliesToRaw =
        value
            .try_into()
            .map_err(|e: toml::de::Error| LoadError::InvalidField {
                field: "applies_to",
                message: e.to_string(),
            })?;
    match &raw {
        AppliesToRaw::Single(s) if s.is_empty() => {
            return Err(LoadError::EmptyAppliesTo(id.as_str().to_owned()));
        }
        AppliesToRaw::Multi(v) if v.is_empty() || v.iter().any(String::is_empty) => {
            return Err(LoadError::EmptyAppliesTo(id.as_str().to_owned()));
        }
        _ => {}
    }
    Ok(raw)
}

fn extract_mode(table: &mut Table) -> Result<Mode, LoadError> {
    match table.remove("mode") {
        None => Ok(Mode::default()),
        Some(v) => v.try_into::<Mode>().map_err(|e| LoadError::InvalidField {
            field: "mode",
            message: e.to_string(),
        }),
    }
}

fn extract_optional_string(
    table: &mut Table,
    key: &'static str,
) -> Result<Option<String>, LoadError> {
    match table.remove(key) {
        None => Ok(None),
        Some(v) => v
            .try_into::<String>()
            .map(Some)
            .map_err(|e| LoadError::InvalidField {
                field: key,
                message: e.to_string(),
            }),
    }
}

fn extract_optional_bool(table: &mut Table, key: &'static str) -> Result<Option<bool>, LoadError> {
    match table.remove(key) {
        None => Ok(None),
        Some(v) => v
            .try_into::<bool>()
            .map(Some)
            .map_err(|e| LoadError::InvalidField {
                field: key,
                message: e.to_string(),
            }),
    }
}

fn extract_template_body(table: &mut Table, id: &RuleId) -> Result<RawTemplateBody, LoadError> {
    // Make sure no constraint-only field got set on a template rule.
    for forbidden in CONSTRAINT_ONLY_FIELDS {
        if table.contains_key(*forbidden) {
            return Err(LoadError::UnexpectedConstraintField {
                rule_id: id.as_str().to_owned(),
                field: forbidden,
            });
        }
    }

    let value = table
        .remove("template")
        .ok_or_else(|| LoadError::EmptyTemplate(id.as_str().to_owned()))?;
    let template = value
        .try_into::<String>()
        .map_err(|e| LoadError::InvalidField {
            field: "template",
            message: e.to_string(),
        })?;
    if template.is_empty() {
        return Err(LoadError::EmptyTemplate(id.as_str().to_owned()));
    }
    Ok(RawTemplateBody { template })
}

fn extract_constraint_body(table: &mut Table, id: &RuleId) -> Result<RawConstraintBody, LoadError> {
    if table.contains_key("template") {
        return Err(LoadError::UnexpectedTemplateField {
            rule_id: id.as_str().to_owned(),
        });
    }

    let charset = match table.remove("charset") {
        None => Charset::default(),
        Some(v) => v
            .try_into::<Charset>()
            .map_err(|e| LoadError::InvalidField {
                field: "charset",
                message: e.to_string(),
            })?,
    };
    let casing = match table.remove("casing") {
        None => Casing::default(),
        Some(v) => v
            .try_into::<Casing>()
            .map_err(|e| LoadError::InvalidField {
                field: "casing",
                message: e.to_string(),
            })?,
    };
    let forbidden_chars = extract_optional_string_vec(table, "forbidden_chars")?;
    let forbidden_patterns = extract_optional_string_vec(table, "forbidden_patterns")?;
    let reserved_words = extract_optional_string_vec(table, "reserved_words")?;
    let max_length = extract_optional_u32(table, "max_length")?.unwrap_or(DEFAULT_MAX_LENGTH);
    let min_length = extract_optional_u32(table, "min_length")?.unwrap_or(DEFAULT_MIN_LENGTH);
    let required_prefix = extract_optional_string(table, "required_prefix")?.unwrap_or_default();
    let required_suffix = extract_optional_string(table, "required_suffix")?.unwrap_or_default();

    Ok(RawConstraintBody {
        charset,
        casing,
        forbidden_chars,
        forbidden_patterns,
        reserved_words,
        max_length,
        min_length,
        required_prefix,
        required_suffix,
    })
}

fn extract_optional_string_vec(
    table: &mut Table,
    key: &'static str,
) -> Result<Vec<String>, LoadError> {
    match table.remove(key) {
        None => Ok(Vec::new()),
        Some(v) => v
            .try_into::<Vec<String>>()
            .map_err(|e| LoadError::InvalidField {
                field: key,
                message: e.to_string(),
            }),
    }
}

fn extract_optional_u32(table: &mut Table, key: &'static str) -> Result<Option<u32>, LoadError> {
    match table.remove(key) {
        None => Ok(None),
        Some(v) => {
            let int = v.as_integer().ok_or_else(|| LoadError::InvalidField {
                field: key,
                message: format!("expected integer, got {}", v.type_str()),
            })?;
            let n = u32::try_from(int).map_err(|_| LoadError::InvalidField {
                field: key,
                message: format!("must fit in u32, got {int}"),
            })?;
            Ok(Some(n))
        }
    }
}

/// Constraint-only fields, listed here so we can reject them when they
/// appear on a template rule. Keeping the list in one place makes it
/// easier to review against §5.2.
const CONSTRAINT_ONLY_FIELDS: &[&str] = &[
    "charset",
    "casing",
    "forbidden_chars",
    "forbidden_patterns",
    "reserved_words",
    "max_length",
    "min_length",
    "required_prefix",
    "required_suffix",
];

fn decode_err(e: &toml::de::Error) -> LoadError {
    LoadError::Decode(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(input: &str) -> RulesDocument {
        load_document(input).expect("expected successful load")
    }

    // --- Happy path --------------------------------------------------------

    #[test]
    fn parses_minimal_template_rule() {
        let doc = parse_ok(
            r#"
schema_version = 1

[[rules]]
id = "shot-assets-v1"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
"#,
        );
        assert_eq!(doc.schema_version, 1);
        assert_eq!(doc.rules.len(), 1);
        assert!(doc.warnings.is_empty());

        let r = &doc.rules[0];
        assert_eq!(r.id.as_str(), "shot-assets-v1");
        assert_eq!(r.kind, RuleKind::Template);
        assert_eq!(r.mode, Mode::Warn); // default
        assert!(!r.override_flag);
        assert_eq!(r.applies_to.patterns(), vec!["./assets/shots/**/*.psd"]);

        match &r.body {
            RawRuleBody::Template(t) => {
                assert_eq!(
                    t.template,
                    "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
                );
            }
            RawRuleBody::Constraint(_) => panic!("expected template body"),
        }
    }

    #[test]
    fn parses_constraint_rule_with_full_surface() {
        let doc = parse_ok(
            r#"
schema_version = 1

[[rules]]
id = "safe-names"
kind = "constraint"
applies_to = ["./assets/**", "./references/**"]
mode = "strict"
description = "Fail the lint on anything scary."
charset = "no_cjk"
casing = "snake"
forbidden_chars = [" ", "　"]
forbidden_patterns = ["^_", "tmp$"]
reserved_words = ["final", "copy"]
max_length = 64
min_length = 4
required_prefix = "ch"
required_suffix = ""
"#,
        );
        assert_eq!(doc.rules.len(), 1);

        let r = &doc.rules[0];
        assert_eq!(r.mode, Mode::Strict);
        assert_eq!(
            r.description.as_deref(),
            Some("Fail the lint on anything scary.")
        );
        assert_eq!(
            r.applies_to.patterns(),
            vec!["./assets/**", "./references/**"]
        );

        match &r.body {
            RawRuleBody::Constraint(c) => {
                assert_eq!(c.charset, Charset::NoCjk);
                assert_eq!(c.casing, Casing::Snake);
                assert_eq!(c.forbidden_chars, vec![" ".to_string(), "\u{3000}".into()]);
                assert_eq!(c.forbidden_patterns, vec!["^_".to_string(), "tmp$".into()]);
                assert_eq!(c.reserved_words, vec!["final".to_string(), "copy".into()]);
                assert_eq!(c.max_length, 64);
                assert_eq!(c.min_length, 4);
                assert_eq!(c.required_prefix, "ch");
                assert!(c.required_suffix.is_empty());
            }
            RawRuleBody::Template(_) => panic!("expected constraint body"),
        }
    }

    #[test]
    fn applies_constraint_defaults_when_fields_omitted() {
        let doc = parse_ok(
            r#"
schema_version = 1

[[rules]]
id = "loose-constraint"
kind = "constraint"
applies_to = "./**"
"#,
        );
        match &doc.rules[0].body {
            RawRuleBody::Constraint(c) => {
                assert_eq!(c.charset, Charset::Utf8);
                assert_eq!(c.casing, Casing::Any);
                assert!(c.forbidden_chars.is_empty());
                assert!(c.forbidden_patterns.is_empty());
                assert!(c.reserved_words.is_empty());
                assert_eq!(c.max_length, DEFAULT_MAX_LENGTH);
                assert_eq!(c.min_length, DEFAULT_MIN_LENGTH);
                assert!(c.required_prefix.is_empty());
                assert!(c.required_suffix.is_empty());
            }
            RawRuleBody::Template(_) => panic!("expected constraint body"),
        }
    }

    // --- Warnings ----------------------------------------------------------

    #[test]
    fn emits_warning_for_unknown_top_level_key_under_known_schema() {
        let doc = parse_ok(
            r"
schema_version = 1
future_setting = true
",
        );
        assert!(matches!(
            &doc.warnings[..],
            [LoadWarning::UnknownTopLevelKey { key }] if key == "future_setting"
        ));
    }

    #[test]
    fn emits_warning_for_unknown_rule_key() {
        let doc = parse_ok(
            r#"
schema_version = 1

[[rules]]
id = "safe-names"
kind = "constraint"
applies_to = "./**"
casign = "snake"  # typo: s/casign/casing/
"#,
        );
        assert!(
            doc.warnings.iter().any(|w| matches!(
                w,
                LoadWarning::UnknownRuleKey { rule_id, key } if rule_id == "safe-names" && key == "casign"
            )),
            "expected UnknownRuleKey warning, got {:?}",
            doc.warnings
        );
    }

    // --- schema_version gate ----------------------------------------------

    #[test]
    fn forward_compat_parses_known_rules_and_preserves_unknowns() {
        // A v1 binary reading v2 content must still apply the rules it
        // understands — silently dropping them would hide real lint
        // violations. Unknown keys get stashed in `extra` (per-rule and
        // top-level) without warnings.
        let doc = parse_ok(
            r#"
schema_version = 2

[[rules]]
id = "future-rule"
kind = "template"
applies_to = "./**"
template = "{prefix}_{seq:03d}.{ext}"
future_rule_knob = "soon"

[new_top_level]
hello = "world"
"#,
        );
        assert_eq!(doc.schema_version, 2);
        assert_eq!(doc.rules.len(), 1);
        assert_eq!(doc.rules[0].id.as_str(), "future-rule");
        assert!(doc.rules[0].extra.contains_key("future_rule_knob"));
        assert!(doc.warnings.is_empty());
        assert!(doc.extra.contains_key("new_top_level"));
        assert!(!doc.extra.contains_key("rules")); // consumed, not left behind
    }

    #[test]
    fn rejects_schema_version_zero() {
        let err = load_document(
            r"
schema_version = 0
",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LoadError::UnsupportedVersion {
                found: 0,
                expected: RULES_SCHEMA_VERSION
            }
        ));
    }

    #[test]
    fn rejects_missing_schema_version() {
        let err = load_document("").unwrap_err();
        assert!(matches!(err, LoadError::MissingField("schema_version")));
    }

    // --- Duplicate id / cross-kind misuse ---------------------------------

    #[test]
    fn rejects_duplicate_rule_ids_in_same_file() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "dup"
kind = "constraint"
applies_to = "./a/**"

[[rules]]
id = "dup"
kind = "constraint"
applies_to = "./b/**"
"#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::DuplicateId(ref s) if s == "dup"));
    }

    #[test]
    fn rejects_template_field_on_constraint() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "mixed"
kind = "constraint"
applies_to = "./**"
template = "{prefix}.{ext}"
"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LoadError::UnexpectedTemplateField { ref rule_id } if rule_id == "mixed"
        ));
    }

    #[test]
    fn rejects_constraint_field_on_template() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "mixed"
kind = "template"
applies_to = "./**"
template = "{prefix}.{ext}"
charset = "ascii"
"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LoadError::UnexpectedConstraintField { ref rule_id, field: "charset" } if rule_id == "mixed"
        ));
    }

    // --- Empty applies_to / template --------------------------------------

    #[test]
    fn rejects_empty_applies_to_string() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "empty"
kind = "constraint"
applies_to = ""
"#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::EmptyAppliesTo(_)));
    }

    #[test]
    fn rejects_empty_applies_to_array() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "empty"
kind = "constraint"
applies_to = []
"#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::EmptyAppliesTo(_)));
    }

    #[test]
    fn rejects_empty_template_body() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "empty"
kind = "template"
applies_to = "./**"
template = ""
"#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::EmptyTemplate(_)));
    }

    // --- Bad rule id ------------------------------------------------------

    #[test]
    fn rejects_invalid_rule_id() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "Bad Id"
kind = "constraint"
applies_to = "./**"
"#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::InvalidRuleId { ref raw, .. } if raw == "Bad Id"));
    }

    // --- Mode parsing -----------------------------------------------------

    #[test]
    fn rejects_unknown_mode() {
        let err = load_document(
            r#"
schema_version = 1

[[rules]]
id = "weird"
kind = "constraint"
applies_to = "./**"
mode = "maybe"
"#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::InvalidField { field: "mode", .. }));
    }
}
