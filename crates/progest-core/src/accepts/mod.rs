//! Placement-rules engine (`core::accepts`).
//!
//! Implements REQUIREMENTS.md §3.13 (placement rules / `accepts`) in
//! the same spirit `core::rules` implements the naming DSL:
//!
//! 1. [`types`] — value types (`AliasName`, `Ext`, `RawAccepts`,
//!    `EffectiveAccepts`) and the `BUILTIN_ALIASES` table. Extension
//!    catalogs are authoritative in
//!    [`docs/ACCEPTS_ALIASES.md`](../../../../docs/ACCEPTS_ALIASES.md).
//! 2. [`loader`] — pull `[accepts]` out of a `.dirmeta.toml`'s raw
//!    `extra` table, pull `[alias.*]` from `.progest/schema.toml`,
//!    validate per §3.13 / `ACCEPTS_ALIASES.md` §3.1.
//! 3. [`resolve`] — compute `effective_accepts(dir)` by walking the
//!    own-set and inheriting ancestor sets when `inherit = true`
//!    (REQUIREMENTS.md §3.13.2).
//! 4. [`evaluate`] — placement lint: compare a file's direct parent
//!    dir's `effective_accepts` against its extension and emit a
//!    [`crate::rules::Violation`] with `category = Placement` and
//!    `placement_details` populated per §3.13.6.
//!
//! Import-ranking for the `suggested_destinations` field ships in a
//! follow-up PR — the types are wired here but the ranking loop is
//! out of scope.

pub mod evaluate;
pub mod loader;
pub mod resolve;
pub mod schema;
pub mod types;

pub use evaluate::{evaluate_placement_for_file, placement_rule_id};
pub use loader::{AcceptsExtraction, AcceptsLoadError, AcceptsWarning, extract_accepts};
pub use resolve::{EffectiveAccepts, ResolveError, compute_effective_accepts, expand_own_accepts};
pub use schema::{
    AliasCatalog, SchemaLoad, SchemaLoadError, SchemaWarning, load_alias_catalog,
    load_alias_catalog_from_table,
};
pub use types::{
    AcceptsToken, BUILTIN_ALIASES, EXT_NONE, Ext, RawAccepts, builtin_alias, is_valid_alias_name,
    normalize_ext, normalize_ext_from_basename,
};
