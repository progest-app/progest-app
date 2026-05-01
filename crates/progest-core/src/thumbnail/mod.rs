pub mod cache;
pub mod ffmpeg;
pub mod generate;
pub mod types;

pub use cache::ThumbnailCache;
pub use generate::{generate_batch, generate_batch_with_progress};
pub use types::*;

use std::path::Path;

use crate::index::Index;
use crate::reconcile::ReconcileOutcome;

pub fn generate_for_outcomes(
    outcomes: &[ReconcileOutcome],
    cache: &ThumbnailCache,
    project_root: &Path,
    index: &dyn Index,
) -> GenerateBatchReport {
    let requests: Vec<ThumbnailRequest> = outcomes
        .iter()
        .filter_map(|outcome| {
            let (ReconcileOutcome::Added { file_id, path }
            | ReconcileOutcome::Updated { file_id, path }) = outcome
            else {
                return None;
            };

            let ext = path.as_str().rsplit_once('.')?.1;
            if !is_supported_extension(ext) {
                return None;
            }

            let row = index.get_file(file_id).ok()??;
            Some(ThumbnailRequest {
                path: path.clone(),
                abs_path: project_root.join(path.as_str()),
                file_id: row.file_id,
                fingerprint: row.fingerprint,
                size: DEFAULT_MAX_DIM,
            })
        })
        .collect();

    if requests.is_empty() {
        return GenerateBatchReport::default();
    }
    generate_batch(&requests, cache, false)
}

pub fn requests_from_index(
    index: &dyn Index,
    project_root: &Path,
    size: u32,
) -> Vec<ThumbnailRequest> {
    let Ok(rows) = index.list_files() else {
        return Vec::new();
    };
    rows.into_iter()
        .filter_map(|row| {
            let ext = row.path.as_str().rsplit_once('.')?.1;
            if !is_supported_extension(ext) {
                return None;
            }
            Some(ThumbnailRequest {
                abs_path: project_root.join(row.path.as_str()),
                path: row.path,
                file_id: row.file_id,
                fingerprint: row.fingerprint,
                size,
            })
        })
        .collect()
}
