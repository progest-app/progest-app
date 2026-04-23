//! Implementation of each `progest` subcommand.
//!
//! Each submodule exposes a single `run` function that the top-level CLI
//! dispatches to. Handlers own I/O concerns (printing summaries, picking
//! the working directory) but defer all non-trivial logic to
//! `progest-core`, keeping them interchangeable with the future GUI / IPC
//! layer.

pub mod clean;
pub mod doctor;
pub mod init;
pub mod scan;
