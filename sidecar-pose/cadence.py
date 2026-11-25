"""Anime 2D Hold-Frame Cadence Detector (Step 5).

Detects duplicate frames produced by 2D animation drawn On Twos (12fps),
On Threes (8fps), or variable hold rhythms using fast downscaled MAD/SSIM
comparison so AI inference only runs on unique frames and Rust can replay
the exact original hold cadence.
"""

from __future__ import annotations

from typing import List, Optional, Tuple
import cv2
import numpy as np


def detect_anime_cadence(
    frames_bgr: List[np.ndarray],
    mad_threshold: float = 2.6,
    roi: Optional[Tuple[int, int, int, int]] = None,
) -> Tuple[List[int], List[int]]:
    """Analyzes sequential frames and returns `(anime_hold_indices, unique_indices)`.

    - `anime_hold_indices[i]` points to the source unique frame index (<= i)
      representing frame `i` (e.g. `[0, 0, 2, 2, 2, 5, 5]`).
    - `unique_indices` lists only the distinct frame indices that need AI inference.
    """
    n = len(frames_bgr)
    if n == 0:
        return [], []
    if n == 1:
        return [0], [0]

    hold_indices: List[int] = [0]
    unique_indices: List[int] = [0]

    def to_small_gray(img: np.ndarray) -> np.ndarray:
        if roi is not None:
            x1, y1, x2, y2 = roi
            h, w = img.shape[:2]
            x1 = max(0, min(w - 1, x1))
            y1 = max(0, min(h - 1, y1))
            x2 = max(x1 + 1, min(w, x2))
            y2 = max(y1 + 1, min(h, y2))
            crop = img[y1:y2, x1:x2]
        else:
            crop = img
        gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY)
        return cv2.resize(gray, (128, 72), interpolation=cv2.INTER_AREA).astype(np.float32)

    ref_gray = to_small_gray(frames_bgr[0])
    ref_idx = 0

    for i in range(1, n):
        cur_gray = to_small_gray(frames_bgr[i])
        mad = float(np.mean(np.abs(cur_gray - ref_gray)))

        # Compute lightweight structural similarity guard for subtle lip/eye movement
        mean_r = float(np.mean(ref_gray))
        mean_c = float(np.mean(cur_gray))
        var_r = float(np.var(ref_gray)) + 1e-4
        var_c = float(np.var(cur_gray)) + 1e-4
        cov = float(np.mean((ref_gray - mean_r) * (cur_gray - mean_c)))
        c1, c2 = 6.5025, 58.5225
        ssim = ((2.0 * mean_r * mean_c + c1) * (2.0 * cov + c2)) / (
            (mean_r * mean_r + mean_c * mean_c + c1) * (var_r + var_c + c2)
        )

        # Frame is considered a held cel if both pixel difference is small and structure matches
        if mad < mad_threshold and ssim > 0.985:
            hold_indices.append(ref_idx)
        else:
            ref_idx = i
            ref_gray = cur_gray
            hold_indices.append(i)
            unique_indices.append(i)

    return hold_indices, unique_indices
