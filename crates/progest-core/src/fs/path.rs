//! Project-root-relative path type.
//!
//! `ProjectPath` is the only path representation that core modules should
//! hand around. It enforces three invariants:
//!
//! 1. The path is **relative** to the project root (no leading separator,
//!    no drive letter, no `\\?\` prefix).
//! 2. Components are separated by `/` regardless of host OS, so that paths
//!    round-trip identically across macOS and (future) Windows.
//! 3. No `.` or `..` component is allowed — callers must resolve traversal
//!    before constructing a `ProjectPath`.
//!
//! Materialization to an absolute [`PathBuf`] happens at the edge, via
//! [`ProjectPath::to_absolute`], so the rest of the codebase stays
//! platform-neutral.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Errors returned when constructing a [`ProjectPath`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProjectPathError {
    #[error("project path must be relative, got absolute: {0}")]
    Absolute(String),
    #[error("project path must not contain `..`: {0}")]
    ParentTraversal(String),
    #[error("project path must not contain a drive or root prefix: {0}")]
    PrefixedComponent(String),
    #[error("project path contains an invalid component (non-UTF-8 or empty segment): {0}")]
    InvalidComponent(String),
}

/// A project-root-relative, forward-slash separated path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectPath(String);

impl ProjectPath {
    /// The root of the project (empty path).
    #[must_use]
    pub fn root() -> Self {
        Self(String::new())
    }

    /// Build a `ProjectPath` from a forward-slash separated string.
    ///
    /// Accepts `""` (root), `"foo"`, `"foo/bar"`. Rejects absolute paths and
    /// any string containing `..` or empty segments (e.g. `"foo//bar"`).
    pub fn new<S: AsRef<str>>(raw: S) -> Result<Self, ProjectPathError> {
        let raw = raw.as_ref();
        if raw.is_empty() {
            return Ok(Self::root());
        }
        if raw.starts_with('/') || raw.starts_with('\\') {
            return Err(ProjectPathError::Absolute(raw.to_string()));
        }
        if raw.contains("//") || raw.contains("\\\\") {
            return Err(ProjectPathError::InvalidComponent(raw.to_string()));
        }
        Self::from_path(Path::new(raw))
    }

    /// Build a `ProjectPath` from a [`Path`] that is already relative to the
    /// project root. Performs the same validation as [`ProjectPath::new`]
    /// and normalizes platform separators to `/`.
    pub fn from_path(path: &Path) -> Result<Self, ProjectPathError> {
        let display = path.to_string_lossy().into_owned();
        let mut segments = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => {
                    let segment = segment
                        .to_str()
                        .ok_or_else(|| ProjectPathError::InvalidComponent(display.clone()))?;
                    if segment.is_empty() {
                        return Err(ProjectPathError::InvalidComponent(display));
                    }
                    segments.push(segment.to_string());
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(ProjectPathError::ParentTraversal(display));
                }
                Component::RootDir => {
                    return Err(ProjectPathError::Absolute(display));
                }
                Component::Prefix(_) => {
                    return Err(ProjectPathError::PrefixedComponent(display));
                }
            }
        }
        Ok(Self(segments.join("/")))
    }

    /// Derive a `ProjectPath` from an absolute path that sits beneath `root`.
    ///
    /// Returns an error if `absolute` is not rooted at `root`.
    pub fn from_absolute(root: &Path, absolute: &Path) -> Result<Self, ProjectPathError> {
        let relative = absolute
            .strip_prefix(root)
            .map_err(|_| ProjectPathError::Absolute(absolute.to_string_lossy().into_owned()))?;
        Self::from_path(relative)
    }

    /// The path as a forward-slash separated string (`""` for root).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` when this path denotes the project root itself.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The parent path, or `None` if this is already the root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        match self.0.rfind('/') {
            Some(idx) => Some(Self(self.0[..idx].to_string())),
            None => Some(Self::root()),
        }
    }

    /// The final path component (`None` if this is the root).
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        if self.is_root() {
            return None;
        }
        Some(match self.0.rfind('/') {
            Some(idx) => &self.0[idx + 1..],
            None => &self.0,
        })
    }

    /// The extension of the final component (e.g. `"psd"` for `"foo.psd"`),
    /// without the leading dot. `None` if the final component has no dot or
    /// begins with one (e.g. `".hidden"`).
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name()?;
        let dot = name.rfind('.')?;
        if dot == 0 {
            return None;
        }
        Some(&name[dot + 1..])
    }

    /// Append a child segment. Each segment is validated the same way as
    /// [`ProjectPath::new`].
    pub fn join<S: AsRef<str>>(&self, child: S) -> Result<Self, ProjectPathError> {
        let child = ProjectPath::new(child)?;
        if child.is_root() {
            return Ok(self.clone());
        }
        if self.is_root() {
            return Ok(child);
        }
        Ok(Self(format!("{}/{}", self.0, child.0)))
    }

    /// Materialize to an absolute path by prepending the project root.
    #[must_use]
    pub fn to_absolute(&self, root: &Path) -> PathBuf {
        if self.is_root() {
            return root.to_path_buf();
        }
        let mut buf = root.to_path_buf();
        for segment in self.0.split('/') {
            buf.push(segment);
        }
        buf
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for ProjectPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProjectPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_empty_and_has_no_parent_or_name() {
        let root = ProjectPath::root();
        assert!(root.is_root());
        assert_eq!(root.as_str(), "");
        assert_eq!(root.parent(), None);
        assert_eq!(root.file_name(), None);
        assert_eq!(root.extension(), None);
    }

    #[test]
    fn new_accepts_relative_forward_slash_paths() {
        let p = ProjectPath::new("assets/foo.psd").unwrap();
        assert_eq!(p.as_str(), "assets/foo.psd");
        assert_eq!(p.file_name(), Some("foo.psd"));
        assert_eq!(p.extension(), Some("psd"));
        assert_eq!(p.parent().unwrap().as_str(), "assets");
    }

    #[test]
    fn new_rejects_absolute_paths() {
        assert!(matches!(
            ProjectPath::new("/abs/path"),
            Err(ProjectPathError::Absolute(_))
        ));
    }

    #[test]
    fn new_rejects_parent_traversal() {
        assert!(matches!(
            ProjectPath::new("foo/../bar"),
            Err(ProjectPathError::ParentTraversal(_))
        ));
    }

    #[test]
    fn new_rejects_empty_segments() {
        assert!(matches!(
            ProjectPath::new("foo//bar"),
            Err(ProjectPathError::InvalidComponent(_))
        ));
    }

    #[test]
    fn new_normalizes_cur_dir() {
        let p = ProjectPath::new("foo/./bar").unwrap();
        assert_eq!(p.as_str(), "foo/bar");
    }

    #[test]
    fn join_appends_segments() {
        let base = ProjectPath::new("assets").unwrap();
        let joined = base.join("foo.psd").unwrap();
        assert_eq!(joined.as_str(), "assets/foo.psd");
    }

    #[test]
    fn join_on_root_returns_child() {
        let root = ProjectPath::root();
        assert_eq!(root.join("assets").unwrap().as_str(), "assets");
    }

    #[test]
    fn join_rejects_absolute_child() {
        let base = ProjectPath::new("assets").unwrap();
        assert!(base.join("/oops").is_err());
    }

    #[test]
    fn extension_returns_none_for_dotfiles() {
        let p = ProjectPath::new(".gitignore").unwrap();
        assert_eq!(p.extension(), None);
    }

    #[test]
    fn to_absolute_prefixes_root() {
        let p = ProjectPath::new("assets/foo.psd").unwrap();
        let abs = p.to_absolute(Path::new("/tmp/project"));
        assert_eq!(abs, PathBuf::from("/tmp/project/assets/foo.psd"));
    }

    #[test]
    fn to_absolute_root_returns_root() {
        let p = ProjectPath::root();
        let abs = p.to_absolute(Path::new("/tmp/project"));
        assert_eq!(abs, PathBuf::from("/tmp/project"));
    }

    #[test]
    fn from_absolute_strips_root() {
        let root = Path::new("/tmp/project");
        let abs = Path::new("/tmp/project/assets/foo.psd");
        let p = ProjectPath::from_absolute(root, abs).unwrap();
        assert_eq!(p.as_str(), "assets/foo.psd");
    }

    #[test]
    fn from_absolute_rejects_outside_root() {
        let root = Path::new("/tmp/project");
        let abs = Path::new("/elsewhere/foo");
        assert!(ProjectPath::from_absolute(root, abs).is_err());
    }

    #[test]
    fn serializes_as_forward_slash_string() {
        let p = ProjectPath::new("assets/foo.psd").unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "\"assets/foo.psd\"");
    }

    #[test]
    fn deserializes_from_string_and_validates() {
        let p: ProjectPath = serde_json::from_str("\"assets/foo.psd\"").unwrap();
        assert_eq!(p.as_str(), "assets/foo.psd");

        // Absolute paths are rejected at the deserialize boundary.
        let err: Result<ProjectPath, _> = serde_json::from_str("\"/abs\"");
        assert!(err.is_err());
    }

    #[test]
    fn root_round_trips_through_empty_string() {
        let json = serde_json::to_string(&ProjectPath::root()).unwrap();
        assert_eq!(json, "\"\"");
        let back: ProjectPath = serde_json::from_str("\"\"").unwrap();
        assert!(back.is_root());
    }
}
