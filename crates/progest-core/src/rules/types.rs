//! Shared value types for the naming-rules engine.
//!
//! Section references below point at `docs/NAMING_RULES_DSL.md`, which is
//! the normative spec this module implements.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fs::ProjectPath;
use crate::identity::FileId;

// --- Rule identifier (§2) ---------------------------------------------------

/// Validated rule identifier, constrained to `^[a-z][a-z0-9_-]{0,63}$`.
///
/// Ids are used as the key for override resolution (§7.2) and as the
/// primary handle in `rule_id` traces (§9), so enforcing the shape at
/// construction keeps every downstream consumer honest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Errors returned when constructing a [`RuleId`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuleIdError {
    #[error("rule id must match ^[a-z][a-z0-9_-]{{0,63}}$, got `{0}`")]
    Invalid(String),
}

impl FromStr for RuleId {
    type Err = RuleIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if is_valid(s) {
            Ok(Self(s.to_owned()))
        } else {
            Err(RuleIdError::Invalid(s.to_owned()))
        }
    }
}

impl<'de> Deserialize<'de> for RuleId {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(d)?;
        raw.parse::<Self>().map_err(serde::de::Error::custom)
    }
}

fn is_valid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}

// --- Kind / Mode / Severity / Category (§2, §5, §8.2) ----------------------

/// Which DSL layer a rule belongs to (§4 vs §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleKind {
    Template,
    Constraint,
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Template => "template",
            Self::Constraint => "constraint",
        })
    }
}

/// User-configured mode determining lint / rename behavior (§8.2).
///
/// `Mode::Warn` is the default when callers leave the field unset in
/// `rules.toml`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Strict,
    #[default]
    Warn,
    Hint,
    Off,
}

impl Mode {
    /// The severity that should be attached to violations emitted under
    /// this mode. `Off` produces no violations and is therefore modeled
    /// as `None` — callers should short-circuit before reaching here.
    #[must_use]
    pub fn violation_severity(self) -> Option<Severity> {
        match self {
            Self::Strict => Some(Severity::Strict),
            Self::Warn => Some(Severity::Warn),
            Self::Hint => Some(Severity::Hint),
            Self::Off => None,
        }
    }
}

/// Severity stamped onto an emitted violation (§8.2, §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Strict,
    Warn,
    Hint,
    /// Evaluation could not complete (§4.6) — e.g. a referenced `{field:}`
    /// was missing from `.meta`. Contributes to a non-zero CLI exit like
    /// `Strict`.
    EvaluationError,
}

impl Severity {
    /// Whether this severity should force a non-zero CLI exit code.
    #[must_use]
    pub fn fails_ci(self) -> bool {
        matches!(self, Self::Strict | Self::EvaluationError)
    }
}

/// Category of an emitted violation (§8.3).
///
/// `Placement` is produced by `core::accepts` and `Sequence` by
/// `core::sequence::drift`, neither of which belongs to `core::rules`.
/// All three live on the same enum so lint reports and exit-code
/// folding can treat every source uniformly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Naming,
    Placement,
    /// Drift detected among sibling numbered sequences (same parent +
    /// normalized stem but differing separator/padding).
    Sequence,
}

// --- Constraint field vocab (§5.3 / §5.4) ----------------------------------

/// Allowed character class for constraint rules (§5.3).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Charset {
    Ascii,
    #[default]
    Utf8,
    NoCjk,
}

/// Permitted casing for basename-minus-extension (§5.4).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Casing {
    #[default]
    Any,
    Snake,
    Kebab,
    Camel,
    Pascal,
}

// --- Specificity / Source / Decision / Trace (§7.4, §9.2) ------------------

/// Specificity score ordered by `(literal_segments, literal_chars)`.
///
/// Higher scores win. Further tie-breakers (source hierarchy and
/// lexicographic `rule_id`) are applied outside this value, so don't
/// embed those into the comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpecificityScore {
    pub literal_segments: u32,
    pub literal_chars: u32,
}

impl SpecificityScore {
    /// Lowest possible score — useful for ruleset bootstrapping.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            literal_segments: 0,
            literal_chars: 0,
        }
    }
}

/// Where a rule was defined relative to the file being evaluated (§7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "layer", rename_all = "snake_case")]
pub enum RuleSource {
    /// Defined in the `.dirmeta.toml` of the file's own directory.
    Own,
    /// Inherited from an ancestor `.dirmeta.toml`; `distance` is the
    /// number of directory steps toward the root (1 = immediate parent).
    Inherited { distance: u16 },
    /// Defined in `.progest/rules.toml`.
    ProjectWide,
}

/// Per-rule decision recorded in a `rule_id` trace (§9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Actually used when producing the (possibly empty) set of violations.
    Winner,
    /// Survived collection but lost the specificity tie-break.
    Shadowed,
    /// Filtered out by `applies_to`; only emitted under `--explain`.
    NotApplicable,
}

/// One entry in the `rule_id` trace for a single evaluated file (§9.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleHit {
    pub rule_id: RuleId,
    pub kind: RuleKind,
    pub source: RuleSource,
    pub decision: Decision,
    pub specificity_score: SpecificityScore,
    /// Short human-readable justification (e.g.
    /// `"winner by literal-segment count (3 vs 2)"`).
    pub explanation: String,
}

// --- Violation (§8.3) ------------------------------------------------------

/// A single lint violation produced by `core::rules`.
///
/// `file_id` is optional because lint may run against files that don't
/// yet have a sidecar (e.g. `progest lint` on a fresh directory). When
/// present, downstream callers such as `core::index` can key violations
/// by `file_id` rather than path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<FileId>,
    pub path: ProjectPath,
    pub rule_id: RuleId,
    pub category: Category,
    pub kind: RuleKind,
    pub severity: Severity,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<RuleHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_names: Vec<String>,
    /// Placement-specific payload (§3.13.6). Populated only when
    /// `category == Placement`; `None` for naming violations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_details: Option<PlacementDetails>,
}

/// Fields required by REQUIREMENTS.md §3.13.6 for `category =
/// "placement"` violations. Kept optional on [`Violation`] so naming
/// rules don't have to carry empty data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementDetails {
    /// Extensions the parent directory accepts (alias-expanded,
    /// normalized to lowercase without leading dot). Empty means the
    /// directory has no `[accepts]` and the violation is purely
    /// advisory — placement lint should not emit in that case, so an
    /// empty vector here is a code smell.
    pub expected_exts: Vec<String>,
    /// Whether the accepting dir won by declaring the allowed set
    /// itself or by inheriting from an ancestor.
    pub winning_rule_source: AcceptsSource,
    /// Ranked destination candidates. Reserved for the
    /// import-ranking pass in a follow-up PR; left empty by the
    /// initial placement lint. Ranking order (top priority first):
    /// own-set match, then inherited match, then MRU, then shallow
    /// path depth.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_destinations: Vec<ProjectPath>,
}

/// Where the `effective_accepts` set that rejected a file came from.
///
/// Separate from [`RuleSource`]: naming rules cascade through
/// project-wide + dirmeta layers, but accepts only distinguishes
/// "declared on this dir" vs "inherited from an ancestor" per
/// REQUIREMENTS.md §3.13.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AcceptsSource {
    Own,
    Inherited,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RuleId validation --------------------------------------------------

    #[test]
    fn rule_id_accepts_canonical_forms() {
        for ok in ["a", "shot-assets-v1", "ascii_only", "rule-42"] {
            ok.parse::<RuleId>()
                .unwrap_or_else(|e| panic!("{ok:?} should parse, got {e:?}"));
        }
    }

    #[test]
    fn rule_id_rejects_empty() {
        let err = "".parse::<RuleId>().unwrap_err();
        assert!(matches!(err, RuleIdError::Invalid(s) if s.is_empty()));
    }

    #[test]
    fn rule_id_rejects_uppercase_start() {
        assert!("Shot".parse::<RuleId>().is_err());
    }

    #[test]
    fn rule_id_rejects_digit_start() {
        assert!("1st".parse::<RuleId>().is_err());
    }

    #[test]
    fn rule_id_rejects_disallowed_chars() {
        for bad in ["shot.v1", "shot v1", "shot@v1", "shot/v1", "日本語"] {
            assert!(bad.parse::<RuleId>().is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn rule_id_rejects_length_over_64() {
        let too_long: String = std::iter::repeat_n('a', 65).collect();
        assert!(too_long.parse::<RuleId>().is_err());

        let max: String = std::iter::repeat_n('a', 64).collect();
        assert!(max.parse::<RuleId>().is_ok());
    }

    #[test]
    fn rule_id_round_trips_via_json() {
        let id: RuleId = "shot-assets-v1".parse().unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"shot-assets-v1\"");
        let back: RuleId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn rule_id_deserialize_rejects_invalid() {
        let err: Result<RuleId, _> = serde_json::from_str("\"Invalid\"");
        assert!(err.is_err());
    }

    // --- Mode / Severity ----------------------------------------------------

    #[test]
    fn mode_default_is_warn() {
        assert_eq!(Mode::default(), Mode::Warn);
    }

    #[test]
    fn mode_off_yields_no_severity() {
        assert!(Mode::Off.violation_severity().is_none());
    }

    #[test]
    fn mode_violation_severity_round_trips_other_modes() {
        assert_eq!(Mode::Strict.violation_severity(), Some(Severity::Strict));
        assert_eq!(Mode::Warn.violation_severity(), Some(Severity::Warn));
        assert_eq!(Mode::Hint.violation_severity(), Some(Severity::Hint));
    }

    #[test]
    fn severity_fails_ci_matrix() {
        assert!(Severity::Strict.fails_ci());
        assert!(Severity::EvaluationError.fails_ci());
        assert!(!Severity::Warn.fails_ci());
        assert!(!Severity::Hint.fails_ci());
    }

    #[test]
    fn charset_default_is_utf8() {
        assert_eq!(Charset::default(), Charset::Utf8);
    }

    #[test]
    fn casing_default_is_any() {
        assert_eq!(Casing::default(), Casing::Any);
    }

    #[test]
    fn charset_serde_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&Charset::NoCjk).unwrap(),
            "\"no_cjk\""
        );
        let parsed: Charset = serde_json::from_str("\"ascii\"").unwrap();
        assert_eq!(parsed, Charset::Ascii);
    }

    // --- SpecificityScore ordering -----------------------------------------

    #[test]
    fn specificity_orders_by_segments_then_chars() {
        let small = SpecificityScore {
            literal_segments: 1,
            literal_chars: 100,
        };
        let bigger_segments = SpecificityScore {
            literal_segments: 2,
            literal_chars: 10,
        };
        assert!(bigger_segments > small);

        let same_segments_more_chars = SpecificityScore {
            literal_segments: 1,
            literal_chars: 101,
        };
        assert!(same_segments_more_chars > small);
    }

    // --- RuleSource / Decision serde ---------------------------------------

    #[test]
    fn rule_source_serializes_with_layer_tag() {
        let own = serde_json::to_string(&RuleSource::Own).unwrap();
        assert_eq!(own, r#"{"layer":"own"}"#);

        let inherited = serde_json::to_string(&RuleSource::Inherited { distance: 2 }).unwrap();
        assert_eq!(inherited, r#"{"layer":"inherited","distance":2}"#);

        let project_wide = serde_json::to_string(&RuleSource::ProjectWide).unwrap();
        assert_eq!(project_wide, r#"{"layer":"project_wide"}"#);
    }

    #[test]
    fn decision_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Decision::Winner).unwrap(),
            "\"winner\""
        );
        assert_eq!(
            serde_json::to_string(&Decision::Shadowed).unwrap(),
            "\"shadowed\""
        );
        assert_eq!(
            serde_json::to_string(&Decision::NotApplicable).unwrap(),
            "\"not_applicable\""
        );
    }

    // --- Violation round-trip ----------------------------------------------

    #[test]
    fn violation_round_trips_via_yaml() {
        let v = Violation {
            file_id: None,
            path: ProjectPath::new("assets/shots/ch010/ch010_bg_forest_v03.psd").unwrap(),
            rule_id: "shot-assets-v1".parse().unwrap(),
            category: Category::Naming,
            kind: RuleKind::Template,
            severity: Severity::Warn,
            reason: "missing seq segment".into(),
            trace: vec![RuleHit {
                rule_id: "shot-assets-v1".parse().unwrap(),
                kind: RuleKind::Template,
                source: RuleSource::ProjectWide,
                decision: Decision::Winner,
                specificity_score: SpecificityScore {
                    literal_segments: 2,
                    literal_chars: 12,
                },
                explanation: "only template rule that matched".into(),
            }],
            suggested_names: vec!["ch010_001_bg_forest_v03.psd".into()],
            placement_details: None,
        };

        let yaml = serde_yaml::to_string(&v).unwrap();
        let back: Violation = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, v);
    }
}
