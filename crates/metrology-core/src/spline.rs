//! Cubic B-spline sampling that reproduces
//! `scipy.ndimage.map_coordinates(img, pts, order=3, mode="nearest")`.
//!
//! Coordinates follow the scipy convention: pixel (i, j) is centered at integer
//! (x = i, y = j), so a step between pixels 9 and 10 sits at x = 9.5.
//!
//! Prefiltering the whole image would cost ~tens of ms, so only a window around the
//! requested region is prefiltered. The window is replicate-padded by `MARGIN` pixels
//! (the same as an infinite "nearest" extension) and prefiltered with scipy's mirror
//! boundary; the IIR pole (sqrt(3) - 2) makes the boundary's influence decay as 0.268^n,
//! so MARGIN = 20 matches scipy to ~1e-11 everywhere, border included.
//!
//! One window can serve many scan lines: build it once for the tool's ROI and sample
//! every line from it. `sample` refuses points outside the region the window was built
//! for, so a wrong-but-plausible value can never come from clamped coefficients.

use crate::image::ImageView;

/// Replicate-padding margin around the requested region.
pub const MARGIN: usize = 20;
/// Largest window we will prefilter (pixels), to bound memory on bad input.
const MAX_WINDOW_PIXELS: usize = 16_000_000;
/// Slack for floating-point round-off when testing "inside the valid region".
const EPS: f64 = 1e-9;

const POLE: f64 = -0.267_949_192_431_122_7; // sqrt(3) - 2
const GAIN: f64 = 6.0; // (1 - z)(1 - 1/z)
/// POLE^HORIZON < 1e-16, so the causal initialisation sum may stop there for long lines.
const HORIZON: usize = 30;

/// Prefiltered B-spline coefficients over a window of the image.
pub struct SplineWindow {
    x0: i64,
    y0: i64,
    w: usize,
    h: usize,
    coef: Vec<f64>,
    /// Region (inclusive) in image coordinates where `sample` is exact.
    valid: (f64, f64, f64, f64),
}

impl SplineWindow {
    /// Window able to evaluate any sample with x in [xmin, xmax], y in [ymin, ymax].
    /// `None` for non-finite bounds or an unreasonably large window.
    pub fn new(img: &ImageView, xmin: f64, xmax: f64, ymin: f64, ymax: f64) -> Option<Self> {
        if ![xmin, xmax, ymin, ymax].iter().all(|v| v.is_finite()) || xmin > xmax || ymin > ymax {
            return None;
        }
        let pad = 2 + MARGIN as i64;
        let lim = 1e9; // keep float -> i64 casts well-defined
        let x0 = xmin.clamp(-lim, lim).floor() as i64 - pad;
        let y0 = ymin.clamp(-lim, lim).floor() as i64 - pad;
        let x1 = xmax.clamp(-lim, lim).ceil() as i64 + pad + 1;
        let y1 = ymax.clamp(-lim, lim).ceil() as i64 + pad + 1;
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        if w.checked_mul(h)? > MAX_WINDOW_PIXELS {
            return None;
        }
        let (iw, ih) = (img.width() as i64, img.height() as i64);
        let mut coef = Vec::with_capacity(w * h);
        for j in 0..h as i64 {
            let row = img.row((y0 + j).clamp(0, ih - 1) as usize);
            for i in 0..w as i64 {
                coef.push(f64::from(row[(x0 + i).clamp(0, iw - 1) as usize]));
            }
        }
        for row in coef.chunks_mut(w) {
            prefilter_line(row);
        }
        prefilter_columns(&mut coef, w, h);
        let valid = (
            (x0 + pad) as f64,
            (x0 + w as i64 - 1 - pad) as f64,
            (y0 + pad) as f64,
            (y0 + h as i64 - 1 - pad) as f64,
        );
        Some(SplineWindow {
            x0,
            y0,
            w,
            h,
            coef,
            valid,
        })
    }

    /// True if `sample(x, y)` is exact there.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (xl, xh, yl, yh) = self.valid;
        x >= xl - EPS && x <= xh + EPS && y >= yl - EPS && y <= yh + EPS
    }

    /// Interpolated value at (x, y), or `None` outside the region the window was built for
    /// (or for non-finite input).
    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        if !self.contains(x, y) {
            return None;
        }
        let (u, v) = (x - self.x0 as f64, y - self.y0 as f64);
        let (iu, iv) = (u.floor(), v.floor());
        let (wx, wy) = (weights(u - iu), weights(v - iv));
        let (iu, iv) = (iu as i64, iv as i64);
        let mut acc = 0.0;
        for (dj, wyj) in wy.iter().enumerate() {
            let j = (iv + dj as i64 - 1).clamp(0, self.h as i64 - 1) as usize;
            let row = &self.coef[j * self.w..(j + 1) * self.w];
            let mut s = 0.0;
            for (di, wxi) in wx.iter().enumerate() {
                let i = (iu + di as i64 - 1).clamp(0, self.w as i64 - 1) as usize;
                s += wxi * row[i];
            }
            acc += wyj * s;
        }
        Some(acc)
    }
}

/// Cubic B-spline basis weights for the four coefficients around fractional offset t.
fn weights(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        (1.0 - t).powi(3) / 6.0,
        (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0,
        (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0,
        t3 / 6.0,
    ]
}

/// (z^(n-1), number of terms) for the causal initialisation of a line of `n` samples.
/// Beyond HORIZON samples both the mirror term and the tail are below 1e-16.
fn causal_init_shape(n: usize) -> (f64, usize) {
    if n - 1 > HORIZON {
        (0.0, HORIZON)
    } else {
        (POLE.powi(n as i32 - 1), n - 2)
    }
}

/// scipy `spline_filter1d(order=3, mode="mirror")` on one line, in place.
fn prefilter_line(c: &mut [f64]) {
    let n = c.len();
    if n < 2 {
        return;
    }
    for v in c.iter_mut() {
        *v *= GAIN;
    }
    let (z_n_1, terms) = causal_init_shape(n);
    let mut z_i = POLE;
    let mut c0 = c[0] + z_n_1 * c[n - 1];
    for i in 1..=terms {
        c0 += z_i * (c[i] + z_n_1 * c[n - 1 - i]);
        z_i *= POLE;
    }
    c[0] = c0 / (1.0 - z_n_1 * z_n_1);
    for i in 1..n {
        c[i] += POLE * c[i - 1];
    }
    c[n - 1] = (POLE * c[n - 2] + c[n - 1]) * POLE / (POLE * POLE - 1.0);
    for i in (0..n - 1).rev() {
        c[i] = POLE * (c[i + 1] - c[i]);
    }
}

/// The same filter along y for every column at once, working row by row so memory is
/// read sequentially (no strided gather).
fn prefilter_columns(c: &mut [f64], w: usize, h: usize) {
    if h < 2 {
        return;
    }
    for v in c.iter_mut() {
        *v *= GAIN;
    }
    let (z_n_1, terms) = causal_init_shape(h);
    let mut first: Vec<f64> = (0..w).map(|x| c[x] + z_n_1 * c[(h - 1) * w + x]).collect();
    let mut z_i = POLE;
    for i in 1..=terms {
        let (ri, rj) = (&c[i * w..(i + 1) * w], &c[(h - 1 - i) * w..(h - i) * w]);
        for x in 0..w {
            first[x] += z_i * (ri[x] + z_n_1 * rj[x]);
        }
        z_i *= POLE;
    }
    let norm = 1.0 - z_n_1 * z_n_1;
    for x in 0..w {
        c[x] = first[x] / norm;
    }
    for j in 1..h {
        let (prev, cur) = c.split_at_mut(j * w);
        let prev = &prev[(j - 1) * w..];
        for x in 0..w {
            cur[x] += POLE * prev[x];
        }
    }
    let k = POLE / (POLE * POLE - 1.0);
    for x in 0..w {
        c[(h - 1) * w + x] = (POLE * c[(h - 2) * w + x] + c[(h - 1) * w + x]) * k;
    }
    for j in (0..h - 1).rev() {
        let (cur, next) = c.split_at_mut((j + 1) * w);
        let cur = &mut cur[j * w..];
        for x in 0..w {
            cur[x] = POLE * (next[x] - cur[x]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (*seed >> 11) as f64 / (1u64 << 53) as f64
    }

    /// The row-vectorised column filter must equal the per-line filter, for short lines
    /// (full mirror formula) and long lines (truncated at HORIZON).
    #[test]
    fn column_prefilter_equals_line_prefilter() {
        let mut seed = 5;
        for h in [2usize, 3, 7, 30, 31, 32, 80] {
            let w = 9;
            let data: Vec<f64> = (0..w * h).map(|_| lcg(&mut seed) * 255.0).collect();
            let mut cols = data.clone();
            prefilter_columns(&mut cols, w, h);
            for x in 0..w {
                let mut line: Vec<f64> = (0..h).map(|j| data[j * w + x]).collect();
                prefilter_line(&mut line);
                for j in 0..h {
                    assert!(
                        (line[j] - cols[j * w + x]).abs() < 1e-12,
                        "h {h} col {x} row {j}"
                    );
                }
            }
        }
    }

    /// Truncating the causal sum at HORIZON must not change the result for long lines.
    #[test]
    fn truncated_init_matches_the_full_sum() {
        let mut seed = 11;
        let data: Vec<f64> = (0..200).map(|_| lcg(&mut seed) * 255.0).collect();
        let mut fast = data.clone();
        prefilter_line(&mut fast);
        // reference: untruncated mirror initialisation
        let n = data.len();
        let mut c: Vec<f64> = data.iter().map(|v| v * GAIN).collect();
        let z_n_1 = POLE.powi(n as i32 - 1);
        let (mut z_i, mut c0) = (POLE, c[0] + z_n_1 * c[n - 1]);
        for i in 1..n - 1 {
            c0 += z_i * (c[i] + z_n_1 * c[n - 1 - i]);
            z_i *= POLE;
        }
        c[0] = c0 / (1.0 - z_n_1 * z_n_1);
        for i in 1..n {
            c[i] += POLE * c[i - 1];
        }
        c[n - 1] = (POLE * c[n - 2] + c[n - 1]) * POLE / (POLE * POLE - 1.0);
        for i in (0..n - 1).rev() {
            c[i] = POLE * (c[i + 1] - c[i]);
        }
        for (a, b) in fast.iter().zip(&c) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn sample_refuses_points_outside_the_built_region_and_bad_input() {
        let px = vec![100.0f32; 50 * 50];
        let img = ImageView::new(50, 50, &px).unwrap();
        let win = SplineWindow::new(&img, 10.0, 12.0, 20.0, 22.0).unwrap();
        assert!(win.sample(11.0, 21.0).is_some());
        assert!(
            win.sample(10.0, 20.0).is_some(),
            "corner of the requested region"
        );
        assert!(win.sample(12.0, 22.0).is_some());
        assert!(win.sample(60.0, 21.0).is_none());
        assert!(win.sample(11.0, -5.0).is_none());
        assert!(win.sample(f64::NAN, 21.0).is_none());
        assert!(win.sample(1e300, 21.0).is_none());
        assert!(
            (win.sample(11.3, 21.7).unwrap() - 100.0).abs() < 1e-9,
            "flat image samples flat"
        );
    }

    #[test]
    fn huge_or_non_finite_bounds_are_rejected_not_wrapped() {
        let px = vec![0.0f32; 16];
        let img = ImageView::new(4, 4, &px).unwrap();
        assert!(SplineWindow::new(&img, f64::NAN, 1.0, 0.0, 1.0).is_none());
        assert!(SplineWindow::new(&img, 5.0, 1.0, 0.0, 1.0).is_none());
        assert!(
            SplineWindow::new(&img, 0.0, 1e12, 0.0, 1e12).is_none(),
            "window too large"
        );
        assert!(
            SplineWindow::new(&img, 1e6, 1e6, -1e6, -1e6).is_some(),
            "far point, small window"
        );
    }
}
