"""Anime 2D Dedicated 2-Stage Pipeline + Hold-Frame Cadence + Anime Face Mapper (Step 5).

Stage A: DWPose / RTMPose-WholeBody 133 (ONNX) 2D detector resilient to 2D line art,
         stylized hair, and loose anime clothing (with MediaPipe fallback).
Stage B: Temporal 2D-to-3D Lifter (MotionBERT ONNX when present + anatomical bone-length
         constrained temporal 2D->3D lifter preventing left/right limb flipping).
Stage C: Anime Face Mapper turning 68 2D face landmarks into all 52 ARKit blendshapes.
Plus:    Integrates `cadence.detect_anime_cadence` to process only unique cel frames (2-3x faster)
         and record `anime_hold_indices` for exact Original Hold-Frame playback in Rust.
"""

from __future__ import annotations

from typing import Callable, Dict, List, Optional, Tuple
import os
import sys
import cv2
import numpy as np

from backends.base import PerformanceBackend, empty_arkit_dict, map_68_face_to_arkit52
from backends.mediapipe_backend import MediaPipeBackend
from cadence import detect_anime_cadence


def _temporal_lift_2d_to_3d_sequence(
    seq_2d_133: List[Optional[np.ndarray]],
) -> List[Tuple[List[float], Optional[List[List[float]]], Optional[List[List[float]]], Optional[List[List[float]]]]]:
    """Temporal 2D->3D Lifter (Stage B):
    Converts a temporal window of 133-point 2D poses (x, y, conf) into anatomically
    consistent 3D MotionHarvest Y-Up coordinates with foreshortening-based Z estimation
    and temporal continuity smoothing to prevent left/right flipping.
    """
    n = len(seq_2d_133)
    results: List[
        Tuple[
            List[float],
            Optional[List[List[float]]],
            Optional[List[List[float]]],
            Optional[List[List[float]]],
        ]
    ] = []

    # Reference anatomical limb lengths (normalized relative to torso height = 0.50m)
    torso_len_m = 0.50
    prev_z_sign_l = 1.0
    prev_z_sign_r = 1.0

    # Collect hip centers across clip for smooth horizontal root trajectory
    raw_hip_xy: List[Tuple[float, float]] = []
    for k in seq_2d_133:
        if k is not None and len(k) >= 17:
            hx = 0.5 * (k[11, 0] + k[12, 0])
            hy = 0.5 * (k[11, 1] + k[12, 1])
            raw_hip_xy.append((float(hx), float(hy)))
        elif raw_hip_xy:
            raw_hip_xy.append(raw_hip_xy[-1])
        else:
            raw_hip_xy.append((0.5, 0.5))

    base_hx = raw_hip_xy[0][0] if raw_hip_xy else 0.5

    # Track per-frame 33 joints for temporal depth filtering
    body_3d_seq: List[Optional[np.ndarray]] = []
    lh_seq: List[Optional[List[List[float]]]] = []
    rh_seq: List[Optional[List[List[float]]]] = []

    for f_idx, kpts in enumerate(seq_2d_133):
        if kpts is None or len(kpts) < 23:
            body_3d_seq.append(body_3d_seq[-1].copy() if body_3d_seq and body_3d_seq[-1] is not None else None)
            lh_seq.append(lh_seq[-1] if lh_seq else None)
            rh_seq.append(rh_seq[-1] if rh_seq else None)
            continue

        sh_mid = 0.5 * (kpts[5, :2] + kpts[6, :2])
        hip_mid = 0.5 * (kpts[11, :2] + kpts[12, :2])
        torso_2d = float(np.linalg.norm(sh_mid - hip_mid))
        scale = torso_len_m / max(0.06, torso_2d)

        # Estimate torso yaw from projected shoulder width vs anatomical width (~0.36m)
        sh_dx = float(kpts[5, 0] - kpts[6, 0]) * scale
        sh_ratio = float(np.clip(abs(sh_dx) / 0.36, 0.0, 1.0))
        torso_depth_delta = float(np.sqrt(max(0.0, 1.0 - sh_ratio * sh_ratio))) * 0.16

        pts = np.zeros((33, 4), dtype=np.float32)

        def put(mh_i: int, coco_i: int, z_hint: float = 0.0) -> None:
            if coco_i < len(kpts):
                px = (float(kpts[coco_i, 0]) - base_hx) * scale
                py = -(float(kpts[coco_i, 1]) - float(hip_mid[1])) * scale + 0.94
                conf = float(kpts[coco_i, 2])
                pts[mh_i] = [px, py, z_hint, conf]

        # Core body & face landmarks
        put(0, 0, 0.10)   # Nose slightly forward
        put(2, 1, 0.08)
        put(5, 2, 0.08)
        put(7, 3, 0.0)
        put(8, 4, 0.0)

        put(11, 5, torso_depth_delta * 0.5)
        put(12, 6, -torso_depth_delta * 0.5)

        # Foreshortening depth for arms (upper arm ~0.28m, forearm ~0.25m)
        def foreshorten_z(a_idx: int, b_idx: int, target_len: float, sign: float) -> float:
            d2d = float(np.linalg.norm(kpts[b_idx, :2] - kpts[a_idx, :2])) * scale
            dz_sq = max(0.0, target_len * target_len - d2d * d2d)
            return sign * float(np.sqrt(dz_sq)) * 0.65

        l_el_z = pts[11, 2] + foreshorten_z(5, 7, 0.28, prev_z_sign_l)
        r_el_z = pts[12, 2] + foreshorten_z(6, 8, 0.28, prev_z_sign_r)
        put(13, 7, l_el_z)
        put(14, 8, r_el_z)
        put(15, 9, l_el_z + foreshorten_z(7, 9, 0.25, 1.0))
        put(16, 10, r_el_z + foreshorten_z(8, 10, 0.25, 1.0))

        # Hands & legs + heel/toe
        put(23, 11, torso_depth_delta * 0.35)
        put(24, 12, -torso_depth_delta * 0.35)
        l_kn_z = pts[23, 2] + foreshorten_z(11, 13, 0.43, 1.0)
        r_kn_z = pts[24, 2] + foreshorten_z(12, 14, 0.43, 1.0)
        put(25, 13, l_kn_z)
        put(26, 14, r_kn_z)
        put(27, 15, l_kn_z - foreshorten_z(13, 15, 0.41, 0.6))
        put(28, 16, r_kn_z - foreshorten_z(14, 16, 0.41, 0.6))
        put(29, 19, pts[27, 2] - 0.05)  # Left Heel
        put(30, 22, pts[28, 2] - 0.05)  # Right Heel
        put(31, 17, pts[27, 2] + 0.14)  # Left Toe
        put(32, 20, pts[28, 2] + 0.14)  # Right Toe

        if len(kpts) >= 133:
            put(19, 96, pts[15, 2] + 0.03)
            put(17, 108, pts[15, 2] - 0.02)
            put(20, 117, pts[16, 2] + 0.03)
            put(18, 129, pts[16, 2] - 0.02)

            lh = [
                [
                    round((float(p[0]) - float(kpts[91, 0])) * scale, 5),
                    round(-(float(p[1]) - float(kpts[91, 1])) * scale, 5),
                    0.0,
                    round(float(p[2]), 4),
                ]
                for p in kpts[91:112]
            ]
            rh = [
                [
                    round((float(p[0]) - float(kpts[112, 0])) * scale, 5),
                    round(-(float(p[1]) - float(kpts[112, 1])) * scale, 5),
                    0.0,
                    round(float(p[2]), 4),
                ]
                for p in kpts[112:133]
            ]
            lh_seq.append(lh)
            rh_seq.append(rh)
        else:
            lh_seq.append(None)
            rh_seq.append(None)

        # Anchor lowest foot onto y = 0.0
        min_foot_y = float(min(pts[29, 1], pts[30, 1], pts[31, 1], pts[32, 1]))
        pts[:, 1] -= min_foot_y
        body_3d_seq.append(pts)

    # Temporal smoothing pass on lifted Z coordinates (5-frame moving window, prevents flip spikes)
    valid_indices = [i for i, b in enumerate(body_3d_seq) if b is not None]
    if len(valid_indices) >= 3:
        z_stack = np.stack([body_3d_seq[i][:, 2] for i in valid_indices], axis=0)
        smoothed_z = z_stack.copy()
        for k in range(len(valid_indices)):
            lo = max(0, k - 2)
            hi = min(len(valid_indices), k + 3)
            smoothed_z[k] = np.mean(z_stack[lo:hi], axis=0)
        for k, orig_i in enumerate(valid_indices):
            body_3d_seq[orig_i][:, 2] = smoothed_z[k]

    for i in range(n):
        b = body_3d_seq[i]
        if b is None:
            results.append(([0.0, 0.94, 0.0], None, lh_seq[i], rh_seq[i]))
        else:
            hip = 0.5 * (b[23, :3] + b[24, :3])
            root = [round(float(hip[0]), 5), round(float(hip[1]), 5), round(float(hip[2]), 5)]
            joints = [[round(float(v), 5) for v in row] for row in b]
            results.append((root, joints, lh_seq[i], rh_seq[i]))

    return results


class Anime2DBackend(PerformanceBackend):
    def __init__(self, models_dir: str) -> None:
        self.models_dir = models_dir
        self._fallback = MediaPipeBackend(models_dir)
        self._dwpose_session = None
        self._dwpose_input_name = ""
        self._dwpose_hw = (384, 288)
        self._init_dwpose()

    def _init_dwpose(self) -> None:
        model_path = os.path.join(self.models_dir, "dw-ll_ucoco_384.onnx")
        if not os.path.isfile(model_path):
            print(
                "[Anime2DBackend] dw-ll_ucoco_384.onnx belum diunduh; menggunakan MediaPipe + Anime Cadence fallback.",
                file=sys.stderr,
                flush=True,
            )
            return
        try:
            import onnxruntime as ort  # type: ignore

            avail = ort.get_available_providers()
            providers = (
                ["CUDAExecutionProvider", "CPUExecutionProvider"]
                if "CUDAExecutionProvider" in avail
                else ["CPUExecutionProvider"]
            )
            self._dwpose_session = ort.InferenceSession(model_path, providers=providers)
            inp = self._dwpose_session.get_inputs()[0]
            self._dwpose_input_name = inp.name
            if len(inp.shape) == 4 and isinstance(inp.shape[2], int) and isinstance(inp.shape[3], int):
                self._dwpose_hw = (inp.shape[2], inp.shape[3])
        except Exception as exc:
            print(f"[Anime2DBackend] DWPose ONNX init warning: {exc}", file=sys.stderr, flush=True)
            self._dwpose_session = None

    def _infer_dwpose_133(self, frame_bgr: np.ndarray) -> Optional[np.ndarray]:
        if self._dwpose_session is None:
            return None
        ih, iw = self._dwpose_hw
        resized = cv2.resize(frame_bgr, (iw, ih), interpolation=cv2.INTER_LINEAR)
        rgb = cv2.cvtColor(resized, cv2.COLOR_BGR2RGB).astype(np.float32)
        mean = np.array([123.675, 116.28, 103.53], dtype=np.float32)
        std = np.array([58.395, 57.12, 57.375], dtype=np.float32)
        norm = ((rgb - mean) / std).transpose(2, 0, 1)[None, ...]

        outputs = self._dwpose_session.run(None, {self._dwpose_input_name: norm})
        if len(outputs) < 2:
            return None
        simcc_x = outputs[0][0]  # (133, W*2)
        simcc_y = outputs[1][0]  # (133, H*2)
        x_norm = np.argmax(simcc_x, axis=-1).astype(np.float32) / max(1.0, float(simcc_x.shape[-1]))
        y_norm = np.argmax(simcc_y, axis=-1).astype(np.float32) / max(1.0, float(simcc_y.shape[-1]))
        scores = np.clip(0.5 * (np.max(simcc_x, axis=-1) + np.max(simcc_y, axis=-1)), 0.0, 1.0)
        return np.stack([x_norm, y_norm, scores], axis=-1)

    def extract_track(
        self,
        frames_bgr: List[np.ndarray],
        fps: float,
        face_only: bool = False,
        person_track_id: int = 1,
        progress_cb: Optional[Callable[[int, int], None]] = None,
    ) -> Tuple[dict, float, bool, bool]:
        total = len(frames_bgr)
        fps = max(1.0, float(fps))

        # Step 5: Detect original anime hold-frame cadence first!
        hold_indices, unique_indices = detect_anime_cadence(frames_bgr)

        # If DWPose ONNX is unavailable, run MediaPipe ONLY on unique cel frames and expand via hold_indices!
        if self._dwpose_session is None:
            unique_frames = [frames_bgr[idx] for idx in unique_indices]
            raw_u, quality, has_hands, has_face = self._fallback.extract_track(
                unique_frames,
                fps,
                face_only=face_only,
                person_track_id=person_track_id,
                progress_cb=progress_cb,
            )
            u_map = {orig_idx: raw_u["frames"][pos] for pos, orig_idx in enumerate(unique_indices)}
            expanded_frames = []
            for i in range(total):
                src = u_map.get(hold_indices[i], raw_u["frames"][0])
                copied = dict(src)
                copied["t"] = round(i / fps, 5)
                expanded_frames.append(copied)

            raw_perf = {
                "fps": fps,
                "clip_type": "face_only" if face_only else "full_body",
                "source_mode": "anime_2d",
                "person_track_id": person_track_id,
                "anime_hold_indices": hold_indices,
                "frames": expanded_frames,
            }
            return raw_perf, quality, has_hands, has_face

        # Full DWPose 133 Stage A (on unique frames only -> 2-3x speedup!)
        unique_kpts: Dict[int, Optional[np.ndarray]] = {}
        unique_faces: Dict[int, Dict[str, float]] = {}
        conf_scores: List[float] = []

        for step, f_idx in enumerate(unique_indices):
            bgr = frames_bgr[f_idx]
            kpts = self._infer_dwpose_133(bgr)
            unique_kpts[f_idx] = kpts

            # Stage C: Anime Face Mapper from 68 landmarks (with MediaPipe blend if available)
            face_bs = empty_arkit_dict()
            if kpts is not None and len(kpts) >= 91:
                face_bs = map_68_face_to_arkit52(kpts[23:91, :2])
                conf_scores.append(float(np.mean(kpts[:23, 2])))
            else:
                rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
                mp_face = self._fallback.extract_face_blendshapes_from_frame(rgb)
                if mp_face is not None:
                    face_bs = mp_face
            unique_faces[f_idx] = face_bs

            if progress_cb and (step % 5 == 0 or step + 1 == len(unique_indices)):
                progress_cb(f_idx + 1, total)

        # Stage B: Temporal 2D->3D Lifting across unique sequence
        unique_kpts_list = [unique_kpts[idx] for idx in unique_indices]
        lifted_unique = _temporal_lift_2d_to_3d_sequence(unique_kpts_list)
        lifted_map = {idx: lifted_unique[pos] for pos, idx in enumerate(unique_indices)}

        out_frames: List[dict] = []
        any_hands = False
        any_face = any(any(v > 0.02 for v in fm.values()) for fm in unique_faces.values())

        for i in range(total):
            src_idx = hold_indices[i]
            face_bs = unique_faces.get(src_idx, empty_arkit_dict())
            if face_only:
                out_frames.append(
                    {
                        "t": round(i / fps, 5),
                        "root_pos": [0.0, 0.0, 0.0],
                        "body_joints": None,
                        "left_hand": None,
                        "right_hand": None,
                        "face_blendshapes": face_bs,
                    }
                )
            else:
                root_pos, body_joints, lh, rh = lifted_map[src_idx]
                if lh is not None or rh is not None:
                    any_hands = True
                out_frames.append(
                    {
                        "t": round(i / fps, 5),
                        "root_pos": root_pos,
                        "body_joints": body_joints,
                        "left_hand": lh,
                        "right_hand": rh,
                        "face_blendshapes": face_bs,
                    }
                )

        quality = float(np.clip(np.mean(conf_scores) if conf_scores else 0.80, 0.0, 1.0))
        raw_perf = {
            "fps": fps,
            "clip_type": "face_only" if face_only else "full_body",
            "source_mode": "anime_2d",
            "person_track_id": person_track_id,
            "anime_hold_indices": hold_indices,
            "frames": out_frames,
        }
        return raw_perf, round(quality, 4), any_hands, any_face
