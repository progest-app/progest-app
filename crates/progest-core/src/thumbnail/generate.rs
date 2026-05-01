use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;
use image::{DynamicImage, ImageReader};

use super::cache::ThumbnailCache;
use super::ffmpeg;
use super::types::{
    CacheKey, GenerateBatchReport, SkipReason, SourceFormat, ThumbnailError, ThumbnailRequest,
    ThumbnailResult, classify_extension,
};

pub fn generate_batch(
    requests: &[ThumbnailRequest],
    cache: &ThumbnailCache,
    force: bool,
) -> GenerateBatchReport {
    generate_batch_with_progress(requests, cache, force, &|_, _, _| {})
}

pub fn generate_batch_with_progress(
    requests: &[ThumbnailRequest],
    cache: &ThumbnailCache,
    force: bool,
    on_progress: &dyn Fn(u64, u64, &str),
) -> GenerateBatchReport {
    let ffmpeg_path = ffmpeg::find_ffmpeg();
    let mut report = GenerateBatchReport::default();
    let total = requests.len() as u64;

    for (i, req) in requests.iter().enumerate() {
        on_progress(i as u64, total, req.path.as_str());
        let result = generate_one(req, cache, force, ffmpeg_path.as_deref());
        match &result {
            ThumbnailResult::Generated { .. } => report.generated += 1,
            ThumbnailResult::Cached { .. } => report.cached += 1,
            ThumbnailResult::Skipped { .. } => report.skipped += 1,
        }
        report.results.push(result);
    }
    report
}

fn generate_one(
    req: &ThumbnailRequest,
    cache: &ThumbnailCache,
    force: bool,
    ffmpeg_path: Option<&Path>,
) -> ThumbnailResult {
    let key = CacheKey {
        file_id: req.file_id,
        fingerprint: req.fingerprint,
        size: req.size,
    };

    if !force && let Some(cached) = cache.get(&key) {
        return ThumbnailResult::Cached {
            path: req.path.as_str().to_owned(),
            cache_path: cached.to_string_lossy().into_owned(),
        };
    }

    if !req.abs_path.is_file() {
        return ThumbnailResult::Skipped {
            path: req.path.as_str().to_owned(),
            reason: SkipReason::SourceMissing,
        };
    }

    let ext = req
        .abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let Some(format) = classify_extension(ext) else {
        return ThumbnailResult::Skipped {
            path: req.path.as_str().to_owned(),
            reason: SkipReason::UnsupportedFormat {
                ext: ext.to_owned(),
            },
        };
    };

    let webp_bytes = match format {
        SourceFormat::Image => generate_image(&req.abs_path, req.size),
        SourceFormat::Psd => generate_psd(&req.abs_path, req.size),
        #[cfg(feature = "heic")]
        SourceFormat::Heic => generate_heic(&req.abs_path, req.size),
        SourceFormat::Video => match ffmpeg_path {
            Some(ff) => generate_video(ff, &req.abs_path, req.size),
            None => {
                return ThumbnailResult::Skipped {
                    path: req.path.as_str().to_owned(),
                    reason: SkipReason::FfmpegNotFound,
                };
            }
        },
    };

    match webp_bytes {
        Ok(data) => match cache.put(&key, &data) {
            Ok(cache_path) => ThumbnailResult::Generated {
                path: req.path.as_str().to_owned(),
                cache_path: cache_path.to_string_lossy().into_owned(),
                bytes: data.len() as u64,
            },
            Err(e) => ThumbnailResult::Skipped {
                path: req.path.as_str().to_owned(),
                reason: SkipReason::GenerationFailed {
                    message: format!("cache write: {e}"),
                },
            },
        },
        Err(e) => ThumbnailResult::Skipped {
            path: req.path.as_str().to_owned(),
            reason: SkipReason::GenerationFailed {
                message: e.to_string(),
            },
        },
    }
}

fn generate_image(path: &Path, max_dim: u32) -> Result<Vec<u8>, ThumbnailError> {
    let img = ImageReader::open(path)
        .map_err(|e| ThumbnailError::DecodeFailed {
            path: path.display().to_string(),
            message: e.to_string(),
        })?
        .decode()
        .map_err(|e| ThumbnailError::DecodeFailed {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
    encode_webp(&resize_to_fit(img, max_dim))
}

fn generate_psd(path: &Path, max_dim: u32) -> Result<Vec<u8>, ThumbnailError> {
    let data = std::fs::read(path)?;
    let psd = psd::Psd::from_bytes(&data).map_err(|e| ThumbnailError::DecodeFailed {
        path: path.display().to_string(),
        message: format!("PSD parse: {e}"),
    })?;

    let rgba = psd.rgba();
    let width = psd.width();
    let height = psd.height();

    let img =
        DynamicImage::ImageRgba8(image::RgbaImage::from_raw(width, height, rgba).ok_or_else(
            || ThumbnailError::DecodeFailed {
                path: path.display().to_string(),
                message: "PSD composite buffer size mismatch".to_string(),
            },
        )?);
    encode_webp(&resize_to_fit(img, max_dim))
}

#[cfg(feature = "heic")]
fn generate_heic(path: &Path, max_dim: u32) -> Result<Vec<u8>, ThumbnailError> {
    let ctx =
        libheif_rs::HeifContext::read_from_file(path.to_str().unwrap_or("")).map_err(|e| {
            ThumbnailError::DecodeFailed {
                path: path.display().to_string(),
                message: format!("HEIF open: {e}"),
            }
        })?;
    let handle = ctx
        .primary_image_handle()
        .map_err(|e| ThumbnailError::DecodeFailed {
            path: path.display().to_string(),
            message: format!("HEIF handle: {e}"),
        })?;

    let lib_heif = libheif_rs::LibHeif::new();
    let decoded = lib_heif
        .decode(
            &handle,
            libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgb),
            None,
        )
        .map_err(|e| ThumbnailError::DecodeFailed {
            path: path.display().to_string(),
            message: format!("HEIF decode: {e}"),
        })?;

    let plane = decoded
        .planes()
        .interleaved
        .ok_or_else(|| ThumbnailError::DecodeFailed {
            path: path.display().to_string(),
            message: "HEIF: no interleaved plane".to_string(),
        })?;

    let width = handle.width();
    let height = handle.height();
    let stride = plane.stride;
    let raw = plane.data;

    let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height as usize {
        let row_start = y * stride;
        let row_end = row_start + (width as usize * 3);
        if row_end <= raw.len() {
            rgb_data.extend_from_slice(&raw[row_start..row_end]);
        }
    }

    let img = DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(width, height, rgb_data).ok_or_else(|| {
            ThumbnailError::DecodeFailed {
                path: path.display().to_string(),
                message: "HEIF buffer size mismatch".to_string(),
            }
        })?,
    );
    encode_webp(&resize_to_fit(img, max_dim))
}

fn generate_video(
    ffmpeg_path: &Path,
    video_path: &Path,
    max_dim: u32,
) -> Result<Vec<u8>, ThumbnailError> {
    let tmp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(ThumbnailError::CacheIo)?;
    let png_path = tmp.path().to_path_buf();

    ffmpeg::extract_frame(ffmpeg_path, video_path, &png_path, "00:00:01").map_err(|e| {
        ThumbnailError::DecodeFailed {
            path: video_path.display().to_string(),
            message: e.to_string(),
        }
    })?;

    generate_image(&png_path, max_dim)
}

fn resize_to_fit(img: DynamicImage, max_dim: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim {
        return img;
    }
    img.resize(max_dim, max_dim, FilterType::Lanczos3)
}

fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>, ThumbnailError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut buf);
    img.write_with_encoder(encoder)
        .map_err(|e| ThumbnailError::DecodeFailed {
            path: String::new(),
            message: format!("WebP encode: {e}"),
        })?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::ProjectPath;
    use crate::identity::{FileId, Fingerprint};
    use image::ImageFormat;
    use std::str::FromStr;

    fn sample_request(abs_path: &Path) -> ThumbnailRequest {
        ThumbnailRequest {
            path: ProjectPath::new("test.png").unwrap(),
            abs_path: abs_path.to_path_buf(),
            file_id: FileId::new_v7(),
            fingerprint: Fingerprint::from_str("blake3:00112233445566778899aabbccddeeff").unwrap(),
            size: 64,
        }
    }

    fn create_test_png(path: &Path, width: u32, height: u32) {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::new(width, height));
        img.save_with_format(path, ImageFormat::Png).unwrap();
    }

    #[test]
    fn generate_image_produces_webp() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("test.png");
        create_test_png(&src, 512, 512);

        let data = generate_image(&src, 256).unwrap();
        assert!(!data.is_empty());
        assert_eq!(&data[..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
    }

    #[test]
    fn resize_preserves_small_images() {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::new(100, 80));
        let resized = resize_to_fit(img, 256);
        assert_eq!(resized.width(), 100);
        assert_eq!(resized.height(), 80);
    }

    #[test]
    fn resize_scales_large_images() {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::new(1024, 512));
        let resized = resize_to_fit(img, 256);
        assert!(resized.width() <= 256);
        assert!(resized.height() <= 256);
    }

    #[test]
    fn generate_batch_skips_missing_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cache = ThumbnailCache::new(tmp.path().join("thumbs"), 1_000_000);
        let req = sample_request(Path::new("/nonexistent/file.png"));

        let report = generate_batch(&[req], &cache, false);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.generated, 0);
    }

    #[test]
    fn generate_batch_uses_cache() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("test.png");
        create_test_png(&src, 64, 64);

        let cache = ThumbnailCache::new(tmp.path().join("thumbs"), 1_000_000);
        let req = sample_request(&src);

        let r1 = generate_batch(std::slice::from_ref(&req), &cache, false);
        assert_eq!(r1.generated, 1);
        assert_eq!(r1.cached, 0);

        let r2 = generate_batch(std::slice::from_ref(&req), &cache, false);
        assert_eq!(r2.generated, 0);
        assert_eq!(r2.cached, 1);
    }

    #[test]
    fn generate_batch_force_regenerates() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("test.png");
        create_test_png(&src, 64, 64);

        let cache = ThumbnailCache::new(tmp.path().join("thumbs"), 1_000_000);
        let req = sample_request(&src);

        generate_batch(std::slice::from_ref(&req), &cache, false);
        let r2 = generate_batch(std::slice::from_ref(&req), &cache, true);
        assert_eq!(r2.generated, 1);
        assert_eq!(r2.cached, 0);
    }
}
