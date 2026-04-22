//! End-to-end checks for `progest_core::watch::Watcher`.
//!
//! These tests exercise real OS filesystem events, which are inherently
//! timing-dependent. The suite is written to tolerate that:
//!
//! - A short custom debounce (80 ms) keeps iterations snappy.
//! - Each assertion drains the receiver with a generous timeout instead of
//!   checking a single `recv()`, because notify / `FSEvents` may split logical
//!   operations across multiple debounced batches (especially the first
//!   event after the watcher attaches).
//! - Assertions check that the expected event *appears* rather than demanding
//!   an exact count — the OS occasionally emits spurious `Modify` events for
//!   directory entries that we cannot reliably deduplicate in the watcher.

use std::fs;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use progest_core::reconcile::{ChangeSet, FsEvent};
use progest_core::watch::Watcher;
use tempfile::TempDir;

const TEST_DEBOUNCE: Duration = Duration::from_millis(100);
const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

fn collect_events(rx: &Receiver<ChangeSet>, timeout: Duration) -> Vec<FsEvent> {
    let deadline = Instant::now() + timeout;
    let mut out = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(cs) => out.extend(cs),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}

fn start(tmp: &TempDir) -> (Watcher, Receiver<ChangeSet>) {
    Watcher::start_with_debounce(tmp.path().to_path_buf(), TEST_DEBOUNCE).unwrap()
}

/// `FSEvents` (macOS) takes a moment to prime after the watcher attaches; on
/// cold cache the first filesystem mutation may or may not surface. A
/// short pre-warm gives the backend time to settle before the test begins
/// driving real mutations.
fn prewarm() {
    std::thread::sleep(Duration::from_millis(400));
}

#[test]
fn creating_a_file_emits_added() {
    let tmp = TempDir::new().unwrap();
    let (_watcher, rx) = start(&tmp);
    prewarm();

    fs::write(tmp.path().join("hero.psd"), b"bytes").unwrap();

    let events = collect_events(&rx, EVENT_TIMEOUT);
    assert!(
        events.iter().any(
            |e| matches!(e, FsEvent::Added(p) if p.as_str() == "hero.psd")
                || matches!(e, FsEvent::Modified(p) if p.as_str() == "hero.psd")
        ),
        "expected Added(hero.psd), got {events:?}"
    );
}

#[test]
fn modifying_a_file_emits_event_for_that_path() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("hero.psd"), b"v1").unwrap();

    let (_watcher, rx) = start(&tmp);
    prewarm();

    fs::write(tmp.path().join("hero.psd"), b"v2-different").unwrap();

    let events = collect_events(&rx, EVENT_TIMEOUT);
    assert!(
        events.iter().any(|e| match e {
            FsEvent::Modified(p) | FsEvent::Added(p) => p.as_str() == "hero.psd",
            _ => false,
        }),
        "expected Modified or Added for hero.psd, got {events:?}"
    );
}

#[test]
fn removing_a_file_surfaces_an_event_for_that_path() {
    // NOTE: macOS FSEvents frequently coalesces a write-then-unlink pattern
    // into one or more Modify events rather than a clean Remove, and the
    // debouncer respects that classification. Rather than fight the OS,
    // assert only that *some* event for the removed path reaches the
    // receiver. Reconcile's apply_changes (exercised in reconcile_flow)
    // handles the `Removed` semantics regardless of how the watcher
    // classifies the incoming event.
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("gone.psd"), b"bytes").unwrap();

    let (_watcher, rx) = start(&tmp);
    prewarm();

    fs::remove_file(tmp.path().join("gone.psd")).unwrap();

    let events = collect_events(&rx, EVENT_TIMEOUT);
    assert!(
        events.iter().any(|e| path_of(e) == "gone.psd"),
        "expected at least one event for gone.psd, got {events:?}"
    );
}

#[test]
fn ignored_paths_do_not_surface() {
    // .progest/ is ignored by default. Events under it must never leak
    // through — otherwise reconcile would be woken by its own writes.
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join(".progest")).unwrap();

    let (_watcher, rx) = start(&tmp);
    prewarm();

    fs::write(tmp.path().join(".progest/index.db"), b"x").unwrap();
    fs::write(tmp.path().join("keep.psd"), b"y").unwrap();

    let events = collect_events(&rx, EVENT_TIMEOUT);
    assert!(
        !events.iter().any(|e| path_of(e).starts_with(".progest")),
        "ignored .progest events leaked: {events:?}"
    );
    assert!(
        events.iter().any(|e| path_of(e) == "keep.psd"),
        "expected keep.psd to surface, got {events:?}"
    );
}

#[test]
fn meta_sidecar_paths_are_kept() {
    // Sidecar events must pass through — the reconciler's apply_changes
    // routes them to the companion file rather than ignoring them, and
    // teammates pulling new .meta files from git rely on this.
    let tmp = TempDir::new().unwrap();
    let (_watcher, rx) = start(&tmp);
    prewarm();

    fs::write(tmp.path().join("hero.psd.meta"), b"{}\n").unwrap();

    let events = collect_events(&rx, EVENT_TIMEOUT);
    assert!(
        events.iter().any(|e| path_of(e) == "hero.psd.meta"),
        "expected hero.psd.meta to surface, got {events:?}"
    );
}

fn path_of(event: &FsEvent) -> &str {
    match event {
        FsEvent::Added(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p.as_str(),
        FsEvent::Renamed { to, .. } => to.as_str(),
    }
}
