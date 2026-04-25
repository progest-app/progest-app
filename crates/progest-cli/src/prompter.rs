//! Stdin-backed [`HolePrompter`] for interactive `--fill-mode prompt`.
//!
//! Lives in the CLI crate (not core) because it owns terminal I/O —
//! `progest-core` is meant to be UI-agnostic so the same logic can
//! run unattended in CI. The implementation is deliberately
//! minimal: no `dialoguer` dep, no fancy line editing, just a
//! prompt-and-read loop on the supplied reader/writer pair.
//!
//! Tests inject `StdinHolePrompter::with_io` so they can drive both
//! sides without touching real stdin/stderr.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Mutex;

use progest_core::naming::{Hole, HolePrompter, PromptError};

/// Reads substitutes from `stdin`, writes prompts to `stderr`.
///
/// `stderr` is used (not `stdout`) so JSON output on `stdout` stays
/// machine-parseable when the CLI is piped (`progest rename --format=json
/// --fill-mode=prompt | jq …`).
pub struct StdinHolePrompter<R, W>
where
    R: Read + Send,
    W: Write + Send,
{
    reader: Mutex<BufReader<R>>,
    writer: Mutex<W>,
}

impl StdinHolePrompter<std::io::Stdin, std::io::Stderr> {
    /// Standard production wiring: stdin for input, stderr for prompts.
    #[must_use]
    pub fn from_stdio() -> Self {
        Self::with_io(std::io::stdin(), std::io::stderr())
    }
}

impl<R, W> StdinHolePrompter<R, W>
where
    R: Read + Send,
    W: Write + Send,
{
    /// Construct from arbitrary reader/writer pairs. Used by tests
    /// to inject canned input and capture prompts.
    pub fn with_io(reader: R, writer: W) -> Self {
        Self {
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
        }
    }
}

impl<R, W> HolePrompter for StdinHolePrompter<R, W>
where
    R: Read + Send,
    W: Write + Send,
{
    fn prompt(&self, hole: &Hole) -> Result<String, PromptError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| PromptError::Invalid("prompter writer mutex poisoned".into()))?;
        // Show the original (e.g. CJK run) so the user knows what
        // they're replacing, plus the kind for context.
        writeln!(
            writer,
            "Replace hole {kind:?} '{origin}' with: ",
            kind = hole.kind,
            origin = hole.origin
        )?;
        writer.flush()?;
        drop(writer);

        let mut reader = self
            .reader
            .lock()
            .map_err(|_| PromptError::Invalid("prompter reader mutex poisoned".into()))?;
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            // EOF before any input — treat as cancellation.
            return Err(PromptError::Cancelled);
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            return Err(PromptError::Invalid(
                "empty substitute is not allowed".into(),
            ));
        }
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use progest_core::naming::HoleKind;

    fn hole(origin: &str) -> Hole {
        Hole {
            origin: origin.into(),
            kind: HoleKind::Cjk,
            pos: 0,
        }
    }

    #[test]
    fn reads_one_substitute_per_prompt() {
        let input = b"shot\n".as_slice();
        let mut output: Vec<u8> = Vec::new();
        let prompter = StdinHolePrompter::with_io(input, &mut output);
        let r = prompter.prompt(&hole("カット")).unwrap();
        assert_eq!(r, "shot");
        let prompt_text = String::from_utf8(output).unwrap();
        assert!(prompt_text.contains("カット"));
        assert!(prompt_text.contains("Cjk"));
    }

    #[test]
    fn strips_trailing_crlf() {
        let input = b"name\r\n".as_slice();
        let mut output: Vec<u8> = Vec::new();
        let prompter = StdinHolePrompter::with_io(input, &mut output);
        let r = prompter.prompt(&hole("h")).unwrap();
        assert_eq!(r, "name");
    }

    #[test]
    fn empty_response_is_invalid() {
        let input = b"\n".as_slice();
        let mut output: Vec<u8> = Vec::new();
        let prompter = StdinHolePrompter::with_io(input, &mut output);
        let err = prompter.prompt(&hole("h")).unwrap_err();
        assert!(matches!(err, PromptError::Invalid(_)));
    }

    #[test]
    fn eof_before_input_is_cancelled() {
        let input = b"".as_slice();
        let mut output: Vec<u8> = Vec::new();
        let prompter = StdinHolePrompter::with_io(input, &mut output);
        let err = prompter.prompt(&hole("h")).unwrap_err();
        assert!(matches!(err, PromptError::Cancelled));
    }

    #[test]
    fn handles_multiple_sequential_prompts() {
        let input = b"first\nsecond\n".as_slice();
        let mut output: Vec<u8> = Vec::new();
        let prompter = StdinHolePrompter::with_io(input, &mut output);
        assert_eq!(prompter.prompt(&hole("a")).unwrap(), "first");
        assert_eq!(prompter.prompt(&hole("b")).unwrap(), "second");
    }
}
