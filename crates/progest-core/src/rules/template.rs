//! Template parser and basename matcher (§4).
//!
//! Lifecycle:
//!
//! 1. [`compile`] takes the raw template string from `rules.toml` and
//!    produces a [`CompiledTemplate`] — validated atom list, regex
//!    fragments for the static placeholders, and metadata such as the
//!    open-ended-slot count. Heavy work (regex compilation) happens
//!    once here.
//! 2. [`match_basename`] takes a [`CompiledTemplate`], the file's
//!    basename, and (optionally) its `.meta` document, and returns a
//!    [`TemplateMatch`]: either a successful capture map or a
//!    human-readable failure reason. Dynamic placeholders
//!    (`{field:}` / `{date:}`) resolve against `.meta` here and are
//!    compared as literals per spec §4.6.
//!
//! Section references below target `docs/NAMING_RULES_DSL.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use regex::Regex;
use thiserror::Error;
use toml::Value;

use super::constraint::split_basename;
use crate::meta::MetaDocument;

// --- Public types ----------------------------------------------------------

/// A fully compiled template ready for match / suggest work.
#[derive(Debug, Clone)]
pub struct CompiledTemplate {
    raw: String,
    atoms: Vec<Atom>,
    /// Number of open-ended slots (§4.7). 0 or 1 by construction.
    open_ended_count: u8,
}

impl CompiledTemplate {
    /// The original template string as written in `rules.toml`.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }

    #[must_use]
    pub fn open_ended_count(&self) -> u8 {
        self.open_ended_count
    }
}

/// A single atom of the compiled template.
#[derive(Debug, Clone)]
pub enum Atom {
    /// Literal text that must appear verbatim in the basename. Already
    /// un-escaped (i.e. `{{` has become `{`).
    Literal(String),
    /// A placeholder whose value is captured from the basename at
    /// match time (spec §4.6 "static" placeholders).
    Static(StaticAtom),
    /// A placeholder whose value comes from `.meta` and is compared
    /// as a literal (spec §4.6 "dynamic" placeholders).
    Dynamic(DynamicAtom),
}

/// Static placeholder: value comes from the basename, not `.meta`.
#[derive(Debug, Clone)]
pub struct StaticAtom {
    pub kind: StaticKind,
    /// Capture-group name in the compiled regex. Unique within a template.
    pub capture: String,
    /// Regex fragment that matches the legal value for this placeholder.
    /// Never anchored — the surrounding template owns anchoring.
    pub regex_fragment: String,
    /// Optional namespace key (`{seq@scene}` → `Some("scene")`).
    pub namespace: Option<String>,
    /// Effective casing/length spec chain, retained for suggest-time
    /// text generation. Evaluation only needs [`Self::regex_fragment`];
    /// this gives [`crate::rules::eval`] something to drive
    /// suggested-name synthesis without reparsing the template.
    pub specs: Vec<FormatSpec>,
    /// Whether this atom consumed the template's single open-ended slot.
    pub is_open_ended: bool,
}

/// Dynamic placeholder: value derived from `.meta`, compared as a literal.
#[derive(Debug, Clone)]
pub struct DynamicAtom {
    pub source: DynamicSource,
    /// Format-spec chain applied to the source value (empty for `date`).
    pub specs: Vec<FormatSpec>,
}

/// Which static placeholder kind this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticKind {
    Prefix,
    Desc,
    Seq,
    Version,
    Ext,
}

impl StaticKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Desc => "desc",
            Self::Seq => "seq",
            Self::Version => "version",
            Self::Ext => "ext",
        }
    }
}

/// Where a dynamic placeholder gets its value from.
#[derive(Debug, Clone)]
pub enum DynamicSource {
    /// `{field:<name>}` → `MetaDocument.custom.<name>`.
    CustomField(String),
    /// `{date:<fmt>}` → `MetaDocument.created_at`.
    Date(DateFormat),
}

/// Format specifier applied in order left-to-right (§4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSpec {
    // Numeric
    ZeroPadded(u8),
    PlainInteger,
    // String
    Snake,
    Kebab,
    Camel,
    Pascal,
    Lower,
    Upper,
    Slug,
}

impl FormatSpec {
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::ZeroPadded(_) | Self::PlainInteger)
    }

    #[must_use]
    pub fn is_string(self) -> bool {
        !self.is_numeric()
    }
}

/// A tokenized date format, e.g. `YYYY-MM-DD` → [Year4, Lit("-"), Month, Lit("-"), Day].
#[derive(Debug, Clone)]
pub struct DateFormat(pub Vec<DateToken>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateToken {
    Literal(String),
    Year4,
    Year2,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

// --- Errors ----------------------------------------------------------------

/// Errors surfaced while compiling a template string.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TemplateError {
    #[error("unexpected end of template inside placeholder `{{...`")]
    UnclosedBrace,
    #[error("unmatched `}}` at byte offset {0}")]
    UnmatchedCloseBrace(usize),
    #[error("nested `{{` inside a placeholder is not allowed")]
    NestedBrace,
    #[error("unknown placeholder `{{{0}}}`")]
    UnknownPlaceholder(String),
    #[error("placeholder `{{{name}}}` does not accept namespace `@{namespace}`")]
    NamespaceOnNonSeq { name: String, namespace: String },
    #[error("placeholder `{{{name}}}` does not accept format specifiers")]
    UnexpectedSpecs { name: String },
    #[error("placeholder `{{{name}}}` requires a format specifier")]
    MissingSpec { name: String },
    #[error("unknown format specifier `{spec}` on `{{{name}}}`")]
    UnknownSpec { name: String, spec: String },
    #[error("numeric specifier `{spec}` is invalid on string placeholder `{{{name}}}`")]
    NumericSpecOnString { name: String, spec: String },
    #[error("string specifier `{spec}` is invalid on numeric placeholder `{{{name}}}`")]
    StringSpecOnNumeric { name: String, spec: String },
    #[error("`{{{name}}}` mixes numeric and string format specifiers; pick one kind (spec §4.4)")]
    MixedDynamicSpecs { name: String },
    #[error("duplicate format specifier `{spec}` on `{{{name}}}`")]
    DuplicateSpec { name: String, spec: String },
    #[error("placeholder `{{{name}}}` requires a field name after `field:`")]
    MissingFieldName { name: String },
    #[error("`{{field:{0}}}` is not a valid custom-field name")]
    InvalidFieldName(String),
    #[error(
        "template uses more than one open-ended placeholder slot (max 1); second was `{{{0}}}`"
    )]
    MultipleOpenEnded(String),
    #[error("template may have at most one `{{ext}}`; saw a second occurrence")]
    DuplicateExt,
    #[error("`{{date:...}}` requires a format body such as `YYYYMMDD`")]
    DateWithoutFormat,
    #[error("unknown date-format token in `{0}`")]
    UnknownDateToken(String),
    #[error("zero-padded specifier `:{0}d` must have a width between 1 and 64")]
    InvalidZeroPaddingWidth(String),
}

/// Errors that happen during match time, not load time.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum EvaluationError {
    #[error("template references `{{field:{name}}}` but custom.{name} is missing")]
    MissingCustomField { name: String },
    #[error(
        "template references `{{field:{name}}}` but custom.{name} has type `{ty}` (expected {expected})"
    )]
    WrongFieldType {
        name: String,
        ty: &'static str,
        expected: &'static str,
    },
    #[error("template references `{{date:...}}` but the .meta document has no `created_at`")]
    MissingCreatedAt,
    #[error(
        "numeric field {name} = {value} does not fit the `:{width}d` zero-padded width of the template"
    )]
    NumericOverflow { name: String, value: i64, width: u8 },
    #[error("internal regex compilation error: {0}")]
    Regex(String),
}

// --- Compile ---------------------------------------------------------------

/// Parse and validate a template string.
///
/// # Errors
///
/// Returns [`TemplateError`] for unbalanced braces, unknown
/// placeholders, mis-typed format specifiers, multiple open-ended
/// slots, duplicate `{ext}`, and invalid date format bodies.
pub fn compile(raw: &str) -> Result<CompiledTemplate, TemplateError> {
    let raw_tokens = tokenize(raw)?;
    let mut atoms = Vec::with_capacity(raw_tokens.len());
    let mut open_ended_count: u8 = 0;
    let mut ext_count = 0;
    let mut capture_counter = 0usize;
    let mut used_capture_names: BTreeSet<String> = BTreeSet::new();

    for rt in raw_tokens {
        match rt {
            RawToken::Literal(s) => atoms.push(Atom::Literal(s)),
            RawToken::Placeholder(body) => {
                let parsed = parse_placeholder(&body)?;
                match parsed {
                    Placeholder::Static {
                        kind,
                        specs,
                        namespace,
                    } => {
                        if matches!(kind, StaticKind::Ext) {
                            ext_count += 1;
                            if ext_count > 1 {
                                return Err(TemplateError::DuplicateExt);
                            }
                        }

                        let (regex_fragment, is_open_ended) = static_regex(kind, &specs);
                        if is_open_ended {
                            open_ended_count += 1;
                            if open_ended_count > 1 {
                                return Err(TemplateError::MultipleOpenEnded(kind.label().into()));
                            }
                        }

                        let capture = make_capture(
                            kind.label(),
                            &mut capture_counter,
                            &mut used_capture_names,
                        );

                        atoms.push(Atom::Static(StaticAtom {
                            kind,
                            capture,
                            regex_fragment,
                            namespace,
                            specs,
                            is_open_ended,
                        }));
                    }
                    Placeholder::Dynamic { source, specs } => {
                        // Spec §4.7: `{field:<name>}` without a *string*
                        // spec (i.e. no spec at all, or numeric-only)
                        // counts toward the single-open-ended budget.
                        // `{date:...}` always produces a fixed literal
                        // string and is never open-ended.
                        if matches!(source, DynamicSource::CustomField(_))
                            && !specs.iter().any(|s| s.is_string())
                        {
                            open_ended_count += 1;
                            if open_ended_count > 1 {
                                let label = match &source {
                                    DynamicSource::CustomField(n) => format!("field:{n}"),
                                    DynamicSource::Date(_) => "date".into(),
                                };
                                return Err(TemplateError::MultipleOpenEnded(label));
                            }
                        }
                        atoms.push(Atom::Dynamic(DynamicAtom { source, specs }));
                    }
                }
            }
        }
    }

    Ok(CompiledTemplate {
        raw: raw.to_owned(),
        atoms,
        open_ended_count,
    })
}

// --- Tokenize --------------------------------------------------------------

enum RawToken {
    Literal(String),
    Placeholder(String),
}

fn tokenize(raw: &str) -> Result<Vec<RawToken>, TemplateError> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = raw.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                buf.push('{');
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                buf.push('}');
                i += 2;
            }
            b'{' => {
                if !buf.is_empty() {
                    out.push(RawToken::Literal(std::mem::take(&mut buf)));
                }
                i += 1;
                // Collect placeholder body up to the matching `}`.
                let start = i;
                while i < bytes.len() && bytes[i] != b'}' {
                    if bytes[i] == b'{' {
                        return Err(TemplateError::NestedBrace);
                    }
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(TemplateError::UnclosedBrace);
                }
                let body = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                out.push(RawToken::Placeholder(body.to_owned()));
                i += 1; // skip closing `}`
            }
            b'}' => return Err(TemplateError::UnmatchedCloseBrace(i)),
            _ => {
                // Copy one UTF-8 char.
                let ch_end = utf8_char_end(bytes, i);
                buf.push_str(std::str::from_utf8(&bytes[i..ch_end]).unwrap_or(""));
                i = ch_end;
            }
        }
    }

    if !buf.is_empty() {
        out.push(RawToken::Literal(buf));
    }
    Ok(out)
}

fn utf8_char_end(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    let n = if b < 0xC0 {
        // ASCII (< 0x80) or continuation byte (0x80..0xC0) — treat as 1.
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    };
    (i + n).min(bytes.len())
}

// --- Placeholder parse -----------------------------------------------------

enum Placeholder {
    Static {
        kind: StaticKind,
        specs: Vec<FormatSpec>,
        namespace: Option<String>,
    },
    Dynamic {
        source: DynamicSource,
        specs: Vec<FormatSpec>,
    },
}

fn parse_placeholder(body: &str) -> Result<Placeholder, TemplateError> {
    let (head_token, specs_tail) = body.split_once(':').unwrap_or((body, ""));

    // Split namespace from head_token: `seq@scene` → ("seq", Some("scene")).
    let (name, namespace) = head_token
        .split_once('@')
        .map_or((head_token, None), |(n, ns)| (n, Some(ns.to_owned())));

    match name {
        "prefix" => parse_static(StaticKind::Prefix, specs_tail, namespace, name),
        "desc" => parse_static(StaticKind::Desc, specs_tail, namespace, name),
        "ext" => parse_static(StaticKind::Ext, specs_tail, namespace, name),
        "seq" => parse_static(StaticKind::Seq, specs_tail, namespace, name),
        "version" => parse_static(StaticKind::Version, specs_tail, namespace, name),
        "field" => parse_field(specs_tail, namespace, name),
        "date" => parse_date(specs_tail, namespace, name),
        _ => Err(TemplateError::UnknownPlaceholder(body.to_owned())),
    }
}

fn parse_static(
    kind: StaticKind,
    specs_tail: &str,
    namespace: Option<String>,
    raw_name: &str,
) -> Result<Placeholder, TemplateError> {
    if let Some(ns) = &namespace
        && !matches!(kind, StaticKind::Seq)
    {
        return Err(TemplateError::NamespaceOnNonSeq {
            name: raw_name.to_owned(),
            namespace: ns.clone(),
        });
    }

    let specs = parse_spec_chain(specs_tail, raw_name)?;
    validate_spec_chain_for_kind(kind, &specs, raw_name)?;

    Ok(Placeholder::Static {
        kind,
        specs,
        namespace,
    })
}

fn parse_field(
    specs_tail: &str,
    namespace: Option<String>,
    raw_name: &str,
) -> Result<Placeholder, TemplateError> {
    if let Some(ns) = namespace {
        return Err(TemplateError::NamespaceOnNonSeq {
            name: raw_name.to_owned(),
            namespace: ns,
        });
    }
    if specs_tail.is_empty() {
        return Err(TemplateError::MissingFieldName {
            name: raw_name.to_owned(),
        });
    }
    let (field_name, rest) = specs_tail.split_once(':').unwrap_or((specs_tail, ""));
    if field_name.is_empty() {
        return Err(TemplateError::MissingFieldName {
            name: raw_name.to_owned(),
        });
    }
    if !is_valid_field_name(field_name) {
        return Err(TemplateError::InvalidFieldName(field_name.to_owned()));
    }
    let specs = parse_spec_chain(rest, raw_name)?;
    // Spec §4.4: a single placeholder's spec chain must be homogeneous —
    // either all numeric or all string. Mixing means the render path is
    // ambiguous (what is `{field:scene:snake:03d}` supposed to produce
    // from an int? from a string?), so we reject it at load time.
    let has_num = specs.iter().any(|s| s.is_numeric());
    let has_str = specs.iter().any(|s| s.is_string());
    if has_num && has_str {
        return Err(TemplateError::MixedDynamicSpecs {
            name: raw_name.to_owned(),
        });
    }
    Ok(Placeholder::Dynamic {
        source: DynamicSource::CustomField(field_name.to_owned()),
        specs,
    })
}

fn parse_date(
    specs_tail: &str,
    namespace: Option<String>,
    raw_name: &str,
) -> Result<Placeholder, TemplateError> {
    if let Some(ns) = namespace {
        return Err(TemplateError::NamespaceOnNonSeq {
            name: raw_name.to_owned(),
            namespace: ns,
        });
    }
    if specs_tail.is_empty() {
        return Err(TemplateError::DateWithoutFormat);
    }
    let fmt = parse_date_format(specs_tail)?;
    Ok(Placeholder::Dynamic {
        source: DynamicSource::Date(fmt),
        specs: Vec::new(),
    })
}

fn is_valid_field_name(s: &str) -> bool {
    // Toml `[custom]` keys can use any unquoted key char, but to stay
    // close to the existing project convention (snake_case on-disk)
    // and to keep parsing here simple we restrict to `[A-Za-z0-9_]+`.
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn parse_spec_chain(tail: &str, name: &str) -> Result<Vec<FormatSpec>, TemplateError> {
    if tail.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut seen: BTreeSet<FormatSpec> = BTreeSet::new();
    for raw_spec in tail.split(':') {
        let spec = parse_single_spec(raw_spec, name)?;
        if !seen.insert(spec) {
            return Err(TemplateError::DuplicateSpec {
                name: name.to_owned(),
                spec: raw_spec.to_owned(),
            });
        }
        out.push(spec);
    }
    Ok(out)
}

fn parse_single_spec(raw_spec: &str, name: &str) -> Result<FormatSpec, TemplateError> {
    if raw_spec.is_empty() {
        return Err(TemplateError::UnknownSpec {
            name: name.to_owned(),
            spec: raw_spec.to_owned(),
        });
    }
    // Numeric: `d` or `0Nd` where N >= 1.
    if raw_spec == "d" {
        return Ok(FormatSpec::PlainInteger);
    }
    if let Some(width_str) = raw_spec.strip_prefix('0').and_then(|s| s.strip_suffix('d')) {
        let width: u8 = width_str
            .parse()
            .map_err(|_| TemplateError::InvalidZeroPaddingWidth(raw_spec.to_owned()))?;
        if width == 0 {
            return Err(TemplateError::InvalidZeroPaddingWidth(raw_spec.to_owned()));
        }
        return Ok(FormatSpec::ZeroPadded(width));
    }
    match raw_spec {
        "snake" => Ok(FormatSpec::Snake),
        "kebab" => Ok(FormatSpec::Kebab),
        "camel" => Ok(FormatSpec::Camel),
        "pascal" => Ok(FormatSpec::Pascal),
        "lower" => Ok(FormatSpec::Lower),
        "upper" => Ok(FormatSpec::Upper),
        "slug" => Ok(FormatSpec::Slug),
        other => Err(TemplateError::UnknownSpec {
            name: name.to_owned(),
            spec: other.to_owned(),
        }),
    }
}

// Add Ord for BTreeMap storage in `parse_spec_chain`.
impl PartialOrd for FormatSpec {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FormatSpec {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        discriminant_index(*self).cmp(&discriminant_index(*other))
    }
}
fn discriminant_index(s: FormatSpec) -> (u8, u8) {
    match s {
        FormatSpec::ZeroPadded(n) => (0, n),
        FormatSpec::PlainInteger => (1, 0),
        FormatSpec::Snake => (2, 0),
        FormatSpec::Kebab => (3, 0),
        FormatSpec::Camel => (4, 0),
        FormatSpec::Pascal => (5, 0),
        FormatSpec::Lower => (6, 0),
        FormatSpec::Upper => (7, 0),
        FormatSpec::Slug => (8, 0),
    }
}

fn validate_spec_chain_for_kind(
    kind: StaticKind,
    specs: &[FormatSpec],
    name: &str,
) -> Result<(), TemplateError> {
    let is_numeric_kind = matches!(kind, StaticKind::Seq | StaticKind::Version);
    for s in specs {
        if is_numeric_kind && s.is_string() {
            return Err(TemplateError::StringSpecOnNumeric {
                name: name.to_owned(),
                spec: spec_name(*s).to_owned(),
            });
        }
        if !is_numeric_kind && s.is_numeric() {
            return Err(TemplateError::NumericSpecOnString {
                name: name.to_owned(),
                spec: spec_name(*s).to_owned(),
            });
        }
    }
    Ok(())
}

fn spec_name(s: FormatSpec) -> &'static str {
    match s {
        FormatSpec::ZeroPadded(_) => "0Nd",
        FormatSpec::PlainInteger => "d",
        FormatSpec::Snake => "snake",
        FormatSpec::Kebab => "kebab",
        FormatSpec::Camel => "camel",
        FormatSpec::Pascal => "pascal",
        FormatSpec::Lower => "lower",
        FormatSpec::Upper => "upper",
        FormatSpec::Slug => "slug",
    }
}

fn parse_date_format(body: &str) -> Result<DateFormat, TemplateError> {
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut i = 0;
    let bytes = body.as_bytes();
    while i < bytes.len() {
        // Try the longest known date-token prefix first.
        if bytes[i..].starts_with(b"YYYY") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Year4);
            i += 4;
        } else if bytes[i..].starts_with(b"YY") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Year2);
            i += 2;
        } else if bytes[i..].starts_with(b"MM") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Month);
            i += 2;
        } else if bytes[i..].starts_with(b"DD") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Day);
            i += 2;
        } else if bytes[i..].starts_with(b"HH") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Hour);
            i += 2;
        } else if bytes[i..].starts_with(b"mm") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Minute);
            i += 2;
        } else if bytes[i..].starts_with(b"ss") {
            flush_literal(&mut literal, &mut tokens);
            tokens.push(DateToken::Second);
            i += 2;
        } else {
            // Reject uppercase letters that aren't recognized tokens —
            // catches typos like `DDD` or `YYYYY` so they don't become
            // silent literals.
            let ch = bytes[i];
            if ch.is_ascii_alphabetic() {
                let remainder: String = std::str::from_utf8(&bytes[i..])
                    .unwrap_or("")
                    .chars()
                    .take(4)
                    .collect();
                return Err(TemplateError::UnknownDateToken(remainder));
            }
            literal.push(ch as char);
            i += 1;
        }
    }
    flush_literal(&mut literal, &mut tokens);
    if tokens.is_empty() {
        return Err(TemplateError::DateWithoutFormat);
    }
    Ok(DateFormat(tokens))
}

fn flush_literal(buf: &mut String, out: &mut Vec<DateToken>) {
    if !buf.is_empty() {
        out.push(DateToken::Literal(std::mem::take(buf)));
    }
}

// --- Static regex fragments ------------------------------------------------

/// Build the regex fragment for a static placeholder.
///
/// The tuple's second member indicates whether this placeholder
/// consumed the template's single open-ended slot (spec §4.7).
fn static_regex(kind: StaticKind, specs: &[FormatSpec]) -> (String, bool) {
    // Numeric placeholders: specs come at most in two shapes.
    if matches!(kind, StaticKind::Seq | StaticKind::Version) {
        for s in specs.iter().rev() {
            match s {
                FormatSpec::ZeroPadded(width) => {
                    return (format!("\\d{{{width}}}"), false);
                }
                FormatSpec::PlainInteger => return ("\\d+".into(), false),
                _ => {}
            }
        }
        // A numeric placeholder with no spec defaults to unbounded
        // digits; spec uses `{seq:03d}` form in all examples but
        // doesn't outright forbid a bare `{seq}`.
        return ("\\d+".into(), false);
    }

    if matches!(kind, StaticKind::Ext) {
        // The extension is pulled out of the basename before regex
        // match. See `match_basename` — we use `[^/]+` here just so
        // suggest-time code that inspects atoms has a sensible
        // placeholder to work with.
        return ("[A-Za-z0-9.]+".into(), false);
    }

    // String placeholders (prefix / desc):
    //   - No spec → open-ended.
    //   - Otherwise → fragment driven by the last spec in the chain,
    //     since each spec transforms the previous stage's output.
    let Some(last) = specs.last() else {
        return ("[^/]+".into(), true);
    };

    let fragment = match last {
        FormatSpec::Snake => "[a-z0-9]+(?:_[a-z0-9]+)*",
        FormatSpec::Kebab | FormatSpec::Slug => "[a-z0-9]+(?:-[a-z0-9]+)*",
        FormatSpec::Camel => "[a-z][a-zA-Z0-9]*",
        FormatSpec::Pascal => "[A-Z][a-zA-Z0-9]*",
        FormatSpec::Lower => "[^/A-Z]+",
        FormatSpec::Upper => "[^/a-z]+",
        // Numeric specs can't end up here because
        // `validate_spec_chain_for_kind` rejected them already.
        FormatSpec::ZeroPadded(_) | FormatSpec::PlainInteger => "[^/]+",
    };
    (fragment.to_owned(), false)
}

fn make_capture(base: &str, counter: &mut usize, seen: &mut BTreeSet<String>) -> String {
    let mut candidate = format!("m_{base}");
    if seen.contains(&candidate) {
        *counter += 1;
        candidate = format!("m_{base}_{counter}");
    }
    seen.insert(candidate.clone());
    candidate
}

// --- Match -----------------------------------------------------------------

/// Result of applying a compiled template to a basename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateMatch {
    pub matched: bool,
    /// Captured values per static placeholder (keyed by [`StaticAtom::capture`]).
    pub captures: BTreeMap<String, String>,
    /// When `matched == false`, a short human-readable reason.
    pub failure_reason: Option<String>,
}

/// Match a basename against a compiled template.
///
/// `meta` is only needed when the template contains `{field:}` or
/// `{date:}` placeholders. Missing `meta` plus a dynamic placeholder
/// produces an [`EvaluationError`] (spec §4.6).
///
/// `compound_exts` is the effective union of
/// [`crate::rules::BUILTIN_COMPOUND_EXTS`] and the project's
/// `.progest/schema.toml` `[extension_compounds]` entries. It feeds
/// `{ext}` matching: the extension is peeled off with longest-match
/// before the stem is regex-checked (spec §4.8), so templates like
/// `{desc:snake}.{ext}` correctly treat `archive.tar.gz` as
/// (stem = "archive", ext = "tar.gz") rather than eating the inner
/// dot into `desc`.
///
/// # Errors
///
/// Returns [`EvaluationError`] only for dynamic-placeholder failures
/// (missing field, wrong type, missing `created_at`, numeric overflow).
/// Template mismatches are returned as `TemplateMatch { matched: false, … }`,
/// not errors, so the evaluator can fold them into violations.
pub fn match_basename(
    template: &CompiledTemplate,
    basename: &str,
    meta: Option<&MetaDocument>,
    compound_exts: &[&str],
) -> Result<TemplateMatch, EvaluationError> {
    let atoms = template.atoms();
    let ext_pos = atoms
        .iter()
        .position(|a| matches!(a, Atom::Static(s) if matches!(s.kind, StaticKind::Ext)));

    // Spec §4.8: peel the longest known compound extension, then match
    // the stem against the non-ext atoms. Without this pre-split, a
    // regex `(?<desc>.+)\.(?<ext>[A-Za-z0-9.]+)` would let
    // `archive.tar.gz` resolve to (desc="archive.tar", ext="gz") or
    // even (desc="archive", ext="tar.gz") non-deterministically.
    let (subject, captured_ext) = if ext_pos.is_some() {
        let (stem, ext) = split_basename(basename, compound_exts);
        let Some(ext) = ext else {
            return Ok(TemplateMatch {
                matched: false,
                captures: BTreeMap::new(),
                failure_reason: Some(format!(
                    "basename `{basename}` has no extension, but template `{raw}` requires `{{ext}}`",
                    raw = template.raw()
                )),
            });
        };
        (stem.to_owned(), Some(ext.to_owned()))
    } else {
        (basename.to_owned(), None)
    };

    let mut pattern = String::from("^");
    let mut capture_keys: Vec<String> = Vec::new();

    for (i, atom) in atoms.iter().enumerate() {
        if Some(i) == ext_pos {
            // `{ext}` is consumed by `split_basename` above — do not
            // emit a regex fragment for it.
            continue;
        }
        match atom {
            Atom::Literal(s) => {
                // The dot separator before `{ext}` is implicitly owned
                // by the split — strip it from the literal so the stem
                // regex does not demand a trailing dot that was already
                // removed from the subject.
                let effective = if ext_pos == Some(i + 1) {
                    s.strip_suffix('.').unwrap_or(s)
                } else {
                    s.as_str()
                };
                pattern.push_str(&regex::escape(effective));
            }
            Atom::Static(s) => {
                write!(
                    pattern,
                    "(?<{cap}>{frag})",
                    cap = s.capture,
                    frag = s.regex_fragment
                )
                .expect("writing to String never fails");
                capture_keys.push(s.capture.clone());
            }
            Atom::Dynamic(d) => {
                let literal = resolve_dynamic(d, meta)?;
                pattern.push_str(&regex::escape(&literal));
            }
        }
    }
    pattern.push('$');

    let re = Regex::new(&pattern).map_err(|e| EvaluationError::Regex(e.to_string()))?;

    match re.captures(&subject) {
        None => Ok(TemplateMatch {
            matched: false,
            captures: BTreeMap::new(),
            failure_reason: Some(describe_mismatch(template, basename, ext_pos.is_some())),
        }),
        Some(caps) => {
            let mut captures = BTreeMap::new();
            for key in capture_keys {
                if let Some(m) = caps.name(&key) {
                    captures.insert(key, m.as_str().to_owned());
                }
            }
            if let Some(pos) = ext_pos
                && let Atom::Static(ext_atom) = &atoms[pos]
                && let Some(ext) = captured_ext
            {
                captures.insert(ext_atom.capture.clone(), ext);
            }
            Ok(TemplateMatch {
                matched: true,
                captures,
                failure_reason: None,
            })
        }
    }
}

/// Generate a short reason string for a failed match.
fn describe_mismatch(template: &CompiledTemplate, basename: &str, _has_ext: bool) -> String {
    format!(
        "basename `{basename}` does not satisfy template `{raw}`",
        raw = template.raw()
    )
}

fn resolve_dynamic(
    atom: &DynamicAtom,
    meta: Option<&MetaDocument>,
) -> Result<String, EvaluationError> {
    match &atom.source {
        DynamicSource::CustomField(name) => {
            let meta =
                meta.ok_or_else(|| EvaluationError::MissingCustomField { name: name.clone() })?;
            let value = meta
                .custom
                .get(name)
                .ok_or_else(|| EvaluationError::MissingCustomField { name: name.clone() })?;
            render_custom_field(name, value, &atom.specs)
        }
        DynamicSource::Date(fmt) => {
            let meta = meta.ok_or(EvaluationError::MissingCreatedAt)?;
            let created = meta.created_at.ok_or(EvaluationError::MissingCreatedAt)?;
            Ok(format_datetime(&created, fmt))
        }
    }
}

fn render_custom_field(
    name: &str,
    value: &Value,
    specs: &[FormatSpec],
) -> Result<String, EvaluationError> {
    let want_numeric = specs.iter().any(|s| s.is_numeric());
    let want_string = specs.iter().any(|s| s.is_string());

    // When any numeric spec is present, the field must carry an integer.
    // Spec §4.6: "numeric spec + string value → evaluation_error".
    if want_numeric {
        let int = value
            .as_integer()
            .ok_or_else(|| EvaluationError::WrongFieldType {
                name: name.to_owned(),
                ty: value.type_str(),
                expected: "integer",
            })?;
        return apply_numeric_specs(name, int, specs);
    }

    // Spec §4.6: "string spec + int value → evaluation_error". We want
    // string-spec transforms to see a real string, not a coerced int
    // rendering — applying `:snake` to `2026` silently masks the fact
    // that the field was misconfigured as an int.
    if want_string && !matches!(value, Value::String(_)) {
        return Err(EvaluationError::WrongFieldType {
            name: name.to_owned(),
            ty: value.type_str(),
            expected: "string",
        });
    }

    // No specs: render any scalar via its Display form as a literal.
    // Tables / arrays still fail because there is no sensible literal
    // for them to collapse to.
    let raw = match value {
        Value::String(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Datetime(dt) => dt.to_string(),
        Value::Array(_) | Value::Table(_) => {
            return Err(EvaluationError::WrongFieldType {
                name: name.to_owned(),
                ty: value.type_str(),
                expected: "scalar",
            });
        }
    };
    Ok(apply_string_specs(&raw, specs))
}

fn apply_numeric_specs(
    name: &str,
    int: i64,
    specs: &[FormatSpec],
) -> Result<String, EvaluationError> {
    // `parse_field` rejects mixed numeric+string spec chains, so any
    // spec reaching this function is numeric. Left-to-right application
    // matters only between numeric specs (§4.4.3).
    let mut out = int.to_string();
    for spec in specs {
        match spec {
            FormatSpec::ZeroPadded(width) => {
                if int < 0 {
                    // Zero-padding negative integers is unusual enough
                    // to flag; spec doesn't cover it, treat as overflow.
                    return Err(EvaluationError::NumericOverflow {
                        name: name.to_owned(),
                        value: int,
                        width: *width,
                    });
                }
                let w = usize::from(*width);
                if out.len() > w {
                    return Err(EvaluationError::NumericOverflow {
                        name: name.to_owned(),
                        value: int,
                        width: *width,
                    });
                }
                out = format!("{out:0>w$}");
            }
            FormatSpec::PlainInteger => {}
            FormatSpec::Snake
            | FormatSpec::Kebab
            | FormatSpec::Camel
            | FormatSpec::Pascal
            | FormatSpec::Lower
            | FormatSpec::Upper
            | FormatSpec::Slug => {
                unreachable!("parse_field rejects mixed numeric/string spec chains")
            }
        }
    }
    Ok(out)
}

fn apply_string_specs(input: &str, specs: &[FormatSpec]) -> String {
    let mut s = input.to_owned();
    for spec in specs {
        s = match spec {
            FormatSpec::Lower => s.to_lowercase(),
            FormatSpec::Upper => s.to_uppercase(),
            FormatSpec::Snake => to_snake(&s),
            FormatSpec::Kebab => to_kebab(&s),
            FormatSpec::Camel => to_camel(&s),
            FormatSpec::Pascal => to_pascal(&s),
            FormatSpec::Slug => to_slug(&s),
            FormatSpec::ZeroPadded(_) | FormatSpec::PlainInteger => s,
        };
    }
    s
}

fn word_chunks(input: &str) -> Vec<String> {
    // Split on anything that isn't alphanumeric, drop empties.
    input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn to_snake(s: &str) -> String {
    word_chunks(s)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn to_kebab(s: &str) -> String {
    word_chunks(s)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

fn to_slug(s: &str) -> String {
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

fn to_camel(s: &str) -> String {
    let mut out = String::new();
    for (idx, word) in word_chunks(s).into_iter().enumerate() {
        if idx == 0 {
            out.push_str(&word.to_lowercase());
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(&chars.collect::<String>().to_lowercase());
            }
        }
    }
    out
}

fn to_pascal(s: &str) -> String {
    let mut out = String::new();
    for word in word_chunks(s) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.collect::<String>().to_lowercase());
        }
    }
    out
}

fn format_datetime(dt: &toml::value::Datetime, fmt: &DateFormat) -> String {
    // Extract the date/time components we care about. TOML datetimes
    // may omit a date or an offset; missing components render as "".
    let (year, month, day) = dt.date.map_or((0, 0, 0), |d| {
        (i32::from(d.year), u32::from(d.month), u32::from(d.day))
    });
    let (hour, minute, second) = dt.time.map_or((0, 0, 0), |t| {
        (u32::from(t.hour), u32::from(t.minute), u32::from(t.second))
    });
    let year2 = year.unsigned_abs() % 100;

    let mut out = String::new();
    for tok in &fmt.0 {
        match tok {
            DateToken::Literal(s) => out.push_str(s),
            DateToken::Year4 => {
                let _ = write!(out, "{year:04}");
            }
            DateToken::Year2 => {
                let _ = write!(out, "{year2:02}");
            }
            DateToken::Month => {
                let _ = write!(out, "{month:02}");
            }
            DateToken::Day => {
                let _ = write!(out, "{day:02}");
            }
            DateToken::Hour => {
                let _ = write!(out, "{hour:02}");
            }
            DateToken::Minute => {
                let _ = write!(out, "{minute:02}");
            }
            DateToken::Second => {
                let _ = write!(out, "{second:02}");
            }
        }
    }
    out
}

// --- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::constraint::BUILTIN_COMPOUND_EXTS;
    use super::*;

    fn meta_with_custom(pairs: &[(&str, toml::Value)]) -> MetaDocument {
        use crate::identity::FileId;
        let mut doc = MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff".parse().unwrap(),
        );
        for (k, v) in pairs {
            doc.custom.insert((*k).into(), v.clone());
        }
        doc
    }

    // --- Tokenization -----------------------------------------------------

    #[test]
    fn compiles_canonical_shot_template() {
        let t = compile("{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}").unwrap();
        assert_eq!(t.open_ended_count(), 1); // {prefix} is open-ended
        assert_eq!(t.atoms().len(), 9); // 4 static + 4 literal + 1 ext = 9
    }

    #[test]
    fn rejects_unclosed_brace() {
        assert!(matches!(
            compile("{prefix"),
            Err(TemplateError::UnclosedBrace)
        ));
    }

    #[test]
    fn rejects_unmatched_close_brace() {
        assert!(matches!(
            compile("prefix}"),
            Err(TemplateError::UnmatchedCloseBrace(_))
        ));
    }

    #[test]
    fn literal_braces_via_double_escape() {
        let t = compile("pre{{x}}post").unwrap();
        // Expect three literals; no placeholders.
        let mut literal = String::new();
        for a in t.atoms() {
            if let Atom::Literal(s) = a {
                literal.push_str(s);
            } else {
                panic!("unexpected non-literal atom");
            }
        }
        assert_eq!(literal, "pre{x}post");
    }

    #[test]
    fn rejects_nested_brace() {
        assert!(matches!(
            compile("{pre{fix}"),
            Err(TemplateError::NestedBrace)
        ));
    }

    // --- Placeholder validation -------------------------------------------

    #[test]
    fn rejects_unknown_placeholder() {
        assert!(matches!(
            compile("{whoami}"),
            Err(TemplateError::UnknownPlaceholder(_))
        ));
    }

    #[test]
    fn rejects_multiple_open_ended_slots() {
        assert!(matches!(
            compile("{prefix}_{desc}"),
            Err(TemplateError::MultipleOpenEnded(_))
        ));
    }

    #[test]
    fn allows_mixed_open_ended_with_cased_desc() {
        // {prefix} open-ended, {desc:snake} is not — legal.
        compile("{prefix}_{desc:snake}.{ext}").unwrap();
    }

    #[test]
    fn rejects_duplicate_ext() {
        assert!(matches!(
            compile("{prefix}.{ext}.{ext}"),
            Err(TemplateError::DuplicateExt)
        ));
    }

    #[test]
    fn rejects_mixed_numeric_and_string_specs() {
        assert!(matches!(
            compile("{seq:snake}"),
            Err(TemplateError::StringSpecOnNumeric { .. })
        ));
        assert!(matches!(
            compile("{desc:03d}"),
            Err(TemplateError::NumericSpecOnString { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_spec() {
        assert!(matches!(
            compile("{desc:snake:snake}"),
            Err(TemplateError::DuplicateSpec { .. })
        ));
    }

    #[test]
    fn rejects_namespace_on_non_seq() {
        assert!(matches!(
            compile("{prefix@scene}"),
            Err(TemplateError::NamespaceOnNonSeq { .. })
        ));
    }

    #[test]
    fn accepts_namespace_on_seq() {
        compile("{seq@scene:03d}").unwrap();
    }

    // --- Date --------------------------------------------------------------

    #[test]
    fn parses_date_format_with_literals_between_tokens() {
        let t = compile("{date:YYYY-MM-DD}").unwrap();
        assert_eq!(t.atoms().len(), 1);
        match &t.atoms()[0] {
            Atom::Dynamic(d) => match &d.source {
                DynamicSource::Date(DateFormat(tokens)) => {
                    assert_eq!(
                        tokens,
                        &vec![
                            DateToken::Year4,
                            DateToken::Literal("-".into()),
                            DateToken::Month,
                            DateToken::Literal("-".into()),
                            DateToken::Day,
                        ]
                    );
                }
                DynamicSource::CustomField(_) => panic!("expected date source"),
            },
            _ => panic!("expected dynamic atom"),
        }
    }

    #[test]
    fn rejects_unknown_date_token() {
        assert!(matches!(
            compile("{date:YYYYY}"),
            Err(TemplateError::UnknownDateToken(_))
        ));
    }

    #[test]
    fn rejects_date_without_format() {
        assert!(matches!(
            compile("{date:}"),
            Err(TemplateError::DateWithoutFormat)
        ));
    }

    // --- Match success ----------------------------------------------------

    #[test]
    fn matches_canonical_shot_filename() {
        let t = compile("{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}").unwrap();
        let m = match_basename(
            &t,
            "ch010_001_bg_forest_v03.psd",
            None,
            BUILTIN_COMPOUND_EXTS,
        )
        .unwrap();
        assert!(m.matched, "failure: {:?}", m.failure_reason);
        assert_eq!(
            m.captures.get("m_prefix").map(String::as_str),
            Some("ch010")
        );
        assert_eq!(m.captures.get("m_seq").map(String::as_str), Some("001"));
        assert_eq!(
            m.captures.get("m_desc").map(String::as_str),
            Some("bg_forest")
        );
        assert_eq!(m.captures.get("m_version").map(String::as_str), Some("03"));
        assert_eq!(m.captures.get("m_ext").map(String::as_str), Some("psd"));
    }

    #[test]
    fn match_rejects_missing_seq_segment() {
        let t = compile("{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}").unwrap();
        let m = match_basename(&t, "ch010_bg_forest_v03.psd", None, BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(!m.matched);
        assert!(m.failure_reason.is_some());
    }

    #[test]
    fn match_rejects_non_snake_desc() {
        let t = compile("{prefix}_{desc:snake}.{ext}").unwrap();
        let m = match_basename(&t, "ch010_BgForest.psd", None, BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(!m.matched);
    }

    // --- Dynamic placeholders ---------------------------------------------

    #[test]
    fn matches_field_placeholder_with_literal_expansion() {
        let t = compile("sc{field:scene:03d}_{desc:slug}.{ext}").unwrap();
        let meta = meta_with_custom(&[("scene", toml::Value::Integer(20))]);
        let m = match_basename(
            &t,
            "sc020_forest-night.tif",
            Some(&meta),
            BUILTIN_COMPOUND_EXTS,
        )
        .unwrap();
        assert!(m.matched, "failure: {:?}", m.failure_reason);
    }

    #[test]
    fn field_placeholder_mismatch_literal_is_a_violation_not_an_error() {
        let t = compile("sc{field:scene:03d}_{desc:slug}.{ext}").unwrap();
        let meta = meta_with_custom(&[("scene", toml::Value::Integer(20))]);
        // custom.scene = 20 → expands to "020", but path has "999"
        let m = match_basename(
            &t,
            "sc999_forest-night.tif",
            Some(&meta),
            BUILTIN_COMPOUND_EXTS,
        )
        .unwrap();
        assert!(!m.matched);
    }

    #[test]
    fn field_missing_is_evaluation_error() {
        let t = compile("sc{field:scene:03d}.{ext}").unwrap();
        let meta = meta_with_custom(&[]);
        let err = match_basename(&t, "sc020.tif", Some(&meta), BUILTIN_COMPOUND_EXTS).unwrap_err();
        assert!(matches!(err, EvaluationError::MissingCustomField { .. }));
    }

    #[test]
    fn field_wrong_type_is_evaluation_error() {
        let t = compile("sc{field:scene:03d}.{ext}").unwrap();
        let meta = meta_with_custom(&[("scene", toml::Value::String("twenty".into()))]);
        let err = match_basename(&t, "sc020.tif", Some(&meta), BUILTIN_COMPOUND_EXTS).unwrap_err();
        assert!(matches!(err, EvaluationError::WrongFieldType { .. }));
    }

    #[test]
    fn field_overflow_is_evaluation_error() {
        let t = compile("sc{field:scene:03d}.{ext}").unwrap();
        // 9999 > 3 digits → overflow
        let meta = meta_with_custom(&[("scene", toml::Value::Integer(9999))]);
        let err = match_basename(&t, "sc9999.tif", Some(&meta), BUILTIN_COMPOUND_EXTS).unwrap_err();
        assert!(matches!(err, EvaluationError::NumericOverflow { .. }));
    }

    #[test]
    fn rejects_mixed_dynamic_spec_chain_at_load_time() {
        assert!(matches!(
            compile("{field:scene:snake:03d}"),
            Err(TemplateError::MixedDynamicSpecs { .. })
        ));
        assert!(matches!(
            compile("{field:scene:03d:snake}"),
            Err(TemplateError::MixedDynamicSpecs { .. })
        ));
    }

    #[test]
    fn rejects_multiple_open_ended_mixed_with_bare_field() {
        // `{prefix}` is open-ended; `{field:name}` with no spec is also
        // open-ended under §4.7. Together they should be rejected.
        assert!(matches!(
            compile("{prefix}_{field:name}.{ext}"),
            Err(TemplateError::MultipleOpenEnded(_))
        ));
    }

    #[test]
    fn numeric_only_field_counts_toward_open_ended_limit() {
        // Two numeric-only `{field:}` placeholders are open-ended per
        // §4.7 — neither carries a string spec.
        assert!(matches!(
            compile("{field:a:03d}_{field:b:03d}.{ext}"),
            Err(TemplateError::MultipleOpenEnded(_))
        ));
    }

    #[test]
    fn string_spec_field_does_not_count_as_open_ended() {
        // `{field:name:snake}` has a string spec → not open-ended.
        let t = compile("{prefix}_{field:name:snake}.{ext}").unwrap();
        assert_eq!(t.open_ended_count(), 1); // just {prefix}
    }

    #[test]
    fn string_spec_on_int_field_is_evaluation_error() {
        let t = compile("{field:name:snake}.{ext}").unwrap();
        let meta = meta_with_custom(&[("name", toml::Value::Integer(42))]);
        let err = match_basename(&t, "42.png", Some(&meta), BUILTIN_COMPOUND_EXTS).unwrap_err();
        assert!(matches!(
            err,
            EvaluationError::WrongFieldType {
                expected: "string",
                ..
            }
        ));
    }

    #[test]
    fn no_spec_field_accepts_int_as_literal() {
        // Without any spec, Display rendering remains the pragmatic
        // default so `{field:shot}` + `shot = 20` → literal "20".
        let t = compile("sc-{field:shot}.{ext}").unwrap();
        let meta = meta_with_custom(&[("shot", toml::Value::Integer(20))]);
        let m = match_basename(&t, "sc-20.png", Some(&meta), BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(m.matched, "failure: {:?}", m.failure_reason);
    }

    // --- Compound extension (§4.8) ---------------------------------------

    #[test]
    fn ext_atom_matches_builtin_compound_longest() {
        // `archive.tar.gz` → (stem="archive", ext="tar.gz") via the
        // builtin compound set. Without compound support, a greedy
        // regex could split this as (stem="archive.tar", ext="gz").
        let t = compile("{desc:snake}.{ext}").unwrap();
        let m = match_basename(&t, "archive.tar.gz", None, BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(m.matched, "failure: {:?}", m.failure_reason);
        assert_eq!(
            m.captures.get("m_desc").map(String::as_str),
            Some("archive")
        );
        assert_eq!(m.captures.get("m_ext").map(String::as_str), Some("tar.gz"));
    }

    #[test]
    fn ext_atom_falls_back_to_last_dot_for_unknown_compound() {
        // `foo.psd.bak` has no entry in BUILTIN_COMPOUND_EXTS, so we
        // peel only `.bak` and the stem keeps its inner dot. The
        // `{desc:snake}` atom does not permit `.` so this is a miss.
        let t = compile("{desc:snake}.{ext}").unwrap();
        let m = match_basename(&t, "foo.psd.bak", None, BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(!m.matched);
    }

    #[test]
    fn ext_atom_unknown_compound_registered_by_project_is_peeled() {
        // When the project declares `psd.bak` in
        // `[extension_compounds]`, it becomes a longest-match candidate
        // and the stem regains a clean spelling.
        let t = compile("{desc:snake}.{ext}").unwrap();
        let compound: &[&str] = &["psd.bak"];
        let m = match_basename(&t, "foo.psd.bak", None, compound).unwrap();
        assert!(m.matched, "failure: {:?}", m.failure_reason);
        assert_eq!(m.captures.get("m_desc").map(String::as_str), Some("foo"));
        assert_eq!(m.captures.get("m_ext").map(String::as_str), Some("psd.bak"));
    }

    #[test]
    fn ext_atom_rejects_extensionless_basename() {
        let t = compile("{prefix}.{ext}").unwrap();
        let m = match_basename(&t, "README", None, BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(!m.matched);
        assert!(
            m.failure_reason
                .as_deref()
                .is_some_and(|r| r.contains("has no extension")),
            "unexpected reason: {:?}",
            m.failure_reason
        );
    }

    // --- Date match -------------------------------------------------------

    #[test]
    fn matches_date_placeholder_from_created_at() {
        let t = compile("snap-{date:YYYYMMDD}.{ext}").unwrap();
        let mut meta = meta_with_custom(&[]);
        meta.created_at = Some("2026-04-20T10:00:00Z".parse().unwrap());
        let m =
            match_basename(&t, "snap-20260420.png", Some(&meta), BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(m.matched);
    }

    #[test]
    fn date_without_created_at_is_evaluation_error() {
        let t = compile("snap-{date:YYYYMMDD}.{ext}").unwrap();
        let meta = meta_with_custom(&[]);
        let err = match_basename(&t, "snap-20260420.png", Some(&meta), BUILTIN_COMPOUND_EXTS)
            .unwrap_err();
        assert!(matches!(err, EvaluationError::MissingCreatedAt));
    }

    // --- Spec chain -------------------------------------------------------

    #[test]
    fn slug_spec_matches_hyphenated_lowercase() {
        let t = compile("{prefix:slug}.{ext}").unwrap();
        let m = match_basename(&t, "forest-night.png", None, BUILTIN_COMPOUND_EXTS).unwrap();
        assert!(m.matched);
        assert!(
            !match_basename(&t, "forest_night.png", None, BUILTIN_COMPOUND_EXTS)
                .unwrap()
                .matched
        );
    }

    #[test]
    fn chained_string_specs_use_last_spec_regex() {
        // `:snake:lower` → regex is `:lower`'s (last in chain).
        // `:lower` allows no uppercase; `snake_case` already lowercase → matches.
        let t = compile("{prefix:snake:lower}.{ext}").unwrap();
        assert!(
            match_basename(&t, "ch010_bg.psd", None, BUILTIN_COMPOUND_EXTS)
                .unwrap()
                .matched
        );
        assert!(
            !match_basename(&t, "ch010_Bg.psd", None, BUILTIN_COMPOUND_EXTS)
                .unwrap()
                .matched
        );
    }

    // --- Utility unit tests -----------------------------------------------

    #[test]
    fn to_slug_maps_symbols_to_hyphens() {
        assert_eq!(to_slug("Ch 10 / Sc 20"), "ch-10-sc-20");
        assert_eq!(to_slug("  hello  world  "), "hello-world");
    }

    #[test]
    fn to_snake_and_kebab_canonicalize() {
        assert_eq!(to_snake("Forest Night"), "forest_night");
        assert_eq!(to_kebab("Forest Night"), "forest-night");
    }

    #[test]
    fn to_camel_and_pascal_canonicalize() {
        assert_eq!(to_camel("forest night"), "forestNight");
        assert_eq!(to_pascal("forest night"), "ForestNight");
    }
}
