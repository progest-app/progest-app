//! Parse `.progest/schema.toml` into an alias catalog for accepts
//! resolution.
//!
//! `[alias.<name>]` entries are layered on top of [`BUILTIN_ALIASES`]
//! per REQUIREMENTS.md §3.13.3 and `docs/ACCEPTS_ALIASES.md` §3. The
//! catalog returned by [`load_alias_catalog`] is the authoritative
//! lookup table the effective-accepts pass uses to expand alias
//! references.
//!
//! Validation is strict per `ACCEPTS_ALIASES.md` §3.1: unknown alias
//! names fail schema load, nested alias references (`":other"`
//! inside `[alias.x].exts`) fail schema load, extensions missing the
//! leading dot fail schema load, and empty `exts` arrays fail schema
//! load. The only non-error warning is overriding a builtin.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use toml::{Table, Value};

use super::types::{BUILTIN_ALIASES, Ext, builtin_alias, is_valid_alias_name, normalize_ext};

/// Resolved alias catalog. Keys are alias names without the `:`
/// prefix; values are the normalized extension set (lowercase, no
/// leading dot). Builtin aliases are included by default; project
/// aliases either add new names or override builtins via full
/// replace.
#[derive(Debug, Clone, Default)]
pub struct AliasCatalog {
    entries: BTreeMap<String, Vec<Ext>>,
}

impl AliasCatalog {
    /// Start from the builtin set only, with no project overrides.
    #[must_use]
    pub fn builtin() -> Self {
        let mut entries = BTreeMap::new();
        for (name, exts) in BUILTIN_ALIASES {
            entries.insert(
                (*name).to_owned(),
                exts.iter().map(|e| normalize_ext(e)).collect(),
            );
        }
        Self { entries }
    }

    /// Return the expansion of `:<name>`, or `None` if the alias is
    /// undefined.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&[Ext]> {
        self.entries.get(name).map(Vec::as_slice)
    }

    /// Iterate over every `(alias_name, extensions)` pair in
    /// deterministic order — used by the `accepts doctor` surface
    /// that lists every resolvable alias.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[Ext])> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Number of aliases currently in the catalog.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Warning emitted while loading `.progest/schema.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaWarning {
    /// A project `[alias.<name>]` entry shadowed a builtin alias.
    /// Override is full-replace per `ACCEPTS_ALIASES.md` §3.1.
    BuiltinAliasOverridden { name: String },
}

/// Fatal error while loading `.progest/schema.toml`.
#[derive(Debug, Error)]
pub enum SchemaLoadError {
    #[error("failed to parse schema.toml: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("`[alias]` must be a TOML table, got {ty}")]
    AliasNotATable { ty: &'static str },

    #[error("`[alias.{name}]` must be a TOML table, got {ty}")]
    AliasEntryNotATable { name: String, ty: &'static str },

    #[error("invalid alias name `{name}` in `[alias.<name>]`")]
    InvalidAliasName { name: String },

    #[error("alias `{name}` is missing the required `exts` array")]
    MissingExts { name: String },

    #[error("`[alias.{name}].exts` must be a non-empty array, got {detail}")]
    BadExts { name: String, detail: String },

    /// `ACCEPTS_ALIASES.md` §3.1: nested alias references are rejected
    /// at schema-load time so downstream resolution never has to
    /// recurse.
    #[error("`[alias.{alias}].exts` cannot reference another alias `:{referenced}`")]
    NestedAliasReference { alias: String, referenced: String },

    /// `ACCEPTS_ALIASES.md` §3.1: ext tokens inside alias definitions
    /// must use the `.ext` form.
    #[error("`[alias.{alias}].exts` token `{token}` must start with `.` or be `\"\"`")]
    MalformedExt { alias: String, token: String },
}

/// Result of a successful schema load.
#[derive(Debug, Clone)]
pub struct SchemaLoad {
    pub catalog: AliasCatalog,
    pub warnings: Vec<SchemaWarning>,
}

/// Parse `.progest/schema.toml` content into an alias catalog,
/// layered on top of the builtin set.
///
/// # Errors
///
/// Returns [`SchemaLoadError`] for malformed TOML or schema
/// violations per `ACCEPTS_ALIASES.md` §3.1.
pub fn load_alias_catalog(input: &str) -> Result<SchemaLoad, SchemaLoadError> {
    let root: Table = toml::from_str(input)?;
    load_alias_catalog_from_table(&root)
}

/// Same as [`load_alias_catalog`] but starts from an already-parsed
/// table. Useful when `.progest/schema.toml` is carrying other
/// sections (e.g. `[extension_compounds]`) the caller wants to pull
/// out itself.
///
/// # Errors
///
/// Returns [`SchemaLoadError`] per the same validation rules as
/// [`load_alias_catalog`].
pub fn load_alias_catalog_from_table(root: &Table) -> Result<SchemaLoad, SchemaLoadError> {
    let mut catalog = AliasCatalog::builtin();
    let mut warnings = Vec::new();

    let Some(alias_value) = root.get("alias") else {
        return Ok(SchemaLoad { catalog, warnings });
    };

    let Value::Table(alias_table) = alias_value else {
        return Err(SchemaLoadError::AliasNotATable {
            ty: alias_value.type_str(),
        });
    };

    for (name, entry) in alias_table {
        if !is_valid_alias_name(name) {
            return Err(SchemaLoadError::InvalidAliasName { name: name.clone() });
        }

        let Value::Table(entry_table) = entry else {
            return Err(SchemaLoadError::AliasEntryNotATable {
                name: name.clone(),
                ty: entry.type_str(),
            });
        };

        let exts_value = entry_table
            .get("exts")
            .ok_or_else(|| SchemaLoadError::MissingExts { name: name.clone() })?;
        let Value::Array(items) = exts_value else {
            return Err(SchemaLoadError::BadExts {
                name: name.clone(),
                detail: format!("expected array, got {}", exts_value.type_str()),
            });
        };
        if items.is_empty() {
            return Err(SchemaLoadError::BadExts {
                name: name.clone(),
                detail: "array must be non-empty".into(),
            });
        }

        let mut exts: Vec<Ext> = Vec::with_capacity(items.len());
        for item in items {
            let Value::String(raw) = item else {
                return Err(SchemaLoadError::BadExts {
                    name: name.clone(),
                    detail: format!("entry must be a string, got {}", item.type_str()),
                });
            };

            if let Some(referenced) = raw.strip_prefix(':') {
                return Err(SchemaLoadError::NestedAliasReference {
                    alias: name.clone(),
                    referenced: referenced.to_owned(),
                });
            }
            if raw.is_empty() {
                exts.push(normalize_ext(""));
                continue;
            }
            if !raw.starts_with('.') {
                return Err(SchemaLoadError::MalformedExt {
                    alias: name.clone(),
                    token: raw.clone(),
                });
            }
            exts.push(normalize_ext(raw));
        }

        // Dedup within the single alias definition (order-preserving).
        let mut seen: BTreeSet<String> = BTreeSet::new();
        exts.retain(|e| seen.insert(e.as_str().to_owned()));

        if builtin_alias(name).is_some() {
            warnings.push(SchemaWarning::BuiltinAliasOverridden { name: name.clone() });
        }
        catalog.entries.insert(name.clone(), exts);
    }

    Ok(SchemaLoad { catalog, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_schema_returns_builtin_catalog() {
        let load = load_alias_catalog("").unwrap();
        assert!(load.warnings.is_empty());
        // All seven builtin aliases reachable.
        for name in ["image", "video", "audio", "raw", "3d", "project", "text"] {
            assert!(
                load.catalog.lookup(name).is_some(),
                "builtin `{name}` missing"
            );
        }
    }

    #[test]
    fn project_alias_adds_new_entry() {
        let load = load_alias_catalog(
            r#"
[alias.studio_3d]
exts = [".fbx", ".usd", ".usda"]
"#,
        )
        .unwrap();

        let exts = load.catalog.lookup("studio_3d").unwrap();
        assert_eq!(exts.len(), 3);
        assert_eq!(exts[0].as_str(), "fbx");
        assert!(load.warnings.is_empty());
    }

    #[test]
    fn project_alias_overrides_builtin_with_warning() {
        let load = load_alias_catalog(
            r#"
[alias.image]
exts = [".jpg", ".jpeg", ".png", ".webp"]
"#,
        )
        .unwrap();

        let exts = load.catalog.lookup("image").unwrap();
        assert_eq!(exts.len(), 4, "override must be full replace, not union");
        assert_eq!(
            load.warnings,
            vec![SchemaWarning::BuiltinAliasOverridden {
                name: "image".into()
            }]
        );
    }

    #[test]
    fn nested_alias_reference_is_rejected() {
        let err = load_alias_catalog(
            r#"
[alias.bundle]
exts = [":image", ".extra"]
"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SchemaLoadError::NestedAliasReference { ref alias, ref referenced }
                if alias == "bundle" && referenced == "image"
        ));
    }

    #[test]
    fn ext_without_leading_dot_is_rejected() {
        let err = load_alias_catalog(
            r#"
[alias.bad]
exts = ["fbx"]
"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SchemaLoadError::MalformedExt { ref alias, ref token }
                if alias == "bad" && token == "fbx"
        ));
    }

    #[test]
    fn empty_exts_array_is_rejected() {
        let err = load_alias_catalog(
            r"
[alias.nothing]
exts = []
",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SchemaLoadError::BadExts { ref name, .. } if name == "nothing"
        ));
    }

    #[test]
    fn invalid_alias_name_is_rejected() {
        let err = load_alias_catalog(
            r#"
[alias."Bad-Name"]
exts = [".fbx"]
"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SchemaLoadError::InvalidAliasName { ref name } if name == "Bad-Name"
        ));
    }

    #[test]
    fn duplicate_entries_inside_alias_are_deduped() {
        let load = load_alias_catalog(
            r#"
[alias.dedup]
exts = [".psd", ".PSD", ".tif"]
"#,
        )
        .unwrap();
        let exts = load.catalog.lookup("dedup").unwrap();
        assert_eq!(exts.len(), 2);
    }
}
