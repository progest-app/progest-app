//! Progest CLI entry point.
//!
//! The CLI is a first-class interface alongside the GUI. Every subcommand
//! should be backed by a `progest-core` API with identical behaviour.
//! See `docs/REQUIREMENTS.md` §3.9 for the full command surface.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use commands::clean::{CaseFlag, CleanArgs, FillFlag};
use commands::delete::DeleteArgs;
use commands::import::ImportArgs;
use commands::lint::LintArgs;
use commands::rename::{RenameArgs, RenameMode};
use commands::search::SearchArgs;
use commands::tag::{TagArgs, TagCommand};
use commands::template::{TemplateApplyArgs, TemplateExportArgs};
use commands::thumbnail::{ThumbnailCleanArgs, ThumbnailGenerateArgs};
use commands::undo::{Direction as UndoDirection, UndoRedoArgs};
use commands::view::{ViewArgs, ViewCommand};
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
    /// Path to the Progest project root. Defaults to discovering the
    /// nearest `.progest/` directory from the current working directory.
    #[arg(short = 'p', long = "project", global = true)]
    project: Option<PathBuf>,

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
        /// Apply a template TOML file after initialization.
        #[arg(long)]
        template: Option<PathBuf>,
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
    /// Move a file to the OS trash (and its .meta sidecar if present).
    Delete {
        /// Project-relative path to the file to delete.
        #[arg(value_name = "PATH")]
        path: String,
        /// Preview only, don't actually delete.
        #[arg(long)]
        dry_run: bool,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        force: bool,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Import external files into the project (copy by default, --move to relocate).
    Import {
        /// Files to import (absolute or relative paths).
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
        /// Destination directory inside the project (project-relative).
        /// Defaults to project root.
        #[arg(long)]
        dest: Option<String>,
        /// Move files instead of copying (destructive).
        #[arg(long = "move")]
        is_move: bool,
        /// Preview only, don't actually import.
        #[arg(long)]
        dry_run: bool,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Search files using the Progest query DSL.
    Search {
        /// The query string (e.g. `tag:character type:psd is:violation`).
        /// Either `query` or `--view` must be provided.
        query: Option<String>,
        /// Use the saved view of this id (from `.progest/views.toml`).
        #[arg(long)]
        view: Option<String>,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
        /// Print validator warnings to stderr (text mode only).
        #[arg(long)]
        explain: bool,
    },
    /// Manage per-file tags.
    Tag {
        #[command(subcommand)]
        op: TagOp,
    },
    /// Manage saved views in `.progest/views.toml`.
    View {
        #[command(subcommand)]
        op: ViewOp,
    },
    /// Manage project templates.
    Template {
        #[command(subcommand)]
        op: TemplateOp,
    },
    /// Generate or manage thumbnail cache.
    Thumbnail {
        #[command(subcommand)]
        op: ThumbnailOp,
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

#[derive(Debug, Subcommand)]
enum TagOp {
    /// Add a tag to one or more files.
    Add {
        tag: String,
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Remove a tag from one or more files.
    Remove {
        tag: String,
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// List the tags attached to one or more files.
    List {
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ViewDisplayArg {
    List,
    Grid,
}

impl ViewDisplayArg {
    fn into_core(self) -> progest_core::search::views::ViewDisplay {
        match self {
            Self::List => progest_core::search::views::ViewDisplay::List,
            Self::Grid => progest_core::search::views::ViewDisplay::Grid,
        }
    }
}

#[derive(Debug, Subcommand)]
enum ViewOp {
    /// Save (or replace) a view by id.
    Save {
        id: String,
        #[arg(long)]
        query: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        group_by: Option<String>,
        /// Display mode hint persisted with the view (flat view).
        /// Defaults to `list`.
        #[arg(long, value_enum)]
        display: Option<ViewDisplayArg>,
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Delete a view by id.
    Delete {
        id: String,
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// List all saved views.
    List {
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum TemplateOp {
    /// Export the current project as a template TOML file.
    Export {
        /// Output file path (required).
        #[arg(long)]
        out: PathBuf,
        /// Comma-separated list of sections to include: rules, schema, views, dirmeta, or `all`.
        /// Directory structure is always included.
        #[arg(long, default_value = "all")]
        include: String,
        /// Template name (defaults to the project name).
        #[arg(long)]
        name: Option<String>,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Apply a template TOML to the current project.
    Apply {
        /// Path to the template TOML file.
        #[arg(value_name = "FILE")]
        template: PathBuf,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum ThumbnailOp {
    /// Generate thumbnails for project files.
    Generate {
        /// Files to generate thumbnails for (project-relative or absolute paths).
        /// When omitted, generates for all indexed files with supported formats.
        #[arg(value_name = "PATH")]
        paths: Vec<std::path::PathBuf>,
        /// Regenerate even if a cached thumbnail exists.
        #[arg(long)]
        force: bool,
        /// Max dimension in pixels (default 256).
        #[arg(long, default_value = "256")]
        size: u32,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,
    },
    /// Remove orphan thumbnails and evict LRU entries over the capacity limit.
    Clean {
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
    let cwd = match cli.project {
        Some(ref p) => dunce::canonicalize(p)
            .with_context(|| format!("resolving project path `{}`", p.display()))?,
        None => std::env::current_dir()?,
    };
    match cli.command {
        Command::Init { name, template } => {
            commands::init::run(&cwd, name, template)?;
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
        Command::Delete {
            path,
            dry_run,
            force,
            format,
        } => {
            let code = commands::delete::run(
                &cwd,
                &DeleteArgs {
                    path,
                    dry_run,
                    force,
                    format,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Import {
            files,
            dest,
            is_move,
            dry_run,
            format,
        } => {
            let code = commands::import::run(
                &cwd,
                &ImportArgs {
                    files,
                    dest,
                    is_move,
                    dry_run,
                    format,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Search {
            query,
            view,
            format,
            explain,
        } => {
            let code = commands::search::run(
                &cwd,
                &SearchArgs {
                    query,
                    view,
                    format,
                    explain,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::Template { op } => {
            let code = match op {
                TemplateOp::Export {
                    out,
                    include,
                    name,
                    format,
                } => commands::template::run_export(
                    &cwd,
                    &TemplateExportArgs {
                        out,
                        include,
                        name,
                        format,
                    },
                )?,
                TemplateOp::Apply { template, format } => {
                    commands::template::run_apply(&cwd, &TemplateApplyArgs { template, format })?
                }
            };
            Ok(to_exit_code(code))
        }
        Command::Thumbnail { op } => {
            let code = match op {
                ThumbnailOp::Generate {
                    paths,
                    force,
                    size,
                    format,
                } => commands::thumbnail::run_generate(
                    &cwd,
                    &ThumbnailGenerateArgs {
                        paths,
                        format,
                        force,
                        size,
                    },
                )?,
                ThumbnailOp::Clean { format } => {
                    commands::thumbnail::run_clean(&cwd, &ThumbnailCleanArgs { format })?
                }
            };
            Ok(to_exit_code(code))
        }
        Command::Tag { op } => {
            let (cmd, format) = match op {
                TagOp::Add { tag, files, format } => (TagCommand::Add { tag, files }, format),
                TagOp::Remove { tag, files, format } => (TagCommand::Remove { tag, files }, format),
                TagOp::List { files, format } => (TagCommand::List { files }, format),
            };
            let code = commands::tag::run(
                &cwd,
                &TagArgs {
                    command: cmd,
                    format,
                },
            )?;
            Ok(to_exit_code(code))
        }
        Command::View { op } => {
            let (cmd, format) = match op {
                ViewOp::Save {
                    id,
                    query,
                    name,
                    description,
                    group_by,
                    display,
                    format,
                } => (
                    ViewCommand::Save {
                        id,
                        name,
                        query,
                        description,
                        group_by,
                        display: display.map(ViewDisplayArg::into_core),
                    },
                    format,
                ),
                ViewOp::Delete { id, format } => (ViewCommand::Delete { id }, format),
                ViewOp::List { format } => (ViewCommand::List, format),
            };
            let code = commands::view::run(
                &cwd,
                &ViewArgs {
                    command: cmd,
                    format,
                },
            )?;
            Ok(to_exit_code(code))
        }
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
