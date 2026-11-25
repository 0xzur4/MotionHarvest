pub mod anime_timing;
pub mod auto_tagger;
pub mod bvh_writer;
pub mod foot_lock;
pub mod motion_blender;
pub mod muscle_curves;
pub mod one_euro;
pub mod skeleton;

use crate::models::{FaceTrackFile, MuscleTrackFile, PolishConfig, RawPerfData};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KinematicsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Raw performance contains no frames")]
    EmptyFrames,
}

#[derive(Debug, Clone)]
pub struct CompiledArtifacts {
    pub frame_count: usize,
    pub duration_sec: f64,
    pub body_tags: Vec<String>,
    pub emotion_tags: Vec<String>,
    pub bvh_path: Option<PathBuf>,
    pub face_path: Option<PathBuf>,
    pub muscle_path: Option<PathBuf>,
    pub bvh_content: Option<String>,
    pub face_data: Option<FaceTrackFile>,
    pub muscle_data: Option<MuscleTrackFile>,
}

/// Orchestrates the sub-15ms pure-Rust compilation pipeline:
/// `RawPerfData -> Trim -> One-Euro (Body & Face) -> Heel/Toe Foot-Lock -> Anime Timing -> BVH + Face + Muscle + Auto-Tags`
pub fn compile_raw_performance(
    raw: &RawPerfData,
    polish: &PolishConfig,
    vault_dir: &Path,
    clip_id: &str,
) -> Result<CompiledArtifacts, KinematicsError> {
    if raw.frames.is_empty() {
        return Err(KinematicsError::EmptyFrames);
    }

    let fps = raw.fps.max(1.0);
    let total = raw.frames.len();
    let trim_start = polish.trim_start.min(total.saturating_sub(1));
    let trim_end = if polish.trim_end == 0 || polish.trim_end >= total {
        total - 1
    } else {
        polish.trim_end.max(trim_start)
    };

    let mut frames = raw.frames[trim_start..=trim_end].to_vec();

    // 1. One-Euro adaptive smoothing (Body/Hands & 52 Face Blendshapes separately)
    one_euro::smooth_body_frames(&mut frames, fps, polish.body_smooth);
    one_euro::smooth_face_frames(&mut frames, fps, polish.face_smooth);

    // 2. Heel + Toe ground contact locking
    let is_face_only = raw.clip_type == "face_only"
        || frames.iter().all(|f| f.body_joints.is_none());

    if polish.foot_lock && !is_face_only {
        foot_lock::apply_foot_lock(&mut frames, fps);
    }

    // 3. Anime timing quantization (simultaneously on body, hands, and face!)
    let frames = anime_timing::apply_anime_timing(
        &frames,
        fps,
        polish.timing_mode,
        &raw.anime_hold_indices,
        trim_start,
    );

    let frame_count = frames.len();
    let duration_sec = frame_count as f64 / fps;

    // 4. Auto-Tagger (Body + Emotion)
    let body_tags = if is_face_only {
        Vec::new()
    } else {
        auto_tagger::detect_body_tags(&frames, fps)
    };
    let emotion_tags = auto_tagger::detect_emotion_tags(&frames);

    // 5. BVH Serialization (21 or 51 bones)
    let (bvh_path, bvh_content) = if !is_face_only {
        let bvh_str = bvh_writer::write_bvh_string(&frames, fps, polish.include_hands);
        let dir = vault_dir.join("bvh");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.bvh", clip_id));
        std::fs::write(&path, &bvh_str)?;
        (Some(path), Some(bvh_str))
    } else {
        (None, None)
    };

    // 6. Face Blendshapes Serialization (`.face.json`)
    let has_any_face = frames
        .iter()
        .any(|f| f.face_blendshapes.as_ref().map(|m| !m.is_empty()).unwrap_or(false));
    let (face_path, face_data) = if (polish.include_face || is_face_only) && has_any_face {
        let face_frames: Vec<BTreeMap<String, f64>> = frames
            .iter()
            .map(|f| f.face_blendshapes.clone().unwrap_or_default())
            .collect();
        let track = FaceTrackFile {
            fps,
            frame_count: face_frames.len(),
            timing_mode: polish.timing_mode.as_str().to_string(),
            frames: face_frames,
        };
        let dir = vault_dir.join("face");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.face.json", clip_id));
        std::fs::write(&path, serde_json::to_string(&track)?)?;
        (Some(path), Some(track))
    } else {
        (None, None)
    };

    // 7. Muscle Driver Curves Serialization (`.muscle.json`)
    let (muscle_path, muscle_data) = if polish.include_muscle && !is_face_only {
        let track: MuscleTrackFile = muscle_curves::compute_muscle_curves(&frames, fps);
        let dir = vault_dir.join("muscle");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.muscle.json", clip_id));
        std::fs::write(&path, serde_json::to_string(&track)?)?;
        (Some(path), Some(track))
    } else {
        (None, None)
    };

    Ok(CompiledArtifacts {
        frame_count,
        duration_sec,
        body_tags,
        emotion_tags,
        bvh_path,
        face_path,
        muscle_path,
        bvh_content,
        face_data,
        muscle_data,
    })
}

pub fn load_raw_perf(path: &Path) -> Result<RawPerfData, KinematicsError> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::kinematics::skeleton::{build_skeleton, lm};
    use crate::models::{RawPerfFrame, TimingMode};
    use tempfile::tempdir;

    fn make_tpose_joints(root_x: f64, root_y: f64, root_z: f64) -> Vec<[f64; 4]> {
        let mut j = vec![[root_x, root_y, root_z, 0.99]; 33];
        j[lm::NOSE] = [root_x, root_y + 0.68, root_z + 0.12, 0.99];
        j[lm::LEFT_EAR] = [root_x + 0.08, root_y + 0.68, root_z, 0.99];
        j[lm::RIGHT_EAR] = [root_x - 0.08, root_y + 0.68, root_z, 0.99];
        j[lm::LEFT_SHOULDER] = [root_x + 0.18, root_y + 0.48, root_z, 0.99];
        j[lm::RIGHT_SHOULDER] = [root_x - 0.18, root_y + 0.48, root_z, 0.99];
        j[lm::LEFT_ELBOW] = [root_x + 0.45, root_y + 0.48, root_z, 0.99];
        j[lm::RIGHT_ELBOW] = [root_x - 0.45, root_y + 0.48, root_z, 0.99];
        j[lm::LEFT_WRIST] = [root_x + 0.70, root_y + 0.48, root_z, 0.99];
        j[lm::RIGHT_WRIST] = [root_x - 0.70, root_y + 0.48, root_z, 0.99];
        j[lm::LEFT_INDEX] = [root_x + 0.79, root_y + 0.48, root_z + 0.02, 0.99];
        j[lm::LEFT_PINKY] = [root_x + 0.78, root_y + 0.48, root_z - 0.02, 0.99];
        j[lm::RIGHT_INDEX] = [root_x - 0.79, root_y + 0.48, root_z + 0.02, 0.99];
        j[lm::RIGHT_PINKY] = [root_x - 0.78, root_y + 0.48, root_z - 0.02, 0.99];
        j[lm::LEFT_HIP] = [root_x + 0.10, root_y, root_z, 0.99];
        j[lm::RIGHT_HIP] = [root_x - 0.10, root_y, root_z, 0.99];
        j[lm::LEFT_KNEE] = [root_x + 0.10, root_y - 0.44, root_z, 0.99];
        j[lm::RIGHT_KNEE] = [root_x - 0.10, root_y - 0.44, root_z, 0.99];
        j[lm::LEFT_ANKLE] = [root_x + 0.10, root_y - 0.86, root_z, 0.99];
        j[lm::RIGHT_ANKLE] = [root_x - 0.10, root_y - 0.86, root_z, 0.99];
        j[lm::LEFT_HEEL] = [root_x + 0.10, root_y - 0.94, root_z - 0.04, 0.99];
        j[lm::RIGHT_HEEL] = [root_x - 0.10, root_y - 0.94, root_z - 0.04, 0.99];
        j[lm::LEFT_FOOT_INDEX] = [root_x + 0.10, root_y - 0.94, root_z + 0.14, 0.99];
        j[lm::RIGHT_FOOT_INDEX] = [root_x - 0.10, root_y - 0.94, root_z + 0.14, 0.99];
        j
    }

    fn make_hand(sign: f64) -> Vec<[f64; 4]> {
        let mut h = vec![[0.0, 0.0, 0.0, 0.95]; 21];
        for f in 0..5 {
            let z = (2.0 - f as f64) * 0.015;
            for seg in 1..=4 {
                let idx = f * 4 + seg;
                h[idx] = [sign * (seg as f64) * 0.025, 0.0, z, 0.95];
            }
        }
        h
    }

    #[test]
    fn test_bvh_21_and_51_bones_valid_no_nan() {
        assert_eq!(build_skeleton(false).len(), 21);
        assert_eq!(build_skeleton(true).len(), 51);

        let frames: Vec<RawPerfFrame> = (0..24)
            .map(|i| {
                let t = i as f64 / 24.0;
                let mut joints = make_tpose_joints(0.0, 0.94, t * 0.8);
                // Swing left arm slightly
                joints[lm::LEFT_ELBOW][1] += (t * 6.0).sin() * 0.15;
                joints[lm::LEFT_WRIST][1] += (t * 6.0).sin() * 0.25;
                RawPerfFrame {
                    t,
                    root_pos: [0.0, 0.94, t * 0.8],
                    body_joints: Some(joints),
                    left_hand: Some(make_hand(1.0)),
                    right_hand: Some(make_hand(-1.0)),
                    face_blendshapes: None,
                }
            })
            .collect();

        let bvh21 = bvh_writer::write_bvh_string(&frames, 24.0, false);
        let bvh51 = bvh_writer::write_bvh_string(&frames, 24.0, true);

        assert!(!bvh21.contains("NaN") && !bvh21.contains("inf"));
        assert!(!bvh51.contains("NaN") && !bvh51.contains("inf"));
        assert_eq!(bvh21.matches("JOINT ").count(), 20); // 1 ROOT + 20 JOINT = 21 bones
        assert_eq!(bvh51.matches("JOINT ").count(), 50); // 1 ROOT + 50 JOINT = 51 bones

        // Check expected channel count per frame line
        let motion_line_21 = bvh21.lines().last().unwrap();
        assert_eq!(motion_line_21.split_whitespace().count(), 3 + 21 * 3);
        let motion_line_51 = bvh51.lines().last().unwrap();
        assert_eq!(motion_line_51.split_whitespace().count(), 3 + 51 * 3);
    }

    #[test]
    fn test_one_euro_reduces_jitter() {
        let mut frames: Vec<RawPerfFrame> = (0..60)
            .map(|i| {
                let noise: f64 = if i % 2 == 0 { 0.05 } else { -0.05 };
                let mut map = BTreeMap::new();
                map.insert("mouthSmileLeft".to_string(), (0.5_f64 + noise).clamp(0.0, 1.0));
                RawPerfFrame {
                    t: i as f64 / 30.0,
                    root_pos: [noise, 0.94, 0.0],
                    body_joints: Some(make_tpose_joints(noise, 0.94, 0.0)),
                    left_hand: None,
                    right_hand: None,
                    face_blendshapes: Some(map),
                }
            })
            .collect();

        let raw_energy: f64 = frames
            .windows(2)
            .map(|w| (w[1].root_pos[0] - w[0].root_pos[0]).abs())
            .sum();

        one_euro::smooth_body_frames(&mut frames, 30.0, 0.85);
        one_euro::smooth_face_frames(&mut frames, 30.0, 0.85);

        let smooth_energy: f64 = frames
            .windows(2)
            .map(|w| (w[1].root_pos[0] - w[0].root_pos[0]).abs())
            .sum();

        assert!(
            smooth_energy < raw_energy * 0.3,
            "Expected One-Euro to cut jitter sharply: raw={} smooth={}",
            raw_energy,
            smooth_energy
        );
    }

    #[test]
    fn test_foot_lock_reduces_drift_over_70_percent() {
        // Simulate planted left foot sliding slowly by 0.004m per frame (0.096 m/s < 0.15 m/s)
        let mut frames: Vec<RawPerfFrame> = (0..30)
            .map(|i| {
                let drift = i as f64 * 0.004;
                let mut joints = make_tpose_joints(0.0, 0.94, 0.0);
                joints[lm::LEFT_HEEL][0] += drift;
                joints[lm::LEFT_FOOT_INDEX][0] += drift;
                joints[lm::LEFT_ANKLE][0] += drift;
                RawPerfFrame {
                    t: i as f64 / 24.0,
                    root_pos: [0.0, 0.94, 0.0],
                    body_joints: Some(joints),
                    left_hand: None,
                    right_hand: None,
                    face_blendshapes: None,
                }
            })
            .collect();

        let drift_before = {
            let j0 = frames[3].body_joints.as_ref().unwrap()[lm::LEFT_HEEL][0];
            let jn = frames[26].body_joints.as_ref().unwrap()[lm::LEFT_HEEL][0];
            (jn - j0).abs()
        };

        foot_lock::apply_foot_lock(&mut frames, 24.0);

        let drift_after = {
            let j0 = frames[3].body_joints.as_ref().unwrap()[lm::LEFT_HEEL][0];
            let jn = frames[26].body_joints.as_ref().unwrap()[lm::LEFT_HEEL][0];
            (jn - j0).abs()
        };

        assert!(
            drift_after < drift_before * 0.25,
            "Expected foot-lock drift reduction > 75%: before={} after={}",
            drift_before,
            drift_after
        );
    }

    #[test]
    fn test_auto_tagger_body_and_emotions() {
        // Running + happy + talking
        let frames: Vec<RawPerfFrame> = (0..48)
            .map(|i| {
                let t = i as f64 / 24.0;
                let z = t * 2.6; // 2.6 m/s -> run
                let mut face = BTreeMap::new();
                face.insert("mouthSmileLeft".into(), 0.72);
                face.insert("mouthSmileRight".into(), 0.76);
                face.insert("jawOpen".into(), if (i / 3) % 2 == 0 { 0.45 } else { 0.04 });
                RawPerfFrame {
                    t,
                    root_pos: [0.0, 0.94, z],
                    body_joints: Some(make_tpose_joints(0.0, 0.94, z)),
                    left_hand: None,
                    right_hand: None,
                    face_blendshapes: Some(face),
                }
            })
            .collect();

        let btags = auto_tagger::detect_body_tags(&frames, 24.0);
        let etags = auto_tagger::detect_emotion_tags(&frames);

        assert!(btags.contains(&"run".to_string()), "btags={:?}", btags);
        assert!(etags.contains(&"happy".to_string()), "etags={:?}", etags);
        assert!(etags.contains(&"talking".to_string()), "etags={:?}", etags);
    }

    #[test]
    fn test_motion_blender_no_coordinate_jump() {
        let clip_a = RawPerfData {
            fps: 24.0,
            clip_type: "full_body".into(),
            source_mode: "fast".into(),
            person_track_id: 1,
            anime_hold_indices: vec![],
            frames: (0..30)
                .map(|i| {
                    let z = i as f64 * 0.04;
                    RawPerfFrame {
                        t: i as f64 / 24.0,
                        root_pos: [0.0, 0.94, z],
                        body_joints: Some(make_tpose_joints(0.0, 0.94, z)),
                        left_hand: None,
                        right_hand: None,
                        face_blendshapes: None,
                    }
                })
                .collect(),
        };

        // Second clip starts far away at (5.0, 0.94, -3.0)
        let clip_b = RawPerfData {
            fps: 24.0,
            clip_type: "full_body".into(),
            source_mode: "fast".into(),
            person_track_id: 1,
            anime_hold_indices: vec![],
            frames: (0..30)
                .map(|i| {
                    let z = -3.0 + i as f64 * 0.04;
                    RawPerfFrame {
                        t: i as f64 / 24.0,
                        root_pos: [5.0, 0.94, z],
                        body_joints: Some(make_tpose_joints(5.0, 0.94, z)),
                        left_hand: None,
                        right_hand: None,
                        face_blendshapes: None,
                    }
                })
                .collect(),
        };

        let blended = motion_blender::blend_raw_clips(&[clip_a, clip_b], 10).unwrap();
        assert_eq!(blended.clip_type, "blended");
        assert_eq!(blended.frames.len(), 50);

        // Verify no teleportation jump between any consecutive frames
        for w in blended.frames.windows(2) {
            let step = (w[1].root_pos[0] - w[0].root_pos[0]).hypot(w[1].root_pos[2] - w[0].root_pos[2]);
            assert!(step < 0.12, "Detected teleportation jump of {:.3}m", step);
        }
    }

    #[test]
    fn test_compile_raw_performance_full_pipeline() {
        let dir = tempdir().unwrap();
        let raw = RawPerfData {
            fps: 24.0,
            clip_type: "full_body".into(),
            source_mode: "anime_2d".into(),
            person_track_id: 1,
            anime_hold_indices: vec![0, 0, 2, 2, 2, 5, 5, 7, 7, 9, 9, 9],
            frames: (0..12)
                .map(|i| {
                    let mut face = BTreeMap::new();
                    face.insert("browDownLeft".into(), 0.55);
                    face.insert("browDownRight".into(), 0.52);
                    RawPerfFrame {
                        t: i as f64 / 24.0,
                        root_pos: [0.0, 0.94, i as f64 * 0.03],
                        body_joints: Some(make_tpose_joints(0.0, 0.94, i as f64 * 0.03)),
                        left_hand: Some(make_hand(1.0)),
                        right_hand: Some(make_hand(-1.0)),
                        face_blendshapes: Some(face),
                    }
                })
                .collect(),
        };

        let polish = PolishConfig {
            timing_mode: TimingMode::OriginalHold,
            ..Default::default()
        };

        let compiled = compile_raw_performance(&raw, &polish, dir.path(), "test_clip").unwrap();
        assert_eq!(compiled.frame_count, 12);
        assert!(compiled.bvh_path.as_ref().unwrap().is_file());
        assert!(compiled.face_path.as_ref().unwrap().is_file());
        assert!(compiled.muscle_path.as_ref().unwrap().is_file());
        assert!(compiled.emotion_tags.contains(&"angry".to_string()));
    }
}
