use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub current: u64,
    pub total: u64,
    pub message: String,
}
