use crate::kinematics::bvh_writer::{extract_hips_yaw, safe_rotation_arc};
use crate::kinematics::skeleton::lm;
use crate::models::{RawPerfData, RawPerfFrame};
use glam::{Quat, Vec3};
use std::collections::{BTreeMap, BTreeSet};

fn smoothstep(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn rotate_point_around_pivot_y(p: [f64; 3], pivot: [f64; 3], q: Quat) -> [f64; 3] {
    let local = Vec3::new(
        (p[0] - pivot[0]) as f32,
        (p[1] - pivot[1]) as f32,
        (p[2] - pivot[2]) as f32,
    );
    let rot = q * local;
    [
        rot.x as f64 + pivot[0],
        rot.y as f64 + pivot[1],
        rot.z as f64 + pivot[2],
    ]
}

fn align_frame_transform(
    frame: &RawPerfFrame,
    src_origin: [f64; 3],
    target_xz: [f64; 2],
    delta_yaw_q: Quat,
) -> RawPerfFrame {
    let mut out = frame.clone();
    let dx = target_xz[0] - src_origin[0];
    let dz = target_xz[1] - src_origin[2];

    let rot_root = rotate_point_around_pivot_y(frame.root_pos, src_origin, delta_yaw_q);
    out.root_pos = [rot_root[0] + dx, rot_root[1], rot_root[2] + dz];

    if let Some(ref mut joints) = out.body_joints {
        for j in joints.iter_mut() {
            let p = rotate_point_around_pivot_y([j[0], j[1], j[2]], src_origin, delta_yaw_q);
            j[0] = p[0] + dx;
            j[1] = p[1];
            j[2] = p[2] + dz;
        }
    }
    if let Some(ref mut lh) = out.left_hand {
        for j in lh.iter_mut() {
            let p = rotate_point_around_pivot_y([j[0], j[1], j[2]], src_origin, delta_yaw_q);
            j[0] = p[0] + dx;
            j[1] = p[1];
            j[2] = p[2] + dz;
        }
    }
    if let Some(ref mut rh) = out.right_hand {
        for j in rh.iter_mut() {
            let p = rotate_point_around_pivot_y([j[0], j[1], j[2]], src_origin, delta_yaw_q);
            j[0] = p[0] + dx;
            j[1] = p[1];
            j[2] = p[2] + dz;
        }
    }
    out
}

/// Blends two pose frames using Quaternion SLERP on limb bone vectors (preserving bone lengths
/// without collapse) and smoothstep crossfade across root position, hands, and 52 face blendshapes.
fn blend_two_frames(a: &RawPerfFrame, b: &RawPerfFrame, alpha_linear: f64) -> RawPerfFrame {
    let w = smoothstep(alpha_linear);
    let wf = w as f32;

    let root_pos = [
        a.root_pos[0] * (1.0 - w) + b.root_pos[0] * w,
        a.root_pos[1] * (1.0 - w) + b.root_pos[1] * w,
        a.root_pos[2] * (1.0 - w) + b.root_pos[2] * w,
    ];

    let body_joints = match (&a.body_joints, &b.body_joints) {
        (Some(ja), Some(jb)) if ja.len() == jb.len() && ja.len() > lm::RIGHT_FOOT_INDEX => {
            Some(slerp_body_joints(ja, jb, wf))
        }
        (Some(ja), Some(jb)) if ja.len() == jb.len() => Some(lerp_joint_list(ja, jb, w)),
        (Some(ja), None) => Some(ja.clone()),
        (None, Some(jb)) => Some(jb.clone()),
        _ => None,
    };

    let left_hand = match (&a.left_hand, &b.left_hand) {
        (Some(ha), Some(hb)) if ha.len() == hb.len() => Some(lerp_joint_list(ha, hb, w)),
        (Some(ha), None) => Some(ha.clone()),
        (None, Some(hb)) => Some(hb.clone()),
        _ => None,
    };

    let right_hand = match (&a.right_hand, &b.right_hand) {
        (Some(ha), Some(hb)) if ha.len() == hb.len() => Some(lerp_joint_list(ha, hb, w)),
        (Some(ha), None) => Some(ha.clone()),
        (None, Some(hb)) => Some(hb.clone()),
        _ => None,
    };

    let face_blendshapes = match (&a.face_blendshapes, &b.face_blendshapes) {
        (Some(fa), Some(fb)) => {
            let mut keys: BTreeSet<&String> = BTreeSet::new();
            keys.extend(fa.keys());
            keys.extend(fb.keys());
            let mut merged = BTreeMap::new();
            for k in keys {
                let va = *fa.get(k).unwrap_or(&0.0);
                let vb = *fb.get(k).unwrap_or(&0.0);
                merged.insert(k.clone(), (va * (1.0 - w) + vb * w).clamp(0.0, 1.0));
            }
            Some(merged)
        }
        (Some(fa), None) => Some(
            fa.iter()
                .map(|(k, v)| (k.clone(), v * (1.0 - w)))
                .collect(),
        ),
        (None, Some(fb)) => Some(fb.iter().map(|(k, v)| (k.clone(), v * w)).collect()),
        (None, None) => None,
    };

    RawPerfFrame {
        t: a.t,
        root_pos,
        body_joints,
        left_hand,
        right_hand,
        face_blendshapes,
    }
}

fn lerp_joint_list(a: &[[f64; 4]], b: &[[f64; 4]], w: f64) -> Vec<[f64; 4]> {
    a.iter()
        .zip(b.iter())
        .map(|(pa, pb)| {
            [
                pa[0] * (1.0 - w) + pb[0] * w,
                pa[1] * (1.0 - w) + pb[1] * w,
                pa[2] * (1.0 - w) + pb[2] * w,
                pa[3] * (1.0 - w) + pb[3] * w,
            ]
        })
        .collect()
}

/// Applies hierarchical Quaternion SLERP along major kinematic chains (hips -> shoulders -> arms,
/// hips -> knees -> ankles -> heels/toes) so blended poses rotate cleanly in arc space.
fn slerp_body_joints(ja: &[[f64; 4]], jb: &[[f64; 4]], w: f32) -> Vec<[f64; 4]> {
    let mut out = lerp_joint_list(ja, jb, w as f64);

    let to_v3 = |list: &[[f64; 4]], idx: usize| -> Vec3 {
        Vec3::new(list[idx][0] as f32, list[idx][1] as f32, list[idx][2] as f32)
    };

    // Parent -> child pairs in kinematic chain order
    let chains: [(usize, usize); 14] = [
        (lm::LEFT_SHOULDER, lm::LEFT_ELBOW),
        (lm::LEFT_ELBOW, lm::LEFT_WRIST),
        (lm::LEFT_WRIST, lm::LEFT_INDEX),
        (lm::LEFT_WRIST, lm::LEFT_PINKY),
        (lm::RIGHT_SHOULDER, lm::RIGHT_ELBOW),
        (lm::RIGHT_ELBOW, lm::RIGHT_WRIST),
        (lm::RIGHT_WRIST, lm::RIGHT_INDEX),
        (lm::RIGHT_WRIST, lm::RIGHT_PINKY),
        (lm::LEFT_HIP, lm::LEFT_KNEE),
        (lm::LEFT_KNEE, lm::LEFT_ANKLE),
        (lm::LEFT_ANKLE, lm::LEFT_HEEL),
        (lm::LEFT_ANKLE, lm::LEFT_FOOT_INDEX),
        (lm::RIGHT_HIP, lm::RIGHT_KNEE),
        (lm::RIGHT_KNEE, lm::RIGHT_ANKLE),
    ];

    for (parent, child) in chains {
        let va = to_v3(ja, child) - to_v3(ja, parent);
        let vb = to_v3(jb, child) - to_v3(jb, parent);
        let len_a = va.length();
        let len_b = vb.length();
        if len_a > 1e-4 && len_b > 1e-4 {
            let dir_a = va / len_a;
            let dir_b = vb / len_b;
            let q_full = safe_rotation_arc(dir_a, dir_b);
            let q_step = Quat::IDENTITY.slerp(q_full, w);
            let blended_dir = (q_step * dir_a).normalize_or_zero();
            let blended_len = len_a * (1.0 - w) + len_b * w;
            let p_out = to_v3(&out, parent);
            let c_pos = p_out + blended_dir * blended_len;
            if c_pos.is_finite() {
                out[child][0] = c_pos.x as f64;
                out[child][1] = c_pos.y as f64;
                out[child][2] = c_pos.z as f64;
            }
        }
    }

    // Also SLERP right foot heel & toe
    for (parent, child) in [
        (lm::RIGHT_ANKLE, lm::RIGHT_HEEL),
        (lm::RIGHT_ANKLE, lm::RIGHT_FOOT_INDEX),
    ] {
        let va = to_v3(ja, child) - to_v3(ja, parent);
        let vb = to_v3(jb, child) - to_v3(jb, parent);
        let len_a = va.length();
        let len_b = vb.length();
        if len_a > 1e-4 && len_b > 1e-4 {
            let dir_a = va / len_a;
            let dir_b = vb / len_b;
            let q_step = Quat::IDENTITY.slerp(safe_rotation_arc(dir_a, dir_b), w);
            let blended_dir = (q_step * dir_a).normalize_or_zero();
            let blended_len = len_a * (1.0 - w) + len_b * w;
            let p_out = to_v3(&out, parent);
            let c_pos = p_out + blended_dir * blended_len;
            if c_pos.is_finite() {
                out[child][0] = c_pos.x as f64;
                out[child][1] = c_pos.y as f64;
                out[child][2] = c_pos.z as f64;
            }
        }
    }

    out
}

/// Concatenates 2 or more clips seamlessly:
/// 1. Aligns each subsequent clip's horizontal `(X, Z)` root position & hips yaw to the previous clip's end.
/// 2. Cross-blends over `transition_frames` using SLERP Quaternion + smoothstep facial crossfade.
pub fn blend_raw_clips(clips: &[RawPerfData], transition_frames: usize) -> Result<RawPerfData, String> {
    if clips.len() < 2 {
        return Err("Minimal 2 klip dibutuhkan untuk Motion Blending".to_string());
    }
    if clips.iter().any(|c| c.frames.is_empty()) {
        return Err("Salah satu klip tidak memiliki frame".to_string());
    }

    let fps = clips[0].fps.max(1.0);
    let mut combined: Vec<RawPerfFrame> = clips[0].frames.clone();

    for next_clip in clips.iter().skip(1) {
        let prev_len = combined.len();
        let next_len = next_clip.frames.len();
        let blend_len = transition_frames
            .min(prev_len / 2)
            .min(next_len / 2);

        let anchor_idx = prev_len.saturating_sub(blend_len.max(1));
        let anchor_frame = &combined[anchor_idx];
        let next_first = &next_clip.frames[0];

        let prev_yaw = extract_hips_yaw(anchor_frame);
        let next_yaw = extract_hips_yaw(next_first);
        let delta_yaw_q = Quat::from_rotation_y(prev_yaw - next_yaw);

        let aligned_next: Vec<RawPerfFrame> = next_clip
            .frames
            .iter()
            .map(|f| {
                align_frame_transform(
                    f,
                    next_first.root_pos,
                    [anchor_frame.root_pos[0], anchor_frame.root_pos[2]],
                    delta_yaw_q,
                )
            })
            .collect();

        if blend_len == 0 {
            combined.extend(aligned_next);
        } else {
            let tail_start = prev_len - blend_len;
            for k in 0..blend_len {
                let alpha = (k + 1) as f64 / (blend_len + 1) as f64;
                let blended = blend_two_frames(&combined[tail_start + k], &aligned_next[k], alpha);
                combined[tail_start + k] = blended;
            }
            combined.extend(aligned_next.into_iter().skip(blend_len));
        }
    }

    // Re-stamp timestamps uniformly at base fps
    for (i, f) in combined.iter_mut().enumerate() {
        f.t = i as f64 / fps;
    }

    Ok(RawPerfData {
        fps,
        clip_type: "blended".to_string(),
        source_mode: "blended".to_string(),
        person_track_id: 0,
        anime_hold_indices: Vec::new(),
        frames: combined,
    })
}
