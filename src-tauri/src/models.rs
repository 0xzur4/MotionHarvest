use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub vault_dir: String,
    pub blender_port: u16,
    pub default_ai_mode: String,
    pub scene_threshold: f64,
    pub use_nvenc: String,
}

impl AppSettings {
    pub fn default_with_vault(vault_dir: String) -> Self {
        Self {
            vault_dir,
            blender_port: 9877,
            default_ai_mode: "fast".to_string(),
            scene_threshold: 12.0,
            use_nvenc: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestConfig {
    pub scene_threshold: f64,
    pub min_clip_sec: f64,
    pub max_clip_sec: f64,
    pub min_visibility: f64,
    pub multi_person: bool,
    pub max_persons: u32,
}

impl Default for HarvestConfig {
    fn default() -> Self {
        Self {
            scene_threshold: 12.0,
            min_clip_sec: 1.2,
            max_clip_sec: 20.0,
            min_visibility: 0.50,
            multi_person: true,
            max_persons: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: String,
    pub source_path: String,
    pub source_name: String,
    pub source_duration_sec: f64,
    pub ai_mode: String,
    pub config: HarvestConfig,
    pub status: String,
    pub total_segments: i64,
    pub done_segments: i64,
    pub face_segments: i64,
    pub skipped_segments: i64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSegment {
    pub id: String,
    pub job_id: String,
    pub segment_index: i64,
    pub start_sec: f64,
    pub end_sec: f64,
    pub temp_video_path: Option<String>,
    pub detected_class: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingMode {
    Smooth,
    Anime12,
    Ghibli8,
    OriginalHold,
}

impl Default for TimingMode {
    fn default() -> Self {
        TimingMode::Smooth
    }
}

impl TimingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimingMode::Smooth => "smooth",
            TimingMode::Anime12 => "anime_12",
            TimingMode::Ghibli8 => "ghibli_8",
            TimingMode::OriginalHold => "original_hold",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolishConfig {
    pub body_smooth: f64,
    pub face_smooth: f64,
    pub foot_lock: bool,
    pub include_hands: bool,
    pub include_face: bool,
    pub include_muscle: bool,
    pub timing_mode: TimingMode,
    pub trim_start: usize,
    pub trim_end: usize,
}

impl Default for PolishConfig {
    fn default() -> Self {
        Self {
            body_smooth: 0.35,
            face_smooth: 0.40,
            foot_lock: true,
            include_hands: true,
            include_face: true,
            include_muscle: true,
            timing_mode: TimingMode::Smooth,
            trim_start: 0,
            trim_end: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionClip {
    pub id: String,
    pub job_id: Option<String>,
    pub collection_id: Option<String>,
    pub clip_type: String,
    pub person_track_id: i64,
    pub title: String,
    pub source_video: String,
    pub source_start_sec: f64,
    pub source_end_sec: f64,
    pub fps: f64,
    pub total_raw_frames: i64,
    pub frame_count: i64,
    pub duration_sec: f64,
    pub quality_score: f64,
    pub has_hands: bool,
    pub has_face: bool,
    pub body_tags: Vec<String>,
    pub emotion_tags: Vec<String>,
    pub is_favorite: bool,
    pub raw_perf_path: String,
    pub bvh_path: Option<String>,
    pub face_path: Option<String>,
    pub muscle_path: Option<String>,
    pub polish_config: PolishConfig,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub clip_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultFilter {
    pub search: Option<String>,
    pub clip_type: Option<String>,
    pub body_tag: Option<String>,
    pub emotion_tag: Option<String>,
    pub collection_id: Option<String>,
    pub favorites_only: Option<bool>,
}

/// Raw performance frame saved by Python sidecar (`*.rawperf.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPerfFrame {
    pub t: f64,
    #[serde(default)]
    pub root_pos: [f64; 3],
    #[serde(default)]
    pub body_joints: Option<Vec<[f64; 4]>>,
    #[serde(default)]
    pub left_hand: Option<Vec<[f64; 4]>>,
    #[serde(default)]
    pub right_hand: Option<Vec<[f64; 4]>>,
    #[serde(default)]
    pub face_blendshapes: Option<BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPerfData {
    pub fps: f64,
    pub clip_type: String,
    pub source_mode: String,
    #[serde(default)]
    pub person_track_id: i64,
    #[serde(default)]
    pub anime_hold_indices: Vec<usize>,
    pub frames: Vec<RawPerfFrame>,
}

/// Polished facial animation file (`*.face.json`) using 52 ARKit blendshapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceTrackFile {
    pub fps: f64,
    pub frame_count: usize,
    pub timing_mode: String,
    pub frames: Vec<BTreeMap<String, f64>>,
}

/// Joint flexure, angular velocity, and impact signals per frame (`*.muscle.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MuscleFrame {
    pub t: f64,
    pub left_elbow_flex: f64,
    pub right_elbow_flex: f64,
    pub left_knee_flex: f64,
    pub right_knee_flex: f64,
    pub shoulder_raise_l: f64,
    pub shoulder_raise_r: f64,
    pub angular_velocity: f64,
    pub impact_spike: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuscleTrackFile {
    pub fps: f64,
    pub frame_count: usize,
    pub frames: Vec<MuscleFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipPreviewData {
    pub clip: MotionClip,
    pub bvh_content: Option<String>,
    pub face_data: Option<FaceTrackFile>,
    pub muscle_data: Option<MuscleTrackFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub package: String, // "basic" | "anime_2d" | "studio_cuda"
    pub filename: String,
    pub url: String,
    pub size_mb: f64,
    pub installed: bool,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub model_name: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub status: String, // "downloading" | "completed" | "failed"
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlenderStatus {
    pub connected: bool,
    pub blender_version: Option<String>,
    pub selected_armature: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlenderSendOptions {
    pub retarget_preset: String, // "raw" | "mixamo" | "rigify" | "vroid"
    pub global_scale: f64,
    pub stepped_anime: bool,
    pub apply_muscle_drivers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub exported_clips: usize,
    pub files_written: Vec<String>,
    pub target_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentClassifiedEvent {
    pub job_id: String,
    pub segment_id: String,
    pub detected_class: String,
    pub person_count: usize,
}
