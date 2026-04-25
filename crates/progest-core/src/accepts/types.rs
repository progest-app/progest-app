//! Shared value types for `core::accepts`.
//!
//! Keeps the alias vocabulary, extension normalization helpers, and
//! the `RawAccepts` shape (post-TOML, pre-alias-expansion) in one
//! place. See [`docs/ACCEPTS_ALIASES.md`](../../../../docs/ACCEPTS_ALIASES.md)
//! for the authoritative extension catalogs.

use crate::rules::Mode;

/// Sentinel for "no extension". Used for files like `README`,
/// `Makefile`, and leading-dot files (`.gitignore`, `.env`) that
/// [`crate::rules::constraint::split_basename`] treats as
/// extensionless.
pub const EXT_NONE: &str = "";

/// A normalized extension token: lowercase, no leading dot.
///
/// Kept as a newtype over `String` so the API can enforce the
/// normalization invariant at the boundary (all inputs go through
/// [`normalize_ext`] / [`normalize_ext_from_basename`]) and so map /
/// set keys stay trivially comparable.
///
/// `Ext("psd")`, `Ext("tar.gz")`, and `Ext("")` are the valid forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ext(String);

impl Ext {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// True for [`EXT_NONE`] — the "no extension" sentinel.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for Ext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            f.write_str("<no-extension>")
        } else {
            f.write_str(&self.0)
        }
    }
}

/// Normalize a user-facing extension token.
///
/// - `".PSD"` → `"psd"`
/// - `"psd"` → `"psd"`
/// - `".tar.gz"` → `"tar.gz"`
/// - `""` → `""` (the [`EXT_NONE`] sentinel)
///
/// Does not validate that the token contains only "safe" characters;
/// the loader is expected to reject obviously-wrong shapes before
/// calling this.
#[must_use]
pub fn normalize_ext(raw: &str) -> Ext {
    let trimmed = raw.trim_start_matches('.');
    Ext(trimmed.to_ascii_lowercase())
}

/// Extract and normalize a file's extension from its basename, using
/// the same longest-match compound logic the rules engine uses.
///
/// Leading-dot files like `.gitignore` and `.env` return [`EXT_NONE`]
/// to match [`crate::rules::constraint::split_basename`] semantics.
#[must_use]
pub fn normalize_ext_from_basename(basename: &str, compound_exts: &[&str]) -> Ext {
    let (_, ext) = crate::rules::split_basename(basename, compound_exts);
    match ext {
        Some(raw) => Ext(raw.to_ascii_lowercase()),
        None => Ext(EXT_NONE.to_owned()),
    }
}

// --- Raw TOML shape --------------------------------------------------------

/// Raw `[accepts]` block parsed out of a `.dirmeta.toml`, before
/// alias expansion or inheritance resolution.
///
/// `exts` retains the user-facing tokens (alias references with `:`
/// prefix kept as-is) so the loader can validate alias names against
/// the project's alias catalog in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAccepts {
    /// `true` = walk up to the parent dir's `effective_accepts` and
    /// union in. Default `false` per REQUIREMENTS.md §3.13.2.
    pub inherit: bool,
    /// Mixed list of extension tokens and alias references. Retained
    /// verbatim from the TOML so the loader can still surface
    /// unknown-alias errors with the original text.
    pub exts: Vec<AcceptsToken>,
    /// Severity mode for placement violations. Defaults to
    /// [`Mode::Warn`] per REQUIREMENTS.md §3.13.1.
    pub mode: Mode,
}

impl Default for RawAccepts {
    fn default() -> Self {
        Self {
            inherit: false,
            exts: Vec::new(),
            mode: Mode::Warn,
        }
    }
}

/// One entry from a user's `[accepts].exts` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptsToken {
    /// Literal extension (already normalized). `""` is valid and
    /// means "files with no extension are accepted".
    Ext(Ext),
    /// Reference to a named alias (`":image"`, `":project"`, …).
    /// Stored **without** the leading `:` colon.
    Alias(String),
}

// --- Builtin alias table ---------------------------------------------------

/// Builtin category alias catalog for v1. Extension sets mirror the
/// authoritative tables in
/// [`docs/ACCEPTS_ALIASES.md`](../../../../docs/ACCEPTS_ALIASES.md) §2.
///
/// Each entry is `(alias_name_without_colon, &[extensions_without_dot])`.
/// Extensions are stored pre-normalized (lowercase, no leading dot)
/// so alias expansion is a no-op transform.
pub const BUILTIN_ALIASES: &[(&str, &[&str])] = &[
    (
        "image",
        &[
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "psd", "psb", "tga", "dds",
            "dpx", "exr", "hdr", "heic", "heif", "avif", "svg",
        ],
    ),
    (
        "video",
        &[
            "mp4", "mov", "mkv", "avi", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts", "mxf", "r3d",
            "braw", "ari",
        ],
    ),
    (
        "audio",
        &[
            "wav", "aif", "aiff", "flac", "alac", "mp3", "aac", "m4a", "ogg", "opus", "wma",
        ],
    ),
    (
        "raw",
        &[
            "arw", "cr2", "cr3", "nef", "raf", "rw2", "orf", "pef", "srw", "x3f", "dng",
        ],
    ),
    (
        "model",
        &[
            "fbx", "obj", "usd", "usda", "usdc", "usdz", "abc", "gltf", "glb", "stl", "ply", "drc",
        ],
    ),
    (
        "scene",
        &[
            "blend", "ma", "mb", "max", "c4d", "hip", "hiplc", "hipnc", "ztl", "zpr", "spp", "sbs",
            "sbsar", "vdb",
        ],
    ),
    (
        "project",
        &[
            "prproj", "aep", "aepx", "psd", "psb", "ai", "drp", "drt", "fcpxml", "nk", "hrox",
            "blend", "ma", "mb", "max", "c4d", "hip", "hiplc", "hipnc", "spp", "sbs", "veg",
            "mocha",
        ],
    ),
    (
        "text",
        &[
            "txt", "md", "markdown", "rst", "org", "adoc", "asciidoc", "log", "csv", "tsv", "json",
            "yaml", "yml", "toml", "xml", "html", "htm", "css", "ini", "cfg", "conf",
        ],
    ),
];

/// Look up a builtin alias by name (without the `:` prefix).
#[must_use]
pub fn builtin_alias(name: &str) -> Option<&'static [&'static str]> {
    BUILTIN_ALIASES
        .iter()
        .find_map(|(k, v)| if *k == name { Some(*v) } else { None })
}

/// True if `name` is a valid alias identifier per
/// [`docs/ACCEPTS_ALIASES.md`](../../../../docs/ACCEPTS_ALIASES.md) §3.1:
/// `^[a-z][a-z0-9_-]*$`. Length cap is 64 to match `RuleId`.
#[must_use]
pub fn is_valid_alias_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_leading_dot_and_lowercases() {
        assert_eq!(normalize_ext(".PSD").as_str(), "psd");
        assert_eq!(normalize_ext("psd").as_str(), "psd");
        assert_eq!(normalize_ext(".Tar.Gz").as_str(), "tar.gz");
    }

    #[test]
    fn normalize_empty_is_sentinel() {
        let e = normalize_ext("");
        assert!(e.is_none());
        assert_eq!(e.as_str(), EXT_NONE);
    }

    #[test]
    fn normalize_from_basename_handles_compound() {
        let e = normalize_ext_from_basename("archive.tar.gz", &["tar.gz"]);
        assert_eq!(e.as_str(), "tar.gz");
    }

    #[test]
    fn normalize_from_basename_leading_dot_is_no_extension() {
        let e = normalize_ext_from_basename(".gitignore", &[]);
        assert!(e.is_none(), "`.gitignore` should normalize to EXT_NONE");
    }

    #[test]
    fn normalize_from_basename_no_extension_file_is_no_extension() {
        let e = normalize_ext_from_basename("README", &[]);
        assert!(e.is_none());
    }

    #[test]
    fn builtin_aliases_cover_the_v1_names() {
        let names: Vec<&str> = BUILTIN_ALIASES.iter().map(|(n, _)| *n).collect();
        for expected in [
            "image", "video", "audio", "raw", "model", "scene", "project", "text",
        ] {
            assert!(
                names.contains(&expected),
                "missing builtin alias `{expected}`"
            );
        }
    }

    #[test]
    fn builtin_aliases_contain_no_leading_dots_or_uppercase() {
        for (name, exts) in BUILTIN_ALIASES {
            for ext in *exts {
                assert!(
                    !ext.starts_with('.'),
                    "alias `{name}` ext `{ext}` has a leading dot"
                );
                assert_eq!(
                    *ext,
                    ext.to_ascii_lowercase(),
                    "alias `{name}` ext `{ext}` is not lowercase"
                );
                assert!(!ext.is_empty(), "alias `{name}` contains empty ext");
            }
        }
    }

    #[test]
    fn builtin_alias_lookup_hits_and_misses() {
        assert!(builtin_alias("image").unwrap().contains(&"png"));
        assert!(builtin_alias("unknown").is_none());
    }

    #[test]
    fn image_alias_contains_svg_and_psb_per_docs() {
        let image = builtin_alias("image").unwrap();
        assert!(image.contains(&"svg"), "`:image` must include svg");
        assert!(image.contains(&"psb"), "`:image` must include psb");
        assert!(image.contains(&"dds"), "`:image` must include dds");
    }

    #[test]
    fn text_alias_excludes_svg() {
        let text = builtin_alias("text").unwrap();
        assert!(!text.contains(&"svg"), "`:text` must not include svg");
    }

    #[test]
    fn video_alias_excludes_prores() {
        let video = builtin_alias("video").unwrap();
        assert!(
            !video.contains(&"prores"),
            "`prores` is a codec, not a file ext — must not appear"
        );
    }

    #[test]
    fn valid_alias_name_accepts_canonical_forms() {
        for ok in ["image", "studio_3d", "my-alias", "a", "a1", "a_"] {
            assert!(is_valid_alias_name(ok), "{ok:?} should be valid");
        }
    }

    #[test]
    fn valid_alias_name_rejects_invalid_forms() {
        for bad in [
            "",
            "Image",
            "3d",
            "-foo",
            "_foo",
            "foo bar",
            "foo.bar",
            "日本語",
        ] {
            assert!(!is_valid_alias_name(bad), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn default_raw_accepts_is_warn_mode_no_inherit_empty() {
        let r = RawAccepts::default();
        assert_eq!(r.mode, Mode::Warn);
        assert!(!r.inherit);
        assert!(r.exts.is_empty());
    }
}
