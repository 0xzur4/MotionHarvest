use crate::kinematics::muscle_curves::compute_muscle_curves;
use crate::kinematics::skeleton::lm;
use crate::models::RawPerfFrame;

/// Automatically detects body motion tags:
/// `idle`, `walk`, `run`, `jump`, `sit_crouch`, `gesture`, `action`.
pub fn detect_body_tags(frames: &[RawPerfFrame], fps: f64) -> Vec<String> {
    if frames.len() < 2 {
        return Vec::new();
    }
    let has_body = frames
        .iter()
        .any(|f| f.body_joints.as_ref().map(|j| j.len() > lm::RIGHT_FOOT_INDEX).unwrap_or(false));
    if !has_body {
        return Vec::new();
    }

    let fps = fps.max(1.0);
    let mut tags = Vec::new();

    // 1. Horizontal root speed (m/s)
    let mut total_dist = 0.0;
    let mut min_root_y = f64::MAX;
    let mut max_root_y = f64::MIN;
    let mut wrist_motion_sum = 0.0;
    let mut min_foot_y = f64::MAX;
    let mut max_both_feet_air = 0.0;

    for i in 0..frames.len() {
        let rp = frames[i].root_pos;
        if rp[1].is_finite() {
            min_root_y = min_root_y.min(rp[1]);
            max_root_y = max_root_y.max(rp[1]);
        }
        if i > 0 {
            let prev = frames[i - 1].root_pos;
            total_dist += (rp[0] - prev[0]).hypot(rp[2] - prev[2]);

            if let (Some(curr_j), Some(prev_j)) =
                (&frames[i].body_joints, &frames[i - 1].body_joints)
            {
                if curr_j.len() > lm::RIGHT_FOOT_INDEX && prev_j.len() > lm::RIGHT_FOOT_INDEX {
                    for &w in &[lm::LEFT_WRIST, lm::RIGHT_WRIST] {
                        let dx = curr_j[w][0] - prev_j[w][0];
                        let dy = curr_j[w][1] - prev_j[w][1];
                        let dz = curr_j[w][2] - prev_j[w][2];
                        wrist_motion_sum += (dx * dx + dy * dy + dz * dz).sqrt();
                    }
                }
            }
        }
        if let Some(ref j) = frames[i].body_joints {
            if j.len() > lm::RIGHT_FOOT_INDEX {
                let lf = j[lm::LEFT_HEEL][1].min(j[lm::LEFT_FOOT_INDEX][1]);
                let rf = j[lm::RIGHT_HEEL][1].min(j[lm::RIGHT_FOOT_INDEX][1]);
                min_foot_y = min_foot_y.min(lf.min(rf));
            }
        }
    }

    if min_foot_y.is_finite() && min_foot_y < f64::MAX / 2.0 {
        for f in frames {
            if let Some(ref j) = f.body_joints {
                if j.len() > lm::RIGHT_FOOT_INDEX {
                    let lf = j[lm::LEFT_HEEL][1].min(j[lm::LEFT_FOOT_INDEX][1]);
                    let rf = j[lm::RIGHT_HEEL][1].min(j[lm::RIGHT_FOOT_INDEX][1]);
                    let air = lf.min(rf) - min_foot_y;
                    if air > max_both_feet_air {
                        max_both_feet_air = air;
                    }
                }
            }
        }
    }

    let duration = (frames.len().saturating_sub(1) as f64 / fps).max(0.1);
    let avg_speed = total_dist / duration;
    let avg_wrist_speed = (wrist_motion_sum / 2.0) / duration;

    // 2. Knee flexion & angular energy via muscle curves
    let muscle = compute_muscle_curves(frames, fps);
    let avg_knee_flex = if !muscle.frames.is_empty() {
        muscle
            .frames
            .iter()
            .map(|m| 0.5 * (m.left_knee_flex + m.right_knee_flex))
            .sum::<f64>()
            / muscle.frames.len() as f64
    } else {
        0.0
    };
    let peak_ang_vel = muscle
        .frames
        .iter()
        .map(|m| m.angular_velocity)
        .fold(0.0_f64, f64::max);

    if avg_speed >= 2.0 {
        tags.push("run".to_string());
    } else if avg_speed >= 0.45 {
        tags.push("walk".to_string());
    }

    if max_both_feet_air >= 0.16 || (max_root_y - min_root_y) >= 0.25 {
        tags.push("jump".to_string());
    }

    if avg_knee_flex >= 0.42 && avg_speed < 0.8 {
        tags.push("sit_crouch".to_string());
    }

    if peak_ang_vel >= 0.55 || (avg_wrist_speed >= 1.6 && avg_speed >= 1.2) {
        tags.push("action".to_string());
    } else if avg_wrist_speed >= 0.55 && avg_speed < 1.2 {
        tags.push("gesture".to_string());
    }

    if tags.is_empty() {
        tags.push("idle".to_string());
    }

    tags
}

/// Automatically detects facial emotion tags from 52 ARKit blendshapes:
/// `happy`, `angry`, `sad`, `surprised`, `talking`.
pub fn detect_emotion_tags(frames: &[RawPerfFrame]) -> Vec<String> {
    let face_frames: Vec<_> = frames
        .iter()
        .filter_map(|f| f.face_blendshapes.as_ref())
        .filter(|m| !m.is_empty())
        .collect();

    if face_frames.is_empty() {
        return Vec::new();
    }

    let get = |m: &std::collections::BTreeMap<String, f64>, k: &str| -> f64 {
        *m.get(k).unwrap_or(&0.0)
    };

    let mut max_happy: f64 = 0.0;
    let mut max_angry: f64 = 0.0;
    let mut max_sad: f64 = 0.0;
    let mut max_surprised: f64 = 0.0;
    let mut jaw_vals = Vec::with_capacity(face_frames.len());

    for m in &face_frames {
        let smile = 0.5 * (get(m, "mouthSmileLeft") + get(m, "mouthSmileRight"));
        let angry = 0.5 * (get(m, "browDownLeft") + get(m, "browDownRight"));
        let frown = 0.5 * (get(m, "mouthFrownLeft") + get(m, "mouthFrownRight"));
        let brow_in = get(m, "browInnerUp");
        let jaw = get(m, "jawOpen");
        let eye_wide = 0.5 * (get(m, "eyeWideLeft") + get(m, "eyeWideRight"));

        max_happy = max_happy.max(smile);
        max_angry = max_angry.max(angry);
        max_sad = max_sad.max(0.5 * (frown + brow_in));
        max_surprised = max_surprised.max(0.5 * (jaw + eye_wide));
        jaw_vals.push(jaw);
    }

    // Also evaluate sums matching blueprint thresholds directly:
    // mouthSmileLeft+Right > 0.45 -> happy, browDownLeft+Right > 0.40 -> angry,
    // mouthFrown + browInnerUp > 0.35 -> sad, jawOpen + eyeWide > 0.45 -> surprised.
    let mut tags = Vec::new();

    let happy_hit = face_frames
        .iter()
        .any(|m| (get(m, "mouthSmileLeft") + get(m, "mouthSmileRight")) > 0.45)
        || max_happy > 0.35;
    let angry_hit = face_frames
        .iter()
        .any(|m| (get(m, "browDownLeft") + get(m, "browDownRight")) > 0.40)
        || max_angry > 0.30;
    let sad_hit = face_frames.iter().any(|m| {
        let frown = 0.5 * (get(m, "mouthFrownLeft") + get(m, "mouthFrownRight"));
        (frown + get(m, "browInnerUp")) > 0.35
    }) || max_sad > 0.30;
    let surprised_hit = face_frames.iter().any(|m| {
        let ew = 0.5 * (get(m, "eyeWideLeft") + get(m, "eyeWideRight"));
        (get(m, "jawOpen") + ew) > 0.45
    }) || max_surprised > 0.40;

    // Talking: repeated oscillation of jawOpen (variance + threshold crossings)
    let talking_hit = if jaw_vals.len() >= 4 {
        let mean = jaw_vals.iter().sum::<f64>() / jaw_vals.len() as f64;
        let variance = jaw_vals
            .iter()
            .map(|v| (v - mean) * (v - mean))
            .sum::<f64>()
            / jaw_vals.len() as f64;
        let mut crossings = 0usize;
        for w in jaw_vals.windows(2) {
            if (w[0] < 0.16 && w[1] >= 0.16) || (w[0] >= 0.16 && w[1] < 0.16) {
                crossings += 1;
            }
        }
        variance > 0.008 && crossings >= 2
    } else {
        false
    };

    if happy_hit {
        tags.push("happy".to_string());
    }
    if angry_hit {
        tags.push("angry".to_string());
    }
    if sad_hit {
        tags.push("sad".to_string());
    }
    if surprised_hit {
        tags.push("surprised".to_string());
    }
    if talking_hit {
        tags.push("talking".to_string());
    }

    tags
}
