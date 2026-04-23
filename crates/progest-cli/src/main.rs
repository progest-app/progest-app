//! Progest CLI entry point.
//!
//! The CLI is a first-class interface alongside the GUI. Every subcommand
//! should be backed by a `progest-core` API with identical behaviour.
//! See `docs/REQUIREMENTS.md` §3.9 for the full command surface.

#![allow(clippy::todo)] // scaffold: Lint/Search populated in M2/M3.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use commands::clean::{CaseFlag, CleanArgs, FillFlag, FormatFlag};

mod commands;

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
    /// Check files against naming rules.
    Lint,
    /// Preview mechanical name-cleanup candidates (REQUIREMENTS §3.5.5).
    Clean {
        /// Restrict the walk to these paths (project-root relative or absolute).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: FormatFlag,
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
    },
    /// Search files using the Progest query DSL.
    Search {
        /// The query string (e.g. `tag:character type:psd is:violation`).
        query: String,
    },
}

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
        Command::Lint => todo!("M2: rule engine lint report"),
        Command::Clean {
            paths,
            format,
            case,
            strip_cjk,
            strip_suffix,
            fill_mode,
            placeholder,
        } => {
            commands::clean::run(
                &cwd,
                &CleanArgs {
                    paths,
                    format,
                    case,
                    strip_cjk,
                    strip_suffix,
                    fill_mode,
                    placeholder,
                },
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Search { query: _ } => todo!("M3: DSL parser + FTS5 query"),
    }
}
