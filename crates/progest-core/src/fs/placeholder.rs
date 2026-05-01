//! Cloud storage placeholder detection.
//!
//! On Windows, `OneDrive` (and similar cloud sync providers) create
//! "placeholder" files that appear in directory listings but whose
//! content has not been downloaded. Reading or fingerprinting them
//! triggers a potentially slow and unwanted download.
//!
//! [`is_cloud_placeholder`] detects these files via the
//! `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` and
//! `FILE_ATTRIBUTE_RECALL_ON_OPEN` file attributes so that the
//! scanner and reconciler can skip them.

use std::path::Path;

/// Returns `true` if `path` is a cloud storage placeholder file
/// that has not been fully downloaded.
///
/// On non-Windows platforms this always returns `false`.
#[cfg(windows)]
pub fn is_cloud_placeholder(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|m| {
            let attrs = m.file_attributes();
            // FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS = 0x00400000
            // FILE_ATTRIBUTE_RECALL_ON_OPEN        = 0x00200000
            (attrs & 0x0040_0000 != 0) || (attrs & 0x0020_0000 != 0)
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_cloud_placeholder(_path: &Path) -> bool {
    false
}
