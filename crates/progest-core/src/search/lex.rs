//! Search DSL lexer (`docs/SEARCH_DSL.md` §2.1).
//!
//! Two-state contextual lexer:
//!
//! * `Mode::Expr` — top-level expression position. Emits structural
//!   tokens (`(`, `)`, `OR`, `-`), keys (`name:` form), and free-text
//!   atoms (bareword / quoted phrase).
//! * `Mode::Value` — entered immediately after a key token. Emits
//!   exactly one value token (bareword or quoted) and returns to
//!   `Expr` mode.
//!
//! The lexer never silently drops characters; any byte not consumed
//! by a rule yields a [`LexError`].

use serde::{Deserialize, Serialize};

/// Single token in the input stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A key, stripped of the trailing `:`. Always followed by a
    /// value token.
    Key(String),
    /// Unquoted bareword (free-text or value).
    Bareword(String),
    /// Double-quoted phrase, escapes (`\\`, `\"`) decoded.
    Quoted(String),
    LParen,
    RParen,
    /// `OR` keyword (uppercase, surrounded by whitespace boundaries).
    Or,
    /// `-` unary negation prefix at expression position.
    Minus,
}

/// Token + 1-based column where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

/// Lexical error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum LexError {
    #[error("unterminated quoted string starting at column {start}")]
    UnterminatedString { start: usize },
    #[error("invalid escape '\\{ch}' in quoted string at column {column}")]
    InvalidEscape { ch: char, column: usize },
    #[error("unexpected newline in query at column {column}")]
    UnexpectedNewline { column: usize },
    #[error("expected value after key at column {column}")]
    ExpectedValue { column: usize },
    #[error("unexpected character {ch:?} at column {column}")]
    Unexpected { ch: char, column: usize },
}

impl LexError {
    pub fn column(&self) -> usize {
        match self {
            Self::UnterminatedString { start } => *start,
            Self::InvalidEscape { column, .. }
            | Self::UnexpectedNewline { column }
            | Self::ExpectedValue { column }
            | Self::Unexpected { column, .. } => *column,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Expr,
    Value,
}

/// Tokenize the entire input string.
pub fn tokenize(input: &str) -> Result<Vec<Spanned>, LexError> {
    let mut lexer = Lexer::new(input);
    let mut out = Vec::new();
    while let Some(tok) = lexer.next_token()? {
        out.push(tok);
    }
    Ok(out)
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    mode: Mode,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            mode: Mode::Expr,
        }
    }

    fn next_token(&mut self) -> Result<Option<Spanned>, LexError> {
        if self.mode == Mode::Value {
            return self.next_value();
        }
        self.skip_ws()?;
        let start = self.pos;
        let Some(b) = self.peek() else {
            return Ok(None);
        };
        match b {
            b'(' => {
                self.pos += 1;
                Ok(Some(spanned(Token::LParen, start, self.pos)))
            }
            b')' => {
                self.pos += 1;
                Ok(Some(spanned(Token::RParen, start, self.pos)))
            }
            b'-' => {
                self.pos += 1;
                Ok(Some(spanned(Token::Minus, start, self.pos)))
            }
            b'"' => {
                let (text, end) = self.read_quoted()?;
                Ok(Some(spanned(Token::Quoted(text), start, end)))
            }
            _ => self.next_expr_word(),
        }
    }

    fn next_expr_word(&mut self) -> Result<Option<Spanned>, LexError> {
        let start = self.pos;
        // Greedily consume bareword characters (`:` excluded).
        let word_end = self.scan_bareword(start);
        if word_end == start {
            let ch = self.peek_char().unwrap_or('?');
            return Err(LexError::Unexpected {
                ch,
                column: column(start),
            });
        }
        let word = &self.src[start..word_end];

        // KEY token: bareword matches `[a-z][a-z0-9_]{0,31}` AND the
        // next byte is `:`.
        if self.bytes.get(word_end) == Some(&b':') && is_valid_key(word) {
            self.pos = word_end + 1;
            self.mode = Mode::Value;
            return Ok(Some(spanned(
                Token::Key(word.to_string()),
                start,
                word_end + 1,
            )));
        }

        // OR keyword: exact ASCII bytes "OR" surrounded by ws/EOF/`)`.
        if word == "OR" && self.is_or_boundary(word_end) {
            self.pos = word_end;
            return Ok(Some(spanned(Token::Or, start, word_end)));
        }

        self.pos = word_end;
        Ok(Some(spanned(
            Token::Bareword(word.to_string()),
            start,
            word_end,
        )))
    }

    fn is_or_boundary(&self, after: usize) -> bool {
        match self.bytes.get(after) {
            None => true,
            Some(&b) => b.is_ascii_whitespace() || b == b')',
        }
    }

    fn next_value(&mut self) -> Result<Option<Spanned>, LexError> {
        let start = self.pos;
        match self.peek() {
            None | Some(b' ' | b'\t' | b')' | b'(') => {
                return Err(LexError::ExpectedValue {
                    column: column(start),
                });
            }
            Some(b'\n' | b'\r') => {
                return Err(LexError::UnexpectedNewline {
                    column: column(start),
                });
            }
            Some(b'"') => {
                let (text, end) = self.read_quoted()?;
                self.mode = Mode::Expr;
                return Ok(Some(spanned(Token::Quoted(text), start, end)));
            }
            _ => {}
        }
        let end = self.scan_value(start);
        self.mode = Mode::Expr;
        let body = &self.src[start..end];
        self.pos = end;
        Ok(Some(spanned(Token::Bareword(body.to_string()), start, end)))
    }

    fn skip_ws(&mut self) -> Result<(), LexError> {
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b' ' | b'\t' => self.pos += 1,
                b'\n' | b'\r' => {
                    return Err(LexError::UnexpectedNewline {
                        column: column(self.pos),
                    });
                }
                _ => return Ok(()),
            }
        }
        Ok(())
    }

    fn read_quoted(&mut self) -> Result<(String, usize), LexError> {
        debug_assert_eq!(self.bytes.get(self.pos), Some(&b'"'));
        let start = self.pos;
        self.pos += 1; // opening quote
        let mut out = String::new();
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'"' => {
                    self.pos += 1;
                    return Ok((out, self.pos));
                }
                b'\n' | b'\r' => {
                    return Err(LexError::UnexpectedNewline {
                        column: column(self.pos),
                    });
                }
                b'\\' => {
                    let next = self.bytes.get(self.pos + 1).copied();
                    match next {
                        Some(b'"') => {
                            out.push('"');
                            self.pos += 2;
                        }
                        Some(b'\\') => {
                            out.push('\\');
                            self.pos += 2;
                        }
                        Some(other) => {
                            return Err(LexError::InvalidEscape {
                                ch: other as char,
                                column: column(self.pos),
                            });
                        }
                        None => {
                            return Err(LexError::UnterminatedString {
                                start: column(start),
                            });
                        }
                    }
                }
                _ => {
                    // Push the next utf-8 char.
                    let ch = self.peek_char_at(self.pos).expect("byte position is valid");
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
        Err(LexError::UnterminatedString {
            start: column(start),
        })
    }

    fn scan_bareword(&self, from: usize) -> usize {
        let mut i = from;
        while let Some(&b) = self.bytes.get(i) {
            if is_bareword_byte(b) {
                i += 1;
            } else if !b.is_ascii() {
                // multi-byte UTF-8 char: include in bareword
                let ch = self
                    .peek_char_at(i)
                    .expect("byte position is valid utf-8 start");
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        i
    }

    fn scan_value(&self, from: usize) -> usize {
        // Value barewords are looser: anything until whitespace,
        // `(`, `)`, EOF.
        let mut i = from;
        while let Some(&b) = self.bytes.get(i) {
            match b {
                b' ' | b'\t' | b'(' | b')' | b'\n' | b'\r' => break,
                _ => {
                    let ch = self
                        .peek_char_at(i)
                        .expect("byte position is valid utf-8 start");
                    i += ch.len_utf8();
                }
            }
        }
        i
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_char(&self) -> Option<char> {
        self.peek_char_at(self.pos)
    }

    fn peek_char_at(&self, pos: usize) -> Option<char> {
        self.src.get(pos..).and_then(|s| s.chars().next())
    }
}

fn spanned(token: Token, start: usize, end: usize) -> Spanned {
    Spanned {
        token,
        start: column(start),
        end: column(end),
    }
}

fn column(byte_offset: usize) -> usize {
    // 1-based column. Bytes are fine since errors point at ASCII
    // operators in practice; multi-byte chars within barewords still
    // produce sensible 1-based positions (end column of the previous
    // token).
    byte_offset + 1
}

fn is_bareword_byte(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' |
        b'_' | b'.' | b'/' | b'*' | b'?' | b'+' | b'-' | b'[' | b']' | b'!' | b'\\'
    )
}

/// `word` matches the key shape `[a-z][a-z0-9_]{0,31}`.
fn is_valid_key(word: &str) -> bool {
    let bytes = word.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Token> {
        tokenize(s).unwrap().into_iter().map(|t| t.token).collect()
    }

    #[test]
    fn empty_query() {
        assert!(toks("").is_empty());
        assert!(toks("   ").is_empty());
    }

    #[test]
    fn simple_freetext() {
        assert_eq!(toks("forest"), vec![Token::Bareword("forest".into())]);
    }

    #[test]
    fn quoted_phrase() {
        assert_eq!(
            toks(r#""forest night""#),
            vec![Token::Quoted("forest night".into())]
        );
    }

    #[test]
    fn quoted_with_escapes() {
        assert_eq!(
            toks(r#""he said \"hi\" \\""#),
            vec![Token::Quoted("he said \"hi\" \\".into())]
        );
    }

    #[test]
    fn key_value() {
        assert_eq!(
            toks("tag:foo"),
            vec![Token::Key("tag".into()), Token::Bareword("foo".into())]
        );
    }

    #[test]
    fn key_quoted_value() {
        assert_eq!(
            toks(r#"name:"foo bar""#),
            vec![Token::Key("name".into()), Token::Quoted("foo bar".into())]
        );
    }

    #[test]
    fn glob_in_value() {
        assert_eq!(
            toks("path:./assets/**"),
            vec![
                Token::Key("path".into()),
                Token::Bareword("./assets/**".into())
            ]
        );
    }

    #[test]
    fn negation() {
        assert_eq!(
            toks("-tag:foo"),
            vec![
                Token::Minus,
                Token::Key("tag".into()),
                Token::Bareword("foo".into()),
            ]
        );
    }

    #[test]
    fn or_keyword() {
        assert_eq!(
            toks("tag:a OR tag:b"),
            vec![
                Token::Key("tag".into()),
                Token::Bareword("a".into()),
                Token::Or,
                Token::Key("tag".into()),
                Token::Bareword("b".into()),
            ]
        );
    }

    #[test]
    fn or_lowercase_is_freetext() {
        assert_eq!(
            toks("a or b"),
            vec![
                Token::Bareword("a".into()),
                Token::Bareword("or".into()),
                Token::Bareword("b".into()),
            ]
        );
    }

    #[test]
    fn parens() {
        assert_eq!(
            toks("(tag:a)"),
            vec![
                Token::LParen,
                Token::Key("tag".into()),
                Token::Bareword("a".into()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn range_value_is_single_bareword() {
        assert_eq!(
            toks("created:2026-01-01..2026-04-30"),
            vec![
                Token::Key("created".into()),
                Token::Bareword("2026-01-01..2026-04-30".into()),
            ]
        );
    }

    #[test]
    fn newline_rejected() {
        assert!(matches!(
            tokenize("foo\nbar"),
            Err(LexError::UnexpectedNewline { .. })
        ));
    }

    #[test]
    fn unterminated_string() {
        assert!(matches!(
            tokenize(r#""hello"#),
            Err(LexError::UnterminatedString { .. })
        ));
    }

    #[test]
    fn empty_value_after_key_errors() {
        assert!(matches!(
            tokenize("tag: foo"),
            Err(LexError::ExpectedValue { .. })
        ));
    }

    #[test]
    fn key_must_be_lowercase() {
        // Uppercase prefix is not a key; the trailing `:` then has
        // no valid lex rule and the lexer errors.
        assert!(matches!(
            tokenize("Tag:foo"),
            Err(LexError::Unexpected { ch: ':', .. })
        ));
    }

    #[test]
    fn double_minus_is_two_minuses() {
        // Lex emits two MINUS; parser will reject `--atom`.
        assert_eq!(
            toks("--tag:foo"),
            vec![
                Token::Minus,
                Token::Minus,
                Token::Key("tag".into()),
                Token::Bareword("foo".into()),
            ]
        );
    }

    #[test]
    fn multibyte_in_bareword() {
        // CJK bareword (free text)
        assert_eq!(toks("森"), vec![Token::Bareword("森".into())]);
    }

    #[test]
    fn multibyte_in_quoted_value() {
        assert_eq!(
            toks(r#"path:"./プロジェクト/**""#),
            vec![
                Token::Key("path".into()),
                Token::Quoted("./プロジェクト/**".into())
            ]
        );
    }
}
