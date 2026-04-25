//! Search DSL validator (`docs/SEARCH_DSL.md` §4 / §6 / §9).
//!
//! Walks the parser AST, classifies clauses (reserved key vs custom
//! field), interprets ranges where the key supports them, and folds
//! type errors into [`Warning`]s + a short-circuiting `AlwaysFalse`
//! marker (so the planner emits `0 = 1` for those nodes — keeping
//! the rest of the boolean tree intact).

#![allow(clippy::single_match_else)]

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};

use super::ast::{Atom, Clause, Expr, Query, Value};

// ----- Schema (custom fields) ----------------------------------------------

/// Custom-field type as declared in `schema.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CustomFieldKind {
    String,
    Int,
    Enum { values: Vec<String> },
}

/// Map of custom-field name → declared kind. Loaded from
/// `schema.toml` `[custom_fields.<name>]` (M3 #4 wires the loader).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomFields {
    inner: BTreeMap<String, CustomFieldKind>,
}

impl CustomFields {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, name: impl Into<String>, kind: CustomFieldKind) {
        self.inner.insert(name.into(), kind);
    }
    pub fn get(&self, name: &str) -> Option<&CustomFieldKind> {
        self.inner.get(name)
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// ----- Validated AST -------------------------------------------------------

/// Output of the validator: structurally identical to the parser AST
/// but with semantic clauses, plus a flat list of warnings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedQuery {
    pub expr: ValidExpr,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidExpr {
    Or(Vec<ValidExpr>),
    And(Vec<ValidExpr>),
    Not(Box<ValidExpr>),
    Atom(ValidAtom),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidAtom {
    Reserved(ReservedClause),
    Custom(CustomClause),
    /// Free-text token that scans `name` and `notes` via FTS5.
    FreeText(FreeTextTerm),
    /// Validator decided the clause cannot match anything (unknown
    /// key, type mismatch, malformed value). Planner lowers this
    /// into `0 = 1`. The associated [`Warning`] is recorded in the
    /// query's warnings list.
    AlwaysFalse(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FreeTextTerm {
    /// Single bareword (trigram fuzzy match against name+notes).
    Bareword(String),
    /// Quoted phrase (literal multi-trigram conjunction).
    Phrase(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "key", rename_all = "snake_case")]
pub enum ReservedClause {
    Tag(String),
    Type(String),
    Kind(KindValue),
    Is(IsValue),
    Name(String),
    Path(String),
    Created(InstantRange),
    Updated(InstantRange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindValue {
    Asset,
    Directory,
    Derived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsValue {
    Violation,
    Orphan,
    Duplicate,
    Misplaced,
}

/// Inclusive datetime range. Both bounds in normalized UTC ISO 8601
/// (`YYYY-MM-DDTHH:MM:SS.fffZ`). Either bound may be omitted for
/// half-open ranges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantRange {
    pub lo: Option<String>,
    pub hi: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomClause {
    pub key: String,
    pub matcher: CustomMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CustomMatcher {
    String(String),
    Int(i64),
    IntRange { lo: Option<i64>, hi: Option<i64> },
    Enum(String),
}

// ----- Warnings ------------------------------------------------------------

/// Validator-emitted warning. All warnings are non-fatal (parser-OK
/// queries still execute), but `AlwaysFalse` clauses suppress
/// matches deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Warning {
    /// `key:` is neither reserved nor declared in `schema.toml`.
    UnknownKey { key: String },
    /// Custom field exists but value cannot be parsed for its kind.
    TypeMismatch { key: String, message: String },
    /// Multiple `type:` clauses joined by implicit AND — never
    /// matches because a file has exactly one extension.
    TypeAndMulti,
    /// Reserved-key value malformed (e.g. unknown `kind:` value).
    ReservedValueInvalid { key: String, value: String },
    /// Range syntax used on a key that doesn't accept ranges.
    RangeOnNonRangeKey { key: String, value: String },
    /// Datetime parse failed for `created:` / `updated:`.
    InvalidDatetime { key: String, value: String },
    /// Glob compilation failed for `name:` / `path:`.
    InvalidGlob { key: String, value: String },
    /// `created:` / `updated:` used twice in one query.
    DuplicateInstantClause { key: String },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey { key } => write!(f, "unknown_key:{key}"),
            Self::TypeMismatch { key, message } => {
                write!(f, "type_mismatch:{key} ({message})")
            }
            Self::TypeAndMulti => f.write_str("type-and-multi"),
            Self::ReservedValueInvalid { key, value } => {
                write!(f, "reserved_value_invalid:{key}={value}")
            }
            Self::RangeOnNonRangeKey { key, value } => {
                write!(f, "range_on_non_range_key:{key}={value}")
            }
            Self::InvalidDatetime { key, value } => {
                write!(f, "invalid_datetime:{key}={value}")
            }
            Self::InvalidGlob { key, value } => write!(f, "invalid_glob:{key}={value}"),
            Self::DuplicateInstantClause { key } => write!(f, "duplicate_instant_clause:{key}"),
        }
    }
}

// ----- Validate entry point ------------------------------------------------

/// Validate a parsed query against the schema.
///
/// The result always succeeds (validation emits warnings, never
/// fails). Warnings are deduplicated in insertion order.
pub fn validate(query: &Query, schema: &CustomFields) -> ValidatedQuery {
    let mut ctx = Ctx {
        warnings: Vec::new(),
        type_clauses_in_and: 0,
        instant_keys_seen: BTreeMap::new(),
    };
    let expr = ctx.validate_expr(&query.root, /* in_and: */ false, schema);
    ValidatedQuery {
        expr,
        warnings: ctx.warnings,
    }
}

#[derive(Default)]
struct Ctx {
    warnings: Vec<Warning>,
    /// Number of `type:` clauses seen in the *current* AND group;
    /// resets when entering a new boolean group.
    type_clauses_in_and: u32,
    /// Tracks `created:` / `updated:` clauses to flag duplicates.
    instant_keys_seen: BTreeMap<String, u32>,
}

impl Ctx {
    fn warn(&mut self, w: Warning) {
        if !self.warnings.contains(&w) {
            self.warnings.push(w);
        }
    }

    fn validate_expr(&mut self, expr: &Expr, in_and: bool, schema: &CustomFields) -> ValidExpr {
        // `instant_keys_seen` is global across the whole query (spec
        // §4.7: "1 query 1 created clause"). `type_clauses_in_and`
        // is per-AND group and resets on OR/AND boundaries.
        match expr {
            Expr::Or(branches) => {
                let saved_type = self.type_clauses_in_and;
                let mut out = Vec::with_capacity(branches.len());
                for b in branches {
                    self.type_clauses_in_and = 0;
                    out.push(self.validate_expr(b, false, schema));
                }
                self.type_clauses_in_and = saved_type;
                ValidExpr::Or(out)
            }
            Expr::And(branches) => {
                let saved_type = self.type_clauses_in_and;
                self.type_clauses_in_and = 0;
                let mut out = Vec::with_capacity(branches.len());
                for b in branches {
                    out.push(self.validate_expr(b, true, schema));
                }
                if self.type_clauses_in_and >= 2 {
                    self.warn(Warning::TypeAndMulti);
                }
                self.type_clauses_in_and = saved_type;
                ValidExpr::And(out)
            }
            Expr::Not(inner) => ValidExpr::Not(Box::new(self.validate_expr(inner, in_and, schema))),
            Expr::Atom(atom) => ValidExpr::Atom(self.validate_atom(atom, in_and, schema)),
        }
    }

    fn validate_atom(&mut self, atom: &Atom, in_and: bool, schema: &CustomFields) -> ValidAtom {
        match atom {
            Atom::FreeBareword(s) => ValidAtom::FreeText(FreeTextTerm::Bareword(s.clone())),
            Atom::FreePhrase(s) => ValidAtom::FreeText(FreeTextTerm::Phrase(s.clone())),
            Atom::Clause(c) => self.validate_clause(c, in_and, schema),
        }
    }

    fn validate_clause(
        &mut self,
        clause: &Clause,
        in_and: bool,
        schema: &CustomFields,
    ) -> ValidAtom {
        let key = clause.key.as_str();
        if let Some(reserved) = self.validate_reserved(key, &clause.value, in_and) {
            return reserved;
        }
        // Not a reserved key — try custom field.
        match schema.get(key) {
            Some(kind) => self.validate_custom(key, &clause.value, kind),
            None => {
                self.warn(Warning::UnknownKey { key: key.into() });
                ValidAtom::AlwaysFalse(format!("unknown_key:{key}"))
            }
        }
    }

    fn validate_reserved(&mut self, key: &str, value: &Value, in_and: bool) -> Option<ValidAtom> {
        let raw = value.as_str();
        let reserved = match key {
            "tag" => ReservedClause::Tag(raw.to_string()),
            "type" => {
                if in_and {
                    self.type_clauses_in_and = self.type_clauses_in_and.saturating_add(1);
                }
                ReservedClause::Type(normalize_type(raw))
            }
            "kind" => match parse_kind(raw) {
                Some(kv) => ReservedClause::Kind(kv),
                None => {
                    self.warn(Warning::ReservedValueInvalid {
                        key: "kind".into(),
                        value: raw.into(),
                    });
                    return Some(ValidAtom::AlwaysFalse(format!(
                        "reserved_value_invalid:kind={raw}"
                    )));
                }
            },
            "is" => match parse_is(raw) {
                Some(iv) => ReservedClause::Is(iv),
                None => {
                    self.warn(Warning::ReservedValueInvalid {
                        key: "is".into(),
                        value: raw.into(),
                    });
                    return Some(ValidAtom::AlwaysFalse(format!(
                        "reserved_value_invalid:is={raw}"
                    )));
                }
            },
            "name" => match validate_glob(raw) {
                Ok(_) => ReservedClause::Name(raw.to_string()),
                Err(_) => {
                    self.warn(Warning::InvalidGlob {
                        key: "name".into(),
                        value: raw.into(),
                    });
                    return Some(ValidAtom::AlwaysFalse(format!("invalid_glob:name={raw}")));
                }
            },
            "path" => match validate_glob(raw) {
                Ok(_) => ReservedClause::Path(raw.to_string()),
                Err(_) => {
                    self.warn(Warning::InvalidGlob {
                        key: "path".into(),
                        value: raw.into(),
                    });
                    return Some(ValidAtom::AlwaysFalse(format!("invalid_glob:path={raw}")));
                }
            },
            "created" | "updated" => {
                let count = self.instant_keys_seen.entry(key.to_string()).or_insert(0);
                *count += 1;
                if *count >= 2 {
                    self.warn(Warning::DuplicateInstantClause {
                        key: key.to_string(),
                    });
                    return Some(ValidAtom::AlwaysFalse(format!(
                        "duplicate_instant_clause:{key}"
                    )));
                }
                match parse_instant_range(raw, value.is_quoted()) {
                    Ok(range) => {
                        if key == "created" {
                            ReservedClause::Created(range)
                        } else {
                            ReservedClause::Updated(range)
                        }
                    }
                    Err(_) => {
                        self.warn(Warning::InvalidDatetime {
                            key: key.into(),
                            value: raw.into(),
                        });
                        return Some(ValidAtom::AlwaysFalse(format!(
                            "invalid_datetime:{key}={raw}"
                        )));
                    }
                }
            }
            _ => return None, // not reserved
        };

        // For non-range reserved keys, error out if the user supplied
        // a `..` syntax in an unquoted value.
        if !value.is_quoted()
            && matches!(key, "tag" | "type" | "kind" | "is" | "name" | "path")
            && raw.contains("..")
            && !key_supports_range(key)
        {
            self.warn(Warning::RangeOnNonRangeKey {
                key: key.into(),
                value: raw.into(),
            });
            // Still emit the clause; range-on-non-range is a hint
            // not a fail, and `name:./foo/../bar` is a legit literal
            // glob the user might have meant.
        }

        Some(ValidAtom::Reserved(reserved))
    }

    fn validate_custom(&mut self, key: &str, value: &Value, kind: &CustomFieldKind) -> ValidAtom {
        let raw = value.as_str();
        match kind {
            CustomFieldKind::String => ValidAtom::Custom(CustomClause {
                key: key.into(),
                matcher: CustomMatcher::String(raw.into()),
            }),
            CustomFieldKind::Int => match parse_int_or_range(raw, value.is_quoted()) {
                Ok(IntValue::Single(n)) => ValidAtom::Custom(CustomClause {
                    key: key.into(),
                    matcher: CustomMatcher::Int(n),
                }),
                Ok(IntValue::Range { lo, hi }) => ValidAtom::Custom(CustomClause {
                    key: key.into(),
                    matcher: CustomMatcher::IntRange { lo, hi },
                }),
                Err(msg) => {
                    self.warn(Warning::TypeMismatch {
                        key: key.into(),
                        message: msg.into(),
                    });
                    ValidAtom::AlwaysFalse(format!("type_mismatch:{key}"))
                }
            },
            CustomFieldKind::Enum { values } => {
                if values.iter().any(|v| v == raw) {
                    ValidAtom::Custom(CustomClause {
                        key: key.into(),
                        matcher: CustomMatcher::Enum(raw.into()),
                    })
                } else {
                    self.warn(Warning::TypeMismatch {
                        key: key.into(),
                        message: format!("'{raw}' is not in enum {values:?}"),
                    });
                    ValidAtom::AlwaysFalse(format!("type_mismatch:{key}"))
                }
            }
        }
    }
}

fn key_supports_range(key: &str) -> bool {
    matches!(key, "created" | "updated")
}

fn normalize_type(raw: &str) -> String {
    raw.trim_start_matches('.').to_ascii_lowercase()
}

fn parse_kind(raw: &str) -> Option<KindValue> {
    match raw {
        "asset" => Some(KindValue::Asset),
        "directory" => Some(KindValue::Directory),
        "derived" => Some(KindValue::Derived),
        _ => None,
    }
}

fn parse_is(raw: &str) -> Option<IsValue> {
    match raw {
        "violation" => Some(IsValue::Violation),
        "orphan" => Some(IsValue::Orphan),
        "duplicate" => Some(IsValue::Duplicate),
        "misplaced" => Some(IsValue::Misplaced),
        _ => None,
    }
}

fn validate_glob(pattern: &str) -> Result<GlobMatcher, globset::Error> {
    Ok(Glob::new(pattern)?.compile_matcher())
}

enum IntValue {
    Single(i64),
    Range { lo: Option<i64>, hi: Option<i64> },
}

fn parse_int_or_range(raw: &str, quoted: bool) -> Result<IntValue, &'static str> {
    if !quoted && let Some((l, r)) = raw.split_once("..") {
        if l.contains("..") || r.contains("..") {
            return Err("malformed range");
        }
        let lo = if l.is_empty() {
            None
        } else {
            Some(l.parse::<i64>().map_err(|_| "lo not an integer")?)
        };
        let hi = if r.is_empty() {
            None
        } else {
            Some(r.parse::<i64>().map_err(|_| "hi not an integer")?)
        };
        if lo.is_none() && hi.is_none() {
            return Err("range needs at least one bound");
        }
        return Ok(IntValue::Range { lo, hi });
    }
    raw.parse::<i64>()
        .map(IntValue::Single)
        .map_err(|_| "not an integer")
}

fn parse_instant_range(raw: &str, quoted: bool) -> Result<InstantRange, &'static str> {
    if !quoted && let Some((l, r)) = raw.split_once("..") {
        if l.contains("..") || r.contains("..") {
            return Err("malformed range");
        }
        let lo = if l.is_empty() {
            None
        } else {
            Some(parse_instant_lower(l)?)
        };
        let hi = if r.is_empty() {
            None
        } else {
            Some(parse_instant_upper(r)?)
        };
        if lo.is_none() && hi.is_none() {
            return Err("range needs at least one bound");
        }
        return Ok(InstantRange { lo, hi });
    }
    // Single point treated as range covering the day if date-only,
    // otherwise the exact instant for both bounds.
    if raw.len() == 10 {
        Ok(InstantRange {
            lo: Some(parse_instant_lower(raw)?),
            hi: Some(parse_instant_upper(raw)?),
        })
    } else {
        let pt = parse_instant_lower(raw)?;
        Ok(InstantRange {
            lo: Some(pt.clone()),
            hi: Some(pt),
        })
    }
}

/// Lower bound of a date or datetime token. `2026-04-25` → start of
/// day in UTC. Any time component is taken as-is (TZ-converted).
fn parse_instant_lower(raw: &str) -> Result<String, &'static str> {
    if let Ok(dt) = parse_datetime(raw) {
        return Ok(format_iso(dt));
    }
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let dt = Utc
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .single()
            .ok_or("ambiguous date")?;
        return Ok(format_iso(dt));
    }
    Err("not a date or datetime")
}

/// Upper bound: `2026-04-25` → 23:59:59.999 of that day in UTC.
fn parse_instant_upper(raw: &str) -> Result<String, &'static str> {
    if let Ok(dt) = parse_datetime(raw) {
        return Ok(format_iso(dt));
    }
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let dt = Utc
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 23, 59, 59)
            .single()
            .ok_or("ambiguous date")?;
        // .999 suffix to strictly cover the day boundary.
        let mut s = format_iso(dt);
        s.replace_range(s.len() - 1.., ".999Z");
        return Ok(s);
    }
    Err("not a date or datetime")
}

const NAIVE_DATETIME_FORMATS: &[&str] = &["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S"];
const OFFSET_DATETIME_FORMATS: &[&str] = &["%Y-%m-%dT%H:%M:%S%.f%:z", "%Y-%m-%dT%H:%M:%S%:z"];

/// Parse `YYYY-MM-DDThh:mm:ss(.fff)?(Z|±hh:mm)`. Always returns UTC.
fn parse_datetime(raw: &str) -> Result<DateTime<Utc>, &'static str> {
    // Z suffix: parse the prefix as NaiveDateTime then mark UTC.
    if let Some(stripped) = raw.strip_suffix('Z') {
        for fmt in NAIVE_DATETIME_FORMATS {
            if let Ok(naive) = NaiveDateTime::parse_from_str(stripped, fmt) {
                return Ok(Utc.from_utc_datetime(&naive));
            }
        }
    }
    // Offset suffix (`+09:00` etc): chrono parses these directly as
    // `DateTime<FixedOffset>`.
    for fmt in OFFSET_DATETIME_FORMATS {
        if let Ok(dt) = DateTime::<FixedOffset>::parse_from_str(raw, fmt) {
            return Ok(dt.with_timezone(&Utc));
        }
    }
    Err("not a datetime")
}

fn format_iso(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::parse::parse;

    fn schema_empty() -> CustomFields {
        CustomFields::new()
    }

    fn schema_with_scene_int() -> CustomFields {
        let mut s = CustomFields::new();
        s.insert("scene", CustomFieldKind::Int);
        s.insert("shot", CustomFieldKind::Int);
        s.insert(
            "status",
            CustomFieldKind::Enum {
                values: vec!["wip".into(), "review".into(), "approved".into()],
            },
        );
        s
    }

    fn validated(q: &str, schema: &CustomFields) -> ValidatedQuery {
        let parsed = parse(q).unwrap();
        validate(&parsed, schema)
    }

    #[test]
    fn reserved_tag() {
        let v = validated("tag:wip", &schema_empty());
        assert!(v.warnings.is_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::Reserved(ReservedClause::Tag(t))) => {
                assert_eq!(t, "wip");
            }
            other => panic!("expected Tag, got {other:?}"),
        }
    }

    #[test]
    fn unknown_key_warns_and_short_circuits() {
        let v = validated("foo:bar", &schema_empty());
        assert_eq!(v.warnings.len(), 1);
        assert!(matches!(&v.warnings[0], Warning::UnknownKey { key } if key == "foo"));
        match v.expr {
            ValidExpr::Atom(ValidAtom::AlwaysFalse(_)) => {}
            other => panic!("expected AlwaysFalse, got {other:?}"),
        }
    }

    #[test]
    fn custom_int_field() {
        let v = validated("scene:10", &schema_with_scene_int());
        assert!(v.warnings.is_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::Custom(CustomClause { matcher, .. })) => {
                assert_eq!(matcher, CustomMatcher::Int(10));
            }
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[test]
    fn custom_int_range() {
        let v = validated("shot:1..50", &schema_with_scene_int());
        assert!(v.warnings.is_empty(), "{:?}", v.warnings);
        match v.expr {
            ValidExpr::Atom(ValidAtom::Custom(CustomClause { matcher, .. })) => {
                assert_eq!(
                    matcher,
                    CustomMatcher::IntRange {
                        lo: Some(1),
                        hi: Some(50)
                    }
                );
            }
            other => panic!("expected Custom range, got {other:?}"),
        }
    }

    #[test]
    fn custom_enum_unknown_value() {
        let v = validated("status:flying", &schema_with_scene_int());
        assert!(matches!(v.warnings[0], Warning::TypeMismatch { .. }));
        assert!(matches!(v.expr, ValidExpr::Atom(ValidAtom::AlwaysFalse(_))));
    }

    #[test]
    fn type_and_multi_warns() {
        let v = validated("type:psd type:tif", &schema_empty());
        assert!(
            v.warnings
                .iter()
                .any(|w| matches!(w, Warning::TypeAndMulti)),
            "warnings: {:?}",
            v.warnings
        );
    }

    #[test]
    fn type_in_or_does_not_warn() {
        let v = validated("type:psd OR type:tif", &schema_empty());
        assert!(
            !v.warnings
                .iter()
                .any(|w| matches!(w, Warning::TypeAndMulti))
        );
    }

    #[test]
    fn kind_invalid_warns() {
        let v = validated("kind:purple", &schema_empty());
        assert!(matches!(
            &v.warnings[0],
            Warning::ReservedValueInvalid { key, .. } if key == "kind"
        ));
        assert!(matches!(v.expr, ValidExpr::Atom(ValidAtom::AlwaysFalse(_))));
    }

    #[test]
    fn is_violation_ok() {
        let v = validated("is:violation", &schema_empty());
        assert!(v.warnings.is_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::Reserved(ReservedClause::Is(IsValue::Violation))) => {}
            other => panic!("expected Is(Violation), got {other:?}"),
        }
    }

    #[test]
    fn date_range_inclusive_day() {
        let v = validated("created:2026-01-01..2026-04-30", &schema_empty());
        assert!(v.warnings.is_empty(), "{:?}", v.warnings);
        match v.expr {
            ValidExpr::Atom(ValidAtom::Reserved(ReservedClause::Created(r))) => {
                assert_eq!(r.lo.as_deref(), Some("2026-01-01T00:00:00Z"));
                assert_eq!(r.hi.as_deref(), Some("2026-04-30T23:59:59.999Z"));
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn date_range_half_open_lo() {
        let v = validated("updated:2026-01-01..", &schema_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::Reserved(ReservedClause::Updated(r))) => {
                assert_eq!(r.lo.as_deref(), Some("2026-01-01T00:00:00Z"));
                assert!(r.hi.is_none());
            }
            other => panic!("expected Updated, got {other:?}"),
        }
    }

    #[test]
    fn datetime_with_offset() {
        let v = validated("created:2026-04-25T09:00:00+09:00", &schema_empty());
        assert!(v.warnings.is_empty(), "{:?}", v.warnings);
        match v.expr {
            ValidExpr::Atom(ValidAtom::Reserved(ReservedClause::Created(r))) => {
                assert_eq!(r.lo.as_deref(), Some("2026-04-25T00:00:00Z"));
                assert_eq!(r.hi.as_deref(), Some("2026-04-25T00:00:00Z"));
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn invalid_date_warns() {
        let v = validated("created:not-a-date", &schema_empty());
        assert!(matches!(
            &v.warnings[0],
            Warning::InvalidDatetime { key, .. } if key == "created"
        ));
        assert!(matches!(v.expr, ValidExpr::Atom(ValidAtom::AlwaysFalse(_))));
    }

    #[test]
    fn duplicate_created_warns() {
        let v = validated("created:2026-01-01 created:2026-02-01", &schema_empty());
        assert!(
            v.warnings
                .iter()
                .any(|w| matches!(w, Warning::DuplicateInstantClause { key } if key == "created"))
        );
    }

    #[test]
    fn name_glob_ok() {
        let v = validated("name:*.psd", &schema_empty());
        assert!(v.warnings.is_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::Reserved(ReservedClause::Name(g))) => {
                assert_eq!(g, "*.psd");
            }
            other => panic!("expected Name, got {other:?}"),
        }
    }

    #[test]
    fn name_glob_invalid_warns() {
        let v = validated("name:[unclosed", &schema_empty());
        assert!(matches!(v.warnings[0], Warning::InvalidGlob { .. }));
    }

    #[test]
    fn freetext_passes_through() {
        let v = validated("forest", &schema_empty());
        assert!(v.warnings.is_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::FreeText(FreeTextTerm::Bareword(s))) => {
                assert_eq!(s, "forest");
            }
            other => panic!("expected FreeText, got {other:?}"),
        }
    }

    #[test]
    fn quoted_freetext_classified_as_phrase() {
        let v = validated(r#""forest night""#, &schema_empty());
        match v.expr {
            ValidExpr::Atom(ValidAtom::FreeText(FreeTextTerm::Phrase(s))) => {
                assert_eq!(s, "forest night");
            }
            other => panic!("expected Phrase, got {other:?}"),
        }
    }
}
