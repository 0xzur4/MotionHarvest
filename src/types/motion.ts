export type AiMode = "fast" | "studio_cuda" | "anime_2d";

export type TimingMode = "smooth" | "anime_12" | "ghibli_8" | "original_hold";

export type RetargetPreset = "raw" | "mixamo" | "rigify" | "vroid";

export interface AppSettings {
  vault_dir: string;
  blender_port: number;
  default_ai_mode: AiMode;
  scene_threshold: number;
  use_nvenc: "auto" | "on" | "off";
}

export interface HarvestConfig {
  scene_threshold: number;
  min_clip_sec: number;
  max_clip_sec: number;
  min_visibility: number;
  multi_person: boolean;
  max_persons: number;
}

export interface BatchJob {
  id: string;
  source_path: string;
  source_name: string;
  source_duration_sec: number;
  ai_mode: AiMode;
  config: HarvestConfig;
  status:
    | "queued"
    | "splitting"
    | "processing"
    | "paused"
    | "completed"
    | "cancelled"
    | "failed";
  total_segments: number;
  done_segments: number;
  face_segments: number;
  skipped_segments: number;
  error_message: string | null;
  created_at: string;
  updated_at: string;
}

export interface PolishConfig {
  body_smooth: number;
  face_smooth: number;
  foot_lock: boolean;
  include_hands: boolean;
  include_face: boolean;
  include_muscle: boolean;
  timing_mode: TimingMode;
  trim_start: number;
  trim_end: number;
}

export interface MotionClip {
  id: string;
  job_id: string | null;
  collection_id: string | null;
  clip_type: "full_body" | "face_only" | "blended";
  person_track_id: number;
  title: string;
  source_video: string;
  source_start_sec: number;
  source_end_sec: number;
  fps: number;
  total_raw_frames: number;
  frame_count: number;
  duration_sec: number;
  quality_score: number;
  has_hands: boolean;
  has_face: boolean;
  body_tags: string[];
  emotion_tags: string[];
  is_favorite: boolean;
  raw_perf_path: string;
  bvh_path: string | null;
  face_path: string | null;
  muscle_path: string | null;
  polish_config: PolishConfig;
  created_at: string;
}

export interface Collection {
  id: string;
  name: string;
  created_at: string;
  clip_count: number;
}

export interface VaultFilter {
  search?: string;
  clip_type?: string;
  body_tag?: string;
  emotion_tag?: string;
  collection_id?: string;
  favorites_only?: boolean;
}

export interface FaceTrackFile {
  fps: number;
  frame_count: number;
  timing_mode: string;
  frames: Record<string, number>[];
}

export interface MuscleFrame {
  t: number;
  left_elbow_flex: number;
  right_elbow_flex: number;
  left_knee_flex: number;
  right_knee_flex: number;
  shoulder_raise_l: number;
  shoulder_raise_r: number;
  angular_velocity: number;
  impact_spike: number;
}

export interface MuscleTrackFile {
  fps: number;
  frame_count: number;
  frames: MuscleFrame[];
}

export interface ClipPreviewData {
  clip: MotionClip;
  bvh_content: string | null;
  face_data: FaceTrackFile | null;
  muscle_data: MuscleTrackFile | null;
}

export interface ModelInfo {
  id: string;
  name: string;
  package: "basic" | "anime_2d" | "studio_cuda";
  filename: string;
  url: string;
  size_mb: number;
  installed: boolean;
  path: string;
  description: string;
}

export interface ModelDownloadProgress {
  model_id: string;
  model_name: string;
  downloaded_bytes: number;
  total_bytes: number;
  percent: number;
  status: "downloading" | "completed" | "failed";
  error: string | null;
}

export interface BlenderStatus {
  connected: boolean;
  blender_version: string | null;
  selected_armature: string | null;
  error: string | null;
}

export interface BlenderSendOptions {
  retarget_preset: RetargetPreset;
  global_scale: number;
  stepped_anime: boolean;
  apply_muscle_drivers: boolean;
}

export interface ExportResult {
  exported_clips: number;
  files_written: string[];
  target_dir: string;
}
