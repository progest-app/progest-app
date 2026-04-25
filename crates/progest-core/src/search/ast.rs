//! Search DSL AST.
//!
//! Mirrors `docs/SEARCH_DSL.md` §2 grammar bit-for-bit. The parser
//! produces these types; `validate` decorates them with semantic
//! information; `plan` lowers them into SQL string + params.
//!
//! The AST is intentionally `serde::Serialize`-friendly so golden
//! tests can compare structural output.

use serde::{Deserialize, Serialize};

/// Top-level parsed query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub root: Expr,
}

/// A boolean expression node.
///
/// `Or` and `And` carry a flat `Vec<Expr>` (left-associative, parser
/// already collapsed nested same-operator branches). `Not` is unary.
/// Empty `Or` / `And` are not constructible by the parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>),
    Not(Box<Expr>),
    Atom(Atom),
}

/// A leaf in the boolean tree: either a `key:value` clause or a
/// free-text term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Atom {
    Clause(Clause),
    /// Unquoted bareword used as free text.
    FreeBareword(String),
    /// Quoted phrase used as free text. Phrase semantics differ from
    /// bareword (literal multi-token match).
    FreePhrase(String),
}

/// `key:value` clause as parsed (no semantic interpretation yet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clause {
    pub key: String,
    pub value: Value,
}

/// A clause value as captured by the parser.
///
/// Range parsing is deferred to `validate`: only keys that accept
/// ranges interpret `..` inside a `Bareword`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Value {
    /// Unquoted value text, exactly as written (excluding leading
    /// `:`). May contain glob metacharacters and `..`.
    Bareword(String),
    /// Quoted value text, with surrounding `"` removed and escapes
    /// (`\\` `\"`) decoded.
    Quoted(String),
}

impl Value {
    pub fn as_str(&self) -> &str {
        match self {
            Value::Bareword(s) | Value::Quoted(s) => s,
        }
    }

    pub fn is_quoted(&self) -> bool {
        matches!(self, Value::Quoted(_))
    }
}
