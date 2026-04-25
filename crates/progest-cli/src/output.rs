//! Shared CLI output format handling.
//!
//! Every reporting subcommand (`lint`, `clean`, `rename`, `undo` /
//! `redo`) accepts `--format text|json` with identical semantics:
//! human-facing text by default, machine-readable JSON on demand. This
//! module owns the single [`OutputFormat`] enum so the subcommands stop
//! redefining it, and provides [`emit_json`] for the recurring "serialize
//! a `Serialize` value to pretty JSON on stdout" path.
//!
//! The text branch stays per-command — every report has its own layout
//! and the dispatcher just routes `OutputFormat::Text` to a custom
//! writer.

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::Serialize;

/// Output format selector shared across reporting subcommands.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Serialize `report` as pretty JSON and write it to stdout with a
/// trailing newline. Adds an `anyhow` context on serialize failure
/// pinpointing which command's report blew up.
pub fn emit_json<T: Serialize>(report: &T, label: &'static str) -> Result<()> {
    let s = serde_json::to_string_pretty(report)
        .with_context(|| format!("serializing {label} report"))?;
    println!("{s}");
    Ok(())
}
