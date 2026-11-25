"""MotionHarvest Studio Warm Python Sidecar (NDJSON stdin/stdout loop).

Protocol (Section 6.3):
  stdin  -> {"cmd":"extract","segment_id":"...","video_path":"...","ai_mode":"fast|studio_cuda|anime_2d",
             "multi_person":true,"max_persons":4,"min_visibility":0.50,"output_dir":"...","models_dir":"..."}
  stdout -> {"event":"classified","segment_id":"...","class":"...","person_count":N}
            {"event":"progress","segment_id":"...","current_frame":60,"total_frames":180}
            {"event":"track_completed","segment_id":"...","person_track_id":1,"clip_type":"full_body",...}
            {"event":"segment_done","segment_id":"...","tracks_produced":N}
"""

from __future__ import annotations

import json
import os
import sys
from typing import Dict, List, Optional, Tuple
import cv2
import numpy as np

from backends.anime_2d_backend import Anime2DBackend
from backends.base import PerformanceBackend
from backends.cuda_studio_backend import CudaStudioBackend
from backends.mediapipe_backend import MediaPipeBackend
from prefilter import SmartPreFilter
from tracker import MultiPersonTracker


def emit(payload: dict) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def load_video_frames(video_path: str) -> Tuple[List[np.ndarray], float]:
    cap = cv2.VideoCapture(video_path)
    if not cap.isOpened():
        raise RuntimeError(f"Tidak dapat membuka video: {video_path}")
    fps = float(cap.get(cv2.CAP_PROP_FPS))
    if not np.isfinite(fps) or fps <= 0.1 or fps > 240.0:
        fps = 24.0
    frames: List[np.ndarray] = []
    while True:
        ok, frame = cap.read()
        if not ok or frame is None:
            break
        frames.append(frame)
    cap.release()
    return frames, fps


class WarmWorker:
    def __init__(self) -> None:
        self._backends: Dict[Tuple[str, str], PerformanceBackend] = {}
        self._prefilters: Dict[str, SmartPreFilter] = {}
        self._trackers: Dict[str, MultiPersonTracker] = {}

    def _get_prefilter(self, models_dir: str) -> SmartPreFilter:
        if models_dir not in self._prefilters:
            self._prefilters[models_dir] = SmartPreFilter(models_dir=models_dir)
        return self._prefilters[models_dir]

    def _get_tracker(self, models_dir: str) -> MultiPersonTracker:
        if models_dir not in self._trackers:
            self._trackers[models_dir] = MultiPersonTracker(models_dir=models_dir)
        return self._trackers[models_dir]

    def _get_backend(self, ai_mode: str, models_dir: str) -> PerformanceBackend:
        key = (ai_mode, models_dir)
        if key not in self._backends:
            if ai_mode == "studio_cuda":
                self._backends[key] = CudaStudioBackend(models_dir)
            elif ai_mode == "anime_2d":
                self._backends[key] = Anime2DBackend(models_dir)
            else:
                self._backends[key] = MediaPipeBackend(models_dir)
        return self._backends[key]

    def handle_extract(self, req: dict) -> None:
        segment_id = str(req.get("segment_id", "seg"))
        video_path = str(req["video_path"])
        ai_mode = str(req.get("ai_mode", "fast"))
        multi_person = bool(req.get("multi_person", True))
        max_persons = int(req.get("max_persons", 4))
        min_visibility = float(req.get("min_visibility", 0.50))
        output_dir = str(req["output_dir"])
        models_dir = str(req.get("models_dir", ""))

        os.makedirs(output_dir, exist_ok=True)

        # 1. Smart 4-Way Pre-Filter
        pf = self._get_prefilter(models_dir)
        pf_res = pf.classify(
            video_path,
            multi_person=multi_person,
            min_visibility=min_visibility,
        )
        emit(
            {
                "event": "classified",
                "segment_id": segment_id,
                "class": pf_res.detected_class,
                "person_count": pf_res.person_count,
            }
        )

        if pf_res.detected_class == "skipped_empty":
            emit({"event": "segment_done", "segment_id": segment_id, "tracks_produced": 0})
            return

        frames_bgr, fps = load_video_frames(video_path)
        if not frames_bgr:
            emit({"event": "segment_done", "segment_id": segment_id, "tracks_produced": 0})
            return

        backend = self._get_backend(ai_mode, models_dir)
        tracks_produced = 0

        def progress_cb(cur: int, tot: int) -> None:
            emit(
                {
                    "event": "progress",
                    "segment_id": segment_id,
                    "current_frame": cur,
                    "total_frames": tot,
                }
            )

        # 2. Multi-Person Splitting vs Single-Subject Extraction
        if pf_res.detected_class == "multi_person" and multi_person:
            tracker = self._get_tracker(models_dir)
            tracks = tracker.track_video_frames(frames_bgr, max_persons=max_persons)
            if not tracks:
                # Fallback to single full-frame extraction if tracker yielded no persistent box
                tracks = []

            if len(tracks) >= 2:
                for track in tracks:
                    cropped_frames: List[np.ndarray] = []
                    last_box = next(iter(track.boxes.values()))
                    for f_idx, frame in enumerate(frames_bgr):
                        box = track.boxes.get(f_idx, last_box)
                        last_box = box
                        x1, y1, x2, y2 = box
                        crop = frame[y1:y2, x1:x2]
                        if crop.size == 0:
                            crop = frame
                        cropped_frames.append(crop)

                    raw_perf, quality, has_hands, has_face = backend.extract_track(
                        cropped_frames,
                        fps=fps,
                        face_only=False,
                        person_track_id=track.track_id,
                        progress_cb=progress_cb,
                    )
                    out_path = os.path.join(
                        output_dir, f"{segment_id}_p{track.track_id}.rawperf.json"
                    )
                    with open(out_path, "w", encoding="utf-8") as fh:
                        json.dump(raw_perf, fh)

                    tracks_produced += 1
                    emit(
                        {
                            "event": "track_completed",
                            "segment_id": segment_id,
                            "person_track_id": track.track_id,
                            "clip_type": raw_perf["clip_type"],
                            "fps": raw_perf["fps"],
                            "frame_count": len(raw_perf["frames"]),
                            "quality_score": quality,
                            "has_hands": has_hands,
                            "has_face": has_face,
                            "raw_perf_path": out_path,
                        }
                    )
                emit(
                    {
                        "event": "segment_done",
                        "segment_id": segment_id,
                        "tracks_produced": tracks_produced,
                    }
                )
                return

        # Single full-body OR face close-up track
        face_only = pf_res.detected_class == "face_closeup"
        raw_perf, quality, has_hands, has_face = backend.extract_track(
            frames_bgr,
            fps=fps,
            face_only=face_only,
            person_track_id=1,
            progress_cb=progress_cb,
        )
        suffix = "face" if face_only else "p1"
        out_path = os.path.join(output_dir, f"{segment_id}_{suffix}.rawperf.json")
        with open(out_path, "w", encoding="utf-8") as fh:
            json.dump(raw_perf, fh)

        tracks_produced = 1
        emit(
            {
                "event": "track_completed",
                "segment_id": segment_id,
                "person_track_id": 1,
                "clip_type": raw_perf["clip_type"],
                "fps": raw_perf["fps"],
                "frame_count": len(raw_perf["frames"]),
                "quality_score": quality,
                "has_hands": has_hands,
                "has_face": has_face,
                "raw_perf_path": out_path,
            }
        )
        emit(
            {
                "event": "segment_done",
                "segment_id": segment_id,
                "tracks_produced": tracks_produced,
            }
        )


def main() -> None:
    worker = WarmWorker()
    emit({"event": "ready", "version": "1.0.0"})

    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as exc:
            emit({"event": "error", "segment_id": "", "message": f"Invalid JSON: {exc}"})
            continue

        cmd = req.get("cmd", "")
        if cmd == "shutdown":
            break
        elif cmd == "ping":
            emit({"event": "ready", "version": "1.0.0"})
        elif cmd == "extract":
            try:
                worker.handle_extract(req)
            except Exception as exc:
                emit(
                    {
                        "event": "error",
                        "segment_id": str(req.get("segment_id", "")),
                        "message": str(exc),
                    }
                )
        else:
            emit(
                {
                    "event": "error",
                    "segment_id": str(req.get("segment_id", "")),
                    "message": f"Unknown command: {cmd}",
                }
            )


if __name__ == "__main__":
    main()
