"""Smart 4-Way Pre-Filter (Step 4).

Samples 5 evenly spaced frames (15%, 35%, 50%, 65%, 85%) from a video segment
and classifies it into one of 4 paths in < 0.5s:
  1. `multi_person`     -> >= 2 clear subjects and multi_person enabled
  2. `full_performance` -> 1 subject with visible torso/hips + shoulders/knees
  3. `face_closeup`     -> No full body, BUT clear face >= 12% of frame width (NOT skipped!)
  4. `skipped_empty`    -> Scenery / empty / blurry frames
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, Tuple
import os
import cv2
import numpy as np

from tracker import MultiPersonTracker


@dataclass
class PreFilterResult:
    detected_class: str  # full_performance | face_closeup | multi_person | skipped_empty
    person_count: int
    max_face_width_ratio: float
    body_visibility: float


class SmartPreFilter:
    def __init__(self, models_dir: Optional[str] = None) -> None:
        self.models_dir = models_dir
        self.tracker = MultiPersonTracker(models_dir=models_dir)
        cascade_path = os.path.join(
            cv2.data.haarcascades, "haarcascade_frontalface_default.xml"
        )
        self._face_cascade = (
            cv2.CascadeClassifier(cascade_path)
            if os.path.isfile(cascade_path)
            else None
        )
        self._pose_detector = None
        self._face_detector = None
        self._init_mediapipe(models_dir)

    def _init_mediapipe(self, models_dir: Optional[str]) -> None:
        try:
            import mediapipe as mp  # type: ignore
            from mediapipe.tasks import python as mp_python  # type: ignore
            from mediapipe.tasks.python import vision  # type: ignore

            if models_dir:
                pose_model = os.path.join(models_dir, "pose_landmarker_heavy.task")
                if os.path.isfile(pose_model):
                    opts = vision.PoseLandmarkerOptions(
                        base_options=mp_python.BaseOptions(model_asset_path=pose_model),
                        running_mode=vision.RunningMode.IMAGE,
                        num_poses=2,
                        min_pose_detection_confidence=0.45,
                    )
                    self._pose_detector = vision.PoseLandmarker.create_from_options(opts)

                face_model = os.path.join(models_dir, "face_landmarker.task")
                if os.path.isfile(face_model):
                    fopts = vision.FaceLandmarkerOptions(
                        base_options=mp_python.BaseOptions(model_asset_path=face_model),
                        running_mode=vision.RunningMode.IMAGE,
                        num_faces=2,
                        output_face_blendshapes=False,
                        min_face_detection_confidence=0.45,
                    )
                    self._face_detector = vision.FaceLandmarker.create_from_options(fopts)
        except Exception:
            pass

    def _sample_five_frames(self, video_path: str) -> List[np.ndarray]:
        cap = cv2.VideoCapture(video_path)
        if not cap.isOpened():
            return []
        total = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
        samples: List[np.ndarray] = []
        if total <= 0:
            while len(samples) < 5:
                ok, f = cap.read()
                if not ok:
                    break
                samples.append(f)
            cap.release()
            return samples

        ratios = [0.15, 0.35, 0.50, 0.65, 0.85]
        for r in ratios:
            idx = max(0, min(total - 1, int(total * r)))
            cap.set(cv2.CAP_PROP_POS_FRAMES, idx)
            ok, frame = cap.read()
            if ok and frame is not None:
                samples.append(frame)
        cap.release()
        return samples

    def _inspect_pose_frame(self, frame_bgr: np.ndarray) -> Tuple[int, float]:
        """Returns `(num_poses_found, core_body_visibility)`."""
        if self._pose_detector is not None:
            try:
                import mediapipe as mp  # type: ignore

                rgb = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2RGB)
                mp_img = mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb)
                res = self._pose_detector.detect(mp_img)
                if res and res.pose_landmarks:
                    best_vis = 0.0
                    valid_poses = 0
                    for lms in res.pose_landmarks:
                        if len(lms) < 27:
                            continue
                        # Core visibility: hips (23, 24) + shoulders (11, 12) + knees (25, 26)
                        hips = 0.5 * (
                            getattr(lms[23], "visibility", 0.0)
                            + getattr(lms[24], "visibility", 0.0)
                        )
                        shoulders = 0.5 * (
                            getattr(lms[11], "visibility", 0.0)
                            + getattr(lms[12], "visibility", 0.0)
                        )
                        knees = 0.5 * (
                            getattr(lms[25], "visibility", 0.0)
                            + getattr(lms[26], "visibility", 0.0)
                        )
                        # Require hips AND (shoulders or knees) visible
                        core = min(hips, max(shoulders, knees))
                        if core > 0.30:
                            valid_poses += 1
                        best_vis = max(best_vis, core)
                    return valid_poses, best_vis
                return 0, 0.0
            except Exception:
                pass

        # Fallback using person box detector
        boxes = self.tracker.detect_people_in_frame(frame_bgr)
        if not boxes:
            return 0, 0.0
        h, w = frame_bgr.shape[:2]
        max_h_ratio = max((b[3] - b[1]) / max(1.0, float(h)) for b in boxes)
        return len(boxes), min(1.0, max_h_ratio * 1.1)

    def _inspect_face_width_ratio(self, frame_bgr: np.ndarray) -> float:
        h, w = frame_bgr.shape[:2]
        if w <= 0:
            return 0.0

        if self._face_detector is not None:
            try:
                import mediapipe as mp  # type: ignore

                rgb = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2RGB)
                mp_img = mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb)
                res = self._face_detector.detect(mp_img)
                if res and res.face_landmarks:
                    best_ratio = 0.0
                    for flm in res.face_landmarks:
                        xs = [p.x for p in flm]
                        if xs:
                            ratio = max(xs) - min(xs)
                            best_ratio = max(best_ratio, float(ratio))
                    return best_ratio
            except Exception:
                pass

        if self._face_cascade is not None:
            gray = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2GRAY)
            faces = self._face_cascade.detectMultiScale(
                gray, scaleFactor=1.15, minNeighbors=4, minSize=(36, 36)
            )
            if len(faces) > 0:
                return max(float(fw) / float(w) for (_, _, fw, _) in faces)
        return 0.0

    def classify(
        self,
        video_path: str,
        multi_person: bool = True,
        min_visibility: float = 0.50,
    ) -> PreFilterResult:
        samples = self._sample_five_frames(video_path)
        if not samples:
            return PreFilterResult("skipped_empty", 0, 0.0, 0.0)

        pose_counts: List[int] = []
        person_box_counts: List[int] = []
        body_vis_scores: List[float] = []
        face_ratios: List[float] = []

        for frame in samples:
            p_count, vis = self._inspect_pose_frame(frame)
            pose_counts.append(p_count)
            body_vis_scores.append(vis)

            face_r = self._inspect_face_width_ratio(frame)
            face_ratios.append(face_r)

            if multi_person:
                boxes = self.tracker.detect_people_in_frame(frame)
                person_box_counts.append(max(len(boxes), p_count))
            else:
                person_box_counts.append(p_count)

        avg_vis = float(np.mean(body_vis_scores)) if body_vis_scores else 0.0
        max_vis = float(np.max(body_vis_scores)) if body_vis_scores else 0.0
        max_face = float(np.max(face_ratios)) if face_ratios else 0.0
        multi_frames = sum(1 for c in person_box_counts if c >= 2)
        max_persons_seen = max(person_box_counts) if person_box_counts else 0

        # 1. Multi-person path (>= 2 clear people in at least 2 sampled frames)
        if multi_person and multi_frames >= 2:
            return PreFilterResult(
                detected_class="multi_person",
                person_count=max_persons_seen,
                max_face_width_ratio=max_face,
                body_visibility=max_vis,
            )

        # 2. Full body performance path (hips + shoulders/knees visible)
        if avg_vis >= min_visibility or (max_vis >= min_visibility and sum(v >= min_visibility * 0.8 for v in body_vis_scores) >= 2):
            return PreFilterResult(
                detected_class="full_performance",
                person_count=max(1, max_persons_seen),
                max_face_width_ratio=max_face,
                body_visibility=avg_vis,
            )

        # 3. Face close-up path (face width >= 12% of frame width -> NEVER skip!)
        if max_face >= 0.12:
            return PreFilterResult(
                detected_class="face_closeup",
                person_count=1,
                max_face_width_ratio=max_face,
                body_visibility=avg_vis,
            )

        # 4. Empty / scenery -> skip immediately
        return PreFilterResult(
            detected_class="skipped_empty",
            person_count=0,
            max_face_width_ratio=max_face,
            body_visibility=avg_vis,
        )
