//! Filesystem change descriptions consumed by [`super::Reconciler::apply_changes`].
//!
//! The watch layer (landing in a follow-up PR) translates raw `notify` events
//! into a debounced [`ChangeSet`] before handing it off. Keeping the type in
//! the reconcile module — rather than under `watch` — means a reconciler that
//! runs without any live watcher (startup scans, periodic CLI runs, tests)
//! can still reuse the same application path by constructing a `ChangeSet`
//! directly.
//!
//! Event semantics follow the requirements doc §4.5:
//!
//! - `Added` — a newly discovered file; may or may not already carry a `.meta`.
//! - `Modified` — the file on disk changed (size, mtime, or contents). The
//!   reconciler decides whether the fingerprint needs recomputing.
//! - `Removed` — the file disappeared. Orphan `.meta` handling happens
//!   separately so the watch layer doesn't have to special-case it.
//! - `Renamed { from, to }` — both paths are already known to be the same
//!   `file_id`; the reconciler keeps identity and updates the indexed path.
//!
//! Duplicate or overlapping events are tolerated — the reconciler applies
//! them idempotently against the index and the sidecar, so a conservative
//! debouncer that errs on the side of over-reporting is safe.

use crate::fs::ProjectPath;

/// Single filesystem event as observed after debouncing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Added(ProjectPath),
    Modified(ProjectPath),
    Removed(ProjectPath),
    Renamed { from: ProjectPath, to: ProjectPath },
}

/// Batch of [`FsEvent`]s for a single reconcile pass.
///
/// The wrapper type carries no ordering guarantees today; the reconciler
/// applies events independently. A future change that introduces cross-event
/// dependencies (e.g. a rename that shadows a later add) will live in this
/// type rather than leaking into every call site.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    events: Vec<FsEvent>,
}

impl ChangeSet {
    /// Build an empty change set; use [`ChangeSet::push`] or
    /// [`ChangeSet::from_events`] to populate it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect an iterator of events into a change set.
    pub fn from_events<I: IntoIterator<Item = FsEvent>>(events: I) -> Self {
        Self {
            events: events.into_iter().collect(),
        }
    }

    /// Append a single event to the batch.
    pub fn push(&mut self, event: FsEvent) {
        self.events.push(event);
    }

    /// `true` when no events were recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Number of events in the batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Iterate over the contained events in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &FsEvent> {
        self.events.iter()
    }
}

impl IntoIterator for ChangeSet {
    type Item = FsEvent;
    type IntoIter = std::vec::IntoIter<FsEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    #[test]
    fn push_accumulates_events_in_order() {
        let mut cs = ChangeSet::new();
        cs.push(FsEvent::Added(p("a.psd")));
        cs.push(FsEvent::Modified(p("a.psd")));
        assert_eq!(cs.len(), 2);
        let events: Vec<_> = cs.iter().cloned().collect();
        assert_eq!(events[0], FsEvent::Added(p("a.psd")));
        assert_eq!(events[1], FsEvent::Modified(p("a.psd")));
    }

    #[test]
    fn from_events_preserves_iterator_order() {
        let cs = ChangeSet::from_events([
            FsEvent::Removed(p("b.psd")),
            FsEvent::Renamed {
                from: p("c.psd"),
                to: p("d.psd"),
            },
        ]);
        assert!(!cs.is_empty());
        assert_eq!(cs.len(), 2);
    }

    #[test]
    fn new_and_default_produce_empty_sets() {
        assert!(ChangeSet::new().is_empty());
        assert!(ChangeSet::default().is_empty());
    }
}
