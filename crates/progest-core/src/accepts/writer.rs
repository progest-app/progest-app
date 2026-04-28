//! Serialize a [`RawAccepts`] back into the dirmeta `extra` table.
//!
//! The reader side ([`super::loader::extract_accepts`]) pulls
//! `[accepts]` out of a `.dirmeta.toml`'s raw `extra` table and
//! validates per REQUIREMENTS.md §3.13. The writer is its inverse:
//! the GUI's directory inspector edits a [`RawAccepts`] and writes
//! it back through the same `extra` table so the dirmeta document
//! round-trips losslessly.
//!
//! Token grammar matches `docs/ACCEPTS_ALIASES.md` §3.1:
//!
//! - [`AcceptsToken::Alias`] → `":<name>"`
//! - [`AcceptsToken::Ext`] with a non-empty ext → `".<ext>"`
//! - [`AcceptsToken::Ext`] with [`EXT_NONE`] → `""`
//!
//! Order is preserved verbatim — the GUI's chip input is the source
//! of truth for "what the user typed", and re-sorting on save would
//! conflict with their explicit order.

use toml::{Table, Value};

use super::types::{AcceptsToken, EXT_NONE, RawAccepts};
use crate::rules::Mode;

/// Inject `accepts` into the dirmeta `extra` table under the
/// `accepts` key, replacing any prior value.
///
/// Use [`remove_accepts`] when the inspector wants to delete the
/// section entirely (which is **not** the same as writing an empty
/// `RawAccepts`: the latter declares "this dir intentionally accepts
/// nothing", the former declares "no placement constraint").
pub fn inject_accepts(extra: &mut Table, accepts: &RawAccepts) {
    let mut table = Table::new();
    table.insert("inherit".into(), Value::Boolean(accepts.inherit));
    let exts: Vec<Value> = accepts.exts.iter().map(token_to_toml).collect();
    table.insert("exts".into(), Value::Array(exts));
    table.insert("mode".into(), Value::String(mode_str(accepts.mode).into()));
    extra.insert("accepts".into(), Value::Table(table));
}

/// Drop `[accepts]` from the dirmeta `extra` table. No-op if absent.
pub fn remove_accepts(extra: &mut Table) {
    extra.remove("accepts");
}

fn token_to_toml(token: &AcceptsToken) -> Value {
    match token {
        AcceptsToken::Alias(name) => Value::String(format!(":{name}")),
        AcceptsToken::Ext(ext) if ext.as_str() == EXT_NONE => Value::String(String::new()),
        AcceptsToken::Ext(ext) => Value::String(format!(".{}", ext.as_str())),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accepts::loader::extract_accepts;
    use crate::accepts::types::normalize_ext;

    fn ext(s: &str) -> AcceptsToken {
        AcceptsToken::Ext(normalize_ext(s))
    }

    fn alias(name: &str) -> AcceptsToken {
        AcceptsToken::Alias(name.into())
    }

    #[test]
    fn round_trip_through_extract_preserves_payload() {
        let original = RawAccepts {
            inherit: true,
            exts: vec![alias("image"), ext(".psd"), ext(".tar.gz"), ext("")],
            mode: Mode::Strict,
        };
        let mut extra = Table::new();
        inject_accepts(&mut extra, &original);
        let parsed = extract_accepts(&extra).unwrap().unwrap();
        assert_eq!(parsed.accepts, original);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn defaults_round_trip_with_warn_mode() {
        let original = RawAccepts::default();
        let mut extra = Table::new();
        inject_accepts(&mut extra, &original);
        let parsed = extract_accepts(&extra).unwrap().unwrap();
        assert_eq!(parsed.accepts.mode, Mode::Warn);
        assert!(!parsed.accepts.inherit);
        assert!(parsed.accepts.exts.is_empty());
    }

    #[test]
    fn inject_replaces_prior_accepts_block() {
        let mut extra = Table::new();
        inject_accepts(
            &mut extra,
            &RawAccepts {
                inherit: false,
                exts: vec![ext(".old")],
                mode: Mode::Warn,
            },
        );
        inject_accepts(
            &mut extra,
            &RawAccepts {
                inherit: true,
                exts: vec![ext(".new")],
                mode: Mode::Hint,
            },
        );
        let parsed = extract_accepts(&extra).unwrap().unwrap();
        assert!(parsed.accepts.inherit);
        assert_eq!(parsed.accepts.mode, Mode::Hint);
        assert_eq!(parsed.accepts.exts, vec![ext(".new")]);
    }

    #[test]
    fn remove_drops_accepts_section() {
        let mut extra = Table::new();
        extra.insert("owner".into(), Value::String("art-team".into()));
        inject_accepts(&mut extra, &RawAccepts::default());
        remove_accepts(&mut extra);
        assert!(extra.get("accepts").is_none());
        assert!(extra.get("owner").is_some(), "unrelated keys must survive");
    }

    #[test]
    fn remove_is_noop_when_absent() {
        let mut extra = Table::new();
        remove_accepts(&mut extra);
        assert!(extra.is_empty());
    }

    #[test]
    fn token_order_is_preserved() {
        // The chip input in the inspector is the source of truth for
        // user-facing order; re-sorting on save would overwrite their
        // intent.
        let original = RawAccepts {
            inherit: false,
            exts: vec![ext(".tif"), alias("image"), ext(".psd")],
            mode: Mode::Warn,
        };
        let mut extra = Table::new();
        inject_accepts(&mut extra, &original);
        let parsed = extract_accepts(&extra).unwrap().unwrap();
        assert_eq!(parsed.accepts.exts, original.exts);
    }

    #[test]
    fn empty_string_token_round_trips_as_no_extension_sentinel() {
        let original = RawAccepts {
            inherit: false,
            exts: vec![ext("")],
            mode: Mode::Warn,
        };
        let mut extra = Table::new();
        inject_accepts(&mut extra, &original);
        let parsed = extract_accepts(&extra).unwrap().unwrap();
        assert_eq!(parsed.accepts.exts, vec![ext("")]);
    }
}
