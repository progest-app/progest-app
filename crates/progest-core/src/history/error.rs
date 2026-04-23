//! Error type shared by the history layer.

use thiserror::Error;

use super::migration::MigrationError;

/// Errors returned by the [`super::Store`] and its `SQLite` backend.
#[derive(Debug, Error)]
pub enum HistoryError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error("failed to encode operation payload: {0}")]
    EncodePayload(serde_json::Error),
    #[error("failed to decode operation payload: {0}")]
    DecodePayload(serde_json::Error),
    #[error("unknown op_kind in history: {0}")]
    InvalidOpKind(String),
    /// Undo requested, but there's nothing to undo — either the log
    /// is empty or every entry is already `consumed`.
    #[error("undo stack is empty")]
    UndoEmpty,
    /// Redo requested, but there's nothing to redo — no entry past
    /// the pointer has been `consumed`.
    #[error("redo stack is empty")]
    RedoEmpty,
}
