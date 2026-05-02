//! Destination ranking for import: find project directories that
//! accept a given extension, ordered by specificity.
//!
//! Implements the `suggested_destinations` follow-up from
//! `core::accepts::evaluate` (M2 stub → M4 fill).
//!
//! Scoring (higher is better):
//! - Own literal match (`.psd` in own set):  **3**
//! - Own alias match (`:image` in own set):  **2**
//! - Inherited match (any source):           **1**
//! - Shallower path breaks ties (fewer `/`).

use std::collections::HashMap;

use serde::Serialize;

use crate::accepts::resolve::EffectiveAccepts;
use crate::accepts::types::Ext;
use crate::fs::ProjectPath;
use crate::index::FileRow;
use crate::rules::AcceptsSource;

/// A suggested destination directory with its score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestedDestination {
    pub path: ProjectPath,
    pub score: u32,
}

/// Score a single directory for a given extension.
///
/// Returns `None` if the dir does not accept the ext at all.
#[must_use]
pub fn score_dir(effective: &EffectiveAccepts, ext: &Ext) -> Option<u32> {
    let source = effective.source_of(ext)?;
    Some(match source {
        AcceptsSource::Own => 3,
        AcceptsSource::Inherited => 1,
    })
}

/// Rank directories by how well they accept the given extension.
///
/// `dirs` is a list of `(dir_path, effective_accepts)` pairs,
/// typically from walking all `.dirmeta.toml` in the project.
///
/// Returns matches sorted by score (desc), then by path depth (asc,
/// shallower first), then lexicographically.
pub fn rank_destinations(
    dirs: &[(ProjectPath, EffectiveAccepts)],
    ext: &Ext,
) -> Vec<SuggestedDestination> {
    let mut scored: Vec<SuggestedDestination> = dirs
        .iter()
        .filter_map(|(path, eff)| {
            score_dir(eff, ext).map(|score| SuggestedDestination {
                path: path.clone(),
                score,
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| depth(&a.path).cmp(&depth(&b.path)))
            .then_with(|| a.path.as_str().cmp(b.path.as_str()))
    });

    scored
}

/// Rank directories by how many files with the given extension they
/// already contain. Directories with no matching files are excluded.
///
/// Returns sorted by count (desc) then depth (asc) then lexicographic.
pub fn rank_by_frequency(rows: &[FileRow], ext: &Ext) -> Vec<SuggestedDestination> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let normalized = ext.as_str().to_lowercase();
    for row in rows {
        let file_ext = row.path.extension().unwrap_or_default().to_lowercase();
        if file_ext == normalized {
            let dir = row.path.as_str().rsplit_once('/').map_or("", |(d, _)| d);
            *counts.entry(dir.to_string()).or_default() += 1;
        }
    }

    let mut scored: Vec<SuggestedDestination> = counts
        .into_iter()
        .filter_map(|(dir, count)| {
            let path = if dir.is_empty() {
                ProjectPath::root()
            } else {
                ProjectPath::new(&dir).ok()?
            };
            Some(SuggestedDestination {
                path,
                score: u32::try_from(count).unwrap_or(u32::MAX),
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| depth(&a.path).cmp(&depth(&b.path)))
            .then_with(|| a.path.as_str().cmp(b.path.as_str()))
    });

    scored
}

/// Merge accepts-based ranking with frequency-based ranking.
///
/// Accepts matches (score 1–3) always rank above frequency-only
/// matches. Within accepts matches, frequency count is used as a
/// secondary signal. Frequency-only dirs get score 0 and sort by count.
pub fn merge_rankings(
    accepts: &[SuggestedDestination],
    frequency: &[SuggestedDestination],
) -> Vec<SuggestedDestination> {
    let freq_map: HashMap<&str, u32> = frequency
        .iter()
        .map(|s| (s.path.as_str(), s.score))
        .collect();

    let mut merged: Vec<(u32, u32, ProjectPath)> = accepts
        .iter()
        .map(|s| {
            let freq = freq_map.get(s.path.as_str()).copied().unwrap_or(0);
            (s.score, freq, s.path.clone())
        })
        .collect();

    let accepts_set: std::collections::HashSet<&str> =
        accepts.iter().map(|s| s.path.as_str()).collect();
    for f in frequency {
        if !accepts_set.contains(f.path.as_str()) {
            merged.push((0, f.score, f.path.clone()));
        }
    }

    merged.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| depth(&a.2).cmp(&depth(&b.2)))
            .then_with(|| a.2.as_str().cmp(b.2.as_str()))
    });

    merged
        .into_iter()
        .map(|(accepts_score, _freq, path)| SuggestedDestination {
            path,
            score: accepts_score,
        })
        .collect()
}

fn depth(p: &ProjectPath) -> usize {
    p.as_str().matches('/').count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accepts::resolve::compute_effective_accepts;
    use crate::accepts::schema::AliasCatalog;
    use crate::accepts::types::{AcceptsToken, RawAccepts, normalize_ext};
    use crate::rules::Mode;

    fn p(s: &str) -> ProjectPath {
        ProjectPath::new(s).unwrap()
    }

    fn raw_own(tokens: Vec<AcceptsToken>) -> RawAccepts {
        RawAccepts {
            inherit: false,
            exts: tokens,
            mode: Mode::Warn,
        }
    }

    fn effective(tokens: Vec<AcceptsToken>) -> EffectiveAccepts {
        compute_effective_accepts(Some(&raw_own(tokens)), &[], &AliasCatalog::builtin())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn own_literal_scores_highest() {
        let eff = effective(vec![AcceptsToken::Ext(normalize_ext(".psd"))]);
        assert_eq!(score_dir(&eff, &normalize_ext(".psd")), Some(3));
    }

    #[test]
    fn no_match_returns_none() {
        let eff = effective(vec![AcceptsToken::Ext(normalize_ext(".psd"))]);
        assert_eq!(score_dir(&eff, &normalize_ext(".mp4")), None);
    }

    #[test]
    fn inherited_scores_lower() {
        let parent = raw_own(vec![AcceptsToken::Ext(normalize_ext(".psd"))]);
        let child = RawAccepts {
            inherit: true,
            exts: vec![AcceptsToken::Ext(normalize_ext(".tif"))],
            mode: Mode::Warn,
        };
        let eff = compute_effective_accepts(Some(&child), &[&parent], &AliasCatalog::builtin())
            .unwrap()
            .unwrap();
        // .tif is own → 3, .psd is inherited → 1
        assert_eq!(score_dir(&eff, &normalize_ext(".tif")), Some(3));
        assert_eq!(score_dir(&eff, &normalize_ext(".psd")), Some(1));
    }

    #[test]
    fn rank_orders_by_score_then_depth() {
        let deep = (
            p("assets/textures/raw"),
            effective(vec![AcceptsToken::Ext(normalize_ext(".psd"))]),
        );
        let shallow = (
            p("assets"),
            effective(vec![AcceptsToken::Ext(normalize_ext(".psd"))]),
        );
        let dirs = vec![deep, shallow];
        let ranked = rank_destinations(&dirs, &normalize_ext(".psd"));
        assert_eq!(ranked.len(), 2);
        // Same score → shallower first
        assert_eq!(ranked[0].path.as_str(), "assets");
        assert_eq!(ranked[1].path.as_str(), "assets/textures/raw");
    }

    #[test]
    fn rank_filters_non_matching() {
        let dirs = vec![(
            p("video"),
            effective(vec![AcceptsToken::Ext(normalize_ext(".mp4"))]),
        )];
        let ranked = rank_destinations(&dirs, &normalize_ext(".psd"));
        assert!(ranked.is_empty());
    }
}
