//! The fixed cleanup pipeline.
//!
//! Ordering is `remove_copy_suffix → remove_cjk → convert_case`. Each
//! stage is individually toggled via [`super::types::CleanupConfig`].
//! The ordering itself is not configurable — see REQUIREMENTS §3.5.5:
//! copy suffixes survive CJK stripping (they're ASCII), and case
//! conversion must see post-strip content so the result is consistent
//! across inputs.
//!
//! Inputs are full basenames (`foo (1).png`). Outputs are
//! [`super::types::NameCandidate`] values with the extension split
//! off. Compound extensions are preserved verbatim via
//! [`crate::rules::BUILTIN_COMPOUND_EXTS`] /
//! [`crate::rules::split_basename`].

use unicode_normalization::UnicodeNormalization as _;

use super::case::convert_case;
use super::types::{CaseStyle, CleanupConfig, Hole, HoleKind, NameCandidate, Segment};
use crate::rules::split_basename;

/// Run the pipeline over `basename` under `cfg`.
///
/// `compound_exts` behaves like the argument of the same name on
/// [`crate::rules::match_basename`]: project-declared compound
/// extensions from `.progest/schema.toml` are merged with the
/// built-in set. Pass `BUILTIN_COMPOUND_EXTS` when the project has
/// no overrides.
///
/// The pipeline is total: given any input, it returns a candidate.
/// Whether the candidate is writable depends on whether it has holes
/// — see [`super::fill::resolve`].
#[must_use]
pub fn clean_basename(
    basename: &str,
    cfg: &CleanupConfig,
    compound_exts: &[&str],
) -> NameCandidate {
    let (stem, ext_opt) = split_basename(basename, compound_exts);

    // Stage 1: copy suffix (ASCII-only; must run before CJK so the
    // Japanese "のコピー" variant is handled by stage 2 only when
    // stage 1 opted out).
    let stem_owned: String = stem.nfc().collect();
    let stem1 = if cfg.remove_copy_suffix {
        remove_copy_suffix(&stem_owned)
    } else {
        stem_owned
    };

    // Stage 2: CJK removal produces segments (may contain holes).
    let segments2 = if cfg.remove_cjk {
        remove_cjk_into_segments(&stem1)
    } else {
        vec![Segment::Literal(stem1)]
    };

    // Stage 3: case conversion applies only to literal segments; holes
    // are opaque. Skipped entirely when the style is `Off`.
    let segments3 = if cfg.convert_case == CaseStyle::Off {
        segments2
    } else {
        apply_case_to_literals(segments2, cfg.convert_case)
    };

    NameCandidate {
        segments: collapse_adjacent_literals(segments3),
        ext: ext_opt.map(str::to_owned),
    }
}

// --- Stage 1: copy-suffix removal ------------------------------------------

/// Strip one OS copy-suffix from the *tail* of `stem` and return the
/// rest. Only the three OS defaults are recognized:
///
/// - macOS / common Unix: ` (N)` where N ≥ 1
/// - Windows Explorer: ` - Copy` and ` - Copy (N)`
/// - Japanese-locale Finder / Explorer: ` のコピー` and ` のコピー N`
///
/// Version tokens like `v01` are preserved by construction (they don't
/// match any of the above shapes). The stripping is non-recursive —
/// `foo (1) (2)` loses `(2)` but keeps `(1)` — because cascading
/// strips risk removing legitimate content (`assets (1)/shot (1)` is
/// a real use case in some studios).
#[must_use]
pub fn remove_copy_suffix(stem: &str) -> String {
    // Order matters: ` - Copy (N)` must be tried before the plain
    // ` (N)` strip, otherwise `strip_paren_number` eats the `(N)` and
    // leaves `foo - Copy` with the dash-copy marker still attached.
    if let Some(rest) = strip_dash_copy(stem) {
        return rest.to_owned();
    }
    if let Some(rest) = strip_japanese_copy(stem) {
        return rest.to_owned();
    }
    if let Some(rest) = strip_paren_number(stem) {
        return rest.to_owned();
    }
    stem.to_owned()
}

fn strip_paren_number(stem: &str) -> Option<&str> {
    // ` (N)` where N is one or more digits.
    let bytes = stem.as_bytes();
    if !bytes.ends_with(b")") {
        return None;
    }
    // Find the matching `(` at `<len - K>` with digits between it and `)`.
    let open = bytes.iter().rposition(|b| *b == b'(')?;
    if open == 0 {
        return None;
    }
    if bytes.get(open - 1) != Some(&b' ') {
        return None;
    }
    let inner = &bytes[open + 1..bytes.len() - 1];
    if inner.is_empty() || !inner.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(&stem[..open - 1])
}

fn strip_dash_copy(stem: &str) -> Option<&str> {
    // ` - Copy` optionally followed by ` (N)`.
    if let Some(rest) = strip_paren_number(stem)
        && let Some(prefix) = rest.strip_suffix(" - Copy")
    {
        return Some(prefix);
    }
    stem.strip_suffix(" - Copy")
}

fn strip_japanese_copy(stem: &str) -> Option<&str> {
    // ` のコピー` with an optional ` N` suffix (Japanese Finder uses a
    // space-delimited counter, not parentheses). We accept the bare
    // form and any digit-only tail.
    const MARKER: &str = " のコピー";
    let idx = stem.rfind(MARKER)?;
    let tail = &stem[idx + MARKER.len()..];
    if tail.is_empty() || tail.bytes().all(|b| b == b' ' || b.is_ascii_digit()) {
        Some(&stem[..idx])
    } else {
        None
    }
}

// --- Stage 2: CJK removal --------------------------------------------------

/// Replace every contiguous run of CJK (Han / Hiragana / Katakana)
/// characters with a [`HoleKind::Cjk`] hole. Runs are not merged
/// across non-CJK characters.
///
/// The classification uses Unicode script ranges hand-listed below —
/// dragging in `unicode-script` for this one use case is more weight
/// than it's worth, and the ranges are well-known.
#[must_use]
pub fn remove_cjk_into_segments(stem: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut buf_literal = String::new();
    let mut buf_cjk = String::new();
    let mut cjk_start: Option<usize> = None;

    for (i, c) in stem.char_indices() {
        if is_cjk_char(c) {
            if !buf_literal.is_empty() {
                out.push(Segment::Literal(std::mem::take(&mut buf_literal)));
            }
            if cjk_start.is_none() {
                cjk_start = Some(i);
            }
            buf_cjk.push(c);
        } else {
            if !buf_cjk.is_empty() {
                out.push(Segment::Hole(Hole {
                    origin: std::mem::take(&mut buf_cjk),
                    kind: HoleKind::Cjk,
                    pos: cjk_start.take().unwrap_or(0),
                }));
            }
            buf_literal.push(c);
        }
    }
    if !buf_cjk.is_empty() {
        out.push(Segment::Hole(Hole {
            origin: buf_cjk,
            kind: HoleKind::Cjk,
            pos: cjk_start.unwrap_or(0),
        }));
    }
    if !buf_literal.is_empty() {
        out.push(Segment::Literal(buf_literal));
    }
    out
}

fn is_cjk_char(c: char) -> bool {
    let u = c as u32;
    // CJK Unified Ideographs (Han): 4E00–9FFF plus Ext-A 3400–4DBF.
    // Ext-B..F and Compat Ideographs are covered by the big ranges.
    // Hiragana: 3040–309F. Katakana: 30A0–30FF. Katakana-phonetic
    // extensions: 31F0–31FF. CJK Compat Ideographs: F900–FAFF.
    matches!(
        u,
        0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0x31F0..=0x31FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
            | 0x2F800..=0x2FA1F
    )
}

// --- Stage 3: case conversion ----------------------------------------------

fn apply_case_to_literals(segments: Vec<Segment>, style: CaseStyle) -> Vec<Segment> {
    segments
        .into_iter()
        .map(|seg| match seg {
            Segment::Literal(s) => {
                // `convert_case` only errors on `Off`, which we've already
                // short-circuited; unwrap is safe here.
                let converted = convert_case(&s, style).unwrap_or(s);
                Segment::Literal(converted)
            }
            Segment::Hole(h) => Segment::Hole(h),
        })
        .collect()
}

// --- Post-process ----------------------------------------------------------

/// The pipeline can produce adjacent `Literal` segments (e.g. when CJK
/// removal splits around a hole and case conversion leaves the
/// literals unchanged). Downstream consumers expect a canonical form
/// where literals never touch; collapse them here.
#[must_use]
pub fn collapse_adjacent_literals(segments: Vec<Segment>) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
    for seg in segments {
        match (out.last_mut(), seg) {
            (Some(Segment::Literal(prev)), Segment::Literal(cur)) => {
                prev.push_str(&cur);
            }
            (_, other) => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::BUILTIN_COMPOUND_EXTS;

    fn cfg() -> CleanupConfig {
        CleanupConfig {
            remove_copy_suffix: true,
            remove_cjk: true,
            convert_case: CaseStyle::Snake,
        }
    }

    // --- remove_copy_suffix ---------------------------------------------

    #[test]
    fn strip_paren_number_handles_single_and_multi_digit() {
        assert_eq!(remove_copy_suffix("foo (1)"), "foo");
        assert_eq!(remove_copy_suffix("foo (12)"), "foo");
        assert_eq!(remove_copy_suffix("foo bar (3)"), "foo bar");
    }

    #[test]
    fn strip_paren_number_rejects_non_digit_body() {
        assert_eq!(remove_copy_suffix("foo (draft)"), "foo (draft)");
        assert_eq!(remove_copy_suffix("foo (1a)"), "foo (1a)");
    }

    #[test]
    fn strip_dash_copy_handles_bare_and_numbered() {
        assert_eq!(remove_copy_suffix("doc - Copy"), "doc");
        assert_eq!(remove_copy_suffix("doc - Copy (3)"), "doc");
    }

    #[test]
    fn strip_japanese_copy_handles_bare_and_numbered() {
        assert_eq!(remove_copy_suffix("メモ のコピー"), "メモ");
        assert_eq!(remove_copy_suffix("メモ のコピー 2"), "メモ");
    }

    #[test]
    fn strip_preserves_version_tokens() {
        assert_eq!(remove_copy_suffix("shot_v01"), "shot_v01");
        assert_eq!(remove_copy_suffix("shot_v01 (1)"), "shot_v01");
    }

    #[test]
    fn strip_does_not_recurse() {
        // `foo (1) (2)` loses the outer copy number but keeps the inner.
        assert_eq!(remove_copy_suffix("foo (1) (2)"), "foo (1)");
    }

    // --- remove_cjk_into_segments ---------------------------------------

    #[test]
    fn cjk_run_collapses_to_single_hole() {
        let segs = remove_cjk_into_segments("カット_v01");
        assert_eq!(segs.len(), 2);
        match &segs[0] {
            Segment::Hole(h) => {
                assert_eq!(h.origin, "カット");
                assert_eq!(h.kind, HoleKind::Cjk);
            }
            seg @ Segment::Literal(_) => panic!("expected hole, got {seg:?}"),
        }
        match &segs[1] {
            Segment::Literal(s) => assert_eq!(s, "_v01"),
            seg @ Segment::Hole(_) => panic!("expected literal body, got {seg:?}"),
        }
    }

    #[test]
    fn multiple_cjk_runs_stay_separate() {
        let segs = remove_cjk_into_segments("カット_主役_v01");
        // [Hole(カット), Literal(_), Hole(主役), Literal(_v01)]
        assert_eq!(segs.len(), 4);
    }

    #[test]
    fn pure_ascii_stays_literal() {
        let segs = remove_cjk_into_segments("shot_v01");
        assert_eq!(segs.len(), 1);
        assert!(matches!(&segs[0], Segment::Literal(s) if s == "shot_v01"));
    }

    #[test]
    fn han_hiragana_katakana_all_classified() {
        // Han: 影, Hiragana: あ, Katakana: ア
        for s in ["影", "あ", "ア"] {
            let segs = remove_cjk_into_segments(s);
            assert_eq!(segs.len(), 1, "{s}");
            assert!(matches!(segs[0], Segment::Hole(_)), "{s}");
        }
    }

    // --- full pipeline ---------------------------------------------------

    #[test]
    fn pipeline_end_to_end_on_mixed_basename() {
        // `(1)` must live at the tail of the stem — that's where the
        // OS actually places copy suffixes — for the copy-suffix stage
        // to see it.
        let cand = clean_basename("カット_MainRole_v01 (1).png", &cfg(), BUILTIN_COMPOUND_EXTS);
        assert_eq!(cand.ext.as_deref(), Some("png"));
        // Expected after pipeline:
        //   stage 1: "カット_MainRole_v01"
        //   stage 2: [Hole(カット), Literal("_MainRole_v01")]
        //   stage 3: [Hole,          Literal("main_role_v01")]
        //
        // heck's `to_snake_case` treats a leading `_` as a separator
        // and drops it. That means sentinel rendering will stitch the
        // hole and the literal together without the original `_`
        // between them. Documented here rather than worked around,
        // because introducing a synthetic separator would change
        // semantics for users who opted into a specific stem shape.
        assert_eq!(cand.segments.len(), 2);
        assert!(matches!(&cand.segments[0], Segment::Hole(_)));
        match &cand.segments[1] {
            Segment::Literal(s) => assert_eq!(s, "main_role_v01"),
            seg @ Segment::Hole(_) => panic!("expected literal, got {seg:?}"),
        }
    }

    #[test]
    fn pipeline_without_cjk_stage_keeps_literal_with_cjk() {
        let cfg = CleanupConfig {
            remove_copy_suffix: true,
            remove_cjk: false,
            convert_case: CaseStyle::Off,
        };
        let cand = clean_basename("カット_v01 (1).png", &cfg, BUILTIN_COMPOUND_EXTS);
        assert_eq!(cand.segments.len(), 1);
        match &cand.segments[0] {
            Segment::Literal(s) => assert_eq!(s, "カット_v01"),
            seg @ Segment::Hole(_) => panic!("expected literal, got {seg:?}"),
        }
    }

    #[test]
    fn pipeline_with_all_stages_off_is_identity_modulo_ext_split() {
        let cfg = CleanupConfig {
            remove_copy_suffix: false,
            remove_cjk: false,
            convert_case: CaseStyle::Off,
        };
        let cand = clean_basename("Shot V01.PNG", &cfg, BUILTIN_COMPOUND_EXTS);
        assert_eq!(cand.ext.as_deref(), Some("PNG"));
        match &cand.segments[0] {
            Segment::Literal(s) => assert_eq!(s, "Shot V01"),
            seg @ Segment::Hole(_) => panic!("expected literal, got {seg:?}"),
        }
    }

    #[test]
    fn pipeline_preserves_compound_extension() {
        let cand = clean_basename("Archive.tar.gz", &cfg(), BUILTIN_COMPOUND_EXTS);
        assert_eq!(cand.ext.as_deref(), Some("tar.gz"));
        match &cand.segments[0] {
            Segment::Literal(s) => assert_eq!(s, "archive"),
            seg @ Segment::Hole(_) => panic!("expected literal, got {seg:?}"),
        }
    }

    #[test]
    fn collapse_adjacent_literals_handles_chains() {
        let segs = vec![
            Segment::Literal("a".into()),
            Segment::Literal("b".into()),
            Segment::Hole(Hole {
                origin: "猫".into(),
                kind: HoleKind::Cjk,
                pos: 2,
            }),
            Segment::Literal("c".into()),
            Segment::Literal("d".into()),
        ];
        let collapsed = collapse_adjacent_literals(segs);
        assert_eq!(collapsed.len(), 3);
        assert!(matches!(&collapsed[0], Segment::Literal(s) if s == "ab"));
        assert!(matches!(&collapsed[1], Segment::Hole(_)));
        assert!(matches!(&collapsed[2], Segment::Literal(s) if s == "cd"));
    }
}
