//! Implementation of each `progest` subcommand.
//!
//! Each submodule owns the I/O concerns (argument parsing, printing
//! summaries, exit codes) for one or more related subcommands and
//! defers all non-trivial logic to `progest-core` so the CLI stays
//! interchangeable with the future GUI / IPC layer. Most modules
//! expose a single `run` entry point; [`undo`] handles both
//! `progest undo` and `progest redo` through a shared driver, and
//! [`clean`] re-exports a few flag enums (`CaseFlag`, `FillFlag`)
//! that [`rename`] also uses since the two subcommands share their
//! cleanup pipeline knobs.

pub mod clean;
pub mod doctor;
pub mod init;
pub mod lint;
pub mod rename;
pub mod scan;
pub mod undo;
