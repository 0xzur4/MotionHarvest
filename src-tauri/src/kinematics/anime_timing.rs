use crate::models::{RawPerfFrame, TimingMode};

/// Computes the frame-hold index mapping for the requested anime timing cadence.
/// Every returned index is guaranteed to be within `0..frame_count`.
pub fn compute_timing_indices(
    frame_count: usize,
    fps: f64,
    mode: TimingMode,
    anime_hold_indices: &[usize],
    trim_start: usize,
) -> Vec<usize> {
    if frame_count == 0 {
        return Vec::new();
    }
    match mode {
        TimingMode::Smooth => (0..frame_count).collect(),
        TimingMode::Anime12 => {
            let step = (fps / 12.0).round().max(1.0) as usize;
            (0..frame_count).map(|i| (i / step) * step).collect()
        }
        TimingMode::Ghibli8 => {
            let step = (fps / 8.0).round().max(1.0) as usize;
            (0..frame_count).map(|i| (i / step) * step).collect()
        }
        TimingMode::OriginalHold => {
            if anime_hold_indices.is_empty() {
                // Fallback to On Twos (12 FPS) if source wasn't analyzed for anime hold cadence
                let step = (fps / 12.0).round().max(1.0) as usize;
                return (0..frame_count).map(|i| (i / step) * step).collect();
            }
            (0..frame_count)
                .map(|local_i| {
                    let global_i = trim_start + local_i;
                    if let Some(&src_global) = anime_hold_indices.get(global_i) {
                        src_global.saturating_sub(trim_start).min(frame_count - 1)
                    } else {
                        local_i
                    }
                })
                .collect()
        }
    }
}

/// Applies the quantized timing cadence simultaneously to body joints, hands, root_pos,
/// and all 52 facial blendshapes while keeping frame timestamps monotonic.
pub fn apply_anime_timing(
    frames: &[RawPerfFrame],
    fps: f64,
    mode: TimingMode,
    anime_hold_indices: &[usize],
    trim_start: usize,
) -> Vec<RawPerfFrame> {
    if frames.is_empty() || mode == TimingMode::Smooth {
        return frames.to_vec();
    }
    let map = compute_timing_indices(frames.len(), fps, mode, anime_hold_indices, trim_start);
    let mut out = Vec::with_capacity(frames.len());
    for (i, &src_idx) in map.iter().enumerate() {
        let mut cloned = frames[src_idx].clone();
        cloned.t = frames[i].t;
        out.push(cloned);
    }
    out
}
