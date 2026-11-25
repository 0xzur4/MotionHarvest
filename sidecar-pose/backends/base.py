"""Base Performance Backend Interface & ARKit 52 / Coordinate Utilities."""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Callable, Dict, List, Optional, Tuple
import numpy as np

ARKIT_52_BLENDSHAPES: List[str] = [
    "browDownLeft", "browDownRight", "browInnerUp", "browOuterUpLeft", "browOuterUpRight",
    "cheekPuff", "cheekSquintLeft", "cheekSquintRight",
    "eyeBlinkLeft", "eyeBlinkRight", "eyeLookDownLeft", "eyeLookDownRight",
    "eyeLookInLeft", "eyeLookInRight", "eyeLookOutLeft", "eyeLookOutRight",
    "eyeLookUpLeft", "eyeLookUpRight", "eyeSquintLeft", "eyeSquintRight",
    "eyeWideLeft", "eyeWideRight",
    "jawForward", "jawLeft", "jawOpen", "jawRight",
    "mouthClose", "mouthDimpleLeft", "mouthDimpleRight", "mouthFrownLeft", "mouthFrownRight",
    "mouthFunnel", "mouthLeft", "mouthLowerDownLeft", "mouthLowerDownRight",
    "mouthPressLeft", "mouthPressRight", "mouthPucker", "mouthRight",
    "mouthRollLower", "mouthRollUpper", "mouthShrugLower", "mouthShrugUpper",
    "mouthSmileLeft", "mouthSmileRight", "mouthStretchLeft", "mouthStretchRight",
    "mouthUpperUpLeft", "mouthUpperUpRight",
    "noseSneerLeft", "noseSneerRight", "tongueOut",
]


def empty_arkit_dict() -> Dict[str, float]:
    return {k: 0.0 for k in ARKIT_52_BLENDSHAPES}


def mediapipe_world_to_mh_yup(
    world_lms: List[Tuple[float, float, float, float]],
    image_hips_xy: Optional[Tuple[float, float]] = None,
) -> Tuple[List[float], List[List[float]]]:
    """Converts MediaPipe 33 world landmarks (+Y down, hip-centered meters)
    into MotionHarvest standardized right-handed Y-Up metric space:
      +Y = up (feet near 0.0, hips ~0.94m)
      +Z = forward
      +X = character's left
    """
    pts: List[List[float]] = []
    for (x, y, z, vis) in world_lms:
        # MediaPipe world coords: +X right (subject's left when facing cam), +Y down, +Z away from cam
        pts.append([float(x), float(-y), float(-z), float(vis)])

    if len(pts) >= 33:
        # Place lowest foot landmark near y = 0.0
        foot_ys = [pts[29][1], pts[30][1], pts[31][1], pts[32][1]]
        min_foot_y = min(foot_ys)
        y_shift = -min_foot_y

        # Estimate horizontal root offset from image-space hip position if available
        rx = (image_hips_xy[0] - 0.5) * 2.2 if image_hips_xy else 0.0
        rz = 0.0

        for p in pts:
            p[0] += rx
            p[1] += y_shift
            p[2] += rz

        hip_x = 0.5 * (pts[23][0] + pts[24][0])
        hip_y = 0.5 * (pts[23][1] + pts[24][1])
        hip_z = 0.5 * (pts[23][2] + pts[24][2])
        root_pos = [round(hip_x, 5), round(hip_y, 5), round(hip_z, 5)]
    else:
        root_pos = [0.0, 0.94, 0.0]

    rounded_pts = [[round(v, 5) for v in p] for p in pts]
    return root_pos, rounded_pts


def map_68_face_to_arkit52(pts68: np.ndarray) -> Dict[str, float]:
    """Geometric Anime/2D Face Mapper (Step 5 Stage C):
    Converts 68 facial landmarks (COCO-WholeBody / iBUG 68 layout)
    into the 52 ARKit Blendshapes dictionary.
    """
    out = empty_arkit_dict()
    if pts68 is None or len(pts68) < 68:
        return out

    pts = np.asarray(pts68, dtype=np.float32)[:, :2]
    # Inter-ocular distance as scale-invariant face normalizer
    l_eye_center = np.mean(pts[36:42], axis=0)
    r_eye_center = np.mean(pts[42:48], axis=0)
    iod = float(np.linalg.norm(r_eye_center - l_eye_center))
    if iod < 1e-3:
        return out

    def dist(a: int, b: int) -> float:
        return float(np.linalg.norm(pts[a] - pts[b]))

    # 1. Eye Aspect Ratio (EAR) -> eyeBlink & eyeWide
    ear_l = (dist(37, 41) + dist(38, 40)) / (2.0 * max(1e-4, dist(36, 39)))
    ear_r = (dist(43, 47) + dist(44, 46)) / (2.0 * max(1e-4, dist(42, 45)))
    out["eyeBlinkLeft"] = float(np.clip((0.26 - ear_l) / 0.16, 0.0, 1.0))
    out["eyeBlinkRight"] = float(np.clip((0.26 - ear_r) / 0.16, 0.0, 1.0))
    out["eyeWideLeft"] = float(np.clip((ear_l - 0.30) / 0.16, 0.0, 1.0))
    out["eyeWideRight"] = float(np.clip((ear_r - 0.30) / 0.16, 0.0, 1.0))

    # 2. Mouth opening -> jawOpen & mouthFunnel
    mouth_w = max(1e-4, dist(48, 54))
    inner_h = (dist(61, 67) + dist(62, 66) + dist(63, 65)) / 3.0
    mar = inner_h / mouth_w
    out["jawOpen"] = float(np.clip((mar - 0.06) / 0.55, 0.0, 1.0))
    out["mouthFunnel"] = float(np.clip((mar - 0.25) / 0.45, 0.0, 1.0))

    # 3. Lip corner curvature -> mouthSmile & mouthFrown
    # In image coordinates, +Y is DOWN, so raised lip corners have smaller Y than mouth center!
    mouth_center_y = 0.5 * (pts[62, 1] + pts[66, 1])
    left_lift = (mouth_center_y - pts[48, 1]) / iod
    right_lift = (mouth_center_y - pts[54, 1]) / iod
    mouth_width_ratio = mouth_w / iod

    smile_bonus = max(0.0, (mouth_width_ratio - 0.78) * 1.4)
    out["mouthSmileLeft"] = float(np.clip(left_lift * 4.5 + smile_bonus, 0.0, 1.0))
    out["mouthSmileRight"] = float(np.clip(right_lift * 4.5 + smile_bonus, 0.0, 1.0))
    out["mouthFrownLeft"] = float(np.clip(-left_lift * 4.5, 0.0, 1.0))
    out["mouthFrownRight"] = float(np.clip(-right_lift * 4.5, 0.0, 1.0))

    # 4. Eyebrows -> browDown (angry), browInnerUp (sad), browOuterUp (surprised)
    # Compare inner brow (21, 22) vs outer brow (17, 26) height above eyes
    l_brow_gap = (l_eye_center[1] - pts[19, 1]) / iod
    r_brow_gap = (r_eye_center[1] - pts[24, 1]) / iod
    l_inner_tilt = (pts[21, 1] - pts[17, 1]) / iod  # positive => inner brow pushed DOWN (angry!)
    r_inner_tilt = (pts[22, 1] - pts[26, 1]) / iod

    out["browDownLeft"] = float(
        np.clip(l_inner_tilt * 5.0 + max(0.0, 0.34 - l_brow_gap) * 3.0, 0.0, 1.0)
    )
    out["browDownRight"] = float(
        np.clip(r_inner_tilt * 5.0 + max(0.0, 0.34 - r_brow_gap) * 3.0, 0.0, 1.0)
    )
    out["browInnerUp"] = float(
        np.clip((-0.5 * (l_inner_tilt + r_inner_tilt)) * 4.5 + max(0.0, 0.5 * (l_brow_gap + r_brow_gap) - 0.42) * 3.0, 0.0, 1.0)
    )
    out["browOuterUpLeft"] = float(np.clip((l_brow_gap - 0.42) * 3.5, 0.0, 1.0))
    out["browOuterUpRight"] = float(np.clip((r_brow_gap - 0.42) * 3.5, 0.0, 1.0))

    return {k: round(v, 4) for k, v in out.items()}


class PerformanceBackend(ABC):
    """Abstract interface implemented by Fast (MediaPipe), Studio CUDA, and Anime 2D backends."""

    @abstractmethod
    def extract_track(
        self,
        frames_bgr: List[np.ndarray],
        fps: float,
        face_only: bool = False,
        person_track_id: int = 1,
        progress_cb: Optional[Callable[[int, int], None]] = None,
    ) -> Tuple[dict, float, bool, bool]:
        """Extracts a complete RawPerfData dict from BGR frames.

        Returns `(rawperf_dict, quality_score, has_hands, has_face)`.
        """
        raise NotImplementedError
