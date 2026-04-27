//! Search engine (`core::search`).
//!
//! Pipeline: `lex → parse → validate → plan → execute`.
//! This crate currently provides the first four stages (pure
//! functions / no DB I/O). The executor lands together with the
//! FTS5 + `custom_fields` schema migration in M3 #4.
//!
//! Reference: `docs/SEARCH_DSL.md` (canonical spec).

pub mod ast;
pub mod execute;
pub mod history;
pub mod lex;
pub mod parse;
pub mod plan;
pub mod validate;
pub mod views;

pub use ast::{Atom, Clause, Expr, Query, Value};
pub use execute::{
    ExecuteError, RichCustomField, RichCustomValue, RichSearchHit, RichViolationCounts, SearchHit,
    execute, project_hits,
};
pub use lex::{LexError, Spanned, Token, tokenize};
pub use parse::{ParseError, parse};
pub use plan::{BindValue, PlannedQuery, plan};
pub use validate::{
    CustomClause, CustomFieldKind, CustomFields, CustomMatcher, FreeTextTerm, InstantRange,
    IsValue, KindValue, ReservedClause, ValidAtom, ValidExpr, ValidatedQuery, Warning, validate,
    validate_with_catalog,
};
