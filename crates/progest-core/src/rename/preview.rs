//! Preview builder: turn rename intents into a [`RenamePreview`].
//!
//! `build_preview` is intentionally side-effect free — it touches the
//! filesystem only through [`FileSystem::exists`] for `TargetExists`
//! detection. The output is a complete plan that the apply path (next
//! commit) will execute atomically.
//!
//! Conflict detection covers four failure modes:
//!
//! - **`Identity`** — the resolved name equals the current name (no-op).
//! - **`TargetExists`** — `to` exists on disk and isn't the `from` of
//!   any other op in the preview (chains like `foo→bar→baz` are
//!   allowed because the atomic apply stages every move through a
//!   shadow before swapping in).
//! - **`DuplicateTarget`** — two requests resolve to the same `to`.
//! - **`Unresolved`** — the candidate still has holes after running
//!   the chosen [`FillMode`]; `to` falls back to `from` so the wire
//!   never carries a sentinel string.
//!
//! Once an op is flagged Unresolved the later passes skip it: there's
//! no meaningful collision check against a target that doesn't exist.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fs::{FileSystem, ProjectPath, ProjectPathError};
use crate::naming::{
    FillMode, HolePrompter, NameCandidate, UnresolvedHoleError, resolve, resolve_with_prompter,
};

use super::ops::{Conflict, ConflictKind, RenameOp};

/// One requested rename, before conflict detection.
///
/// Callers (CLI, future Tauri layer) hand a slice of these to
/// [`build_preview`]; the builder returns a [`RenamePreview`] with
/// every op annotated.
#[derive(Debug, Clone)]
pub struct RenameRequest {
    pub from: ProjectPath,
    /// Candidate basename produced by `core::naming` (or a hand-built
    /// literal). May contain holes; resolution happens inside
    /// [`build_preview`] via the shared [`FillMode`].
    pub candidate: NameCandidate,
    pub rule_id: Option<String>,
    pub group_id: Option<String>,
}

impl RenameRequest {
    #[must_use]
    pub fn new(from: ProjectPath, candidate: NameCandidate) -> Self {
        Self {
            from,
            candidate,
            rule_id: None,
            group_id: None,
        }
    }

    #[must_use]
    pub fn with_rule_id(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    #[must_use]
    pub fn with_group_id(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }
}

/// Result of [`build_preview`]: a flat list of [`RenameOp`] in the
/// same order as the input requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenamePreview {
    pub ops: Vec<RenameOp>,
}

impl RenamePreview {
    /// `true` when every op is conflict-free (apply can run without
    /// operator override).
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.ops.iter().all(RenameOp::is_clean)
    }

    /// Iterator over conflict-free ops, in input order.
    pub fn clean_ops(&self) -> impl Iterator<Item = &RenameOp> {
        self.ops.iter().filter(|o| o.is_clean())
    }

    /// Iterator over ops carrying at least one conflict.
    pub fn conflicting_ops(&self) -> impl Iterator<Item = &RenameOp> {
        self.ops.iter().filter(|o| !o.is_clean())
    }
}

/// Errors returned by [`build_preview`].
#[derive(Debug, Error)]
pub enum PreviewError {
    /// A request named the project root as its source. Renaming the
    /// root itself is meaningless; surface it as a hard error rather
    /// than producing a degenerate op.
    #[error("rename source must not be the project root")]
    RootSource,
    /// The resolved basename combined with the parent directory could
    /// not be turned into a valid [`ProjectPath`] (e.g. the basename
    /// contained `/`).
    #[error("invalid project path produced by candidate resolution: {0}")]
    Path(#[from] ProjectPathError),
}

/// Build a [`RenamePreview`] from a slice of requests.
///
/// The builder runs in two passes per request:
///
/// 1. Resolve the candidate via [`crate::naming::resolve`] under
///    `fill_mode`. On success the `to` path is `parent.join(basename)`;
///    on failure the op is flagged `Unresolved` and `to` falls back
///    to `from`.
/// 2. After every request is materialized, three sweeps annotate
///    `Identity`, `DuplicateTarget`, and `TargetExists` conflicts.
///
/// `fs` is consulted only via [`FileSystem::exists`].
///
/// # Errors
/// See [`PreviewError`].
///
/// # Panics
/// Internally guarded: a request whose `from` is the project root is
/// rejected up front with [`PreviewError::RootSource`], so the
/// subsequent `parent()` is unreachable in error.
pub fn build_preview(
    requests: &[RenameRequest],
    fill_mode: &FillMode,
    fs: &dyn FileSystem,
) -> Result<RenamePreview, PreviewError> {
    build_preview_inner(requests, fill_mode, None, fs)
}

/// Like [`build_preview`] but resolves holes through `prompter`
/// instead of returning [`ConflictKind::Unresolved`]. Equivalent to
/// running with [`FillMode::Prompt`] but with a real interactive
/// resolver hooked in.
///
/// # Errors
/// See [`PreviewError`].
pub fn build_preview_with_prompter(
    requests: &[RenameRequest],
    prompter: &dyn HolePrompter,
    fs: &dyn FileSystem,
) -> Result<RenamePreview, PreviewError> {
    build_preview_inner(requests, &FillMode::Prompt, Some(prompter), fs)
}

fn build_preview_inner(
    requests: &[RenameRequest],
    fill_mode: &FillMode,
    prompter: Option<&dyn HolePrompter>,
    fs: &dyn FileSystem,
) -> Result<RenamePreview, PreviewError> {
    let mut ops: Vec<RenameOp> = Vec::with_capacity(requests.len());

    for req in requests {
        if req.from.is_root() {
            return Err(PreviewError::RootSource);
        }
        let parent = req.from.parent().expect("non-root path has a parent");

        let resolution = match (fill_mode, prompter) {
            (FillMode::Prompt, Some(p)) => resolve_with_prompter(&req.candidate, p),
            _ => resolve(&req.candidate, fill_mode),
        };
        let (to, mut conflicts) = match resolution {
            Ok(resolution) => {
                let to = parent.join(&resolution.basename)?;
                (to, Vec::new())
            }
            Err(err) => {
                let conflict = unresolved_conflict(&err);
                (req.from.clone(), vec![conflict])
            }
        };

        // Identity check piggy-backs onto the first pass: cheap and
        // makes the later sweeps simpler (they can short-circuit).
        if conflicts.is_empty() && to == req.from {
            conflicts.push(Conflict {
                kind: ConflictKind::Identity,
                message: format!("rename target equals source: {}", req.from),
            });
        }

        ops.push(RenameOp {
            from: req.from.clone(),
            to,
            rule_id: req.rule_id.clone(),
            group_id: req.group_id.clone(),
            conflicts,
        });
    }

    annotate_duplicate_targets(&mut ops);
    annotate_target_exists(&mut ops, fs);

    Ok(RenamePreview { ops })
}

fn unresolved_conflict(err: &UnresolvedHoleError) -> Conflict {
    let message = match err {
        UnresolvedHoleError::HolesRemain { count, origins } => format!(
            "candidate has {count} unresolved hole(s): {}",
            origins.join(", ")
        ),
        UnresolvedHoleError::PromptUnavailable => {
            "FillMode::Prompt requires an interactive resolver; not available in this context"
                .to_string()
        }
        UnresolvedHoleError::PrompterFailed { origin, reason } => {
            format!("interactive resolver failed for '{origin}': {reason}")
        }
    };
    Conflict {
        kind: ConflictKind::Unresolved,
        message,
    }
}

fn annotate_duplicate_targets(ops: &mut [RenameOp]) {
    let mut counts: HashMap<&ProjectPath, usize> = HashMap::new();
    for op in ops.iter() {
        if !is_resolved(op) {
            continue;
        }
        *counts.entry(&op.to).or_insert(0) += 1;
    }
    let dups: HashSet<ProjectPath> = counts
        .into_iter()
        .filter(|&(_, n)| n > 1)
        .map(|(p, _)| p.clone())
        .collect();
    if dups.is_empty() {
        return;
    }
    for op in ops.iter_mut() {
        if !is_resolved(op) {
            continue;
        }
        if dups.contains(&op.to) {
            op.conflicts.push(Conflict {
                kind: ConflictKind::DuplicateTarget,
                message: format!("multiple ops in this preview rename to {}", op.to),
            });
        }
    }
}

fn annotate_target_exists(ops: &mut [RenameOp], fs: &dyn FileSystem) {
    let froms: HashSet<ProjectPath> = ops.iter().map(|o| o.from.clone()).collect();
    for op in ops.iter_mut() {
        if !is_resolved(op) {
            continue;
        }
        // Identity already flagged above; skip to keep messages tidy.
        if op.from == op.to {
            continue;
        }
        // Target is the source of another op: a chain (foo→bar→baz).
        // Atomic apply handles this via shadow staging, so it's not a
        // collision.
        if froms.contains(&op.to) {
            continue;
        }
        if fs.exists(&op.to) {
            op.conflicts.push(Conflict {
                kind: ConflictKind::TargetExists,
                message: format!("target already exists on disk: {}", op.to),
            });
        }
    }
}

fn is_resolved(op: &RenameOp) -> bool {
    !op.conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::Unresolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;
    use crate::naming::types::{Hole, HoleKind, NameCandidate, Segment};

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn literal_candidate(stem: &str, ext: &str) -> NameCandidate {
        NameCandidate {
            segments: vec![Segment::Literal(stem.into())],
            ext: Some(ext.into()),
        }
    }

    fn hole_candidate() -> NameCandidate {
        NameCandidate {
            segments: vec![
                Segment::Hole(Hole {
                    origin: "カット".into(),
                    kind: HoleKind::Cjk,
                    pos: 0,
                }),
                Segment::Literal("_v01".into()),
            ],
            ext: Some("png".into()),
        }
    }

    fn write(fs: &MemFileSystem, path: &str) {
        fs.write_atomic(&p(path), b"x").unwrap();
    }

    #[test]
    fn single_clean_request_has_no_conflicts() {
        let fs = MemFileSystem::new();
        write(&fs, "assets/old.psd");
        let req = RenameRequest::new(p("assets/old.psd"), literal_candidate("new", "psd"));
        let preview = build_preview(&[req], &FillMode::Skip, &fs).unwrap();
        assert_eq!(preview.ops.len(), 1);
        let op = &preview.ops[0];
        assert_eq!(op.to.as_str(), "assets/new.psd");
        assert!(op.is_clean());
        assert!(preview.is_clean());
    }

    #[test]
    fn rule_id_and_group_id_propagate() {
        let fs = MemFileSystem::new();
        write(&fs, "a.psd");
        let req = RenameRequest::new(p("a.psd"), literal_candidate("b", "psd"))
            .with_rule_id("shot-assets-v1")
            .with_group_id("bulk-7");
        let preview = build_preview(&[req], &FillMode::Skip, &fs).unwrap();
        let op = &preview.ops[0];
        assert_eq!(op.rule_id.as_deref(), Some("shot-assets-v1"));
        assert_eq!(op.group_id.as_deref(), Some("bulk-7"));
    }

    #[test]
    fn identity_when_resolved_name_equals_current() {
        let fs = MemFileSystem::new();
        write(&fs, "same.psd");
        let req = RenameRequest::new(p("same.psd"), literal_candidate("same", "psd"));
        let preview = build_preview(&[req], &FillMode::Skip, &fs).unwrap();
        let op = &preview.ops[0];
        assert!(matches!(
            op.conflicts.as_slice(),
            [Conflict {
                kind: ConflictKind::Identity,
                ..
            }]
        ));
    }

    #[test]
    fn target_exists_when_other_file_already_at_destination() {
        let fs = MemFileSystem::new();
        write(&fs, "a.psd");
        write(&fs, "b.psd"); // pre-existing collision target
        let req = RenameRequest::new(p("a.psd"), literal_candidate("b", "psd"));
        let preview = build_preview(&[req], &FillMode::Skip, &fs).unwrap();
        let op = &preview.ops[0];
        assert_eq!(op.to.as_str(), "b.psd");
        assert!(
            op.conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::TargetExists)
        );
    }

    #[test]
    fn chain_of_renames_does_not_conflict() {
        let fs = MemFileSystem::new();
        write(&fs, "foo.psd");
        write(&fs, "bar.psd");
        // Chain: foo→bar (bar exists on disk but is also being moved out)
        //        bar→baz
        let reqs = [
            RenameRequest::new(p("foo.psd"), literal_candidate("bar", "psd")),
            RenameRequest::new(p("bar.psd"), literal_candidate("baz", "psd")),
        ];
        let preview = build_preview(&reqs, &FillMode::Skip, &fs).unwrap();
        assert!(
            preview.is_clean(),
            "chain renames should not conflict: {preview:?}"
        );
    }

    #[test]
    fn duplicate_target_flags_every_offender() {
        let fs = MemFileSystem::new();
        write(&fs, "a.psd");
        write(&fs, "b.psd");
        let reqs = [
            RenameRequest::new(p("a.psd"), literal_candidate("c", "psd")),
            RenameRequest::new(p("b.psd"), literal_candidate("c", "psd")),
        ];
        let preview = build_preview(&reqs, &FillMode::Skip, &fs).unwrap();
        for op in &preview.ops {
            assert!(
                op.conflicts
                    .iter()
                    .any(|c| c.kind == ConflictKind::DuplicateTarget),
                "expected DuplicateTarget on op: {op:?}"
            );
        }
    }

    #[test]
    fn unresolved_under_skip_records_holes_and_falls_back_to_from() {
        let fs = MemFileSystem::new();
        write(&fs, "assets/カット_v01.png");
        let req = RenameRequest::new(p("assets/カット_v01.png"), hole_candidate());
        let preview = build_preview(&[req], &FillMode::Skip, &fs).unwrap();
        let op = &preview.ops[0];
        assert_eq!(op.to, op.from, "unresolved ops must not invent a target");
        let conflicts = &op.conflicts;
        assert_eq!(
            conflicts.len(),
            1,
            "unresolved op should not also be flagged Identity / TargetExists: {conflicts:?}"
        );
        assert_eq!(conflicts[0].kind, ConflictKind::Unresolved);
        assert!(conflicts[0].message.contains("カット"));
    }

    #[test]
    fn prompt_mode_without_resolver_is_unresolved() {
        let fs = MemFileSystem::new();
        write(&fs, "x.png");
        let req = RenameRequest::new(p("x.png"), hole_candidate());
        let preview = build_preview(&[req], &FillMode::Prompt, &fs).unwrap();
        let op = &preview.ops[0];
        assert_eq!(op.conflicts.len(), 1);
        assert_eq!(op.conflicts[0].kind, ConflictKind::Unresolved);
        assert!(op.conflicts[0].message.contains("Prompt"));
    }

    #[test]
    fn placeholder_mode_resolves_holes_into_target() {
        let fs = MemFileSystem::new();
        write(&fs, "x.png");
        let req = RenameRequest::new(p("x.png"), hole_candidate());
        let preview = build_preview(&[req], &FillMode::Placeholder("_".into()), &fs).unwrap();
        let op = &preview.ops[0];
        assert!(op.is_clean(), "placeholder fills holes: {op:?}");
        assert_eq!(op.to.as_str(), "__v01.png");
    }

    #[test]
    fn root_source_returns_error() {
        let fs = MemFileSystem::new();
        // Directly construct a request with the root path. The CLI would
        // never produce this, but the type system allows it, so we
        // surface a clean error rather than panic in `parent()`.
        let req = RenameRequest::new(ProjectPath::root(), literal_candidate("x", "psd"));
        let err = build_preview(&[req], &FillMode::Skip, &fs).unwrap_err();
        assert!(matches!(err, PreviewError::RootSource));
    }

    struct StubPrompter(String);
    impl HolePrompter for StubPrompter {
        fn prompt(
            &self,
            _: &crate::naming::types::Hole,
        ) -> Result<String, crate::naming::PromptError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn build_preview_with_prompter_resolves_holes_into_target() {
        let fs = MemFileSystem::new();
        write(&fs, "x.png");
        let req = RenameRequest::new(p("x.png"), hole_candidate());
        let preview =
            build_preview_with_prompter(&[req], &StubPrompter("scene".into()), &fs).unwrap();
        let op = &preview.ops[0];
        assert!(op.is_clean(), "prompter resolves holes: {op:?}");
        assert_eq!(op.to.as_str(), "scene_v01.png");
    }

    #[test]
    fn preview_helpers_partition_clean_and_conflicting_ops() {
        let fs = MemFileSystem::new();
        write(&fs, "a.psd");
        write(&fs, "b.psd");
        let reqs = [
            RenameRequest::new(p("a.psd"), literal_candidate("aa", "psd")),
            // Conflict: target b.psd exists and isn't being moved out.
            RenameRequest::new(p("a.psd"), literal_candidate("b", "psd")),
        ];
        let preview = build_preview(&reqs, &FillMode::Skip, &fs).unwrap();
        assert_eq!(preview.clean_ops().count(), 1);
        assert_eq!(preview.conflicting_ops().count(), 1);
        assert!(!preview.is_clean());
    }
}
