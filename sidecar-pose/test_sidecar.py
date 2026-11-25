"""Verification tests for Step 4 & Step 5 of MotionHarvest Studio Python Sidecar."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
import cv2
import numpy as np

from backends.anime_2d_backend import Anime2DBackend, _temporal_lift_2d_to_3d_sequence
from backends.base import map_68_face_to_arkit52
from backends.cuda_studio_backend import CudaStudioBackend
from backends.mediapipe_backend import MediaPipeBackend
from cadence import detect_anime_cadence
from tracker import pad_box


def test_anime_cadence_detector() -> None:
    rng = np.random.default_rng(42)
    f0 = rng.integers(0, 255, size=(120, 160, 3), dtype=np.uint8)
    f2 = rng.integers(0, 255, size=(120, 160, 3), dtype=np.uint8)
    f5 = rng.integers(0, 255, size=(120, 160, 3), dtype=np.uint8)

    # Simulate On-Twos (0, 0) + On-Threes (2, 2, 2) + On-Twos (5, 5)
    seq = [f0, f0.copy(), f2, f2.copy(), f2.copy(), f5, f5.copy()]
    holds, uniques = detect_anime_cadence(seq)
    assert holds == [0, 0, 2, 2, 2, 5, 5], f"Unexpected holds: {holds}"
    assert uniques == [0, 2, 5], f"Unexpected uniques: {uniques}"
    print("[PASS] test_anime_cadence_detector")


def _make_neutral_face68() -> np.ndarray:
    pts = np.zeros((68, 2), dtype=np.float32)
    # Left eye (36..41) around (40, 50), Right eye (42..47) around (80, 50) -> IOD = 40
    pts[36] = [34, 50]
    pts[37] = [38, 47]
    pts[38] = [42, 47]
    pts[39] = [46, 50]
    pts[40] = [42, 53]
    pts[41] = [38, 53]

    pts[42] = [74, 50]
    pts[43] = [78, 47]
    pts[44] = [82, 47]
    pts[45] = [86, 50]
    pts[46] = [82, 53]
    pts[47] = [78, 53]

    # Eyebrows (17..21 L, 22..26 R)
    pts[17] = [32, 36]
    pts[19] = [40, 34]
    pts[21] = [48, 36]
    pts[22] = [72, 36]
    pts[24] = [80, 34]
    pts[26] = [88, 36]

    # Mouth corners (48, 54) & inner lips (61..67)
    pts[48] = [46, 82]
    pts[54] = [74, 82]
    pts[61] = [56, 81]
    pts[62] = [60, 81]
    pts[63] = [64, 81]
    pts[65] = [64, 83]
    pts[66] = [60, 83]
    pts[67] = [56, 83]
    return pts


def test_anime_face_mapper_52_arkit() -> None:
    # Smile test
    smile_pts = _make_neutral_face68()
    smile_pts[48] = [42, 76]  # corners pulled wide & UP (smaller Y)
    smile_pts[54] = [78, 76]
    res_smile = map_68_face_to_arkit52(smile_pts)
    assert len(res_smile) == 52
    assert res_smile["mouthSmileLeft"] > 0.50 and res_smile["mouthSmileRight"] > 0.50, res_smile

    # Angry test (inner brows pushed DOWN)
    angry_pts = _make_neutral_face68()
    angry_pts[21] = [48, 43]
    angry_pts[22] = [72, 43]
    res_angry = map_68_face_to_arkit52(angry_pts)
    assert res_angry["browDownLeft"] > 0.45 and res_angry["browDownRight"] > 0.45, res_angry

    # Blink + Jaw open test
    open_pts = _make_neutral_face68()
    open_pts[37, 1] = 49.8
    open_pts[41, 1] = 50.2
    open_pts[38, 1] = 49.8
    open_pts[40, 1] = 50.2
    open_pts[65, 1] = 98.0
    open_pts[66, 1] = 98.0
    open_pts[67, 1] = 98.0
    res_open = map_68_face_to_arkit52(open_pts)
    assert res_open["eyeBlinkLeft"] > 0.70, res_open
    assert res_open["jawOpen"] > 0.65, res_open
    print("[PASS] test_anime_face_mapper_52_arkit")


def test_temporal_2d_to_3d_lifter() -> None:
    seq = []
    for i in range(16):
        k = np.zeros((133, 3), dtype=np.float32)
        k[:, 2] = 0.9
        # Basic standing human 2D pose
        k[0, :2] = [0.50, 0.15]   # nose
        k[5, :2] = [0.56, 0.28]   # l_sh
        k[6, :2] = [0.44, 0.28]   # r_sh
        k[7, :2] = [0.64, 0.42]   # l_el
        k[8, :2] = [0.36, 0.42]   # r_el
        k[9, :2] = [0.68, 0.54]   # l_wr
        k[10, :2] = [0.32, 0.54]  # r_wr
        k[11, :2] = [0.54, 0.54]  # l_hip
        k[12, :2] = [0.46, 0.54]  # r_hip
        k[13, :2] = [0.54, 0.74]  # l_knee
        k[14, :2] = [0.46, 0.74]  # r_knee
        k[15, :2] = [0.54, 0.92]  # l_ankle
        k[16, :2] = [0.46, 0.92]  # r_ankle
        k[17, :2] = [0.55, 0.96]  # l_toe
        k[19, :2] = [0.53, 0.96]  # l_heel
        k[20, :2] = [0.45, 0.96]  # r_toe
        k[22, :2] = [0.47, 0.96]  # r_heel
        seq.append(k)

    lifted = _temporal_lift_2d_to_3d_sequence(seq)
    assert len(lifted) == 16
    for root, body, lh, rh in lifted:
        assert body is not None and len(body) == 33
        assert lh is not None and len(lh) == 21
        assert rh is not None and len(rh) == 21
        arr = np.array(body, dtype=np.float32)
        assert np.all(np.isfinite(arr)), "NaN detected in lifted 3D joints"
        # Head must be above feet in Y-Up space
        assert body[0][1] > body[29][1] + 0.5
    print("[PASS] test_temporal_2d_to_3d_lifter")


def test_worker_ndjson_empty_scenery_skip() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        vid_path = os.path.join(tmp, "empty_sky.mp4")
        fourcc = cv2.VideoWriter_fourcc(*"mp4v")
        vw = cv2.VideoWriter(vid_path, fourcc, 24.0, (320, 240))
        sky = np.full((240, 320, 3), (220, 160, 80), dtype=np.uint8)
        for _ in range(36):
            vw.write(sky)
        vw.release()

        worker_py = os.path.join(os.path.dirname(os.path.abspath(__file__)), "worker.py")
        proc = subprocess.Popen(
            [sys.executable, "-u", worker_py],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        assert proc.stdout is not None and proc.stdin is not None
        ready_line = proc.stdout.readline().strip()
        ready_evt = json.loads(ready_line)
        assert ready_evt["event"] == "ready", ready_evt

        t0 = time.perf_counter()
        req = {
            "cmd": "extract",
            "segment_id": "seg_sky",
            "video_path": vid_path,
            "ai_mode": "fast",
            "multi_person": True,
            "max_persons": 4,
            "min_visibility": 0.5,
            "output_dir": tmp,
            "models_dir": tmp,
        }
        proc.stdin.write(json.dumps(req) + "\n")
        proc.stdin.flush()

        events = []
        while True:
            line = proc.stdout.readline().strip()
            if not line:
                break
            evt = json.loads(line)
            events.append(evt)
            if evt.get("event") in ("segment_done", "error"):
                break
        elapsed = time.perf_counter() - t0

        proc.stdin.write(json.dumps({"cmd": "shutdown"}) + "\n")
        proc.stdin.flush()
        proc.wait(timeout=5)

        classes = [e["class"] for e in events if e.get("event") == "classified"]
        assert classes == ["skipped_empty"], f"Expected skipped_empty, got {events}"
        assert events[-1]["tracks_produced"] == 0
        print(f"[PASS] test_worker_ndjson_empty_scenery_skip ({elapsed:.3f}s)")


def test_all_three_backends_graceful_fallback() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        frames = [np.zeros((240, 320, 3), dtype=np.uint8) for _ in range(6)]
        for mode_name, cls in [
            ("fast", MediaPipeBackend),
            ("studio_cuda", CudaStudioBackend),
            ("anime_2d", Anime2DBackend),
        ]:
            backend = cls(tmp)
            raw, q, has_hands, has_face = backend.extract_track(
                frames, fps=24.0, face_only=False, person_track_id=1
            )
            assert raw["source_mode"] == mode_name
            assert len(raw["frames"]) == 6
            if mode_name == "anime_2d":
                assert len(raw["anime_hold_indices"]) == 6
        assert pad_box((100, 100, 200, 300), 640, 480, 0.2) == (80, 60, 220, 340)
    print("[PASS] test_all_three_backends_graceful_fallback")


if __name__ == "__main__":
    test_anime_cadence_detector()
    test_anime_face_mapper_52_arkit()
    test_temporal_2d_to_3d_lifter()
    test_all_three_backends_graceful_fallback()
    test_worker_ndjson_empty_scenery_skip()
    print("ALL SIDECAR TESTS PASSED!")
