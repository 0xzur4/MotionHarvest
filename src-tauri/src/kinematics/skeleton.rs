use glam::Vec3;

/// MediaPipe / WholeBody 33-joint core body landmark indices.
pub mod lm {
    pub const NOSE: usize = 0;
    pub const LEFT_EYE: usize = 2;
    pub const RIGHT_EYE: usize = 5;
    pub const LEFT_EAR: usize = 7;
    pub const RIGHT_EAR: usize = 8;
    pub const LEFT_SHOULDER: usize = 11;
    pub const RIGHT_SHOULDER: usize = 12;
    pub const LEFT_ELBOW: usize = 13;
    pub const RIGHT_ELBOW: usize = 14;
    pub const LEFT_WRIST: usize = 15;
    pub const RIGHT_WRIST: usize = 16;
    pub const LEFT_PINKY: usize = 17;
    pub const RIGHT_PINKY: usize = 18;
    pub const LEFT_INDEX: usize = 19;
    pub const RIGHT_INDEX: usize = 20;
    pub const LEFT_HIP: usize = 23;
    pub const RIGHT_HIP: usize = 24;
    pub const LEFT_KNEE: usize = 25;
    pub const RIGHT_KNEE: usize = 26;
    pub const LEFT_ANKLE: usize = 27;
    pub const RIGHT_ANKLE: usize = 28;
    pub const LEFT_HEEL: usize = 29;
    pub const RIGHT_HEEL: usize = 30;
    pub const LEFT_FOOT_INDEX: usize = 31;
    pub const RIGHT_FOOT_INDEX: usize = 32;
}

/// MediaPipe 21-joint hand landmark indices (per hand).
pub mod hand_lm {
    pub const WRIST: usize = 0;
    pub const THUMB_CMC: usize = 1;
    pub const THUMB_MCP: usize = 2;
    pub const THUMB_IP: usize = 3;
    pub const THUMB_TIP: usize = 4;
    pub const INDEX_MCP: usize = 5;
    pub const INDEX_PIP: usize = 6;
    pub const INDEX_DIP: usize = 7;
    pub const INDEX_TIP: usize = 8;
    pub const MIDDLE_MCP: usize = 9;
    pub const MIDDLE_PIP: usize = 10;
    pub const MIDDLE_DIP: usize = 11;
    pub const MIDDLE_TIP: usize = 12;
    pub const RING_MCP: usize = 13;
    pub const RING_PIP: usize = 14;
    pub const RING_DIP: usize = 15;
    pub const RING_TIP: usize = 16;
    pub const PINKY_MCP: usize = 17;
    pub const PINKY_PIP: usize = 18;
    pub const PINKY_DIP: usize = 19;
    pub const PINKY_TIP: usize = 20;
}

#[derive(Debug, Clone)]
pub struct BoneDef {
    pub id: usize,
    pub name: &'static str,
    pub parent: Option<usize>,
    /// Offset in centimeters (standard BVH scale: ~170 cm human height).
    pub offset: Vec3,
    /// Rest direction vector (unit) pointing toward child or end site.
    pub rest_dir: Vec3,
    /// Optional end-site offset (leaf bones).
    pub end_site: Option<Vec3>,
    /// True if this bone belongs to the 30-bone hand/finger extension.
    pub is_finger: bool,
}

pub const BONE_HIPS: usize = 0;
pub const BONE_SPINE: usize = 1;
pub const BONE_CHEST: usize = 2;
pub const BONE_NECK: usize = 3;
pub const BONE_HEAD: usize = 4;
pub const BONE_L_SHOULDER: usize = 5;
pub const BONE_L_UPPER_ARM: usize = 6;
pub const BONE_L_LOWER_ARM: usize = 7;
pub const BONE_L_HAND: usize = 8;
pub const BONE_R_SHOULDER: usize = 9;
pub const BONE_R_UPPER_ARM: usize = 10;
pub const BONE_R_LOWER_ARM: usize = 11;
pub const BONE_R_HAND: usize = 12;
pub const BONE_L_UPPER_LEG: usize = 13;
pub const BONE_L_LOWER_LEG: usize = 14;
pub const BONE_L_FOOT: usize = 15;
pub const BONE_L_TOE: usize = 16;
pub const BONE_R_UPPER_LEG: usize = 17;
pub const BONE_R_LOWER_LEG: usize = 18;
pub const BONE_R_FOOT: usize = 19;
pub const BONE_R_TOE: usize = 20;

/// Builds the 21-bone core hierarchy (Body + Heel/Toe) or the full 51-bone hierarchy (+30 Fingers).
pub fn build_skeleton(include_hands: bool) -> Vec<BoneDef> {
    let mut bones = vec![
        BoneDef {
            id: BONE_HIPS,
            name: "Hips",
            parent: None,
            offset: Vec3::ZERO,
            rest_dir: Vec3::Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_SPINE,
            name: "Spine",
            parent: Some(BONE_HIPS),
            offset: Vec3::new(0.0, 11.0, 0.0),
            rest_dir: Vec3::Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_CHEST,
            name: "Chest",
            parent: Some(BONE_SPINE),
            offset: Vec3::new(0.0, 17.0, 0.0),
            rest_dir: Vec3::Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_NECK,
            name: "Neck",
            parent: Some(BONE_CHEST),
            offset: Vec3::new(0.0, 19.0, 0.0),
            rest_dir: Vec3::Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_HEAD,
            name: "Head",
            parent: Some(BONE_NECK),
            offset: Vec3::new(0.0, 9.5, 0.0),
            rest_dir: Vec3::Y,
            end_site: Some(Vec3::new(0.0, 15.0, 0.0)),
            is_finger: false,
        },
        // Left Arm (+X in T-pose)
        BoneDef {
            id: BONE_L_SHOULDER,
            name: "LeftShoulder",
            parent: Some(BONE_CHEST),
            offset: Vec3::new(5.5, 14.5, 0.0),
            rest_dir: Vec3::X,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_L_UPPER_ARM,
            name: "LeftUpperArm",
            parent: Some(BONE_L_SHOULDER),
            offset: Vec3::new(11.5, 0.0, 0.0),
            rest_dir: Vec3::X,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_L_LOWER_ARM,
            name: "LeftLowerArm",
            parent: Some(BONE_L_UPPER_ARM),
            offset: Vec3::new(27.0, 0.0, 0.0),
            rest_dir: Vec3::X,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_L_HAND,
            name: "LeftHand",
            parent: Some(BONE_L_LOWER_ARM),
            offset: Vec3::new(24.5, 0.0, 0.0),
            rest_dir: Vec3::X,
            end_site: if include_hands {
                None
            } else {
                Some(Vec3::new(9.5, 0.0, 0.0))
            },
            is_finger: false,
        },
        // Right Arm (-X in T-pose)
        BoneDef {
            id: BONE_R_SHOULDER,
            name: "RightShoulder",
            parent: Some(BONE_CHEST),
            offset: Vec3::new(-5.5, 14.5, 0.0),
            rest_dir: Vec3::NEG_X,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_R_UPPER_ARM,
            name: "RightUpperArm",
            parent: Some(BONE_R_SHOULDER),
            offset: Vec3::new(-11.5, 0.0, 0.0),
            rest_dir: Vec3::NEG_X,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_R_LOWER_ARM,
            name: "RightLowerArm",
            parent: Some(BONE_R_UPPER_ARM),
            offset: Vec3::new(-27.0, 0.0, 0.0),
            rest_dir: Vec3::NEG_X,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_R_HAND,
            name: "RightHand",
            parent: Some(BONE_R_LOWER_ARM),
            offset: Vec3::new(-24.5, 0.0, 0.0),
            rest_dir: Vec3::NEG_X,
            end_site: if include_hands {
                None
            } else {
                Some(Vec3::new(-9.5, 0.0, 0.0))
            },
            is_finger: false,
        },
        // Left Leg (with Ankle/Foot -> Toe)
        BoneDef {
            id: BONE_L_UPPER_LEG,
            name: "LeftUpperLeg",
            parent: Some(BONE_HIPS),
            offset: Vec3::new(9.5, -4.0, 0.0),
            rest_dir: Vec3::NEG_Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_L_LOWER_LEG,
            name: "LeftLowerLeg",
            parent: Some(BONE_L_UPPER_LEG),
            offset: Vec3::new(0.0, -42.0, 0.0),
            rest_dir: Vec3::NEG_Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_L_FOOT,
            name: "LeftFoot",
            parent: Some(BONE_L_LOWER_LEG),
            offset: Vec3::new(0.0, -40.0, 0.0),
            rest_dir: Vec3::new(0.0, -0.35, 0.9367).normalize(),
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_L_TOE,
            name: "LeftToe",
            parent: Some(BONE_L_FOOT),
            offset: Vec3::new(0.0, -5.5, 13.0),
            rest_dir: Vec3::Z,
            end_site: Some(Vec3::new(0.0, 0.0, 6.5)),
            is_finger: false,
        },
        // Right Leg (with Ankle/Foot -> Toe)
        BoneDef {
            id: BONE_R_UPPER_LEG,
            name: "RightUpperLeg",
            parent: Some(BONE_HIPS),
            offset: Vec3::new(-9.5, -4.0, 0.0),
            rest_dir: Vec3::NEG_Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_R_LOWER_LEG,
            name: "RightLowerLeg",
            parent: Some(BONE_R_UPPER_LEG),
            offset: Vec3::new(0.0, -42.0, 0.0),
            rest_dir: Vec3::NEG_Y,
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_R_FOOT,
            name: "RightFoot",
            parent: Some(BONE_R_LOWER_LEG),
            offset: Vec3::new(0.0, -40.0, 0.0),
            rest_dir: Vec3::new(0.0, -0.35, 0.9367).normalize(),
            end_site: None,
            is_finger: false,
        },
        BoneDef {
            id: BONE_R_TOE,
            name: "RightToe",
            parent: Some(BONE_R_FOOT),
            offset: Vec3::new(0.0, -5.5, 13.0),
            rest_dir: Vec3::Z,
            end_site: Some(Vec3::new(0.0, 0.0, 6.5)),
            is_finger: false,
        },
    ];

    if include_hands {
        add_hand_fingers(&mut bones, BONE_L_HAND, 1.0, true);
        add_hand_fingers(&mut bones, BONE_R_HAND, -1.0, false);
    }

    bones
}

fn add_hand_fingers(bones: &mut Vec<BoneDef>, hand_bone: usize, sign_x: f32, is_left: bool) {
    let dir = Vec3::new(sign_x, 0.0, 0.0);
    let specs: [(&'static str, &'static str, &'static str, Vec3, f32, f32, f32); 5] = if is_left {
        [
            ("LeftThumb1", "LeftThumb2", "LeftThumb3", Vec3::new(3.0, -1.0, 2.8), 3.2, 2.6, 2.1),
            ("LeftIndex1", "LeftIndex2", "LeftIndex3", Vec3::new(8.0, 0.2, 2.4), 3.6, 2.4, 1.8),
            ("LeftMiddle1", "LeftMiddle2", "LeftMiddle3", Vec3::new(8.4, 0.3, 0.6), 4.0, 2.6, 2.0),
            ("LeftRing1", "LeftRing2", "LeftRing3", Vec3::new(8.0, 0.2, -1.2), 3.7, 2.4, 1.8),
            ("LeftPinky1", "LeftPinky2", "LeftPinky3", Vec3::new(7.4, 0.0, -2.8), 3.0, 2.0, 1.6),
        ]
    } else {
        [
            ("RightThumb1", "RightThumb2", "RightThumb3", Vec3::new(-3.0, -1.0, 2.8), 3.2, 2.6, 2.1),
            ("RightIndex1", "RightIndex2", "RightIndex3", Vec3::new(-8.0, 0.2, 2.4), 3.6, 2.4, 1.8),
            ("RightMiddle1", "RightMiddle2", "RightMiddle3", Vec3::new(-8.4, 0.3, 0.6), 4.0, 2.6, 2.0),
            ("RightRing1", "RightRing2", "RightRing3", Vec3::new(-8.0, 0.2, -1.2), 3.7, 2.4, 1.8),
            ("RightPinky1", "RightPinky2", "RightPinky3", Vec3::new(-7.4, 0.0, -2.8), 3.0, 2.0, 1.6),
        ]
    };

    for (n1, n2, n3, base_offset, l1, l2, l3) in specs {
        let id1 = bones.len();
        bones.push(BoneDef {
            id: id1,
            name: n1,
            parent: Some(hand_bone),
            offset: base_offset,
            rest_dir: dir,
            end_site: None,
            is_finger: true,
        });
        let id2 = bones.len();
        bones.push(BoneDef {
            id: id2,
            name: n2,
            parent: Some(id1),
            offset: dir * l1,
            rest_dir: dir,
            end_site: None,
            is_finger: true,
        });
        let id3 = bones.len();
        bones.push(BoneDef {
            id: id3,
            name: n3,
            parent: Some(id2),
            offset: dir * l2,
            rest_dir: dir,
            end_site: Some(dir * l3),
            is_finger: true,
        });
    }
}

/// Returns the child indices for a given bone id.
pub fn children_of(bones: &[BoneDef], parent_id: usize) -> Vec<usize> {
    bones
        .iter()
        .filter(|b| b.parent == Some(parent_id))
        .map(|b| b.id)
        .collect()
}
