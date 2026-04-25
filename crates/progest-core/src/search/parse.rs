//! Search DSL parser (`docs/SEARCH_DSL.md` §2.2).
//!
//! Recursive-descent parser over the token stream from
//! [`super::lex`]. Produces an [`ast::Query`] with `OR > AND > NOT`
//! precedence and left-associative operators.

use serde::{Deserialize, Serialize};

use super::ast::{Atom, Clause, Expr, Query, Value};
use super::lex::{LexError, Spanned, Token, tokenize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ParseError {
    #[error("lex error: {0}")]
    Lex(#[from] LexError),
    #[error("unexpected '--' at column {column} — '-' is unary, '--' is reserved")]
    DoubleMinus { column: usize },
    #[error("unexpected token at column {column}: {message}")]
    Unexpected { column: usize, message: String },
    #[error("empty group at column {column}")]
    EmptyGroup { column: usize },
    #[error("missing closing ')' for group opened at column {column}")]
    UnclosedGroup { column: usize },
    #[error("'OR' must be followed by an expression at column {column}")]
    DanglingOr { column: usize },
    #[error("'-' must be followed by an atom at column {column}")]
    DanglingMinus { column: usize },
    #[error("query is empty (use --allow-empty to match all files)")]
    Empty,
}

impl ParseError {
    pub fn column(&self) -> Option<usize> {
        match self {
            Self::Lex(e) => Some(e.column()),
            Self::DoubleMinus { column }
            | Self::Unexpected { column, .. }
            | Self::EmptyGroup { column }
            | Self::UnclosedGroup { column }
            | Self::DanglingOr { column }
            | Self::DanglingMinus { column } => Some(*column),
            Self::Empty => None,
        }
    }
}

/// Parse a query string into an AST.
pub fn parse(input: &str) -> Result<Query, ParseError> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut parser = Parser::new(&tokens);
    let expr = parser.parse_or()?;
    parser.expect_eof()?;
    Ok(Query { root: expr })
}

struct Parser<'a> {
    tokens: &'a [Spanned],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Spanned]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Spanned> {
        self.tokens.get(self.pos)
    }

    fn peek_token(&self) -> Option<&Token> {
        self.peek().map(|s| &s.token)
    }

    fn bump(&mut self) -> &Spanned {
        let s = &self.tokens[self.pos];
        self.pos += 1;
        s
    }

    fn current_column(&self) -> usize {
        self.peek()
            .map_or_else(|| self.tokens.last().map_or(1, |s| s.end), |s| s.start)
    }

    fn expect_eof(&mut self) -> Result<(), ParseError> {
        if self.pos != self.tokens.len() {
            return Err(ParseError::Unexpected {
                column: self.current_column(),
                message: format!("trailing token: {:?}", self.peek_token().unwrap()),
            });
        }
        Ok(())
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_and()?;
        let mut branches = vec![first];
        while matches!(self.peek_token(), Some(Token::Or)) {
            let or_col = self.peek().unwrap().start;
            self.bump(); // consume OR
            if matches!(self.peek_token(), None | Some(Token::RParen)) {
                return Err(ParseError::DanglingOr { column: or_col });
            }
            branches.push(self.parse_and()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            Expr::Or(branches)
        })
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_term()?;
        let mut terms = vec![first];
        while self.is_term_start() {
            terms.push(self.parse_term()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            Expr::And(terms)
        })
    }

    fn is_term_start(&self) -> bool {
        matches!(
            self.peek_token(),
            Some(
                Token::Minus
                    | Token::LParen
                    | Token::Key(_)
                    | Token::Bareword(_)
                    | Token::Quoted(_)
            )
        )
    }

    fn parse_term(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.peek_token(), Some(Token::Minus)) {
            let minus_col = self.peek().unwrap().start;
            self.bump();
            // Disallow `--`
            if matches!(self.peek_token(), Some(Token::Minus)) {
                return Err(ParseError::DoubleMinus { column: minus_col });
            }
            if !self.is_atom_start() {
                return Err(ParseError::DanglingMinus { column: minus_col });
            }
            let atom = self.parse_atom()?;
            Ok(Expr::Not(Box::new(atom)))
        } else {
            self.parse_atom()
        }
    }

    fn is_atom_start(&self) -> bool {
        matches!(
            self.peek_token(),
            Some(Token::LParen | Token::Key(_) | Token::Bareword(_) | Token::Quoted(_))
        )
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek().unwrap().clone();
        match span.token {
            Token::LParen => self.parse_group(span.start),
            Token::Key(_) => self.parse_clause(),
            Token::Bareword(s) => {
                self.bump();
                Ok(Expr::Atom(Atom::FreeBareword(s)))
            }
            Token::Quoted(s) => {
                self.bump();
                Ok(Expr::Atom(Atom::FreePhrase(s)))
            }
            _ => Err(ParseError::Unexpected {
                column: span.start,
                message: format!("expected atom, got {:?}", span.token),
            }),
        }
    }

    fn parse_group(&mut self, open_col: usize) -> Result<Expr, ParseError> {
        self.bump(); // consume `(`
        if matches!(self.peek_token(), Some(Token::RParen)) {
            return Err(ParseError::EmptyGroup { column: open_col });
        }
        let inner = self.parse_or()?;
        match self.peek_token() {
            Some(Token::RParen) => {
                self.bump();
                Ok(inner)
            }
            _ => Err(ParseError::UnclosedGroup { column: open_col }),
        }
    }

    fn parse_clause(&mut self) -> Result<Expr, ParseError> {
        let key_span = self.bump().clone();
        let Token::Key(key) = key_span.token else {
            unreachable!("parse_clause called with non-Key token");
        };
        let value_span = match self.peek() {
            Some(s) => s.clone(),
            None => {
                return Err(ParseError::Unexpected {
                    column: key_span.end,
                    message: "expected value after key".into(),
                });
            }
        };
        let value = match &value_span.token {
            Token::Bareword(s) => Value::Bareword(s.clone()),
            Token::Quoted(s) => Value::Quoted(s.clone()),
            _ => {
                return Err(ParseError::Unexpected {
                    column: value_span.start,
                    message: "expected bareword or quoted value".into(),
                });
            }
        };
        self.bump();
        Ok(Expr::Atom(Atom::Clause(Clause { key, value })))
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Query {
        parse(s).unwrap_or_else(|e| panic!("parse({s:?}) failed: {e}"))
    }

    fn err(s: &str) -> ParseError {
        parse(s).unwrap_err()
    }

    fn clause(k: &str, v: Value) -> Expr {
        Expr::Atom(Atom::Clause(Clause {
            key: k.into(),
            value: v,
        }))
    }

    fn bw(v: &str) -> Value {
        Value::Bareword(v.into())
    }

    fn qv(v: &str) -> Value {
        Value::Quoted(v.into())
    }

    #[test]
    fn single_clause() {
        assert_eq!(p("tag:foo").root, clause("tag", bw("foo")));
    }

    #[test]
    fn implicit_and() {
        let q = p("tag:foo type:psd");
        match q.root {
            Expr::And(branches) => {
                assert_eq!(branches.len(), 2);
                assert_eq!(branches[0], clause("tag", bw("foo")));
                assert_eq!(branches[1], clause("type", bw("psd")));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn or_lower_than_and() {
        let q = p("a OR b c");
        // Should be: (a) OR (b AND c)
        match q.root {
            Expr::Or(branches) => {
                assert_eq!(branches.len(), 2);
                assert_eq!(branches[0], Expr::Atom(Atom::FreeBareword("a".into())));
                match &branches[1] {
                    Expr::And(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert_eq!(inner[0], Expr::Atom(Atom::FreeBareword("b".into())));
                        assert_eq!(inner[1], Expr::Atom(Atom::FreeBareword("c".into())));
                    }
                    other => panic!("expected And inside Or, got {other:?}"),
                }
            }
            other => panic!("expected Or root, got {other:?}"),
        }
    }

    #[test]
    fn group_overrides_precedence() {
        let q = p("(a OR b) c");
        match q.root {
            Expr::And(branches) => {
                assert_eq!(branches.len(), 2);
                match &branches[0] {
                    Expr::Or(_) => {}
                    other => panic!("expected Or as first AND branch, got {other:?}"),
                }
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn negation() {
        let q = p("-tag:foo");
        match q.root {
            Expr::Not(inner) => assert_eq!(*inner, clause("tag", bw("foo"))),
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn negation_of_group() {
        let q = p("-(tag:a tag:b)");
        match q.root {
            Expr::Not(inner) => match &*inner {
                Expr::And(_) => {}
                other => panic!("expected And inside Not, got {other:?}"),
            },
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn freetext_then_clause() {
        let q = p("forest tag:wip type:psd");
        match q.root {
            Expr::And(branches) => {
                assert_eq!(branches.len(), 3);
                assert_eq!(branches[0], Expr::Atom(Atom::FreeBareword("forest".into())));
                assert_eq!(branches[1], clause("tag", bw("wip")));
                assert_eq!(branches[2], clause("type", bw("psd")));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn quoted_phrase_is_freetext() {
        let q = p(r#""forest night""#);
        match q.root {
            Expr::Atom(Atom::FreePhrase(s)) => assert_eq!(s, "forest night"),
            other => panic!("expected FreePhrase, got {other:?}"),
        }
    }

    #[test]
    fn clause_with_quoted_value() {
        let q = p(r#"name:"foo bar""#);
        assert_eq!(q.root, clause("name", qv("foo bar")));
    }

    #[test]
    fn double_minus_errors() {
        match err("--tag:foo") {
            ParseError::DoubleMinus { column } => assert_eq!(column, 1),
            other => panic!("expected DoubleMinus, got {other:?}"),
        }
    }

    #[test]
    fn empty_query_errors() {
        assert!(matches!(parse(""), Err(ParseError::Empty)));
        assert!(matches!(parse("   "), Err(ParseError::Empty)));
    }

    #[test]
    fn empty_group_errors() {
        match err("()") {
            ParseError::EmptyGroup { .. } => {}
            other => panic!("expected EmptyGroup, got {other:?}"),
        }
    }

    #[test]
    fn dangling_or_errors() {
        match err("tag:a OR") {
            ParseError::DanglingOr { .. } => {}
            other => panic!("expected DanglingOr, got {other:?}"),
        }
    }

    #[test]
    fn unclosed_group_errors() {
        match err("(tag:a") {
            ParseError::UnclosedGroup { .. } => {}
            other => panic!("expected UnclosedGroup, got {other:?}"),
        }
    }

    #[test]
    fn or_three_way() {
        let q = p("a OR b OR c");
        match q.root {
            Expr::Or(branches) => assert_eq!(branches.len(), 3),
            other => panic!("expected Or with 3 branches, got {other:?}"),
        }
    }

    #[test]
    fn complex_query_structure() {
        // (tag:wip OR tag:review) -is:violation
        let q = p("(tag:wip OR tag:review) -is:violation");
        match q.root {
            Expr::And(branches) => {
                assert_eq!(branches.len(), 2);
                assert!(matches!(&branches[0], Expr::Or(_)));
                assert!(matches!(&branches[1], Expr::Not(_)));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn range_value_kept_as_bareword() {
        let q = p("created:2026-01-01..2026-04-30");
        assert_eq!(q.root, clause("created", bw("2026-01-01..2026-04-30")));
    }
}
