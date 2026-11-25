use crate::kinematics::skeleton::lm;
use crate::models::RawPerfFrame;

const CONTACT_HEIGHT_TOLERANCE_M: f64 = 0.04;
const CONTACT_MAX_SPEED_MPS: f64 = 0.15;
const EASE_FRAMES: usize = 3;

fn smoothstep(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Estimates ground plane Y from the 5th percentile of lowest Heel/Toe vertical positions.
pub fn estimate_ground_y(frames: &[RawPerfFrame]) -> f64 {
    let mut ys = Vec::with_capacity(frames.len() * 4);
    for f in frames {
        if let Some(ref joints) = f.body_joints {
            if joints.len() > lm::RIGHT_FOOT_INDEX {
                for &idx in &[
                    lm::LEFT_HEEL,
                    lm::LEFT_FOOT_INDEX,
                    lm::RIGHT_HEEL,
                    lm::RIGHT_FOOT_INDEX,
                ] {
                    let y = joints[idx][1];
                    if y.is_finite() {
                        ys.push(y);
                    }
                }
            }
        }
    }
    if ys.is_empty() {
        return 0.0;
    }
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p5_idx = ((ys.len() as f64) * 0.05).floor() as usize;
    ys[p5_idx.min(ys.len() - 1)]
}

/// Locks Left & Right Heel + Toe contacts on the floor plane to eliminate foot skating.
/// Applies 3-frame smoothstep ease-in/out at contact boundaries and updates root_pos
/// proportionally when planted.
pub fn apply_foot_lock(frames: &mut [RawPerfFrame], fps: f64) {
    if frames.len() < 3 {
        return;
    }
    let has_feet = frames
        .iter()
        .all(|f| f.body_joints.as_ref().map(|j| j.len() > lm::RIGHT_FOOT_INDEX).unwrap_or(false));
    if !has_feet {
        return;
    }

    let ground_y = estimate_ground_y(frames);
    let fps = fps.max(1.0);

    // Process Left foot (Ankle, Heel, Toe) and Right foot independently
    let left_delta = lock_single_foot(
        frames,
        lm::LEFT_ANKLE,
        lm::LEFT_HEEL,
        lm::LEFT_FOOT_INDEX,
        ground_y,
        fps,
    );
    let right_delta = lock_single_foot(
        frames,
        lm::RIGHT_ANKLE,
        lm::RIGHT_HEEL,
        lm::RIGHT_FOOT_INDEX,
        ground_y,
        fps,
    );

    // Stabilize root horizontal drift subtly when either foot is planted
    for i in 0..frames.len() {
        let (dlx, dlz, wl) = left_delta[i];
        let (drx, drz, wr) = right_delta[i];
        let wsum = wl + wr;
        if wsum > 1e-4 {
            let corr_x = (dlx * wl + drx * wr) / wsum;
            let corr_z = (dlz * wl + drz * wr) / wsum;
            frames[i].root_pos[0] += corr_x * 0.35;
            frames[i].root_pos[2] += corr_z * 0.35;
        }
    }
}

/// Returns per-frame `(delta_x, delta_z, contact_weight)` for a single foot.
fn lock_single_foot(
    frames: &mut [RawPerfFrame],
    ankle_idx: usize,
    heel_idx: usize,
    toe_idx: usize,
    ground_y: f64,
    fps: f64,
) -> Vec<(f64, f64, f64)> {
    let n = frames.len();
    let mut is_planted = vec![false; n];

    // Step 1: detect frames where either Heel or Toe is on floor and moving slowly
    for i in 0..n {
        let joints = frames[i].body_joints.as_ref().unwrap();
        let heel = joints[heel_idx];
        let toe = joints[toe_idx];

        let prev_i = i.saturating_sub(1);
        let prev_joints = frames[prev_i].body_joints.as_ref().unwrap();
        let p_heel = prev_joints[heel_idx];
        let p_toe = prev_joints[toe_idx];

        let heel_speed = ((heel[0] - p_heel[0]).hypot(heel[2] - p_heel[2])) * fps;
        let toe_speed = ((toe[0] - p_toe[0]).hypot(toe[2] - p_toe[2])) * fps;

        let heel_contact =
            heel[1] < ground_y + CONTACT_HEIGHT_TOLERANCE_M && heel_speed < CONTACT_MAX_SPEED_MPS;
        let toe_contact =
            toe[1] < ground_y + CONTACT_HEIGHT_TOLERANCE_M && toe_speed < CONTACT_MAX_SPEED_MPS;

        is_planted[i] = heel_contact || toe_contact;
    }

    // Fill 1-frame contact gaps for temporal stability
    for i in 1..n.saturating_sub(1) {
        if !is_planted[i] && is_planted[i - 1] && is_planted[i + 1] {
            is_planted[i] = true;
        }
    }

    let mut deltas = vec![(0.0, 0.0, 0.0); n];

    // Step 2: find contiguous planted spans and anchor X/Z + floor Y
    let mut i = 0;
    while i < n {
        if !is_planted[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && is_planted[i] {
            i += 1;
        }
        let end = i; // exclusive
        let span_len = end - start;
        if span_len < 2 {
            continue;
        }

        // Compute anchor as average of first 2 frames of contact for stable placement
        let anchor_heel_x;
        let anchor_heel_z;
        let anchor_toe_x;
        let anchor_toe_z;
        {
            let j0 = frames[start].body_joints.as_ref().unwrap();
            let j1 = frames[(start + 1).min(end - 1)].body_joints.as_ref().unwrap();
            anchor_heel_x = 0.5 * (j0[heel_idx][0] + j1[heel_idx][0]);
            anchor_heel_z = 0.5 * (j0[heel_idx][2] + j1[heel_idx][2]);
            anchor_toe_x = 0.5 * (j0[toe_idx][0] + j1[toe_idx][0]);
            anchor_toe_z = 0.5 * (j0[toe_idx][2] + j1[toe_idx][2]);
        }

        for k in start..end {
            let from_start = k - start;
            let to_end = (end - 1) - k;
            let w_in = if from_start < EASE_FRAMES {
                smoothstep((from_start + 1) as f64 / EASE_FRAMES as f64)
            } else {
                1.0
            };
            let w_out = if to_end < EASE_FRAMES {
                smoothstep((to_end + 1) as f64 / EASE_FRAMES as f64)
            } else {
                1.0
            };
            let w = w_in.min(w_out);

            let joints = frames[k].body_joints.as_mut().unwrap();
            let old_heel_x = joints[heel_idx][0];
            let old_heel_z = joints[heel_idx][2];
            let old_toe_x = joints[toe_idx][0];
            let old_toe_z = joints[toe_idx][2];

            let new_heel_x = old_heel_x * (1.0 - w) + anchor_heel_x * w;
            let new_heel_z = old_heel_z * (1.0 - w) + anchor_heel_z * w;
            let new_toe_x = old_toe_x * (1.0 - w) + anchor_toe_x * w;
            let new_toe_z = old_toe_z * (1.0 - w) + anchor_toe_z * w;

            let dx = 0.5 * ((new_heel_x - old_heel_x) + (new_toe_x - old_toe_x));
            let dz = 0.5 * ((new_heel_z - old_heel_z) + (new_toe_z - old_toe_z));

            joints[heel_idx][0] = new_heel_x;
            joints[heel_idx][2] = new_heel_z;
            joints[toe_idx][0] = new_toe_x;
            joints[toe_idx][2] = new_toe_z;
            joints[ankle_idx][0] += dx;
            joints[ankle_idx][2] += dz;

            // Clamp foot height cleanly onto floor plane
            if joints[heel_idx][1] < ground_y + CONTACT_HEIGHT_TOLERANCE_M {
                joints[heel_idx][1] = joints[heel_idx][1] * (1.0 - w) + ground_y * w;
            }
            if joints[toe_idx][1] < ground_y + CONTACT_HEIGHT_TOLERANCE_M {
                joints[toe_idx][1] = joints[toe_idx][1] * (1.0 - w) + ground_y * w;
            }

            deltas[k] = (dx, dz, w);
        }
    }

    deltas
}
