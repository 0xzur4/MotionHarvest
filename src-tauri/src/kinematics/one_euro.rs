use crate::models::RawPerfFrame;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

/// Classic One-Euro Filter for low-latency adaptive jitter removal.
#[derive(Debug, Clone)]
pub struct OneEuroFilter {
    min_cutoff: f64,
    beta: f64,
    d_cutoff: f64,
    x_prev: Option<f64>,
    dx_prev: f64,
}

impl OneEuroFilter {
    pub fn new(min_cutoff: f64, beta: f64, d_cutoff: f64) -> Self {
        Self {
            min_cutoff: min_cutoff.max(0.05),
            beta: beta.max(0.0),
            d_cutoff: d_cutoff.max(0.1),
            x_prev: None,
            dx_prev: 0.0,
        }
    }

    fn alpha(cutoff: f64, dt: f64) -> f64 {
        let tau = 1.0 / (2.0 * PI * cutoff);
        1.0 / (1.0 + tau / dt.max(1e-4))
    }

    pub fn filter(&mut self, x: f64, dt: f64) -> f64 {
        if !x.is_finite() {
            return self.x_prev.unwrap_or(0.0);
        }
        let Some(prev) = self.x_prev else {
            self.x_prev = Some(x);
            self.dx_prev = 0.0;
            return x;
        };
        let dt = dt.max(1e-4);
        let dx = (x - prev) / dt;
        let a_d = Self::alpha(self.d_cutoff, dt);
        let dx_hat = a_d * dx + (1.0 - a_d) * self.dx_prev;
        let cutoff = self.min_cutoff + self.beta * dx_hat.abs();
        let a = Self::alpha(cutoff, dt);
        let x_hat = a * x + (1.0 - a) * prev;
        self.x_prev = Some(x_hat);
        self.dx_prev = dx_hat;
        x_hat
    }
}

/// Maps normalized UI smoothing strength (`0.0..=1.0`) to `(min_cutoff, beta)`.
pub fn smoothing_params(strength: f64) -> (f64, f64) {
    let s = strength.clamp(0.0, 1.0);
    // Higher strength -> lower min_cutoff -> stronger smoothing when stationary.
    let min_cutoff = 5.0 * (1.0 - s) + 0.35 * s;
    let beta = 0.45 * (1.0 - s) + 0.015 * s;
    (min_cutoff, beta)
}

/// Applies One-Euro smoothing to `root_pos`, `body_joints`, `left_hand`, and `right_hand`.
pub fn smooth_body_frames(frames: &mut [RawPerfFrame], fps: f64, body_smooth: f64) {
    if frames.len() < 2 || body_smooth <= 0.001 {
        return;
    }
    let dt = 1.0 / fps.max(1.0);
    let (min_cutoff, beta) = smoothing_params(body_smooth);

    let mut root_filters = [
        OneEuroFilter::new(min_cutoff, beta, 1.0),
        OneEuroFilter::new(min_cutoff, beta, 1.0),
        OneEuroFilter::new(min_cutoff, beta, 1.0),
    ];

    let body_count = frames
        .iter()
        .find_map(|f| f.body_joints.as_ref().map(|v| v.len()))
        .unwrap_or(0);
    let mut body_filters: Vec<[OneEuroFilter; 3]> = (0..body_count)
        .map(|_| {
            [
                OneEuroFilter::new(min_cutoff, beta, 1.0),
                OneEuroFilter::new(min_cutoff, beta, 1.0),
                OneEuroFilter::new(min_cutoff, beta, 1.0),
            ]
        })
        .collect();

    let mut lhand_filters: Vec<[OneEuroFilter; 3]> = (0..21)
        .map(|_| {
            [
                OneEuroFilter::new(min_cutoff * 1.2, beta * 1.2, 1.0),
                OneEuroFilter::new(min_cutoff * 1.2, beta * 1.2, 1.0),
                OneEuroFilter::new(min_cutoff * 1.2, beta * 1.2, 1.0),
            ]
        })
        .collect();

    let mut rhand_filters: Vec<[OneEuroFilter; 3]> = (0..21)
        .map(|_| {
            [
                OneEuroFilter::new(min_cutoff * 1.2, beta * 1.2, 1.0),
                OneEuroFilter::new(min_cutoff * 1.2, beta * 1.2, 1.0),
                OneEuroFilter::new(min_cutoff * 1.2, beta * 1.2, 1.0),
            ]
        })
        .collect();

    for frame in frames.iter_mut() {
        for axis in 0..3 {
            frame.root_pos[axis] = root_filters[axis].filter(frame.root_pos[axis], dt);
        }
        if let Some(ref mut joints) = frame.body_joints {
            for (j_idx, j) in joints.iter_mut().enumerate().take(body_filters.len()) {
                for axis in 0..3 {
                    j[axis] = body_filters[j_idx][axis].filter(j[axis], dt);
                }
            }
        }
        if let Some(ref mut lh) = frame.left_hand {
            for (j_idx, j) in lh.iter_mut().enumerate().take(lhand_filters.len()) {
                for axis in 0..3 {
                    j[axis] = lhand_filters[j_idx][axis].filter(j[axis], dt);
                }
            }
        }
        if let Some(ref mut rh) = frame.right_hand {
            for (j_idx, j) in rh.iter_mut().enumerate().take(rhand_filters.len()) {
                for axis in 0..3 {
                    j[axis] = rhand_filters[j_idx][axis].filter(j[axis], dt);
                }
            }
        }
    }
}

/// Applies One-Euro smoothing across all 52 ARKit facial blendshapes to stop facial flicker.
pub fn smooth_face_frames(frames: &mut [RawPerfFrame], fps: f64, face_smooth: f64) {
    if frames.len() < 2 || face_smooth <= 0.001 {
        return;
    }
    let dt = 1.0 / fps.max(1.0);
    let (min_cutoff, beta) = smoothing_params(face_smooth);

    let mut keys: BTreeSet<String> = BTreeSet::new();
    for f in frames.iter() {
        if let Some(ref map) = f.face_blendshapes {
            for k in map.keys() {
                keys.insert(k.clone());
            }
        }
    }
    if keys.is_empty() {
        return;
    }

    let mut filters: BTreeMap<String, OneEuroFilter> = keys
        .iter()
        .map(|k| (k.clone(), OneEuroFilter::new(min_cutoff, beta, 1.0)))
        .collect();

    for frame in frames.iter_mut() {
        if let Some(ref mut map) = frame.face_blendshapes {
            for k in &keys {
                let raw_val = *map.get(k).unwrap_or(&0.0);
                if let Some(flt) = filters.get_mut(k) {
                    let smoothed = flt.filter(raw_val, dt).clamp(0.0, 1.0);
                    map.insert(k.clone(), smoothed);
                }
            }
        }
    }
}
