//! TOML schema for sidecar `.meta` files.
//!
//! Every tracked file has a sibling sidecar (`foo.psd` → `foo.psd.meta`) that
//! serves as the single source of truth for identity, tags, notes, and custom
//! fields. The `SQLite` index is a rebuildable cache derived from these files;
//! when they disagree, `.meta` wins.
//!
//! The schema is versioned via [`SCHEMA_VERSION`]. Unknown top-level keys and
//! unknown keys within known sections are preserved verbatim so that a future
//! Progest release can add fields without older installs stripping them on
//! save — important because `.meta` files live in git and travel between
//! teammates who may upgrade at different times.
//!
//! See `docs/REQUIREMENTS.md` §3.2 for the authoritative schema.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml::Table;
use toml::value::Datetime;

use crate::identity::{FileId, Fingerprint};

/// Current schema version written to fresh `.meta` files.
///
/// Bump only when the on-disk layout changes in a non-backward-compatible
/// way. Additive fields should be handled via the preserved `extra` tables
/// without a version bump.
pub const SCHEMA_VERSION: u32 = 1;

/// Errors surfaced by [`MetaDocument::from_toml_str`] / [`MetaDocument::to_toml_string`].
#[derive(Debug, Error)]
pub enum MetaError {
    #[error("failed to parse .meta TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize .meta TOML: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("unsupported schema_version {found}; this build understands {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
}

/// Parsed representation of a `.meta` sidecar file.
///
/// Fields mirror `docs/REQUIREMENTS.md` §3.2. The `extra` table captures any
/// top-level keys not known to this build so that load → save round-trips
/// without data loss.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetaDocument {
    /// Schema version written to disk. Validated against [`SCHEMA_VERSION`] on load.
    pub schema_version: u32,

    /// Stable per-file identifier. Required.
    pub file_id: FileId,

    /// Content fingerprint (blake3, 128-bit truncated). Required.
    pub content_fingerprint: Fingerprint,

    /// When the file is a copy of another tracked file, the source's `file_id`.
    ///
    /// Serialized as an empty string when absent to match the on-disk
    /// convention in the requirements doc — keeping the key present signals
    /// "this file has been considered for a copy relationship" rather than
    /// "this build didn't know about the key".
    #[serde(default, with = "source_file_id_serde")]
    pub source_file_id: Option<FileId>,

    /// Creation timestamp as recorded by the tool that first wrote this file.
    ///
    /// Stored as TOML's native datetime so both RFC 3339 timestamps and
    /// local-datetime values round-trip without a lossy string conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Datetime>,

    /// `[core]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<CoreSection>,

    /// `[naming]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingSection>,

    /// `[tags]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<TagsSection>,

    /// `[notes]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<NotesSection>,

    /// `[custom]` section — free-form user-defined fields.
    ///
    /// Typed schemas for custom fields arrive with the rules engine in M2;
    /// until then we preserve whatever the user or another tool has written.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub custom: Table,

    /// `[meta_internal]` section — local-only state, excluded from git merges.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub meta_internal: Table,

    /// Unknown top-level keys preserved verbatim for forward compatibility.
    #[serde(flatten)]
    pub extra: Table,
}

impl MetaDocument {
    /// Construct a minimal document for a freshly tracked file.
    ///
    /// Only the required identity fields are set; every other section is left
    /// empty so that callers can populate them incrementally as features
    /// (rules, tags, notes) become available.
    #[must_use]
    pub fn new(file_id: FileId, content_fingerprint: Fingerprint) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            file_id,
            content_fingerprint,
            source_file_id: None,
            created_at: None,
            core: None,
            naming: None,
            tags: None,
            notes: None,
            custom: Table::new(),
            meta_internal: Table::new(),
            extra: Table::new(),
        }
    }

    /// Parse a `.meta` document from its TOML text.
    ///
    /// Rejects documents whose `schema_version` differs from
    /// [`SCHEMA_VERSION`] so that callers don't silently drop fields added
    /// in a future format revision.
    pub fn from_toml_str(input: &str) -> Result<Self, MetaError> {
        let doc: Self = toml::from_str(input)?;
        if doc.schema_version != SCHEMA_VERSION {
            return Err(MetaError::UnsupportedVersion {
                found: doc.schema_version,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(doc)
    }

    /// Serialize this document to TOML.
    pub fn to_toml_string(&self) -> Result<String, MetaError> {
        Ok(toml::to_string(self)?)
    }
}

/// `[core]` section — coarse classification of the tracked item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreSection {
    pub kind: Kind,
    pub status: Status,
    #[serde(flatten)]
    pub extra: Table,
}

/// What kind of item the sidecar describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Asset,
    Directory,
    Derived,
}

/// Lifecycle status of the tracked item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Active,
    Archived,
    Deprecated,
}

/// `[naming]` section — last rule evaluation result, used for lint caching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamingSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_validated_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_validated_at: Option<Datetime>,
    #[serde(flatten)]
    pub extra: Table,
}

/// `[tags]` section — a sorted, deduplicated set of user-defined tags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagsSection {
    #[serde(default)]
    pub list: Vec<String>,
    #[serde(flatten)]
    pub extra: Table,
}

impl TagsSection {
    /// Normalize [`Self::list`] to a sorted, deduplicated set.
    ///
    /// Callers should invoke this before saving so that git diffs remain
    /// stable across independent edits (requirement §3.2: "ソート済み集合").
    pub fn normalize(&mut self) {
        self.list.sort();
        self.list.dedup();
    }
}

/// `[notes]` section — free-form markdown-ish user notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotesSection {
    #[serde(default)]
    pub body: String,
    #[serde(flatten)]
    pub extra: Table,
}

mod source_file_id_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::identity::FileId;

    // serde's `with` contract requires `serialize(value: &T, ...)` where
    // `T = Option<FileId>`, so we can't take `Option<&FileId>` here.
    #[allow(clippy::ref_option)]
    pub fn serialize<S: Serializer>(value: &Option<FileId>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(id) => s.serialize_str(&id.to_string()),
            None => s.serialize_str(""),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<FileId>, D::Error> {
        let raw = String::deserialize(d)?;
        if raw.is_empty() {
            Ok(None)
        } else {
            raw.parse::<FileId>()
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"
source_file_id = ""
created_at = 2026-04-20T10:00:00Z

[core]
kind = "asset"
status = "active"

[naming]
rule_id = "shot-assets-v1"
last_validated_name = "ch010_sc020_bg_forest_v03.psd"
last_validated_at = 2026-04-20T10:00:00Z

[tags]
list = ["approved", "forest", "night"]

[notes]
body = """
差し替え候補あり
"""

[custom]
scene = 20
shot = 10
asset_type = "bg"

[meta_internal]
last_seen_at = 2026-04-20T10:15:33Z
"#;

    #[test]
    fn parses_full_sample_schema() {
        let doc = MetaDocument::from_toml_str(SAMPLE).unwrap();

        assert_eq!(doc.schema_version, SCHEMA_VERSION);
        assert_eq!(
            doc.file_id.to_string(),
            "0190f3d7-5dbc-7abc-8000-0123456789ab"
        );
        assert_eq!(
            doc.content_fingerprint.to_string(),
            "blake3:00112233445566778899aabbccddeeff"
        );
        assert!(doc.source_file_id.is_none());

        let core = doc.core.as_ref().expect("core section");
        assert_eq!(core.kind, Kind::Asset);
        assert_eq!(core.status, Status::Active);

        let tags = doc.tags.as_ref().expect("tags section");
        assert_eq!(tags.list, vec!["approved", "forest", "night"]);

        let custom = &doc.custom;
        assert_eq!(
            custom.get("scene").and_then(toml::Value::as_integer),
            Some(20)
        );
        assert_eq!(
            custom.get("asset_type").and_then(toml::Value::as_str),
            Some("bg")
        );
    }

    #[test]
    fn round_trips_sample_schema() {
        let first = MetaDocument::from_toml_str(SAMPLE).unwrap();
        let rendered = first.to_toml_string().unwrap();
        let second = MetaDocument::from_toml_str(&rendered).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn preserves_unknown_top_level_keys() {
        let input = r#"
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"
future_field = "from-tomorrow"

[future_section]
hello = "world"
"#;
        let doc = MetaDocument::from_toml_str(input).unwrap();
        assert_eq!(
            doc.extra.get("future_field").and_then(toml::Value::as_str),
            Some("from-tomorrow")
        );
        let rendered = doc.to_toml_string().unwrap();
        assert!(rendered.contains("future_field = \"from-tomorrow\""));
        assert!(rendered.contains("[future_section]"));
        assert!(rendered.contains("hello = \"world\""));
    }

    #[test]
    fn preserves_unknown_keys_within_known_sections() {
        let input = r#"
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"

[core]
kind = "asset"
status = "active"
future_core_field = 42
"#;
        let doc = MetaDocument::from_toml_str(input).unwrap();
        let core = doc.core.as_ref().unwrap();
        assert_eq!(
            core.extra
                .get("future_core_field")
                .and_then(toml::Value::as_integer),
            Some(42)
        );

        let rendered = doc.to_toml_string().unwrap();
        assert!(rendered.contains("future_core_field = 42"));
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let input = r#"
schema_version = 99
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"
"#;
        let err = MetaDocument::from_toml_str(input).unwrap_err();
        assert!(matches!(
            err,
            MetaError::UnsupportedVersion {
                found: 99,
                expected: SCHEMA_VERSION,
            }
        ));
    }

    #[test]
    fn missing_file_id_is_a_parse_error() {
        let input = r#"
schema_version = 1
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"
"#;
        let err = MetaDocument::from_toml_str(input).unwrap_err();
        assert!(matches!(err, MetaError::Parse(_)));
    }

    #[test]
    fn missing_fingerprint_is_a_parse_error() {
        let input = r#"
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
"#;
        let err = MetaDocument::from_toml_str(input).unwrap_err();
        assert!(matches!(err, MetaError::Parse(_)));
    }

    #[test]
    fn source_file_id_roundtrips_as_empty_string_when_absent() {
        let mut doc = MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff".parse().unwrap(),
        );
        doc.source_file_id = None;
        let rendered = doc.to_toml_string().unwrap();
        assert!(
            rendered.contains("source_file_id = \"\""),
            "expected explicit empty-string source_file_id in:\n{rendered}"
        );
    }

    #[test]
    fn source_file_id_roundtrips_when_set() {
        let src = FileId::new_v7();
        let mut doc = MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff".parse().unwrap(),
        );
        doc.source_file_id = Some(src);
        let rendered = doc.to_toml_string().unwrap();
        let reparsed = MetaDocument::from_toml_str(&rendered).unwrap();
        assert_eq!(reparsed.source_file_id, Some(src));
    }

    #[test]
    fn new_produces_minimal_serializable_document() {
        let doc = MetaDocument::new(
            FileId::new_v7(),
            "blake3:00112233445566778899aabbccddeeff".parse().unwrap(),
        );
        let rendered = doc.to_toml_string().unwrap();
        let reparsed = MetaDocument::from_toml_str(&rendered).unwrap();
        assert_eq!(doc, reparsed);
    }

    #[test]
    fn tags_normalize_sorts_and_dedupes() {
        let mut tags = TagsSection {
            list: vec!["z".into(), "a".into(), "m".into(), "a".into(), "z".into()],
            extra: Table::new(),
        };
        tags.normalize();
        assert_eq!(tags.list, vec!["a", "m", "z"]);
    }

    #[test]
    fn kind_rejects_unknown_variant() {
        let input = r#"
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
content_fingerprint = "blake3:00112233445566778899aabbccddeeff"

[core]
kind = "mystery"
status = "active"
"#;
        let err = MetaDocument::from_toml_str(input).unwrap_err();
        assert!(matches!(err, MetaError::Parse(_)));
    }
}
