use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("ffmpeg not found")]
    NotFound,
    #[error("ffmpeg exited with status {status}: {stderr}")]
    Failed { status: i32, stderr: String },
    #[error("ffmpeg I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PROGEST_FFMPEG_PATH") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        for name in &["ffmpeg", "ffmpeg.exe"] {
            let adjacent = exe_dir.join(name);
            if adjacent.is_file() {
                return Some(adjacent);
            }
        }
    }

    which::which("ffmpeg").ok()
}

pub fn extract_frame(
    ffmpeg: &Path,
    video_path: &Path,
    output_png: &Path,
    timestamp: &str,
) -> Result<(), FfmpegError> {
    let child = Command::new(ffmpeg)
        .args(["-y", "-ss", timestamp, "-i"])
        .arg(video_path)
        .args(["-vframes", "1", "-f", "image2"])
        .arg(output_png)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let output = wait_with_timeout(child, Duration::from_secs(30))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(FfmpegError::Failed {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .to_string(),
        })
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, FfmpegError> {
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let stderr = child
                .stderr
                .take()
                .map(|mut s| {
                    let mut buf = Vec::new();
                    std::io::Read::read_to_end(&mut s, &mut buf).ok();
                    buf
                })
                .unwrap_or_default();
            return Ok(std::process::Output {
                status,
                stdout: Vec::new(),
                stderr,
            });
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(FfmpegError::Failed {
                status: -1,
                stderr: "timeout after 30s".to_string(),
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_ffmpeg_returns_some_when_available() {
        let result = find_ffmpeg();
        if let Some(path) = &result {
            assert!(path.is_file() || path.exists());
        }
    }
}
