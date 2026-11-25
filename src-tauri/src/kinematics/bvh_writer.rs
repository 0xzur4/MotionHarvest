use crate::kinematics::skeleton::{
    build_skeleton, children_of, hand_lm, lm, BoneDef, BONE_CHEST, BONE_HEAD, BONE_HIPS,
    BONE_L_FOOT, BONE_L_HAND, BONE_L_LOWER_ARM, BONE_L_LOWER_LEG, BONE_L_SHOULDER, BONE_L_TOE,
    BONE_L_UPPER_ARM, BONE_L_UPPER_LEG, BONE_NECK, BONE_R_FOOT, BONE_R_HAND, BONE_R_LOWER_ARM,
    BONE_R_LOWER_LEG, BONE_R_SHOULDER, BONE_R_TOE, BONE_R_UPPER_ARM, BONE_R_UPPER_LEG, BONE_SPINE,
};
use crate::models::RawPerfFrame;
use glam::{EulerRot, Mat3, Quat, Vec3};
use std::fmt::Write as FmtWrite;

fn safe_f32(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn joint_vec(joints: &[[f64; 4]], idx: usize) -> Option<Vec3> {
    joints.get(idx).and_then(|j| {
        let v = Vec3::new(j[0] as f32, j[1] as f32, j[2] as f32);
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    })
}

fn safe_dir(from: Vec3, to: Vec3, fallback: Vec3) -> Vec3 {
    let d = to - from;
    let len = d.length();
    if len > 1e-4 && d.is_finite() {
        d / len
    } else {
        fallback.normalize_or_zero()
    }
}

/// Safe shortest-arc quaternion between two unit vectors without NaN on anti-parallel inputs.
pub fn safe_rotation_arc(from: Vec3, to: Vec3) -> Quat {
    let a = from.normalize_or_zero();
    let b = to.normalize_or_zero();
    if a.length_squared() < 1e-6 || b.length_squared() < 1e-6 {
        return Quat::IDENTITY;
    }
    let dot = a.dot(b).clamp(-1.0, 1.0);
    if dot > 0.9999 {
        return Quat::IDENTITY;
    }
    if dot < -0.9999 {
        let mut ortho = Vec3::X.cross(a);
        if ortho.length_squared() < 1e-4 {
            ortho = Vec3::Z.cross(a);
        }
        return Quat::from_axis_angle(ortho.normalize(), std::f32::consts::PI);
    }
    Quat::from_rotation_arc(a, b).normalize()
}

/// Solves hips root world orientation from hip axis (+X = left) and spine axis (+Y = up).
pub fn solve_hips_quat(joints: &[[f64; 4]]) -> Quat {
    let Some(l_hip) = joint_vec(joints, lm::LEFT_HIP) else {
        return Quat::IDENTITY;
    };
    let Some(r_hip) = joint_vec(joints, lm::RIGHT_HIP) else {
        return Quat::IDENTITY;
    };
    let Some(l_sh) = joint_vec(joints, lm::LEFT_SHOULDER) else {
        return Quat::IDENTITY;
    };
    let Some(r_sh) = joint_vec(joints, lm::RIGHT_SHOULDER) else {
        return Quat::IDENTITY;
    };

    let hip_mid = 0.5 * (l_hip + r_hip);
    let sh_mid = 0.5 * (l_sh + r_sh);

    let y_axis = safe_dir(hip_mid, sh_mid, Vec3::Y);
    let raw_x = safe_dir(r_hip, l_hip, Vec3::X);
    let mut z_axis = raw_x.cross(y_axis);
    if z_axis.length_squared() < 1e-5 {
        z_axis = Vec3::Z;
    } else {
        z_axis = z_axis.normalize();
    }
    let x_axis = y_axis.cross(z_axis).normalize_or_zero();
    if x_axis.length_squared() < 1e-5 {
        return Quat::IDENTITY;
    }
    let mat = Mat3::from_cols(x_axis, y_axis, z_axis);
    let q = Quat::from_mat3(&mat).normalize();
    if q.is_finite() {
        q
    } else {
        Quat::IDENTITY
    }
}

/// Extracts hips horizontal facing yaw angle in radians (used by Motion Blender alignment).
pub fn extract_hips_yaw(frame: &RawPerfFrame) -> f32 {
    let Some(ref joints) = frame.body_joints else {
        return 0.0;
    };
    let Some(l_hip) = joint_vec(joints, lm::LEFT_HIP) else {
        return 0.0;
    };
    let Some(r_hip) = joint_vec(joints, lm::RIGHT_HIP) else {
        return 0.0;
    };
    let left = l_hip - r_hip;
    // In rest T-pose, left = (+X, 0, 0). Forward = left x up = (left.z, 0, -left.x) or (-left.z, 0, left.x):
    // (+1, 0, 0) x (0, 1, 0) = (0, 0, +1) -> fwd_x = -left.z, fwd_z = left.x
    let fwd_x = -left.z;
    let fwd_z = left.x;
    if fwd_x.hypot(fwd_z) < 1e-4 {
        0.0
    } else {
        fwd_x.atan2(fwd_z)
    }
}

/// Computes local bone quaternions for all bones in the skeleton for a single frame.
pub fn solve_frame_quaternions(
    frame: &RawPerfFrame,
    bones: &[BoneDef],
    include_hands: bool,
) -> Vec<Quat> {
    let n = bones.len();
    let mut local_quats = vec![Quat::IDENTITY; n];
    let mut global_quats = vec![Quat::IDENTITY; n];

    let Some(ref joints) = frame.body_joints else {
        return local_quats;
    };
    if joints.len() <= lm::RIGHT_FOOT_INDEX {
        return local_quats;
    }

    let j = |idx: usize| joint_vec(joints, idx).unwrap_or(Vec3::ZERO);

    let l_hip = j(lm::LEFT_HIP);
    let r_hip = j(lm::RIGHT_HIP);
    let hip_mid = 0.5 * (l_hip + r_hip);
    let l_sh = j(lm::LEFT_SHOULDER);
    let r_sh = j(lm::RIGHT_SHOULDER);
    let sh_mid = 0.5 * (l_sh + r_sh);
    let spine_mid = hip_mid.lerp(sh_mid, 0.45);
    let nose = j(lm::NOSE);
    let ear_mid = 0.5 * (j(lm::LEFT_EAR) + j(lm::RIGHT_EAR));
    let head_top = ear_mid + Vec3::new(0.0, 0.08, 0.0);

    // Target world direction per core bone (indices 0..21)
    let mut target_dirs: Vec<Option<Vec3>> = vec![None; n];
    target_dirs[BONE_SPINE] = Some(safe_dir(hip_mid, spine_mid, Vec3::Y));
    target_dirs[BONE_CHEST] = Some(safe_dir(spine_mid, sh_mid, Vec3::Y));
    target_dirs[BONE_NECK] = Some(safe_dir(sh_mid, ear_mid, Vec3::Y));
    target_dirs[BONE_HEAD] = Some(safe_dir(ear_mid, 0.5 * (nose + head_top), Vec3::Y));

    // Left Arm
    let l_elbow = j(lm::LEFT_ELBOW);
    let l_wrist = j(lm::LEFT_WRIST);
    let l_hand_tip = 0.5 * (j(lm::LEFT_INDEX) + j(lm::LEFT_PINKY));
    target_dirs[BONE_L_SHOULDER] = Some(safe_dir(sh_mid, l_sh, Vec3::X));
    target_dirs[BONE_L_UPPER_ARM] = Some(safe_dir(l_sh, l_elbow, Vec3::X));
    target_dirs[BONE_L_LOWER_ARM] = Some(safe_dir(l_elbow, l_wrist, Vec3::X));
    target_dirs[BONE_L_HAND] = Some(safe_dir(l_wrist, l_hand_tip, Vec3::X));

    // Right Arm
    let r_elbow = j(lm::RIGHT_ELBOW);
    let r_wrist = j(lm::RIGHT_WRIST);
    let r_hand_tip = 0.5 * (j(lm::RIGHT_INDEX) + j(lm::RIGHT_PINKY));
    target_dirs[BONE_R_SHOULDER] = Some(safe_dir(sh_mid, r_sh, Vec3::NEG_X));
    target_dirs[BONE_R_UPPER_ARM] = Some(safe_dir(r_sh, r_elbow, Vec3::NEG_X));
    target_dirs[BONE_R_LOWER_ARM] = Some(safe_dir(r_elbow, r_wrist, Vec3::NEG_X));
    target_dirs[BONE_R_HAND] = Some(safe_dir(r_wrist, r_hand_tip, Vec3::NEG_X));

    // Left Leg + Heel/Toe
    let l_knee = j(lm::LEFT_KNEE);
    let l_ankle = j(lm::LEFT_ANKLE);
    let l_heel = j(lm::LEFT_HEEL);
    let l_toe = j(lm::LEFT_FOOT_INDEX);
    target_dirs[BONE_L_UPPER_LEG] = Some(safe_dir(l_hip, l_knee, Vec3::NEG_Y));
    target_dirs[BONE_L_LOWER_LEG] = Some(safe_dir(l_knee, l_ankle, Vec3::NEG_Y));
    target_dirs[BONE_L_FOOT] = Some(safe_dir(l_ankle, l_toe, Vec3::new(0.0, -0.35, 0.9367)));
    target_dirs[BONE_L_TOE] = Some(safe_dir(l_heel, l_toe, Vec3::Z));

    // Right Leg + Heel/Toe
    let r_knee = j(lm::RIGHT_KNEE);
    let r_ankle = j(lm::RIGHT_ANKLE);
    let r_heel = j(lm::RIGHT_HEEL);
    let r_toe = j(lm::RIGHT_FOOT_INDEX);
    target_dirs[BONE_R_UPPER_LEG] = Some(safe_dir(r_hip, r_knee, Vec3::NEG_Y));
    target_dirs[BONE_R_LOWER_LEG] = Some(safe_dir(r_knee, r_ankle, Vec3::NEG_Y));
    target_dirs[BONE_R_FOOT] = Some(safe_dir(r_ankle, r_toe, Vec3::new(0.0, -0.35, 0.9367)));
    target_dirs[BONE_R_TOE] = Some(safe_dir(r_heel, r_toe, Vec3::Z));

    // Optional 30 Finger bones (indices 21..51)
    if include_hands && n >= 51 {
        let finger_chains: [(usize, usize, usize, usize); 5] = [
            (hand_lm::THUMB_CMC, hand_lm::THUMB_MCP, hand_lm::THUMB_IP, hand_lm::THUMB_TIP),
            (hand_lm::INDEX_MCP, hand_lm::INDEX_PIP, hand_lm::INDEX_DIP, hand_lm::INDEX_TIP),
            (hand_lm::MIDDLE_MCP, hand_lm::MIDDLE_PIP, hand_lm::MIDDLE_DIP, hand_lm::MIDDLE_TIP),
            (hand_lm::RING_MCP, hand_lm::RING_PIP, hand_lm::RING_DIP, hand_lm::RING_TIP),
            (hand_lm::PINKY_MCP, hand_lm::PINKY_PIP, hand_lm::PINKY_DIP, hand_lm::PINKY_TIP),
        ];

        if let Some(ref lh) = frame.left_hand {
            if lh.len() >= 21 {
                for (f_idx, (a, b, c, d)) in finger_chains.iter().enumerate() {
                    let base_bone = 21 + f_idx * 3;
                    if let (Some(pa), Some(pb), Some(pc), Some(pd)) = (
                        joint_vec(lh, *a),
                        joint_vec(lh, *b),
                        joint_vec(lh, *c),
                        joint_vec(lh, *d),
                    ) {
                        target_dirs[base_bone] = Some(safe_dir(pa, pb, Vec3::X));
                        target_dirs[base_bone + 1] = Some(safe_dir(pb, pc, Vec3::X));
                        target_dirs[base_bone + 2] = Some(safe_dir(pc, pd, Vec3::X));
                    }
                }
            }
        }

        if let Some(ref rh) = frame.right_hand {
            if rh.len() >= 21 {
                for (f_idx, (a, b, c, d)) in finger_chains.iter().enumerate() {
                    let base_bone = 36 + f_idx * 3;
                    if let (Some(pa), Some(pb), Some(pc), Some(pd)) = (
                        joint_vec(rh, *a),
                        joint_vec(rh, *b),
                        joint_vec(rh, *c),
                        joint_vec(rh, *d),
                    ) {
                        target_dirs[base_bone] = Some(safe_dir(pa, pb, Vec3::NEG_X));
                        target_dirs[base_bone + 1] = Some(safe_dir(pb, pc, Vec3::NEG_X));
                        target_dirs[base_bone + 2] = Some(safe_dir(pc, pd, Vec3::NEG_X));
                    }
                }
            }
        }
    }

    // Solve Hips root first
    let q_hips = solve_hips_quat(joints);
    local_quats[BONE_HIPS] = q_hips;
    global_quats[BONE_HIPS] = q_hips;

    // Solve children in topological order (our bone ids are strictly ordered parent < child)
    for b in bones.iter().skip(1) {
        let parent_id = b.parent.unwrap_or(BONE_HIPS);
        let parent_q = global_quats[parent_id];

        let q_local = if let Some(target_world) = target_dirs[b.id] {
            let target_local = parent_q.inverse() * target_world;
            safe_rotation_arc(b.rest_dir, target_local)
        } else {
            Quat::IDENTITY
        };

        let q_local_clean = if q_local.is_finite() {
            q_local.normalize()
        } else {
            Quat::IDENTITY
        };
        local_quats[b.id] = q_local_clean;
        global_quats[b.id] = (parent_q * q_local_clean).normalize();
    }

    local_quats
}

/// Serializes polished frames into standard BVH text (21 or 51 bones) with ZXY Euler order.
pub fn write_bvh_string(frames: &[RawPerfFrame], fps: f64, include_hands: bool) -> String {
    let bones = build_skeleton(include_hands);
    let mut out = String::with_capacity(frames.len() * bones.len() * 24 + 4096);

    out.push_str("HIERARCHY\n");
    let mut traversal_order = Vec::with_capacity(bones.len());
    write_bone_hierarchy(&bones, BONE_HIPS, 0, &mut out, &mut traversal_order);

    let frame_time = 1.0 / fps.max(1.0);
    let _ = writeln!(out, "MOTION");
    let _ = writeln!(out, "Frames: {}", frames.len());
    let _ = writeln!(out, "Frame Time: {:.6}", frame_time);

    for frame in frames {
        let quats = solve_frame_quaternions(frame, &bones, include_hands);
        // Convert meters -> centimeters for BVH root translation
        let rx = safe_f32((frame.root_pos[0] * 100.0) as f32);
        let ry = safe_f32((frame.root_pos[1] * 100.0) as f32);
        let rz = safe_f32((frame.root_pos[2] * 100.0) as f32);
        let _ = write!(out, "{:.4} {:.4} {:.4}", rx, ry, rz);

        for &bone_id in &traversal_order {
            let q = quats[bone_id];
            let (z_rad, x_rad, y_rad) = q.to_euler(EulerRot::ZXY);
            let z_deg = safe_f32(z_rad.to_degrees());
            let x_deg = safe_f32(x_rad.to_degrees());
            let y_deg = safe_f32(y_rad.to_degrees());
            let _ = write!(out, " {:.4} {:.4} {:.4}", z_deg, x_deg, y_deg);
        }
        out.push('\n');
    }

    out
}

fn write_bone_hierarchy(
    bones: &[BoneDef],
    bone_id: usize,
    depth: usize,
    out: &mut String,
    order: &mut Vec<usize>,
) {
    let bone = &bones[bone_id];
    order.push(bone_id);

    let indent = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);
    if depth == 0 {
        let _ = writeln!(out, "{}ROOT {}", indent, bone.name);
    } else {
        let _ = writeln!(out, "{}JOINT {}", indent, bone.name);
    }
    let _ = writeln!(out, "{}{{", indent);
    let _ = writeln!(
        out,
        "{}OFFSET {:.4} {:.4} {:.4}",
        inner,
        safe_f32(bone.offset.x),
        safe_f32(bone.offset.y),
        safe_f32(bone.offset.z)
    );
    if depth == 0 {
        let _ = writeln!(
            out,
            "{}CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation",
            inner
        );
    } else {
        let _ = writeln!(out, "{}CHANNELS 3 Zrotation Xrotation Yrotation", inner);
    }

    let kids = children_of(bones, bone_id);
    if kids.is_empty() {
        let tip = bone.end_site.unwrap_or(bone.rest_dir * 6.0);
        let _ = writeln!(out, "{}End Site", inner);
        let _ = writeln!(out, "{}{{", inner);
        let _ = writeln!(
            out,
            "{}  OFFSET {:.4} {:.4} {:.4}",
            inner,
            safe_f32(tip.x),
            safe_f32(tip.y),
            safe_f32(tip.z)
        );
        let _ = writeln!(out, "{}}}", inner);
    } else {
        for kid in kids {
            write_bone_hierarchy(bones, kid, depth + 1, out, order);
        }
    }

    let _ = writeln!(out, "{}}}", indent);
}
