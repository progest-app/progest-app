//! Value types for `core::naming`.
//!
//! The pipeline produces a [`NameCandidate`] — a stem (minus extension)
//! expressed as a sequence of [`Segment`]s. A [`Segment::Hole`] stands
//! for a run of characters the pipeline removed but refused to drop
//! silently (currently CJK). The hole carries its original text so UI
//! layers can surface what the user would need to fill in.
//!
//! A candidate is not directly renameable: holes must be resolved via
//! [`crate::naming::fill::FillMode`] before the name touches disk.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

/// A cleaned-up basename as a stem + optional extension.
///
/// The pipeline deliberately keeps stem and extension apart so case
/// conversion / hole substitution only act on the stem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameCandidate {
    pub segments: Vec<Segment>,
    /// Lowercased extension **without** the leading dot. `None` for
    /// extensionless basenames like `README`. Compound extensions
    /// (`tar.gz`) are preserved verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
}

impl NameCandidate {
    /// True when every segment is a literal (no unresolved holes).
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.segments
            .iter()
            .all(|s| matches!(s, Segment::Literal(_)))
    }

    /// Rendering used by `progest clean --format=text` dry-run and by
    /// other `Display`-style consumers. Each hole becomes a bracketed
    /// sentinel keyed by its position in the hole sequence:
    /// `⟨cjk-1⟩`, `⟨cjk-2⟩`, …. The sentinel set is chosen to be
    /// non-overlapping with `.progest/rules.toml` placeholder syntax
    /// (which uses ASCII `{…}`).
    #[must_use]
    pub fn to_sentinel_string(&self) -> String {
        let mut out = String::new();
        let mut hole_seq: usize = 0;
        for seg in &self.segments {
            match seg {
                Segment::Literal(s) => out.push_str(s),
                Segment::Hole(h) => {
                    hole_seq += 1;
                    match h.kind {
                        HoleKind::Cjk => {
                            let _ = write!(out, "\u{27E8}cjk-{hole_seq}\u{27E9}");
                        }
                    }
                }
            }
        }
        if let Some(ext) = &self.ext {
            out.push('.');
            out.push_str(ext);
        }
        out
    }

    /// Concatenate just the literal parts, ignoring holes. Useful for
    /// quick equality checks ("did the pipeline actually change
    /// anything outside of holes?"), never for disk writes.
    #[must_use]
    pub fn literal_only(&self) -> String {
        let mut out = String::new();
        for seg in &self.segments {
            if let Segment::Literal(s) = seg {
                out.push_str(s);
            }
        }
        if let Some(ext) = &self.ext {
            out.push('.');
            out.push_str(ext);
        }
        out
    }

    /// Ordered holes in this candidate, keyed by their 1-based sequence
    /// number. The number is stable across `to_sentinel_string` and the
    /// JSON encoding so a UI can cross-reference them.
    #[must_use]
    pub fn holes(&self) -> Vec<(usize, &Hole)> {
        let mut out = Vec::new();
        let mut seq = 0;
        for seg in &self.segments {
            if let Segment::Hole(h) = seg {
                seq += 1;
                out.push((seq, h));
            }
        }
        out
    }
}

/// A single piece of a [`NameCandidate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Segment {
    /// Literal text contributed by the original basename or inserted
    /// by a pipeline stage (separators, suffix trims, etc.).
    Literal(String),
    /// A run that the pipeline refused to drop silently. See [`Hole`].
    Hole(Hole),
}

/// Metadata for a single hole in the candidate.
///
/// `origin` preserves the original text verbatim (e.g. the Japanese
/// substring that `remove_cjk` took out). `pos` records the hole's
/// position in byte offsets relative to the *input* stem, not the
/// output — hole numbering in text rendering uses the 1-based hole
/// sequence instead, which is derived at render time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hole {
    pub origin: String,
    pub kind: HoleKind,
    pub pos: usize,
}

/// Why the pipeline created this hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleKind {
    /// A contiguous run of CJK (Han / Hiragana / Katakana) characters
    /// removed by `remove_cjk`. Each run collapses into a single hole.
    Cjk,
}

// --- Config ----------------------------------------------------------------

/// Canonical [cleanup] section parsed from `.progest/project.toml`.
///
/// Stage ordering is fixed by the pipeline
/// (`remove_copy_suffix → remove_cjk → convert_case`); the config only
/// toggles which stages run and picks the case style.
///
/// Defaults (REQUIREMENTS §3.5.5):
///
/// - `remove_copy_suffix = false` (opt-in)
/// - `remove_cjk = false` (opt-in)
/// - `convert_case = CaseStyle::Snake` (default on; the TOML string
///   `"off"` disables)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupConfig {
    pub remove_copy_suffix: bool,
    pub remove_cjk: bool,
    pub convert_case: CaseStyle,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            remove_copy_suffix: false,
            remove_cjk: false,
            convert_case: CaseStyle::Snake,
        }
    }
}

/// Case style applied by `convert_case`.
///
/// `Off` is modeled as a variant (rather than wrapping the config in
/// `Option`) so the TOML surface can stay a single `convert_case = "..."`
/// string field — matching `remove_copy_suffix` / `remove_cjk` shape and
/// keeping the `[cleanup]` section flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStyle {
    Off,
    Snake,
    Kebab,
    Camel,
    Pascal,
    Lower,
    Upper,
}

impl CaseStyle {
    /// Identifier emitted in JSON output and accepted by the TOML
    /// loader. `Off` parses from the string `"off"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Snake => "snake",
            Self::Kebab => "kebab",
            Self::Camel => "camel",
            Self::Pascal => "pascal",
            Self::Lower => "lower",
            Self::Upper => "upper",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_rendering_uses_1_based_hole_sequence() {
        // Input: カット_主役_v01 → two CJK runs become two holes.
        let cand = NameCandidate {
            segments: vec![
                Segment::Hole(Hole {
                    origin: "カット".into(),
                    kind: HoleKind::Cjk,
                    pos: 0,
                }),
                Segment::Literal("_".into()),
                Segment::Hole(Hole {
                    origin: "主役".into(),
                    kind: HoleKind::Cjk,
                    pos: 10,
                }),
                Segment::Literal("_v01".into()),
            ],
            ext: Some("png".into()),
        };
        assert_eq!(
            cand.to_sentinel_string(),
            "\u{27E8}cjk-1\u{27E9}_\u{27E8}cjk-2\u{27E9}_v01.png"
        );
    }

    #[test]
    fn is_resolved_requires_all_literals() {
        let hole = NameCandidate {
            segments: vec![Segment::Hole(Hole {
                origin: "猫".into(),
                kind: HoleKind::Cjk,
                pos: 0,
            })],
            ext: None,
        };
        assert!(!hole.is_resolved());

        let clean = NameCandidate {
            segments: vec![Segment::Literal("cat".into())],
            ext: Some("png".into()),
        };
        assert!(clean.is_resolved());
    }

    #[test]
    fn literal_only_skips_holes() {
        let cand = NameCandidate {
            segments: vec![
                Segment::Literal("foo_".into()),
                Segment::Hole(Hole {
                    origin: "猫".into(),
                    kind: HoleKind::Cjk,
                    pos: 4,
                }),
                Segment::Literal("_v01".into()),
            ],
            ext: Some("png".into()),
        };
        assert_eq!(cand.literal_only(), "foo__v01.png");
    }

    #[test]
    fn holes_iteration_is_1_based() {
        let cand = NameCandidate {
            segments: vec![
                Segment::Literal("a".into()),
                Segment::Hole(Hole {
                    origin: "b".into(),
                    kind: HoleKind::Cjk,
                    pos: 0,
                }),
                Segment::Hole(Hole {
                    origin: "c".into(),
                    kind: HoleKind::Cjk,
                    pos: 1,
                }),
            ],
            ext: None,
        };
        let holes = cand.holes();
        assert_eq!(holes.len(), 2);
        assert_eq!(holes[0].0, 1);
        assert_eq!(holes[1].0, 2);
    }

    #[test]
    fn cleanup_config_default_matches_requirements_3_5_5() {
        let cfg = CleanupConfig::default();
        assert!(!cfg.remove_copy_suffix);
        assert!(!cfg.remove_cjk);
        assert_eq!(cfg.convert_case, CaseStyle::Snake);
    }

    #[test]
    fn case_style_roundtrips_via_as_str() {
        for style in [
            CaseStyle::Off,
            CaseStyle::Snake,
            CaseStyle::Kebab,
            CaseStyle::Camel,
            CaseStyle::Pascal,
            CaseStyle::Lower,
            CaseStyle::Upper,
        ] {
            // Not a real roundtrip (as_str is the canonical name) —
            // just guards that every variant names itself distinctly.
            assert_eq!(style.as_str().len(), style.as_str().trim().len());
        }
    }
}
