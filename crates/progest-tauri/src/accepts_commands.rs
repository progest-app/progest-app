//! IPC for the directory inspector's `[accepts]` editor.
//!
//! `accepts_read` returns the dir's own `[accepts]` (if any), the
//! computed `effective_accepts` (own ∪ inherited chain when the dir
//! opted in), the upward chain of ancestors that actually carry an
//! `[accepts]` block, and the project's alias catalog so the
//! frontend can render the alias picker without a second round-trip.
//!
//! `accepts_write` validates the payload (mode, alias names, alias
//! resolvability) and round-trips through `core::accepts::writer` +
//! `core::meta::dirmeta` so unrelated keys in the dirmeta — `tags`,
//! `notes`, `custom`, etc. — survive the edit.
//!
//! Wire shapes are tagged-union JSON: `AcceptsTokenWire` uses
//! `{"type": "alias", "name": "image"}` /
//! `{"type": "ext", "value": "psd"}` so the React side can pattern
//! match on a discriminator. `EffectiveExtWire.source` is `"own"` or
//! `"inherited"` so the inspector can color-code provenance.

use progest_core::accepts::{
    AcceptsToken, AliasCatalog, EffectiveAccepts, RawAccepts, ResolveError,
    compute_effective_accepts, expand_own_accepts, extract_accepts, inject_accepts,
    is_valid_alias_name, normalize_ext, remove_accepts,
};
use progest_core::fs::ProjectPath;
use progest_core::meta::{load_dirmeta, save_dirmeta};
use progest_core::rules::{AcceptsSource, Mode};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{load_alias_catalog_for_ctx, no_project_error};
use crate::state::AppState;

/// One accepts token on the wire. Tagged-union for unambiguous
/// pattern matching on the JS side.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AcceptsTokenWire {
    /// `:<name>` reference. Stored without the leading colon, same as
    /// [`AcceptsToken::Alias`].
    Alias { name: String },
    /// Literal extension. Empty string means "no extension"
    /// ([`progest_core::accepts::EXT_NONE`]).
    Ext { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAcceptsWire {
    pub inherit: bool,
    pub exts: Vec<AcceptsTokenWire>,
    /// `"strict"` / `"warn"` / `"hint"` / `"off"`.
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveExtWire {
    /// Normalized extension; `""` for the no-extension sentinel.
    pub ext: String,
    /// `"own"` or `"inherited"`.
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveAcceptsWire {
    pub exts: Vec<EffectiveExtWire>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainEntryWire {
    /// Project-relative ancestor path. `""` for the project root.
    pub dir: String,
    pub accepts: RawAcceptsWire,
}

#[derive(Debug, Clone, Serialize)]
pub struct AliasEntryWire {
    pub name: String,
    pub exts: Vec<String>,
    pub builtin: bool,
}

/// Response shape for `accepts_read`.
#[derive(Debug, Clone, Serialize)]
pub struct AcceptsReadResponse {
    /// Echo of the (normalized) dir path the inspector queried.
    pub dir: String,
    /// `Some(_)` only when the dir's own dirmeta carries an
    /// `[accepts]` block. `None` ⇒ "no placement constraint" per
    /// REQUIREMENTS.md §3.13.1.
    pub own: Option<RawAcceptsWire>,
    /// Computed `effective_accepts`. Mirrors `core::accepts::EffectiveAccepts`
    /// — `None` when there's no own block (inheritance can't fire
    /// without a child opting in via `inherit = true`).
    pub effective: Option<EffectiveAcceptsWire>,
    /// Ancestors that carry their own `[accepts]` block, parent-first
    /// just like `compute_effective_accepts`'s `chain` argument.
    pub chain: Vec<ChainEntryWire>,
    /// Project alias catalog — builtin entries plus anything in
    /// `.progest/schema.toml [alias]`. Sorted by name for stable UI.
    pub aliases: Vec<AliasEntryWire>,
    /// Non-fatal warnings from the loader (unknown future keys, etc.).
    pub warnings: Vec<String>,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn accepts_read(
    dir: String,
    state: State<'_, AppState>,
) -> Result<AcceptsReadResponse, String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;

    let project_path = parse_dir_path(&dir)?;

    // Own `[accepts]`.
    let own_doc = load_dirmeta(&ctx.fs, &project_path).map_err(|e| format!("load dirmeta: {e}"))?;
    let mut warnings: Vec<String> = Vec::new();
    let own = if let Some(doc) = &own_doc {
        match extract_accepts(&doc.extra) {
            Ok(Some(extraction)) => {
                for w in &extraction.warnings {
                    warnings.push(format_accepts_warning(w));
                }
                Some(extraction.accepts)
            }
            Ok(None) => None,
            Err(e) => return Err(format!("parse own [accepts]: {e}")),
        }
    } else {
        None
    };

    // Walk the chain of ancestors, parent-first.
    let mut chain_raws: Vec<(ProjectPath, RawAccepts)> = Vec::new();
    let mut cursor = project_path.parent();
    while let Some(ancestor) = cursor {
        if let Some(doc) = load_dirmeta(&ctx.fs, &ancestor)
            .map_err(|e| format!("load dirmeta `{ancestor}`: {e}"))?
        {
            match extract_accepts(&doc.extra) {
                Ok(Some(extraction)) => {
                    chain_raws.push((ancestor.clone(), extraction.accepts));
                }
                Ok(None) => {}
                Err(e) => return Err(format!("parse ancestor [accepts] `{ancestor}`: {e}")),
            }
        }
        cursor = ancestor.parent();
    }

    let catalog = load_alias_catalog_for_ctx(ctx);
    let chain_refs: Vec<&RawAccepts> = chain_raws.iter().map(|(_, r)| r).collect();
    let effective =
        compute_effective_accepts(own.as_ref(), &chain_refs, &catalog).map_err(|e| match e {
            ResolveError::UnknownAlias(name) => {
                format!("unknown alias `:{name}` referenced in [accepts]")
            }
        })?;

    Ok(AcceptsReadResponse {
        dir: project_path_to_wire(&project_path),
        own: own.as_ref().map(raw_to_wire),
        effective: effective.map(eff_to_wire),
        chain: chain_raws
            .into_iter()
            .map(|(p, r)| ChainEntryWire {
                dir: project_path_to_wire(&p),
                accepts: raw_to_wire(&r),
            })
            .collect(),
        aliases: catalog_to_wire(&catalog),
        warnings,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn accepts_write(
    dir: String,
    accepts: Option<RawAcceptsWire>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;

    let project_path = parse_dir_path(&dir)?;
    let mut doc = load_dirmeta(&ctx.fs, &project_path)
        .map_err(|e| format!("load dirmeta: {e}"))?
        .unwrap_or_default();

    match accepts {
        Some(wire) => {
            let raw = raw_from_wire(wire)?;
            // Catch unknown-alias references before we hit disk so the
            // inspector can show the error inline instead of writing a
            // file that immediately fails the next lint pass.
            let catalog = load_alias_catalog_for_ctx(ctx);
            expand_own_accepts(&raw, &catalog).map_err(|e| match e {
                ResolveError::UnknownAlias(name) => {
                    format!(
                        "unknown alias `:{name}` — define it under `.progest/schema.toml [alias]`"
                    )
                }
            })?;
            inject_accepts(&mut doc.extra, &raw);
        }
        None => remove_accepts(&mut doc.extra),
    }

    save_dirmeta(&ctx.fs, &project_path, &doc).map_err(|e| format!("save dirmeta: {e}"))?;
    Ok(())
}

// --- helpers ---------------------------------------------------------------

fn parse_dir_path(dir: &str) -> Result<ProjectPath, String> {
    let rel = dir.trim_matches('/');
    let rel = if rel == "." { "" } else { rel };
    if rel.is_empty() {
        return Ok(ProjectPath::root());
    }
    ProjectPath::new(rel).map_err(|e| format!("path `{rel}`: {e}"))
}

fn project_path_to_wire(p: &ProjectPath) -> String {
    if p.is_root() {
        String::new()
    } else {
        p.as_str().to_string()
    }
}

fn raw_to_wire(r: &RawAccepts) -> RawAcceptsWire {
    RawAcceptsWire {
        inherit: r.inherit,
        exts: r.exts.iter().map(token_to_wire).collect(),
        mode: mode_str(r.mode).to_string(),
    }
}

fn token_to_wire(t: &AcceptsToken) -> AcceptsTokenWire {
    match t {
        AcceptsToken::Alias(name) => AcceptsTokenWire::Alias { name: name.clone() },
        AcceptsToken::Ext(ext) => AcceptsTokenWire::Ext {
            value: ext.as_str().to_string(),
        },
    }
}

fn raw_from_wire(w: RawAcceptsWire) -> Result<RawAccepts, String> {
    let mode = match w.mode.as_str() {
        "strict" => Mode::Strict,
        "warn" => Mode::Warn,
        "hint" => Mode::Hint,
        "off" => Mode::Off,
        other => {
            return Err(format!(
                "invalid mode `{other}` (expected strict/warn/hint/off)"
            ));
        }
    };
    let exts = w
        .exts
        .into_iter()
        .map(token_from_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RawAccepts {
        inherit: w.inherit,
        exts,
        mode,
    })
}

fn token_from_wire(w: AcceptsTokenWire) -> Result<AcceptsToken, String> {
    match w {
        AcceptsTokenWire::Alias { name } => {
            if !is_valid_alias_name(&name) {
                return Err(format!(
                    "invalid alias name `:{name}` (must match `^[a-z][a-z0-9_-]*$`, ≤64 chars)"
                ));
            }
            Ok(AcceptsToken::Alias(name))
        }
        AcceptsTokenWire::Ext { value } => Ok(AcceptsToken::Ext(normalize_ext(&value))),
    }
}

fn eff_to_wire(eff: EffectiveAccepts) -> EffectiveAcceptsWire {
    EffectiveAcceptsWire {
        exts: eff
            .exts
            .into_iter()
            .map(|(ext, source)| EffectiveExtWire {
                ext: ext.into_string(),
                source: match source {
                    AcceptsSource::Own => "own",
                    AcceptsSource::Inherited => "inherited",
                }
                .to_string(),
            })
            .collect(),
        mode: mode_str(eff.mode).to_string(),
    }
}

fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Strict => "strict",
        Mode::Warn => "warn",
        Mode::Hint => "hint",
        Mode::Off => "off",
    }
}

fn catalog_to_wire(catalog: &AliasCatalog) -> Vec<AliasEntryWire> {
    let builtin_set: std::collections::HashSet<&str> = progest_core::accepts::BUILTIN_ALIASES
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let mut out: Vec<AliasEntryWire> = catalog
        .iter()
        .map(|(name, exts)| AliasEntryWire {
            name: name.to_string(),
            exts: exts.iter().map(|e| e.as_str().to_string()).collect(),
            builtin: builtin_set.contains(name),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn format_accepts_warning(w: &progest_core::accepts::AcceptsWarning) -> String {
    match w {
        progest_core::accepts::AcceptsWarning::UnknownKey { key } => {
            format!("unknown key `{key}` in [accepts] (preserved verbatim for forward-compat)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dir_path_root_forms_normalize_to_root() {
        for raw in ["", "/", ".", "./", "/."] {
            assert!(
                parse_dir_path(raw).unwrap().is_root(),
                "expected `{raw}` to normalize to project root",
            );
        }
    }

    #[test]
    fn parse_dir_path_strips_leading_and_trailing_slashes() {
        let p = parse_dir_path("/assets/images/").unwrap();
        assert_eq!(p.as_str(), "assets/images");
    }

    #[test]
    fn raw_from_wire_rejects_unknown_mode() {
        let wire = RawAcceptsWire {
            inherit: false,
            exts: Vec::new(),
            mode: "maybe".into(),
        };
        let err = raw_from_wire(wire).unwrap_err();
        assert!(err.contains("invalid mode"));
    }

    #[test]
    fn raw_from_wire_rejects_invalid_alias_name() {
        let wire = RawAcceptsWire {
            inherit: false,
            exts: vec![AcceptsTokenWire::Alias {
                name: "Image".into(),
            }],
            mode: "warn".into(),
        };
        let err = raw_from_wire(wire).unwrap_err();
        assert!(err.contains("invalid alias name"));
    }

    #[test]
    fn raw_from_wire_normalizes_extension_tokens() {
        let wire = RawAcceptsWire {
            inherit: false,
            exts: vec![AcceptsTokenWire::Ext {
                value: ".PSD".into(),
            }],
            mode: "warn".into(),
        };
        let raw = raw_from_wire(wire).unwrap();
        assert_eq!(raw.exts.len(), 1);
        match &raw.exts[0] {
            AcceptsToken::Ext(e) => assert_eq!(e.as_str(), "psd"),
            other => panic!("expected ext token, got {other:?}"),
        }
    }

    #[test]
    fn raw_round_trip_through_wire_preserves_payload() {
        let original = RawAccepts {
            inherit: true,
            exts: vec![
                AcceptsToken::Alias("image".into()),
                AcceptsToken::Ext(normalize_ext(".psd")),
                AcceptsToken::Ext(normalize_ext("")),
            ],
            mode: Mode::Strict,
        };
        let wire = raw_to_wire(&original);
        let back = raw_from_wire(wire).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn catalog_to_wire_marks_builtin_entries() {
        let catalog = AliasCatalog::builtin();
        let wire = catalog_to_wire(&catalog);
        for entry in &wire {
            assert!(
                entry.builtin,
                "builtin catalog should mark `{}` as builtin",
                entry.name
            );
        }
        // Sorted by name for stable UI.
        let names: Vec<&str> = wire.iter().map(|e| e.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }
}
