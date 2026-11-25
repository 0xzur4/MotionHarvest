use crate::models::{HarvestConfig, JobSegment};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use uuid::Uuid;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Creates a `Command` that never flashes a black console window on Windows GUI builds.
fn new_hidden_command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[derive(Debug, Error)]
pub enum SplitterError {
    #[error("Failed to execute {0}: {1}")]
    Exec(String, std::io::Error),
    #[error("Command {0} failed: {1}")]
    CommandFailed(String, String),
    #[error("Failed to parse ffprobe JSON: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub duration_sec: f64,
    pub fps: f64,
}

/// Resolves `ffmpeg` or `ffprobe` from PATH, or falls back to standard Windows install locations
/// (including WinGet `Gyan.FFmpeg`) so newly installed binaries work immediately without reboot.
pub fn resolve_media_tool(name: &str) -> PathBuf {
    // Check PATH first
    if let Ok(path_var) = std::env::var("PATH") {
        let exe_name = if cfg!(windows) {
            format!("{}.exe", name)
        } else {
            name.to_string()
        };
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(&exe_name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    #[cfg(windows)]
    {
        let exe_name = format!("{}.exe", name);
        let mut search_roots = vec![
            PathBuf::from(r"C:\ffmpeg\bin"),
            PathBuf::from(r"C:\Program Files\ffmpeg\bin"),
        ];
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let links = PathBuf::from(&local_app).join("Microsoft").join("WinGet").join("Links");
            search_roots.push(links);
            let pkgs = PathBuf::from(&local_app).join("Microsoft").join("WinGet").join("Packages");
            if let Ok(entries) = std::fs::read_dir(&pkgs) {
                for ent in entries.flatten() {
                    let fname = ent.file_name().to_string_lossy().to_ascii_lowercase();
                    if fname.contains("ffmpeg") {
                        for walk in walkdir::WalkDir::new(ent.path()).max_depth(3).into_iter().flatten() {
                            if walk.file_name().to_string_lossy().eq_ignore_ascii_case(&exe_name) {
                                return walk.into_path();
                            }
                        }
                    }
                }
            }
        }
        for root in search_roots {
            let c = root.join(&exe_name);
            if c.is_file() {
                return c;
            }
        }
    }

    PathBuf::from(name)
}

fn parse_frame_rate(raw: &str) -> f64 {
    let trimmed = raw.trim();
    if let Some((num_s, den_s)) = trimmed.split_once('/') {
        let num = num_s.parse::<f64>().unwrap_or(24.0);
        let den = den_s.parse::<f64>().unwrap_or(1.0);
        if den > 0.0 && num > 0.0 {
            return num / den;
        }
    }
    trimmed.parse::<f64>().unwrap_or(24.0)
}

/// Probes duration and FPS of a video via `ffprobe`.
pub fn probe_video(video_path: &Path) -> Result<VideoInfo, SplitterError> {
    let ffprobe = resolve_media_tool("ffprobe");
    let output = new_hidden_command(&ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-show_entries",
            "stream=r_frame_rate,codec_type",
            "-of",
            "json",
        ])
        .arg(video_path)
        .output()
        .map_err(|e| SplitterError::Exec("ffprobe".into(), e))?;

    if !output.status.success() {
        return Err(SplitterError::CommandFailed(
            "ffprobe".into(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let val: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| SplitterError::Parse(e.to_string()))?;

    let duration_sec = val
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let mut fps = 24.0;
    if let Some(streams) = val.get("streams").and_then(|s| s.as_array()) {
        for s in streams {
            if s.get("codec_type").and_then(|v| v.as_str()) == Some("video") {
                if let Some(rate_str) = s.get("r_frame_rate").and_then(|v| v.as_str()) {
                    fps = parse_frame_rate(rate_str);
                    break;
                }
            }
        }
    }

    Ok(VideoInfo {
        duration_sec,
        fps: fps.clamp(1.0, 240.0),
    })
}

/// Runs FFmpeg `scdet` filter and extracts cut timestamps (`lavfi.scd.time:<sec>`).
pub fn detect_scene_cuts(video_path: &Path, scene_threshold: f64) -> Result<Vec<f64>, SplitterError> {
    let ffmpeg = resolve_media_tool("ffmpeg");
    let filter = format!("scdet=threshold={:.2}", scene_threshold.clamp(1.0, 100.0));
    let output = new_hidden_command(&ffmpeg)
        .args(["-hide_banner", "-i"])
        .arg(video_path)
        .args(["-vf", &filter, "-an", "-f", "null", "-"])
        .output()
        .map_err(|e| SplitterError::Exec("ffmpeg".into(), e))?;

    if !output.status.success() {
        return Err(SplitterError::CommandFailed(
            "ffmpeg scdet".into(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let re = Regex::new(r"lavfi\.scd\.time:\s*([0-9]+(?:\.[0-9]+)?)").unwrap();
    let mut cuts = Vec::new();
    for cap in re.captures_iter(&stderr) {
        if let Some(m) = cap.get(1) {
            if let Ok(ts) = m.as_str().parse::<f64>() {
                if ts > 0.05 {
                    cuts.push(ts);
                }
            }
        }
    }

    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut deduped: Vec<f64> = Vec::new();
    for t in cuts {
        if deduped
            .last()
            .map(|prev| (t - prev).abs() > 0.25)
            .unwrap_or(true)
        {
            deduped.push(t);
        }
    }
    Ok(deduped)
}

/// Normalizes cut timestamps into clean `(start_sec, end_sec)` blocks obeying
/// `min_clip_sec` and `max_clip_sec`.
pub fn build_segment_ranges(
    duration_sec: f64,
    cuts: &[f64],
    min_clip_sec: f64,
    max_clip_sec: f64,
) -> Vec<(f64, f64)> {
    if duration_sec <= 0.05 {
        return Vec::new();
    }

    let min_s = min_clip_sec.max(0.2);
    let max_s = max_clip_sec.max(min_s + 0.5);

    let mut boundaries = vec![0.0];
    for &c in cuts {
        if c > 0.1 && c < duration_sec - 0.1 {
            boundaries.push(c);
        }
    }
    boundaries.push(duration_sec);

    let mut ranges = Vec::new();
    for win in boundaries.windows(2) {
        let start = win[0];
        let end = win[1];
        let len = end - start;
        if len < min_s {
            continue;
        }
        if len <= max_s {
            ranges.push((start, end));
        } else {
            let chunks = (len / max_s).ceil() as usize;
            let chunk_len = len / chunks as f64;
            for i in 0..chunks {
                let s = start + i as f64 * chunk_len;
                let e = if i + 1 == chunks {
                    end
                } else {
                    start + (i + 1) as f64 * chunk_len
                };
                if e - s >= min_s {
                    ranges.push((s, e));
                }
            }
        }
    }

    // If a short standalone clip was shorter than min_clip_sec, still process it as 1 segment
    if ranges.is_empty() && duration_sec >= 0.25 {
        ranges.push((0.0, duration_sec));
    }

    ranges
}

pub fn has_nvenc() -> bool {
    let ffmpeg = resolve_media_tool("ffmpeg");
    new_hidden_command(&ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("h264_nvenc")
        })
        .unwrap_or(false)
}

pub fn cut_segment_file(
    video_path: &Path,
    output_path: &Path,
    start_sec: f64,
    end_sec: f64,
    try_nvenc: bool,
) -> Result<(), SplitterError> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ffmpeg = resolve_media_tool("ffmpeg");
    let duration = (end_sec - start_sec).max(0.1);
    let start_str = format!("{:.3}", start_sec);
    let dur_str = format!("{:.3}", duration);

    if try_nvenc {
        let nv = new_hidden_command(&ffmpeg)
            .args([
                "-hide_banner",
                "-y",
                "-ss",
                &start_str,
                "-i",
            ])
            .arg(video_path)
            .args([
                "-t",
                &dur_str,
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p1",
                "-an",
                "-movflags",
                "+faststart",
            ])
            .arg(output_path)
            .output();

        if let Ok(out) = nv {
            if out.status.success() && output_path.exists() {
                return Ok(());
            }
        }
    }

    let out = new_hidden_command(&ffmpeg)
        .args([
            "-hide_banner",
            "-y",
            "-ss",
            &start_str,
            "-i",
        ])
        .arg(video_path)
        .args([
            "-t",
            &dur_str,
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
            "-an",
            "-movflags",
            "+faststart",
        ])
        .arg(output_path)
        .output()
        .map_err(|e| SplitterError::Exec("ffmpeg cut".into(), e))?;

    if !out.status.success() {
        return Err(SplitterError::CommandFailed(
            "ffmpeg cut".into(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ));
    }
    Ok(())
}

/// Splits a video file into clean camera-shot temp clips under `<vault_dir>/temp/`.
pub fn split_video_into_segments(
    job_id: &str,
    video_path: &Path,
    temp_dir: &Path,
    config: &HarvestConfig,
    use_nvenc_setting: &str,
) -> Result<(VideoInfo, Vec<JobSegment>), SplitterError> {
    std::fs::create_dir_all(temp_dir)?;
    let info = probe_video(video_path)?;
    let cuts = detect_scene_cuts(video_path, config.scene_threshold)?;
    let ranges = build_segment_ranges(
        info.duration_sec,
        &cuts,
        config.min_clip_sec,
        config.max_clip_sec,
    );

    let try_nvenc = match use_nvenc_setting {
        "on" => true,
        "off" => false,
        _ => has_nvenc(),
    };

    let mut segments = Vec::with_capacity(ranges.len());
    for (idx, (start_sec, end_sec)) in ranges.into_iter().enumerate() {
        let seg_id = Uuid::new_v4().to_string();
        let out_file: PathBuf = temp_dir.join(format!("{}_seg_{:04}.mp4", job_id, idx));
        cut_segment_file(video_path, &out_file, start_sec, end_sec, try_nvenc)?;
        segments.push(JobSegment {
            id: seg_id,
            job_id: job_id.to_string(),
            segment_index: idx as i64,
            start_sec,
            end_sec,
            temp_video_path: Some(out_file.to_string_lossy().to_string()),
            detected_class: None,
            status: "pending".to_string(),
            error_message: None,
        });
    }

    Ok((info, segments))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::VaultDb;
    use tempfile::tempdir;

    #[test]
    fn test_build_segment_ranges_min_max() {
        let ranges = build_segment_ranges(35.0, &[0.5, 4.0, 29.0], 1.2, 12.0);
        // [0.0..0.5] dropped (<1.2), [0.5..4.0] kept (3.5s),
        // [4.0..29.0] = 25s -> split into 3 chunks (~8.33s each), [29.0..35.0] kept (6s)
        assert_eq!(ranges.len(), 5);
        for (s, e) in &ranges {
            assert!((e - s) >= 1.2 && (e - s) <= 12.01);
        }
    }

    #[test]
    fn test_scdet_three_color_scenes_end_to_end() {
        // Generate a 12-second video with 3 distinct scenes (4s red, 4s lime/bright green, 4s blue)
        // Note: CSS `green` (#008000) has nearly identical luma (Y=75) to `red` (Y=76),
        // whereas `lime` (#00FF00, Y=150) produces a true scene cut in scdet luma analysis.
        let dir = tempdir().unwrap();
        let test_vid = dir.path().join("three_scenes.mp4");
        let ffmpeg = resolve_media_tool("ffmpeg");
        let gen = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=320x240:d=4:r=24",
                "-f",
                "lavfi",
                "-i",
                "color=c=lime:s=320x240:d=4:r=24",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=320x240:d=4:r=24",
                "-filter_complex",
                "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
                "-map",
                "[v]",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&test_vid)
            .output()
            .expect("ffmpeg must be available in PATH for Step 3 test");

        assert!(
            gen.status.success(),
            "Failed to generate test video: {}",
            String::from_utf8_lossy(&gen.stderr)
        );

        let vault_dir = dir.path().join("vault");
        let db = VaultDb::open(&vault_dir).unwrap();
        let cfg = HarvestConfig {
            scene_threshold: 12.0,
            min_clip_sec: 1.2,
            max_clip_sec: 20.0,
            ..Default::default()
        };
        let job = db
            .create_job(
                &test_vid.to_string_lossy(),
                "three_scenes.mp4",
                "fast",
                &cfg,
            )
            .unwrap();

        let (info, segments) = split_video_into_segments(
            &job.id,
            &test_vid,
            &vault_dir.join("temp"),
            &cfg,
            "off",
        )
        .unwrap();

        assert!((info.duration_sec - 12.0).abs() < 0.6);
        assert_eq!(segments.len(), 3, "Expected exactly 3 camera scenes");

        db.insert_segments(&segments).unwrap();
        let saved = db.list_segments_for_job(&job.id).unwrap();
        assert_eq!(saved.len(), 3);
        for seg in &saved {
            let p = seg.temp_video_path.as_ref().unwrap();
            assert!(Path::new(p).is_file(), "Segment file missing: {}", p);
        }
    }
}
