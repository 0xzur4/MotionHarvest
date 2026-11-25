"""Fast Mode Backend — MediaPipe Whole-Body (Body 33 + Hands 21x2 + Face 52 ARKit).

Runs cleanly on CPU/iGPU using MediaPipe Tasks Landmarkers (`*.task` in models_dir).
When `face_only=True` (from Pre-Filter `face_closeup`), runs FaceLandmarker only for high speed.
"""

from __future__ import annotations

from typing import Callable, Dict, List, Optional, Tuple
import os
import sys
import cv2
import numpy as np

from backends.base import (
    ARKIT_52_BLENDSHAPES,
    PerformanceBackend,
    empty_arkit_dict,
    mediapipe_world_to_mh_yup,
)


class MediaPipeBackend(PerformanceBackend):
    def __init__(self, models_dir: str) -> None:
        self.models_dir = models_dir
        self._pose = None
        self._hands = None
        self._face = None
        self._init_tasks()

    def _init_tasks(self) -> None:
        try:
            from mediapipe.tasks import python as mp_python  # type: ignore
            from mediapipe.tasks.python import vision  # type: ignore

            pose_path = os.path.join(self.models_dir, "pose_landmarker_heavy.task")
            if os.path.isfile(pose_path):
                self._pose = vision.PoseLandmarker.create_from_options(
                    vision.PoseLandmarkerOptions(
                        base_options=mp_python.BaseOptions(model_asset_path=pose_path),
                        running_mode=vision.RunningMode.IMAGE,
                        num_poses=1,
                        min_pose_detection_confidence=0.45,
                        min_pose_presence_confidence=0.45,
                    )
                )

            hand_path = os.path.join(self.models_dir, "hand_landmarker.task")
            if os.path.isfile(hand_path):
                self._hands = vision.HandLandmarker.create_from_options(
                    vision.HandLandmarkerOptions(
                        base_options=mp_python.BaseOptions(model_asset_path=hand_path),
                        running_mode=vision.RunningMode.IMAGE,
                        num_hands=2,
                        min_hand_detection_confidence=0.40,
                        min_hand_presence_confidence=0.40,
                    )
                )

            face_path = os.path.join(self.models_dir, "face_landmarker.task")
            if os.path.isfile(face_path):
                self._face = vision.FaceLandmarker.create_from_options(
                    vision.FaceLandmarkerOptions(
                        base_options=mp_python.BaseOptions(model_asset_path=face_path),
                        running_mode=vision.RunningMode.IMAGE,
                        num_faces=1,
                        output_face_blendshapes=True,
                        min_face_detection_confidence=0.40,
                        min_face_presence_confidence=0.40,
                    )
                )
        except Exception as exc:
            print(f"[MediaPipeBackend] Task init warning: {exc}", file=sys.stderr, flush=True)

    def extract_face_blendshapes_from_frame(self, frame_rgb: np.ndarray) -> Optional[Dict[str, float]]:
        if self._face is None:
            return None
        try:
            import mediapipe as mp  # type: ignore

            mp_img = mp.Image(image_format=mp.ImageFormat.SRGB, data=frame_rgb)
            res = self._face.detect(mp_img)
            if res and res.face_blendshapes and len(res.face_blendshapes) > 0:
                blend = empty_arkit_dict()
                for cat in res.face_blendshapes[0]:
                    name = cat.category_name
                    if name in blend:
                        blend[name] = round(float(np.clip(cat.score, 0.0, 1.0)), 4)
                return blend
        except Exception:
            pass
        return None

    def _extract_hands_from_frame(
        self, frame_rgb: np.ndarray
    ) -> Tuple[Optional[List[List[float]]], Optional[List[List[float]]]]:
        if self._hands is None:
            return None, None
        left_hand: Optional[List[List[float]]] = None
        right_hand: Optional[List[List[float]]] = None
        try:
            import mediapipe as mp  # type: ignore

            mp_img = mp.Image(image_format=mp.ImageFormat.SRGB, data=frame_rgb)
            res = self._hands.detect(mp_img)
            if res and res.hand_world_landmarks:
                for idx, world_lms in enumerate(res.hand_world_landmarks):
                    side = "Left"
                    if res.handedness and idx < len(res.handedness) and res.handedness[idx]:
                        side = res.handedness[idx][0].category_name
                    pts = [
                        [round(float(p.x), 5), round(float(-p.y), 5), round(float(-p.z), 5), 0.95]
                        for p in world_lms
                    ]
                    if len(pts) == 21:
                        if side.lower() == "left":
                            left_hand = pts
                        else:
                            right_hand = pts
        except Exception:
            pass
        return left_hand, right_hand

    def extract_track(
        self,
        frames_bgr: List[np.ndarray],
        fps: float,
        face_only: bool = False,
        person_track_id: int = 1,
        progress_cb: Optional[Callable[[int, int], None]] = None,
    ) -> Tuple[dict, float, bool, bool]:
        import mediapipe as mp  # type: ignore

        total = len(frames_bgr)
        fps = max(1.0, float(fps))
        out_frames: List[dict] = []

        vis_accum: List[float] = []
        any_hands = False
        any_face = False

        last_root = [0.0, 0.94, 0.0]
        last_body: Optional[List[List[float]]] = None
        last_lh: Optional[List[List[float]]] = None
        last_rh: Optional[List[List[float]]] = None
        last_face: Optional[Dict[str, float]] = None

        for i, bgr in enumerate(frames_bgr):
            t = round(i / fps, 5)
            rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)

            face_bs = self.extract_face_blendshapes_from_frame(rgb)
            if face_bs is not None:
                any_face = True
                last_face = face_bs
                if face_only:
                    vis_accum.append(0.95)
            elif last_face is not None:
                face_bs = last_face

            if face_only:
                out_frames.append(
                    {
                        "t": t,
                        "root_pos": [0.0, 0.0, 0.0],
                        "body_joints": None,
                        "left_hand": None,
                        "right_hand": None,
                        "face_blendshapes": face_bs if face_bs is not None else empty_arkit_dict(),
                    }
                )
            else:
                root_pos = last_root
                body_joints = last_body

                if self._pose is not None:
                    try:
                        mp_img = mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb)
                        pres = self._pose.detect(mp_img)
                        if pres and pres.pose_world_landmarks and len(pres.pose_world_landmarks) > 0:
                            wlms = pres.pose_world_landmarks[0]
                            world_tuples = [
                                (p.x, p.y, p.z, getattr(p, "visibility", 0.9))
                                for p in wlms
                            ]
                            img_hips = None
                            if pres.pose_landmarks and len(pres.pose_landmarks[0]) > 24:
                                il = pres.pose_landmarks[0]
                                img_hips = (
                                    0.5 * (il[23].x + il[24].x),
                                    0.5 * (il[23].y + il[24].y),
                                )
                            root_pos, body_joints = mediapipe_world_to_mh_yup(world_tuples, img_hips)
                            last_root = root_pos
                            last_body = body_joints
                            vis_accum.append(float(np.mean([p[3] for p in world_tuples])))
                    except Exception:
                        pass

                lh, rh = self._extract_hands_from_frame(rgb)
                if lh is not None:
                    any_hands = True
                    last_lh = lh
                else:
                    lh = last_lh
                if rh is not None:
                    any_hands = True
                    last_rh = rh
                else:
                    rh = last_rh

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

        quality = float(np.clip(np.mean(vis_accum) if vis_accum else 0.5, 0.0, 1.0))
        clip_type = "face_only" if face_only else "full_body"
        raw_perf = {
            "fps": fps,
            "clip_type": clip_type,
            "source_mode": "fast",
            "person_track_id": person_track_id,
            "anime_hold_indices": [],
            "frames": out_frames,
        }
        return raw_perf, round(quality, 4), any_hands, any_face
