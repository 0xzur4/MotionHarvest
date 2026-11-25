use crate::kinematics::skeleton::lm;
use crate::models::{MuscleFrame, MuscleTrackFile, RawPerfFrame};
use glam::Vec3;

fn jvec(joints: &[[f64; 4]], idx: usize) -> Vec3 {
    joints
        .get(idx)
        .map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32))
        .unwrap_or(Vec3::ZERO)
}

/// Computes normalized bend factor in `[0.0, 1.0]` where 0.0 = fully straight (180 deg)
/// and 1.0 = tightly bent (~30 deg).
fn joint_flex_factor(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    let v1 = (a - b).normalize_or_zero();
    let v2 = (c - b).normalize_or_zero();
    if v1.length_squared() < 1e-5 || v2.length_squared() < 1e-5 {
        return 0.0;
    }
    let angle_deg = v1.dot(v2).clamp(-1.0, 1.0).acos().to_degrees() as f64;
    // 180 deg -> 0.0, 35 deg -> 1.0
    ((180.0 - angle_deg) / 145.0).clamp(0.0, 1.0)
}

/// Computes normalized shoulder raise factor `[0.0, 1.0]` from shoulder->elbow angle relative to down.
fn shoulder_raise_factor(shoulder: Vec3, elbow: Vec3) -> f64 {
    let d = (elbow - shoulder).normalize_or_zero();
    if d.length_squared() < 1e-5 {
        return 0.0;
    }
    // Straight down (y = -1) => 0.0, raised overhead (y = +1) => 1.0
    ((d.y as f64 + 1.0) * 0.5).clamp(0.0, 1.0)
}

/// Generates normalized muscle driver and secondary-physics curves (`.muscle.json`)
/// ready to bind to Blender Corrective Shape Keys & Pose Drivers.
pub fn compute_muscle_curves(frames: &[RawPerfFrame], fps: f64) -> MuscleTrackFile {
    let fps = fps.max(1.0);
    let mut out = Vec::with_capacity(frames.len());

    let mut prev_angles: Option<[f64; 6]> = None;
    let mut prev_vel_mag: f64 = 0.0;
    let mut prev_root_vy: f64 = 0.0;

    for (i, frame) in frames.iter().enumerate() {
        let mut mf = MuscleFrame {
            t: frame.t,
            ..Default::default()
        };

        if let Some(ref joints) = frame.body_joints {
            if joints.len() > lm::RIGHT_FOOT_INDEX {
                let l_sh = jvec(joints, lm::LEFT_SHOULDER);
                let l_el = jvec(joints, lm::LEFT_ELBOW);
                let l_wr = jvec(joints, lm::LEFT_WRIST);
                let r_sh = jvec(joints, lm::RIGHT_SHOULDER);
                let r_el = jvec(joints, lm::RIGHT_ELBOW);
                let r_wr = jvec(joints, lm::RIGHT_WRIST);

                let l_hip = jvec(joints, lm::LEFT_HIP);
                let l_kn = jvec(joints, lm::LEFT_KNEE);
                let l_an = jvec(joints, lm::LEFT_ANKLE);
                let r_hip = jvec(joints, lm::RIGHT_HIP);
                let r_kn = jvec(joints, lm::RIGHT_KNEE);
                let r_an = jvec(joints, lm::RIGHT_ANKLE);

                mf.left_elbow_flex = joint_flex_factor(l_sh, l_el, l_wr);
                mf.right_elbow_flex = joint_flex_factor(r_sh, r_el, r_wr);
                mf.left_knee_flex = joint_flex_factor(l_hip, l_kn, l_an);
                mf.right_knee_flex = joint_flex_factor(r_hip, r_kn, r_an);
                mf.shoulder_raise_l = shoulder_raise_factor(l_sh, l_el);
                mf.shoulder_raise_r = shoulder_raise_factor(r_sh, r_el);

                let curr_angles = [
                    mf.left_elbow_flex,
                    mf.right_elbow_flex,
                    mf.left_knee_flex,
                    mf.right_knee_flex,
                    mf.shoulder_raise_l,
                    mf.shoulder_raise_r,
                ];

                let ang_vel = if let Some(prev) = prev_angles {
                    let sum_delta: f64 = curr_angles
                        .iter()
                        .zip(prev.iter())
                        .map(|(a, b)| (a - b).abs())
                        .sum();
                    // Normalize angular velocity into 0.0..=1.0
                    ((sum_delta * fps) / 6.0).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                mf.angular_velocity = ang_vel;

                // Impact spike detects sudden deceleration (landing from jump or punch snap)
                let root_vy = if i > 0 {
                    (frame.root_pos[1] - frames[i - 1].root_pos[1]) * fps
                } else {
                    0.0
                };
                let landing_decel = (root_vy - prev_root_vy).max(0.0) / 8.0;
                let limb_snap = (prev_vel_mag - ang_vel).max(0.0) * 1.8;
                mf.impact_spike = (landing_decel + limb_snap).clamp(0.0, 1.0);

                prev_angles = Some(curr_angles);
                prev_vel_mag = ang_vel;
                prev_root_vy = root_vy;
            }
        }

        out.push(mf);
    }

    MuscleTrackFile {
        fps,
        frame_count: out.len(),
        frames: out,
    }
}
