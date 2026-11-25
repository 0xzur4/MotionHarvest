"""Studio GPU CUDA Backend — RTMPose3D WholeBody 133 (ONNX CUDA) + Face 52 ARKit (Step 4).

Executes RTMPose3D WholeBody 133 via `onnxruntime` with `CUDAExecutionProvider`
for studio-grade body, heel/toe foot contact, and finger precision, paired with
52 ARKit face blendshapes. Gracefully falls back to MediaPipeBackend if the
RTMPose3D ONNX weights have not yet been downloaded.
"""

from __future__ import annotations

from typing import Callable, List, Optional, Tuple
import os
import sys
import cv2
import numpy as np

from backends.base import PerformanceBackend, map_68_face_to_arkit52
from backends.mediapipe_backend import MediaPipeBackend


def _coco_wb133_to_mh33(kpts133: np.ndarray) -> Tuple[List[float], List[List[float]]]:
    """Maps COCO-WholeBody 133 3D keypoints (body 0..16, feet 17..22, face 23..90,
    left hand 91..111, right hand 112..132) into MotionHarvest's 33-joint Y-Up schema.
    """
    pts = np.zeros((33, 4), dtype=np.float32)

    def set_pt(mh_idx: int, coco_idx: int) -> None:
        if coco_idx < len(kpts133):
            x, y, z, s = kpts133[coco_idx]
            pts[mh_idx] = [x, -y, -z, s]

    # Face anchors
    set_pt(0, 0)   # Nose
    set_pt(2, 1)   # Left Eye
    set_pt(5, 2)   # Right Eye
    set_pt(7, 3)   # Left Ear
    set_pt(8, 4)   # Right Ear
    # Upper body
    set_pt(11, 5)  # Left Shoulder
    set_pt(12, 6)  # Right Shoulder
    set_pt(13, 7)  # Left Elbow
    set_pt(14, 8)  # Right Elbow
    set_pt(15, 9)  # Left Wrist
    set_pt(16, 10) # Right Wrist
    # Hands index/pinky tips from WB hand blocks (91..111 L, 112..132 R)
    set_pt(19, 96)   # Left Index MCP
    set_pt(17, 108)  # Left Pinky MCP
    set_pt(20, 117)  # Right Index MCP
    set_pt(18, 129)  # Right Pinky MCP
    # Lower body + Heel & Toe (COCO-WholeBody 17..22)
    set_pt(23, 11) # Left Hip
    set_pt(24, 12) # Right Hip
    set_pt(25, 13) # Left Knee
    set_pt(26, 14) # Right Knee
    set_pt(27, 15) # Left Ankle
    set_pt(28, 16) # Right Ankle
    set_pt(29, 19) # Left Heel
    set_pt(30, 22) # Right Heel
    set_pt(31, 17) # Left Big Toe / FootIndex
    set_pt(32, 20) # Right Big Toe / FootIndex

    # Normalize vertical floor height to y = 0.0 and metric scale (~1.7m)
    foot_y = float(min(pts[29, 1], pts[30, 1], pts[31, 1], pts[32, 1]))
    head_y = float(pts[0, 1])
    span = max(0.3, head_y - foot_y)
    scale = 1.62 / span
    pts[:, 0] *= scale
    pts[:, 1] = (pts[:, 1] - foot_y) * scale
    pts[:, 2] *= scale

    hip_mid = 0.5 * (pts[23, :3] + pts[24, :3])
    root_pos = [round(float(hip_mid[0]), 5), round(float(hip_mid[1]), 5), round(float(hip_mid[2]), 5)]
    body_list = [[round(float(v), 5) for v in row] for row in pts]
    return root_pos, body_list


class CudaStudioBackend(PerformanceBackend):
    def __init__(self, models_dir: str) -> None:
        self.models_dir = models_dir
        self._fallback = MediaPipeBackend(models_dir)
        self._session = None
        self._input_name = ""
        self._input_hw = (384, 288)
        self._init_onnx()

    def _init_onnx(self) -> None:
        candidates = [
            os.path.join(self.models_dir, "rtmw3d-x_8xb64_cocktail14-384x288.onnx"),
            os.path.join(self.models_dir, "rtmw3d-l_8xb64_cocktail14-384x288.onnx"),
        ]
        model_path = next((p for p in candidates if os.path.isfile(p)), None)
        if model_path is None:
            print(
                "[CudaStudioBackend] rtmw3d ONNX belum diunduh; fallback sementara ke MediaPipe Whole-Body.",
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
            self._session = ort.InferenceSession(model_path, providers=providers)
            inp = self._session.get_inputs()[0]
            self._input_name = inp.name
            if len(inp.shape) == 4 and isinstance(inp.shape[2], int) and isinstance(inp.shape[3], int):
                self._input_hw = (inp.shape[2], inp.shape[3])
        except Exception as exc:
            print(f"[CudaStudioBackend] ONNX init error ({exc}); using MediaPipe fallback.", file=sys.stderr, flush=True)
            self._session = None

    def _infer_rtmpose3d(self, frame_bgr: np.ndarray) -> Optional[np.ndarray]:
        if self._session is None:
            return None
        ih, iw = self._input_hw
        resized = cv2.resize(frame_bgr, (iw, ih), interpolation=cv2.INTER_LINEAR)
        rgb = cv2.cvtColor(resized, cv2.COLOR_BGR2RGB).astype(np.float32)
        mean = np.array([123.675, 116.28, 103.53], dtype=np.float32)
        std = np.array([58.395, 57.12, 57.375], dtype=np.float32)
        norm = ((rgb - mean) / std).transpose(2, 0, 1)[None, ...]

        outputs = self._session.run(None, {self._input_name: norm})
        if len(outputs) < 2:
            return None

        # SimCC X, Y (and optional Z) decoding
        simcc_x = outputs[0][0]  # (133, W*2)
        simcc_y = outputs[1][0]  # (133, H*2)
        simcc_z = outputs[2][0] if len(outputs) >= 3 else None

        x_idx = np.argmax(simcc_x, axis=-1).astype(np.float32)
        y_idx = np.argmax(simcc_y, axis=-1).astype(np.float32)
        max_x = np.max(simcc_x, axis=-1)
        max_y = np.max(simcc_y, axis=-1)
        scores = np.clip(0.5 * (max_x + max_y), 0.0, 1.0)

        x_norm = (x_idx / max(1.0, float(simcc_x.shape[-1])) - 0.5) * 2.0
        y_norm = (y_idx / max(1.0, float(simcc_y.shape[-1])) - 0.5) * 2.0
        if simcc_z is not None:
            z_idx = np.argmax(simcc_z, axis=-1).astype(np.float32)
            z_norm = (z_idx / max(1.0, float(simcc_z.shape[-1])) - 0.5) * 2.0
        else:
            z_norm = np.zeros_like(x_norm)

        return np.stack([x_norm, y_norm, z_norm, scores], axis=-1)

    def extract_track(
        self,
        frames_bgr: List[np.ndarray],
        fps: float,
        face_only: bool = False,
        person_track_id: int = 1,
        progress_cb: Optional[Callable[[int, int], None]] = None,
    ) -> Tuple[dict, float, bool, bool]:
        if face_only or self._session is None:
            raw, q, h, f = self._fallback.extract_track(
                frames_bgr, fps, face_only=face_only, person_track_id=person_track_id, progress_cb=progress_cb
            )
            raw["source_mode"] = "studio_cuda"
            return raw, q, h, f

        total = len(frames_bgr)
        fps = max(1.0, float(fps))
        out_frames: List[dict] = []
        scores_accum: List[float] = []
        any_hands = False
        any_face = False

        for i, bgr in enumerate(frames_bgr):
            t = round(i / fps, 5)
            kpts133 = self._infer_rtmpose3d(bgr)
            rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
            face_bs = self._fallback.extract_face_blendshapes_from_frame(rgb)

            if kpts133 is not None and len(kpts133) >= 133:
                root_pos, body_joints = _coco_wb133_to_mh33(kpts133)
                lh_raw = kpts133[91:112]
                rh_raw = kpts133[112:133]
                lh = [[round(float(p[0]), 5), round(float(-p[1]), 5), round(float(-p[2]), 5), round(float(p[3]), 4)] for p in lh_raw]
                rh = [[round(float(p[0]), 5), round(float(-p[1]), 5), round(float(-p[2]), 5), round(float(p[3]), 4)] for p in rh_raw]
                if float(np.mean(lh_raw[:, 3])) > 0.25 or float(np.mean(rh_raw[:, 3])) > 0.25:
                    any_hands = True
                if face_bs is None:
                    face_bs = map_68_face_to_arkit52(kpts133[23:91, :2])
                if face_bs is not None:
                    any_face = True
                scores_accum.append(float(np.mean(kpts133[:23, 3])))
            else:
                root_pos = [0.0, 0.94, 0.0]
                body_joints = None
                lh = None
                rh = None

            out_frames.append(
                {
                    "t": t,
                    "root_pos": root_pos,
                    "body_joints": body_joints,
                    "left_hand": lh,
                    "right_hand": rh,
                    "face_blendshapes": face_bs,
                }
            )

            if progress_cb and (i % 10 == 0 or i + 1 == total):
                progress_cb(i + 1, total)

        quality = float(np.clip(np.mean(scores_accum) if scores_accum else 0.75, 0.0, 1.0))
        raw_perf = {
            "fps": fps,
            "clip_type": "full_body",
            "source_mode": "studio_cuda",
            "person_track_id": person_track_id,
            "anime_hold_indices": [],
            "frames": out_frames,
        }
        return raw_perf, round(quality, 4), any_hands, any_face
