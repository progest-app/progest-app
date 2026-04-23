//! Bridge between `core::sequence` and `core::rename`.
//!
//! [`requests_from_sequence`] turns a detected [`Sequence`] into one
//! [`RenameRequest`] per member, replacing the stem prefix while
//! preserving the separator, numeric index, zero-padding, and
//! extension. Every emitted request shares a freshly-generated
//! `group_id` so the apply layer's auto-grouping path doesn't kick
//! in (caller intent wins) and undo can reverse the whole sequence
//! as one unit.
//!
//! Stem-only replacement is the safe default: the user said `frame_*`
//! → `shot_*`, not "renumber the whole batch from 0". Renumbering is
//! a separate operation and lands as a follow-up.

use uuid::Uuid;

use crate::naming::types::{NameCandidate, Segment};
use crate::sequence::Sequence;

use super::preview::RenameRequest;

/// Render one [`RenameRequest`] per member of `seq`, replacing each
/// member's `stem_prefix` with `new_stem`.
///
/// Members are processed in the order they appear in
/// [`Sequence::members`] (already sorted by index ascending). Every
/// returned request carries the same `group_id`.
#[must_use]
pub fn requests_from_sequence(seq: &Sequence, new_stem: &str) -> Vec<RenameRequest> {
    let group_id = format!("seq-{}", Uuid::now_v7().simple());
    seq.members
        .iter()
        .map(|member| {
            let pad = seq.padding;
            let rendered_stem = format!(
                "{new_stem}{sep}{idx:0pad$}",
                sep = seq.separator,
                idx = member.index,
            );
            let candidate = NameCandidate {
                segments: vec![Segment::Literal(rendered_stem)],
                ext: Some(seq.extension.clone()),
            };
            RenameRequest::new(member.path.clone(), candidate).with_group_id(group_id.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::ProjectPath;
    use crate::sequence::detect_sequences;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn seq_from(paths: &[ProjectPath]) -> Sequence {
        let detection = detect_sequences(paths);
        assert_eq!(
            detection.sequences.len(),
            1,
            "expected exactly one sequence in fixture"
        );
        detection.sequences.into_iter().next().unwrap()
    }

    #[test]
    fn replaces_stem_preserving_index_padding_and_extension() {
        let seq = seq_from(&[
            p("shots/sc01/frame_0001.exr"),
            p("shots/sc01/frame_0002.exr"),
            p("shots/sc01/frame_0003.exr"),
        ]);
        let reqs = requests_from_sequence(&seq, "shot");
        assert_eq!(reqs.len(), 3);

        // The candidate encodes the new basename literally; resolve
        // via FillMode::Skip to render the final string for assertion.
        let basenames: Vec<String> = reqs
            .iter()
            .map(|r| {
                crate::naming::resolve(&r.candidate, &crate::naming::FillMode::Skip)
                    .unwrap()
                    .basename
            })
            .collect();
        assert_eq!(
            basenames,
            vec!["shot_0001.exr", "shot_0002.exr", "shot_0003.exr"]
        );
    }

    #[test]
    fn preserves_each_member_original_path_as_from() {
        let seq = seq_from(&[p("frame_001.exr"), p("frame_002.exr")]);
        let reqs = requests_from_sequence(&seq, "shot");
        let froms: Vec<&str> = reqs.iter().map(|r| r.from.as_str()).collect();
        assert_eq!(froms, vec!["frame_001.exr", "frame_002.exr"]);
    }

    #[test]
    fn all_members_share_one_group_id() {
        let seq = seq_from(&[p("a_001.exr"), p("a_002.exr"), p("a_003.exr")]);
        let reqs = requests_from_sequence(&seq, "b");
        let groups: Vec<Option<&str>> = reqs.iter().map(|r| r.group_id.as_deref()).collect();
        let first = groups[0].expect("group_id must be set");
        for g in &groups {
            assert_eq!(*g, Some(first));
        }
    }

    #[test]
    fn distinct_calls_yield_distinct_group_ids() {
        let seq = seq_from(&[p("a_001.exr"), p("a_002.exr")]);
        let r1 = requests_from_sequence(&seq, "x");
        let r2 = requests_from_sequence(&seq, "x");
        assert_ne!(r1[0].group_id, r2[0].group_id);
    }

    #[test]
    fn separator_and_extension_are_preserved_verbatim() {
        // Dot-separated sequence (`shot.0001.exr`).
        let seq = seq_from(&[p("shot.0001.exr"), p("shot.0002.exr")]);
        let reqs = requests_from_sequence(&seq, "render");
        let basename = crate::naming::resolve(&reqs[0].candidate, &crate::naming::FillMode::Skip)
            .unwrap()
            .basename;
        assert_eq!(basename, "render.0001.exr");
    }

    #[test]
    fn empty_separator_renders_without_inserting_one() {
        let seq = seq_from(&[p("shot0001.exr"), p("shot0002.exr")]);
        let reqs = requests_from_sequence(&seq, "frame");
        let basename = crate::naming::resolve(&reqs[0].candidate, &crate::naming::FillMode::Skip)
            .unwrap()
            .basename;
        assert_eq!(basename, "frame0001.exr");
    }
}
