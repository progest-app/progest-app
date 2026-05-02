//! Lightweight media metadata extraction (dimensions, duration, codec).
//!
//! Reads only file headers where possible — no full decode. Used by the
//! inspector IPC to show file properties on demand.

use std::path::Path;
use std::process::Command;

use image::ImageReader;
use serde::Serialize;

use crate::thumbnail::ffmpeg::find_ffmpeg;

#[derive(Debug, Clone, Serialize, Default)]
pub struct MediaInfo {
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u32>,
}

pub fn probe(abs_path: &Path) -> MediaInfo {
    let mut info = MediaInfo::default();

    if let Ok(meta) = std::fs::metadata(abs_path) {
        info.size_bytes = meta.len();
    }

    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "tif" | "ico" | "avif"
        | "exr" | "hdr" => {
            probe_image(abs_path, &mut info);
        }
        "psd" => {
            probe_psd(abs_path, &mut info);
        }
        "heic" | "heif" | "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v" | "flv" | "wmv"
        | "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "aiff" | "opus" => {
            probe_ffprobe(abs_path, &mut info);
        }
        _ => {}
    }

    info
}

fn probe_image(path: &Path, info: &mut MediaInfo) {
    if let Ok(reader) = ImageReader::open(path)
        && let Ok((w, h)) = reader.into_dimensions()
    {
        info.width = Some(w);
        info.height = Some(h);
    }
}

fn probe_psd(path: &Path, info: &mut MediaInfo) {
    if let Ok(bytes) = std::fs::read(path)
        && let Ok(psd) = psd::Psd::from_bytes(&bytes)
    {
        info.width = Some(psd.width());
        info.height = Some(psd.height());
    }
}

fn probe_ffprobe(path: &Path, info: &mut MediaInfo) {
    let ffmpeg_path = find_ffmpeg();
    let ffprobe_path = ffmpeg_path.as_deref().and_then(|p| {
        let probe = p.parent()?.join("ffprobe");
        probe.exists().then_some(probe)
    });

    let ffprobe = match ffprobe_path {
        Some(p) => p,
        None => {
            if which::which("ffprobe").is_ok() {
                std::path::PathBuf::from("ffprobe")
            } else {
                return;
            }
        }
    };

    let output = Command::new(&ffprobe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output();

    let Ok(output) = output else { return };
    if !output.status.success() {
        return;
    }

    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return;
    };

    if let Some(duration_str) = json
        .pointer("/format/duration")
        .and_then(serde_json::Value::as_str)
    {
        info.duration_secs = duration_str.parse::<f64>().ok();
    }

    if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(serde_json::Value::as_str);
            match codec_type {
                Some("video") if info.codec.is_none() => {
                    info.codec = stream
                        .get("codec_name")
                        .and_then(serde_json::Value::as_str)
                        .map(String::from);
                    info.width = stream
                        .get("width")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|v| v.try_into().ok());
                    info.height = stream
                        .get("height")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|v| v.try_into().ok());
                    if let Some(fps_str) = stream
                        .get("r_frame_rate")
                        .and_then(serde_json::Value::as_str)
                    {
                        info.fps = parse_fps(fps_str);
                    }
                    info.bit_depth = stream
                        .get("bits_per_raw_sample")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|s| s.parse::<u32>().ok());
                }
                Some("audio") if info.audio_codec.is_none() => {
                    info.audio_codec = stream
                        .get("codec_name")
                        .and_then(serde_json::Value::as_str)
                        .map(String::from);
                    info.sample_rate = stream
                        .get("sample_rate")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|s| s.parse::<u32>().ok());
                    info.channels = stream
                        .get("channels")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|v| v.try_into().ok());
                    if info.codec.is_none()
                        && let Some(dur) = stream
                            .pointer("/duration")
                            .or_else(|| json.pointer("/format/duration"))
                            .and_then(serde_json::Value::as_str)
                    {
                        info.duration_secs = dur.parse::<f64>().ok();
                    }
                }
                _ => {}
            }
        }
    }
}

fn parse_fps(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<f64>().ok()?;
        let den = parts[1].parse::<f64>().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
    }
    s.parse::<f64>().ok()
}
