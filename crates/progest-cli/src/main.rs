//! Progest CLI entry point.
//!
//! The CLI is a first-class interface alongside the GUI. Every subcommand
//! should be backed by a `progest-core` API with identical behaviour.
//! See `docs/REQUIREMENTS.md` §3.9 for the full command surface.

#![allow(clippy::todo)] // scaffold: Search populated in M3.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use commands::clean::{CaseFlag, CleanArgs, FillFlag};
use commands::lint::LintArgs;
use commands::rename::{RenameArgs, RenameMode};
use commands::undo::{Direction as UndoDirection, UndoRedoArgs};
use output::OutputFormat;

mod commands;
mod context;
mod output;
mod prompter;
mod walk;

/// Naming-rule-first file management for creative projects.
#[derive(Debug, Parser)]
#[command(name = "progest", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize a new Progest project in the current directory.
    Init {
        /// Optional display name for the project. Defaults to the directory basename.
        #[arg(long)]
        name: Option<String>,
    },
    /// Walk the project and (re)build the index.
    Scan,
    /// Report integrity issues (orphan meta, UUID clashes, drift).
    Doctor,
    /// Check files against naming / placement / sequence rules and
    /// report violations grouped by category.
    Lint {
        /// Restrict the walk to these paths (project-root relative or absolute).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
        /// Keep rule traces for every evaluated file (not just
        /// violating ones). Produces a much larger JSON payload.
        #[arg(long)]
        explain: bool,
    },
    /// Preview or apply mechanical name-cleanup candidates (REQUIREMENTS §3.5.5).
    Clean {
        /// Restrict the walk to these paths (project-root relative or absolute).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
        /// Override `[cleanup].convert_case`.
        #[arg(long, value_enum)]
        case: Option<CaseFlag>,
        /// Force `remove_cjk` on regardless of config.
        #[arg(long)]
        strip_cjk: bool,
        /// Force `remove_copy_suffix` on regardless of config.
        #[arg(long)]
        strip_suffix: bool,
        /// How to resolve CJK holes when rendering the final name.
        #[arg(long, default_value = "skip", value_enum)]
        fill_mode: FillFlag,
        /// Placeholder string substituted for each hole under `--fill-mode=placeholder`.
        #[arg(long)]
        placeholder: Option<String>,
        /// After previewing, commit the changed-and-resolvable candidates
        /// through the same atomic apply path as `progest rename`.
        #[arg(long)]
        apply: bool,
    },
    /// Preview or apply renames against the project (M2).
    Rename {
        /// Restrict the walk to these paths (project-root relative or absolute).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Read a `RenameOp[]` JSON array from stdin instead of walking paths.
        #[arg(long)]
        from_stdin: bool,
        /// Preview only (default) or commit the rename to disk.
        #[arg(long, default_value = "preview", value_enum)]
        mode: RenameMode,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
        /// Override `[cleanup].convert_case`.
        #[arg(long, value_enum)]
        case: Option<CaseFlag>,
        /// Force `remove_cjk` on regardless of config.
        #[arg(long)]
        strip_cjk: bool,
        /// Force `remove_copy_suffix` on regardless of config.
        #[arg(long)]
        strip_suffix: bool,
        /// How to resolve CJK holes when rendering the final name.
        #[arg(long, default_value = "skip", value_enum)]
        fill_mode: FillFlag,
        /// Placeholder string substituted for each hole under `--fill-mode=placeholder`.
        #[arg(long)]
        placeholder: Option<String>,
        /// Run sequence detection on PATH... and rename each sequence
        /// by replacing the stem prefix with this value (preserves
        /// numeric index, padding, separator, and extension).
        #[arg(long, value_name = "STEM")]
        sequence_stem: Option<String>,
    },
    /// Search files using the Progest query DSL.
    Search {
        /// The query string (e.g. `tag:character type:psd is:violation`).
        query: String,
    },
    /// Undo the top of the history stack. Default unwinds the whole
    /// `group_id` (a bulk rename / sequence); `--entry` limits to one.
    Undo {
        /// Only undo the single top entry, even if it's part of a group.
        #[arg(long)]
        entry: bool,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Redo the most recently undone entry or group.
    Redo {
        /// Only redo the single entry, even if it's part of a group.
        #[arg(long)]
        entry: bool,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
}

fn to_exit_code(code: i32) -> ExitCode {
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(u8::try_from(code).unwrap_or(1))
    }
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<ExitCode> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    match cli.command {
        Command::Init { name } => {
            commands::init::run(&cwd, name)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Scan => {
            commands::scan::run(&cwd)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Doctor => commands::doctor::run(&cwd),
        Command::Lint {
            paths,
            format,
            explain,
        } => {
            let code = commands::lint::run(
                &cwd,
                &LintArgs {
                    paths,
                    format,
                    explain,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Clean {
            paths,
            format,
            case,
            strip_cjk,
            strip_suffix,
            fill_mode,
            placeholder,
            apply,
        } => {
            let code = commands::clean::run(
                &cwd,
                &CleanArgs {
                    paths,
                    format,
                    case,
                    strip_cjk,
                    strip_suffix,
                    fill_mode,
                    placeholder,
                    apply,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Rename {
            paths,
            from_stdin,
            mode,
            format,
            case,
            strip_cjk,
            strip_suffix,
            fill_mode,
            placeholder,
            sequence_stem,
        } => {
            let code = commands::rename::run(
                &cwd,
                &RenameArgs {
                    paths,
                    format,
                    mode,
                    from_stdin,
                    case,
                    strip_cjk,
                    strip_suffix,
                    fill_mode,
                    placeholder,
                    sequence_stem,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Search { query: _ } => todo!("M3: DSL parser + FTS5 query"),
        Command::Undo { entry, format } => {
            let code = commands::undo::run(
                &cwd,
                &UndoRedoArgs {
                    entry_only: entry,
                    format,
                    direction: UndoDirection::Undo,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Redo { entry, format } => {
            let code = commands::undo::run(
                &cwd,
                &UndoRedoArgs {
                    entry_only: entry,
                    format,
                    direction: UndoDirection::Redo,
                },
            )?;
            Ok(to_exit_code(code))
        }
    }
}
