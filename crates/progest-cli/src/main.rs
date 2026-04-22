//! Progest CLI entry point.
//!
//! The CLI is a first-class interface alongside the GUI. Every subcommand
//! should be backed by a `progest-core` API with identical behaviour.
//! See `docs/REQUIREMENTS.md` §3.9 for the full command surface.

#![allow(clippy::todo)] // scaffold: Lint/Search populated in M2/M3.

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
        Command::Search { query: _ } => todo!("M3: DSL parser + FTS5 query"),
    }
}
