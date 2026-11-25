"""Multi-Person Detector & ByteTrack Tracker (Step 4).

Splits segments with >= 2 visible subjects into consistent per-person crops
so every character receives its own clean `*.rawperf.json` track.
Uses Ultralytics YOLO + ByteTrack when available, with a zero-dependency
OpenCV HOG + IoU fallback so multi-person splitting always works offline.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple
import cv2
import numpy as np


@dataclass
class PersonTrack:
    track_id: int
    # Frame index -> padded bounding box (x1, y1, x2, y2)
    boxes: Dict[int, Tuple[int, int, int, int]]
    avg_area: float


def _iou(a: Tuple[int, int, int, int], b: Tuple[int, int, int, int]) -> float:
    ix1 = max(a[0], b[0])
    iy1 = max(a[1], b[1])
    ix2 = min(a[2], b[2])
    iy2 = min(a[3], b[3])
    iw = max(0, ix2 - ix1)
    ih = max(0, iy2 - iy1)
    inter = iw * ih
    if inter <= 0:
        return 0.0
    area_a = max(1, (a[2] - a[0]) * (a[3] - a[1]))
    area_b = max(1, (b[2] - b[0]) * (b[3] - b[1]))
    return inter / float(area_a + area_b - inter)


def pad_box(
    box: Tuple[int, int, int, int], width: int, height: int, pad_ratio: float = 0.20
) -> Tuple[int, int, int, int]:
    x1, y1, x2, y2 = box
    bw = x2 - x1
    bh = y2 - y1
    px = int(bw * pad_ratio)
    py = int(bh * pad_ratio)
    return (
        max(0, x1 - px),
        max(0, y1 - py),
        min(width, x2 + px),
        min(height, y2 + py),
    )


class MultiPersonTracker:
    def __init__(self, models_dir: Optional[str] = None) -> None:
        self.models_dir = models_dir
        self._yolo = None
        self._hog = cv2.HOGDescriptor()
        self._hog.setSVMDetector(cv2.HOGDescriptor_getDefaultPeopleDetector())

        try:
            from ultralytics import YOLO  # type: ignore

            self._yolo = YOLO("yolov8n.pt")
        except Exception:
            self._yolo = None

    def detect_people_in_frame(self, frame_bgr: np.ndarray) -> List[Tuple[int, int, int, int]]:
        h, w = frame_bgr.shape[:2]
        if self._yolo is not None:
            try:
                results = self._yolo.predict(
                    frame_bgr, classes=[0], conf=0.35, verbose=False
                )
                boxes: List[Tuple[int, int, int, int]] = []
                if results and results[0].boxes is not None:
                    for b in results[0].boxes.xyxy.cpu().numpy():
                        x1, y1, x2, y2 = [int(v) for v in b[:4]]
                        if (x2 - x1) >= 24 and (y2 - y1) >= 48:
                            boxes.append((max(0, x1), max(0, y1), min(w, x2), min(h, y2)))
                return boxes
            except Exception:
                pass

        # Fallback: OpenCV HOG people detector
        scale = 1.0
        work = frame_bgr
        if w > 640:
            scale = 640.0 / float(w)
            work = cv2.resize(frame_bgr, (640, max(128, int(h * scale))))

        rects, weights = self._hog.detectMultiScale(
            work, winStride=(8, 8), padding=(8, 8), scale=1.06
        )
        raw_boxes: List[Tuple[int, int, int, int]] = []
        for idx, (rx, ry, rw, rh) in enumerate(rects):
            score = float(weights[idx]) if idx < len(weights) else 0.5
            if score < 0.35:
                continue
            x1 = int(rx / scale)
            y1 = int(ry / scale)
            x2 = int((rx + rw) / scale)
            y2 = int((ry + rh) / scale)
            if (x2 - x1) >= 24 and (y2 - y1) >= 48:
                raw_boxes.append((max(0, x1), max(0, y1), min(w, x2), min(h, y2)))

        # Non-maximum suppression
        kept: List[Tuple[int, int, int, int]] = []
        raw_boxes.sort(key=lambda b: (b[2] - b[0]) * (b[3] - b[1]), reverse=True)
        for box in raw_boxes:
            if all(_iou(box, k) < 0.45 for k in kept):
                kept.append(box)
        return kept

    def track_video_frames(
        self, frames_bgr: List[np.ndarray], max_persons: int = 4
    ) -> List[PersonTrack]:
        if not frames_bgr:
            return []
        h, w = frames_bgr[0].shape[:2]

        # Path A: Ultralytics YOLO + ByteTrack
        if self._yolo is not None:
            try:
                tracks_map: Dict[int, Dict[int, Tuple[int, int, int, int]]] = {}
                for f_idx, frame in enumerate(frames_bgr):
                    res = self._yolo.track(
                        frame,
                        persist=True,
                        tracker="bytetrack.yaml",
                        classes=[0],
                        conf=0.35,
                        verbose=False,
                    )
                    if res and res[0].boxes is not None and res[0].boxes.id is not None:
                        xyxys = res[0].boxes.xyxy.cpu().numpy()
                        ids = res[0].boxes.id.int().cpu().tolist()
                        for b, tid in zip(xyxys, ids):
                            box = pad_box(
                                (int(b[0]), int(b[1]), int(b[2]), int(b[3])),
                                w,
                                h,
                                pad_ratio=0.20,
                            )
                            tracks_map.setdefault(int(tid), {})[f_idx] = box
                if tracks_map:
                    return self._finalize_tracks(tracks_map, len(frames_bgr), max_persons)
            except Exception:
                pass

        # Path B: Lightweight IoU + Centroid Tracker (with 20% padding)
        active: Dict[int, Tuple[int, int, int, int]] = {}
        tracks_map = {}
        next_tid = 1

        for f_idx, frame in enumerate(frames_bgr):
            # Detect every 2 frames for speed, reuse last box on intermediate frame
            if f_idx % 2 == 0 or not active:
                dets = self.detect_people_in_frame(frame)
                matched_dets = set()
                new_active: Dict[int, Tuple[int, int, int, int]] = {}

                for tid, prev_box in active.items():
                    best_i = -1
                    best_iou = 0.25
                    for d_idx, det in enumerate(dets):
                        if d_idx in matched_dets:
                            continue
                        score = _iou(prev_box, det)
                        if score > best_iou:
                            best_iou = score
                            best_i = d_idx
                    if best_i >= 0:
                        matched_dets.add(best_i)
                        new_active[tid] = dets[best_i]

                for d_idx, det in enumerate(dets):
                    if d_idx not in matched_dets:
                        new_active[next_tid] = det
                        next_tid += 1

                active = new_active

            for tid, box in active.items():
                tracks_map.setdefault(tid, {})[f_idx] = pad_box(box, w, h, pad_ratio=0.20)

        return self._finalize_tracks(tracks_map, len(frames_bgr), max_persons)

    def _finalize_tracks(
        self,
        tracks_map: Dict[int, Dict[int, Tuple[int, int, int, int]]],
        total_frames: int,
        max_persons: int,
    ) -> List[PersonTrack]:
        min_frames = max(3, int(total_frames * 0.20))
        candidates: List[PersonTrack] = []
        for tid, boxes in tracks_map.items():
            if len(boxes) < min_frames:
                continue
            areas = [(b[2] - b[0]) * (b[3] - b[1]) for b in boxes.values()]
            avg_area = float(np.mean(areas)) if areas else 0.0
            candidates.append(PersonTrack(track_id=tid, boxes=boxes, avg_area=avg_area))

        # Rank by visibility * size, keep up to max_persons, re-index cleanly 1..k
        candidates.sort(key=lambda t: len(t.boxes) * t.avg_area, reverse=True)
        selected = candidates[: max(1, max_persons)]
        for idx, track in enumerate(selected, start=1):
            track.track_id = idx
        return selected
