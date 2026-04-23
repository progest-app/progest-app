//! Detection algorithm: partition a slice of paths into sequences
//! and singletons.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::fs::ProjectPath;

use super::types::{Sequence, SequenceDetection, SequenceMember};

/// Minimum number of members a sequence needs. A solo numbered file
/// (e.g. just `001.exr`) goes to [`SequenceDetection::singletons`].
pub const MIN_MEMBERS: usize = 2;

/// `^(stem)(separator)(digits)\.(ext)$` — see module docs for the
/// grouping rule. `stem` is lazy so the engine prefers the shortest
/// match, leaving the longest possible separator + digit run.
const PATTERN: &str = r"^(?P<stem>.*?)(?P<sep>[._-]?)(?P<num>\d+)\.(?P<ext>[^.]+)$";

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(PATTERN).expect("PATTERN is a compile-time constant"))
}

/// Composite group key: members sharing the full key form one
/// [`Sequence`]. See module docs for the field-by-field semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GroupKey {
    parent: ProjectPath,
    stem_prefix: String,
    separator: String,
    padding: usize,
    extension: String,
}

/// Partition `paths` into [`Sequence`]s and singletons.
///
/// The algorithm is deterministic: re-running with the same input
/// yields identical output (sequences sorted by
/// `(parent, stem_prefix)`, members sorted by `index`, singletons
/// sorted lexicographically).
///
/// Paths whose basenames don't match the trailing-digit pattern, and
/// paths that would join a group with fewer than [`MIN_MEMBERS`]
/// members, both end up in [`SequenceDetection::singletons`].
#[must_use]
pub fn detect_sequences(paths: &[ProjectPath]) -> SequenceDetection {
    let re = pattern();
    let mut groups: HashMap<GroupKey, Vec<SequenceMember>> = HashMap::new();
    let mut singletons: Vec<ProjectPath> = Vec::new();

    for path in paths {
        if path.is_root() {
            continue;
        }
        let Some(basename) = path.file_name() else {
            singletons.push(path.clone());
            continue;
        };
        let parent = path.parent().unwrap_or_else(ProjectPath::root);

        let Some(caps) = re.captures(basename) else {
            singletons.push(path.clone());
            continue;
        };

        let stem = caps.name("stem").map_or("", |m| m.as_str());
        let sep = caps.name("sep").map_or("", |m| m.as_str());
        let num = caps.name("num").map_or("", |m| m.as_str());
        let ext = caps.name("ext").map_or("", |m| m.as_str());

        // Regex guarantees `num` is at least one digit and parses
        // as a non-negative integer; `parse::<u64>` only fails on
        // overflow. Treat overflow as "not a sequence" — there's no
        // realistic frame number that exceeds u64.
        let Ok(index) = num.parse::<u64>() else {
            singletons.push(path.clone());
            continue;
        };

        let key = GroupKey {
            parent,
            stem_prefix: stem.into(),
            separator: sep.into(),
            padding: num.len(),
            extension: ext.into(),
        };
        groups.entry(key).or_default().push(SequenceMember {
            path: path.clone(),
            index,
        });
    }

    let mut sequences: Vec<Sequence> = Vec::new();
    for (key, mut members) in groups {
        if members.len() < MIN_MEMBERS {
            for m in members {
                singletons.push(m.path);
            }
            continue;
        }
        members.sort_by_key(|m| m.index);
        sequences.push(Sequence {
            parent: key.parent,
            stem_prefix: key.stem_prefix,
            separator: key.separator,
            padding: key.padding,
            extension: key.extension,
            members,
        });
    }

    sequences.sort_by(|a, b| {
        a.parent
            .as_str()
            .cmp(b.parent.as_str())
            .then(a.stem_prefix.cmp(&b.stem_prefix))
            .then(a.separator.cmp(&b.separator))
            .then(a.padding.cmp(&b.padding))
            .then(a.extension.cmp(&b.extension))
    });
    singletons.sort();

    SequenceDetection {
        sequences,
        singletons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    #[test]
    fn five_frames_form_one_sequence_no_singletons() {
        let paths: Vec<_> = (1..=5)
            .map(|i| p(&format!("shots/sc01/frame_{i:04}.exr")))
            .collect();
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 1);
        assert!(r.singletons.is_empty());
        let seq = &r.sequences[0];
        assert_eq!(seq.parent.as_str(), "shots/sc01");
        assert_eq!(seq.stem_prefix, "frame");
        assert_eq!(seq.separator, "_");
        assert_eq!(seq.padding, 4);
        assert_eq!(seq.extension, "exr");
        assert_eq!(seq.members.len(), 5);
        assert_eq!(seq.members[0].index, 1);
        assert_eq!(seq.members[4].index, 5);
    }

    #[test]
    fn gaps_allowed_within_a_sequence() {
        let paths = vec![
            p("frame_001.exr"),
            p("frame_002.exr"),
            p("frame_005.exr"), // gap of 003-004
        ];
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 1);
        assert_eq!(r.sequences[0].members.len(), 3);
        let indices: Vec<u64> = r.sequences[0].members.iter().map(|m| m.index).collect();
        assert_eq!(indices, vec![1, 2, 5]);
    }

    #[test]
    fn padding_mismatch_yields_separate_groups() {
        // `frame_001` (padding=3) vs `frame_1` (padding=1) — distinct
        // groups, both below MIN_MEMBERS as singletons of their own
        // groups, so both end up in singletons.
        let paths = vec![p("frame_001.exr"), p("frame_1.exr")];
        let r = detect_sequences(&paths);
        assert!(r.sequences.is_empty());
        assert_eq!(r.singletons.len(), 2);
    }

    #[test]
    fn padding_mismatch_with_two_each_yields_two_sequences() {
        let paths = vec![
            p("frame_001.exr"),
            p("frame_002.exr"),
            p("frame_1.exr"),
            p("frame_2.exr"),
        ];
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 2);
        assert!(r.singletons.is_empty());
        let paddings: Vec<usize> = r.sequences.iter().map(|s| s.padding).collect();
        assert!(paddings.contains(&1) && paddings.contains(&3));
    }

    #[test]
    fn different_extension_yields_separate_groups() {
        let paths = vec![
            p("frame_001.exr"),
            p("frame_002.exr"),
            p("frame_001.png"),
            p("frame_002.png"),
        ];
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 2);
    }

    #[test]
    fn different_parent_yields_separate_groups() {
        let paths = vec![
            p("shots/sc01/frame_001.exr"),
            p("shots/sc01/frame_002.exr"),
            p("shots/sc02/frame_001.exr"),
            p("shots/sc02/frame_002.exr"),
        ];
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 2);
        let parents: Vec<&str> = r.sequences.iter().map(|s| s.parent.as_str()).collect();
        assert!(parents.contains(&"shots/sc01"));
        assert!(parents.contains(&"shots/sc02"));
    }

    #[test]
    fn solo_numbered_file_is_a_singleton() {
        let paths = vec![p("only_one_001.exr")];
        let r = detect_sequences(&paths);
        assert!(r.sequences.is_empty());
        assert_eq!(r.singletons.len(), 1);
    }

    #[test]
    fn non_numbered_basenames_are_singletons() {
        let paths = vec![p("readme.md"), p("notes.txt")];
        let r = detect_sequences(&paths);
        assert!(r.sequences.is_empty());
        assert_eq!(r.singletons.len(), 2);
    }

    #[test]
    fn separator_styles_are_kept_distinct() {
        // `_`, `.`, `-`, and empty are all separate groups.
        let paths = vec![
            p("a_001.exr"),
            p("a_002.exr"),
            p("a.001.exr"),
            p("a.002.exr"),
            p("a-001.exr"),
            p("a-002.exr"),
            p("a001.exr"),
            p("a002.exr"),
        ];
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 4);
        let seps: Vec<&str> = r.sequences.iter().map(|s| s.separator.as_str()).collect();
        for want in ["_", ".", "-", ""] {
            assert!(seps.contains(&want), "missing separator: '{want}'");
        }
    }

    #[test]
    fn empty_stem_prefix_is_supported() {
        let paths = vec![p("0001.exr"), p("0002.exr")];
        let r = detect_sequences(&paths);
        assert_eq!(r.sequences.len(), 1);
        assert_eq!(r.sequences[0].stem_prefix, "");
        assert_eq!(r.sequences[0].padding, 4);
    }

    #[test]
    fn members_sorted_ascending_regardless_of_input_order() {
        let paths = vec![p("frame_005.exr"), p("frame_001.exr"), p("frame_003.exr")];
        let r = detect_sequences(&paths);
        let indices: Vec<u64> = r.sequences[0].members.iter().map(|m| m.index).collect();
        assert_eq!(indices, vec![1, 3, 5]);
    }

    #[test]
    fn detection_is_deterministic_across_runs() {
        let mut paths = vec![
            p("b/frame_002.exr"),
            p("a/frame_001.exr"),
            p("a/frame_002.exr"),
            p("b/frame_001.exr"),
        ];
        let r1 = detect_sequences(&paths);
        paths.reverse();
        let r2 = detect_sequences(&paths);
        assert_eq!(r1, r2);
    }

    proptest! {
        /// Every input path is accounted for in exactly one place
        /// (sequence member or singleton), and no path is duplicated
        /// in the output.
        #[test]
        fn every_path_is_partitioned_exactly_once(
            stems in proptest::collection::vec("[a-z]{1,4}", 1..=5),
            indices in proptest::collection::vec(1u64..=20, 1..=10),
            paddings in proptest::collection::vec(1usize..=4, 1..=10),
            exts in proptest::collection::vec("(exr|png|psd)", 1..=3),
        ) {
            let mut paths: Vec<ProjectPath> = Vec::new();
            for (i, idx) in indices.iter().enumerate() {
                let stem = &stems[i % stems.len()];
                let pad = paddings[i % paddings.len()];
                let ext = &exts[i % exts.len()];
                let basename = format!("{stem}_{idx:0pad$}.{ext}");
                paths.push(ProjectPath::new(basename).unwrap());
            }
            let r = detect_sequences(&paths);

            let mut seen: Vec<ProjectPath> = Vec::new();
            for s in &r.sequences {
                for m in &s.members {
                    seen.push(m.path.clone());
                }
                prop_assert!(s.members.len() >= MIN_MEMBERS);
                // Members sorted ascending.
                for w in s.members.windows(2) {
                    prop_assert!(w[0].index <= w[1].index);
                }
            }
            for sg in &r.singletons {
                seen.push(sg.clone());
            }

            // Same multiset of paths in vs out.
            let mut input_sorted = paths.clone();
            input_sorted.sort();
            seen.sort();
            prop_assert_eq!(input_sorted, seen);
        }
    }
}
