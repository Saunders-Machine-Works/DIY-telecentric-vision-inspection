//! 1-D profile processing: sampling along a line, Gaussian smoothing, gradient and
//! sub-pixel edge location (PROJECT_BACKGROUND 5.1/5.2). The smoothing and gradient
//! match scipy.ndimage.gaussian_filter1d(mode="nearest") and numpy.gradient.
//!
//! Non-finite samples are not handled specially: a NaN poisons every smoothed value
//! within the kernel radius, so callers must reject NaN images first (the illumination
//! check does).

use crate::image::ImageView;
use crate::spline::SplineWindow;

/// Largest Gaussian sigma accepted (samples); larger values are clamped.
pub const MAX_SIGMA: f64 = 50.0;

/// Which transitions to accept, in the direction of increasing profile index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    /// dark -> bright
    Rising,
    /// bright -> dark
    Falling,
    Either,
}

/// How the gradient peak is refined from its three samples around the maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refine {
    /// Parabola through (a, b, c). Matches the numpy/scipy reference but carries a
    /// phase-dependent bias of up to ~0.02 px (zero at whole and half pixels).
    Parabola,
    /// Parabola through ln(a), ln(b), ln(c), i.e. a Gaussian. The gradient of a blurred step
    /// is Gaussian, so this is unbiased for it. Falls back to `Parabola` if a or c is 0.
    Gaussian,
}

/// One located edge. `position` is in profile samples (1 sample = 1 px along the line).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub position: f64,
    /// Interpolated peak |gradient| in gray levels per sample.
    pub strength: f64,
    pub rising: bool,
    /// Within the smoothing kernel (plus the gradient stencil) of either profile end: the
    /// replicate padding and one-sided gradient bias such an edge, so do not trust it.
    pub near_end: bool,
}

fn kernel_radius(sigma: f64) -> usize {
    (4.0 * sigma + 0.5) as usize
}

/// scipy `gaussian_filter1d(x, sigma, mode="nearest")` (truncate = 4.0).
pub fn gaussian_smooth(x: &[f64], sigma: f64) -> Vec<f64> {
    if x.is_empty() || sigma <= 0.0 || sigma.is_nan() {
        return x.to_vec();
    }
    let sigma = sigma.min(MAX_SIGMA);
    let radius = kernel_radius(sigma);
    let mut kernel: Vec<f64> = (0..=2 * radius)
        .map(|k| {
            let d = k as f64 - radius as f64;
            (-0.5 * d * d / (sigma * sigma)).exp()
        })
        .collect();
    let sum: f64 = kernel.iter().sum();
    kernel.iter_mut().for_each(|k| *k /= sum);
    let n = x.len();
    let last = n as i64 - 1;
    let tap = |i: usize| -> f64 {
        kernel
            .iter()
            .enumerate()
            .map(|(k, w)| w * x[(i as i64 + k as i64 - radius as i64).clamp(0, last) as usize])
            .sum()
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if i >= radius && i + radius < n {
            // interior: no edge clamping needed
            let win = &x[i - radius..=i + radius];
            out.push(kernel.iter().zip(win).map(|(w, v)| w * v).sum());
        } else {
            out.push(tap(i));
        }
    }
    out
}

/// numpy.gradient with unit spacing: central differences inside, one-sided at the ends.
pub fn gradient(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let mut g = Vec::with_capacity(n);
    g.push(x[1] - x[0]);
    g.extend((1..n - 1).map(|i| (x[i + 1] - x[i - 1]) / 2.0));
    g.push(x[n - 1] - x[n - 2]);
    g
}

/// A contrast-relative edge threshold: `fraction` of the 5th-to-95th percentile range of the
/// profile. An absolute gray-level threshold drifts with backlight level; this does not.
/// A blurred step of height H with blur sigma b has a gradient peak of H / (b * sqrt(2 pi)),
/// about 0.33 H for b = 1.2, so fraction 0.15 sits at roughly half the nominal peak.
pub fn contrast_threshold(values: &[f64], fraction: f64) -> f64 {
    if values.len() < 2 {
        return f64::INFINITY;
    }
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 2 {
        return f64::INFINITY;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    fraction * (at(0.95) - at(0.05))
}

/// Every edge in `profile` using `Refine::Gaussian`. See `locate_edges_with`.
pub fn locate_edges(
    profile: &[f64],
    sigma: f64,
    min_strength: f64,
    polarity: Polarity,
) -> Vec<Edge> {
    locate_edges_with(profile, sigma, min_strength, polarity, Refine::Gaussian)
}

/// Every edge in `profile`: peaks of the signed gradient of the smoothed profile that
/// reach `min_strength`, refined from the 3 samples around the maximum. Ordered by position.
///
/// A neighbouring sample of the opposite sign belongs to a different edge lobe, so it is
/// treated as zero when refining (a narrow dark line gives two separate edges, not one
/// bent peak). Even so, two opposite edges closer than ~6 samples still bias each other
/// through the smoothing tails (worst case 0.018 px at 6 samples, 0.25 px at 4, ~0 at 8+
/// for sigma 1.0), so features narrower than that must not be trusted to sub-pixel accuracy.
///
/// `sigma` is clamped to `MAX_SIGMA` (50 samples) without error; sigma <= 0 or NaN disables
/// smoothing.
pub fn locate_edges_with(
    profile: &[f64],
    sigma: f64,
    min_strength: f64,
    polarity: Polarity,
    refine: Refine,
) -> Vec<Edge> {
    let n = profile.len();
    let radius = if sigma > 0.0 && !sigma.is_nan() {
        kernel_radius(sigma.min(MAX_SIGMA))
    } else {
        0
    };
    let g = gradient(&gaussian_smooth(profile, sigma));
    let mut out = Vec::new();
    for i in 1..g.len().saturating_sub(1) {
        let s = if g[i] > 0.0 { 1.0 } else { -1.0 };
        let (a, b, c) = ((s * g[i - 1]).max(0.0), s * g[i], (s * g[i + 1]).max(0.0));
        if b < min_strength || !(b > a && b >= c) {
            continue;
        }
        let rising = s > 0.0;
        match polarity {
            Polarity::Rising if !rising => continue,
            Polarity::Falling if rising => continue,
            _ => {}
        }
        let (d, strength) = match refine {
            Refine::Gaussian if a > 0.0 && c > 0.0 => {
                let (la, lb, lc) = (a.ln(), b.ln(), c.ln());
                let denom = la - 2.0 * lb + lc;
                let d = if denom != 0.0 {
                    0.5 * (la - lc) / denom
                } else {
                    0.0
                };
                (d, (lb - 0.25 * (la - lc) * d).exp())
            }
            _ => {
                let denom = a - 2.0 * b + c;
                let d = if denom != 0.0 {
                    0.5 * (a - c) / denom
                } else {
                    0.0
                };
                (d, b - 0.25 * (a - c) * d)
            }
        };
        out.push(Edge {
            position: i as f64 + d,
            strength,
            rising,
            near_end: i < radius + 2 || i + radius + 2 >= n,
        });
    }
    out
}

/// A crossing must be followed (or preceded) by at least this many samples that lie in the dark
/// region, starting `SILHOUETTE_GAP` away from the crossing, to count as a silhouette edge.
const SILHOUETTE_GAP: usize = 8;
const SILHOUETTE_SPAN: usize = 60;

/// Half-width, in samples, of the window the crossing spline is fitted on. The error of a natural
/// end condition falls off as about 0.27 per sample, so 8 samples leave under 1e-4 of it.
const CROSSING_WINDOW: usize = 8;

/// Second derivatives of the natural cubic spline through unit-spaced samples `y` (Thomas
/// algorithm on the tridiagonal system).
fn spline_second_derivatives(y: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut m = vec![0.0; n];
    if n < 3 {
        return m;
    }
    let mut c = vec![0.0; n]; // modified super-diagonal
    let mut d = vec![0.0; n]; // modified right-hand side
    for i in 1..n - 1 {
        let rhs = 6.0 * (y[i + 1] - 2.0 * y[i] + y[i - 1]);
        let denom = 4.0 - c[i - 1];
        c[i] = 1.0 / denom;
        d[i] = (rhs - d[i - 1]) / denom;
    }
    for i in (1..n - 1).rev() {
        m[i] = d[i] - c[i] * m[i + 1];
    }
    m
}

/// Value of the cubic between samples `k` and `k + 1` at `t` in [0, 1].
fn spline_value(y: &[f64], m: &[f64], k: usize, t: f64) -> f64 {
    let (a, b) = (1.0 - t, t);
    a * y[k] + b * y[k + 1] + ((a * a * a - a) * m[k] + (b * b * b - b) * m[k + 1]) / 6.0
}

/// Where the profile crosses `level` between samples `i` and `i + 1`, refined on the natural
/// cubic spline through the samples within `CROSSING_WINDOW` of the crossing. A spline through
/// many samples (not a cubic through four) keeps the phase error of the crossing below 0.005 px
/// for blurred edges, where a 4-point Catmull-Rom leaves up to 0.04 px that does not cancel in a
/// width unless both edges have the same sub-pixel phase.
fn refine_crossing(v: &[f64], i: usize, level: f64) -> f64 {
    let lo = i.saturating_sub(CROSSING_WINDOW);
    let hi = (i + 1 + CROSSING_WINDOW).min(v.len() - 1);
    let y = &v[lo..=hi];
    let m = spline_second_derivatives(y);
    let k = i - lo;
    let (mut a, mut b) = (0.0f64, 1.0f64);
    let sign = (y[k] - level).signum();
    for _ in 0..50 {
        let mid = 0.5 * (a + b);
        if (spline_value(y, &m, k, mid) - level).signum() == sign {
            a = mid;
        } else {
            b = mid;
        }
    }
    i as f64 + 0.5 * (a + b)
}

/// The left and right silhouette edges of a dark part on a bright field along one scan line, as
/// positions in samples: the 50 % crossings between the line's bright and dark levels (its 95th
/// and 5th percentile). For a symmetric blur the 50 % crossing is the true boundary at any edge
/// angle, so soft edges (a thread flank crossing the scan at a shallow angle) are located as
/// accurately as sharp ones, with no gradient threshold to fail on them.
///
/// The left edge is the first bright->dark crossing followed by a dark region and the right edge
/// the last dark->bright crossing preceded by one, so a speck of dust or a scratch (a narrow
/// dark dip) is skipped. `None` if there is no such pair or the part is narrower than 2 gaps.
pub fn silhouette_edges(values: &[f64]) -> Option<(f64, f64)> {
    let n = values.len();
    if n < 4 * SILHOUETTE_SPAN || values.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| sorted[((n - 1) as f64 * q).round() as usize];
    let (bright, dark) = (at(0.95), at(0.05));
    if bright - dark < 20.0 {
        return None;
    }
    let level = 0.5 * (bright + dark);
    let dark_limit = 0.35 * bright + 0.65 * dark;
    let mean_over = |lo: isize, hi: isize| -> f64 {
        let (lo, hi) = (
            lo.clamp(0, n as isize) as usize,
            hi.clamp(0, n as isize) as usize,
        );
        if hi <= lo {
            f64::MAX
        } else {
            values[lo..hi].iter().sum::<f64>() / (hi - lo) as f64
        }
    };
    let (g, s) = (SILHOUETTE_GAP as isize, SILHOUETTE_SPAN as isize);
    let crossings: Vec<(usize, bool)> = (0..n - 1)
        .filter(|&i| (values[i] >= level) != (values[i + 1] >= level))
        .map(|i| (i, values[i + 1] >= level)) // true = dark->bright (rising)
        .collect();
    let left = crossings
        .iter()
        .find(|&&(i, rising)| !rising && mean_over(i as isize + g, i as isize + s) < dark_limit)?;
    let right = crossings
        .iter()
        .rev()
        .find(|&&(i, rising)| rising && mean_over(i as isize - s, i as isize - g) < dark_limit)?;
    let (l, r) = (
        refine_crossing(values, left.0, level),
        refine_crossing(values, right.0, level),
    );
    (r > l + 2.0 * SILHOUETTE_GAP as f64).then_some((l, r))
}
/// Intensity profile sampled every 1 px along a segment.
pub struct Profile {
    pub values: Vec<f64>,
    pub origin: (f64, f64),
    /// Unit direction of increasing index.
    pub direction: (f64, f64),
}

impl Profile {
    /// Sample `img` from `p0` toward `p1` at 1 px spacing (x, y in pixel-center coords),
    /// building a window just for this segment. For many lines, build one `SplineWindow`
    /// for the tool's ROI and use `from_line_in` ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â it is several times cheaper.
    /// `None` if the segment is shorter than 2 px, non-finite, or its window is too big.
    pub fn from_line(img: &ImageView, p0: (f64, f64), p1: (f64, f64)) -> Option<Profile> {
        let win = SplineWindow::new(
            img,
            p0.0.min(p1.0),
            p0.0.max(p1.0),
            p0.1.min(p1.1),
            p0.1.max(p1.1),
        )?;
        Profile::from_line_in(&win, p0, p1)
    }

    /// Profile along an axis-aligned pixel row (`horizontal`) or column at integer position
    /// `c`, covering indices `lo..hi`. A cubic B-spline interpolates exactly at integer
    /// coordinates, so this equals `from_line` there without building a window.
    /// `None` if the range is shorter than 2 samples or outside the image.
    pub fn along_axis(
        img: &ImageView,
        horizontal: bool,
        c: usize,
        lo: usize,
        hi: usize,
    ) -> Option<Profile> {
        let (len, across) = if horizontal {
            (img.width(), img.height())
        } else {
            (img.height(), img.width())
        };
        if hi <= lo + 1 || hi > len || c >= across {
            return None;
        }
        let values: Vec<f64> = if horizontal {
            img.row(c)[lo..hi].iter().map(|&v| f64::from(v)).collect()
        } else {
            (lo..hi).map(|y| f64::from(img.row(y)[c])).collect()
        };
        let (origin, direction) = if horizontal {
            ((lo as f64, c as f64), (1.0, 0.0))
        } else {
            ((c as f64, lo as f64), (0.0, 1.0))
        };
        Some(Profile {
            values,
            origin,
            direction,
        })
    }

    /// Sample along a segment from an existing window. `None` if the segment is shorter than
    /// 2 px or leaves the region the window was built for.
    pub fn from_line_in(win: &SplineWindow, p0: (f64, f64), p1: (f64, f64)) -> Option<Profile> {
        let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
        let len = dx.hypot(dy);
        if !len.is_finite() || len < 2.0 {
            return None;
        }
        let dir = (dx / len, dy / len);
        let n = len.floor() as usize + 1;
        let mut values = Vec::with_capacity(n);
        for k in 0..n {
            values.push(win.sample(p0.0 + dir.0 * k as f64, p0.1 + dir.1 * k as f64)?);
        }
        Some(Profile {
            values,
            origin: p0,
            direction: dir,
        })
    }

    /// Image coordinates of profile position `pos` (in samples).
    pub fn point_at(&self, pos: f64) -> (f64, f64) {
        (
            self.origin.0 + self.direction.0 * pos,
            self.origin.1 + self.direction.1 * pos,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Abramowitz & Stegun 7.1.26 (|error| < 1.5e-7), enough for 0.01 px ground truth.
    fn erf(x: f64) -> f64 {
        let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
        let poly = t
            * (0.254_829_592
                + t * (-0.284_496_736
                    + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
        (1.0 - poly * (-x * x).exp()).copysign(x)
    }

    /// Blurred step of a given gray range at `x0` (blur sigma in samples).
    fn step(n: usize, x0: f64, lo: f64, hi: f64, blur: f64) -> Vec<f64> {
        (0..n)
            .map(|i| lo + (hi - lo) * 0.5 * (1.0 + erf((i as f64 - x0) / (blur * 2f64.sqrt()))))
            .collect()
    }

    /// Deterministic pseudo-noise (uniform, then summed to look Gaussian) so tests are repeatable.
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f64 {
            let mut s = 0.0;
            for _ in 0..12 {
                self.0 = self
                    .0
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                s += (self.0 >> 11) as f64 / (1u64 << 53) as f64;
            }
            s - 6.0 // ~N(0,1)
        }
    }

    #[test]
    fn noise_free_step_recovered_within_two_hundredths_of_a_pixel_at_every_subpixel_offset() {
        for k in 0..20 {
            let x0 = 30.0 + k as f64 * 0.05;
            let edges = locate_edges(&step(64, x0, 20.0, 200.0, 1.2), 1.0, 5.0, Polarity::Either);
            assert_eq!(edges.len(), 1, "x0 {x0}");
            let err = edges[0].position - x0;
            assert!(
                err.abs() < 0.02,
                "x0 {x0}: located {} (err {err:.4})",
                edges[0].position
            );
            assert!(edges[0].rising);
        }
    }

    /// Worst error over 100 sub-pixel phases for a noise-free blurred step.
    fn worst_phase_error(blur: f64, refine: Refine) -> f64 {
        (0..100)
            .map(|k| {
                let x0 = 30.0 + k as f64 * 0.01;
                let e = locate_edges_with(
                    &step(64, x0, 20.0, 200.0, blur),
                    1.0,
                    5.0,
                    Polarity::Either,
                    refine,
                );
                (e[0].position - x0).abs()
            })
            .fold(0.0, f64::max)
    }

    /// Documents the parabola bias (a smooth function of phase, ~0.017 px at blur 1.2) and
    /// shows the Gaussian refinement removes it. The step generator itself has error < 2e-7.
    #[test]
    fn gaussian_refinement_removes_the_parabola_phase_bias() {
        for blur in [0.8, 1.2, 2.0] {
            let (par, gau) = (
                worst_phase_error(blur, Refine::Parabola),
                worst_phase_error(blur, Refine::Gaussian),
            );
            assert!(par > 0.008 && par < 0.025, "blur {blur}: parabola {par}");
            assert!(gau < 0.004, "blur {blur}: gaussian {gau}");
        }
    }

    #[test]
    fn gaussian_refinement_falls_back_to_parabola_when_a_neighbour_is_zero() {
        // an ideal 1-sample step with no smoothing: gradient neighbours of the peak are 0
        let mut p = vec![0.0; 20];
        p[10..].fill(100.0);
        let g = locate_edges_with(&p, 0.0, 10.0, Polarity::Either, Refine::Gaussian);
        let q = locate_edges_with(&p, 0.0, 10.0, Polarity::Either, Refine::Parabola);
        assert_eq!(g.len(), 2 - 1);
        assert_eq!(g, q);
        assert!(g[0].position.is_finite());
    }

    #[test]
    fn falling_step_has_falling_polarity_and_polarity_filter_applies() {
        let p = step(64, 30.4, 210.0, 25.0, 1.2);
        let e = locate_edges(&p, 1.0, 5.0, Polarity::Falling);
        assert_eq!(e.len(), 1);
        assert!(!e[0].rising);
        assert!((e[0].position - 30.4).abs() < 0.02);
        assert!(locate_edges(&p, 1.0, 5.0, Polarity::Rising).is_empty());
    }

    /// Edge location scatter and bias for a given per-sample noise level, over 300 trials.
    fn noisy_trials(noise_sd: f64) -> (f64, f64) {
        let mut noise = Noise(12345);
        let x0 = 30.37;
        let (mut sum, mut sum2, trials) = (0.0, 0.0, 300);
        for _ in 0..trials {
            let p: Vec<f64> = step(64, x0, 20.0, 200.0, 1.2)
                .iter()
                .map(|v| v + noise_sd * noise.next())
                .collect();
            let e = locate_edges(&p, 1.0, 20.0, Polarity::Either);
            assert_eq!(e.len(), 1);
            let err = e[0].position - x0;
            sum += err;
            sum2 += err * err;
        }
        let mean = sum / trials as f64;
        (mean, (sum2 / trials as f64 - mean * mean).sqrt())
    }

    /// Measured camera noise is ~1.6 gray levels per frame, so an 8-frame mean has ~0.57.
    /// The edge must be unbiased and its scatter must be small and scale with the noise.
    #[test]
    fn noisy_step_is_unbiased_and_scatter_scales_with_noise() {
        let (bias_avg, sd_avg) = noisy_trials(1.6 / 8f64.sqrt());
        let (bias_one, sd_one) = noisy_trials(1.6);
        assert!(
            bias_avg.abs() < 0.005 && bias_one.abs() < 0.01,
            "bias {bias_avg:.4} / {bias_one:.4} px"
        );
        assert!(sd_avg < 0.015, "8-frame mean scatter {sd_avg:.4} px");
        assert!(sd_one < 0.045, "single-frame scatter {sd_one:.4} px");
        let ratio = sd_one / sd_avg;
        assert!(
            (2.0..4.5).contains(&ratio),
            "scatter should scale ~sqrt(8)=2.8 with noise, got {ratio:.2}"
        );
    }
    #[test]
    fn below_threshold_and_flat_profiles_give_no_edges() {
        assert!(locate_edges(&[100.0; 50], 1.0, 1.0, Polarity::Either).is_empty());
        let weak = step(64, 30.0, 100.0, 104.0, 1.2);
        assert!(locate_edges(&weak, 1.0, 5.0, Polarity::Either).is_empty());
        assert_eq!(locate_edges(&weak, 1.0, 0.5, Polarity::Either).len(), 1);
    }

    #[test]
    fn tiny_inputs_do_not_panic() {
        for n in 0..4 {
            let v = vec![1.0; n];
            let _ = locate_edges(&v, 1.0, 1.0, Polarity::Either);
            let _ = gaussian_smooth(&v, 1.0);
            assert_eq!(gradient(&v).len(), n);
        }
    }

    #[test]
    fn gaussian_of_constant_is_constant_and_kernel_is_normalised() {
        let s = gaussian_smooth(&[7.0; 30], 1.3);
        assert!(s.iter().all(|v| (v - 7.0).abs() < 1e-12));
        let mut imp = vec![0.0; 41];
        imp[20] = 1.0;
        let total: f64 = gaussian_smooth(&imp, 1.0).iter().sum();
        assert!((total - 1.0).abs() < 1e-12);
    }

    /// A vertical blurred edge at x = 20.3 sampled along a horizontal line and along a
    /// 30-degree line: the located image point must lie on x = 20.3 in both cases.
    #[test]
    fn line_profile_locates_an_image_edge_at_any_angle() {
        let (w, h) = (64usize, 48usize);
        let px: Vec<f32> = (0..w * h)
            .map(|i| {
                (20.0 + 180.0 * 0.5 * (1.0 + erf(((i % w) as f64 - 20.3) / (1.2 * 2f64.sqrt()))))
                    as f32
            })
            .collect();
        let img = ImageView::new(w, h, &px).unwrap();
        let horiz = Profile::from_line(&img, (8.0, 24.0), (40.0, 24.0)).unwrap();
        let e = &locate_edges(&horiz.values, 1.0, 5.0, Polarity::Either)[0];
        let (x, y) = horiz.point_at(e.position);
        assert!((x - 20.3).abs() < 0.03, "x {x}");
        assert!((y - 24.0).abs() < 1e-12);

        let a = 30f64.to_radians();
        let p0 = (8.0, 10.0);
        let p1 = (p0.0 + 40.0 * a.cos(), p0.1 + 40.0 * a.sin());
        let tilted = Profile::from_line(&img, p0, p1).unwrap();
        let e = &locate_edges(&tilted.values, 1.0, 5.0, Polarity::Either)[0];
        let (x, _) = tilted.point_at(e.position);
        assert!((x - 20.3).abs() < 0.03, "x {x}");
    }

    #[test]
    fn degenerate_lines_are_rejected() {
        let px = vec![0.0f32; 100];
        let img = ImageView::new(10, 10, &px).unwrap();
        assert!(Profile::from_line(&img, (1.0, 1.0), (1.5, 1.0)).is_none());
        assert!(Profile::from_line(&img, (f64::NAN, 1.0), (5.0, 1.0)).is_none());
    }

    /// An edge within the smoothing kernel of a profile end is flagged; interior edges are not.
    #[test]
    fn edges_near_the_profile_ends_are_flagged() {
        // sigma 1.0 => radius 4 => flagged when i < 6 or i + 6 >= n
        let early = locate_edges(&step(64, 3.5, 20.0, 200.0, 1.2), 1.0, 5.0, Polarity::Either);
        assert!(early[0].near_end, "edge at 3.5 of 64");
        let late = locate_edges(
            &step(64, 60.5, 20.0, 200.0, 1.2),
            1.0,
            5.0,
            Polarity::Either,
        );
        assert!(late[0].near_end, "edge at 60.5 of 64");
        let mid = locate_edges(
            &step(64, 30.5, 20.0, 200.0, 1.2),
            1.0,
            5.0,
            Polarity::Either,
        );
        assert!(!mid[0].near_end);
        let (lo, hi) = (
            locate_edges(&step(64, 7.5, 20.0, 200.0, 1.2), 1.0, 5.0, Polarity::Either),
            0,
        );
        assert!(
            !lo[0].near_end && hi == 0,
            "edge at 7.5 is outside the flagged zone"
        );
    }

    /// A thin dark line is a falling edge immediately followed by a rising one. Each edge
    /// must be located from its own lobe: the neighbouring opposite lobe must not bend it.
    #[test]
    fn adjacent_opposite_edges_do_not_pull_each_other() {
        let n = 64;
        // dark line 3 px wide centred at 30.0 (edges at 28.5 and 31.5), light blur 0.6
        let p: Vec<f64> = step(n, 28.5, 200.0, 40.0, 0.6)
            .iter()
            .zip(step(n, 31.5, 40.0, 200.0, 0.6))
            .map(|(a, b)| a + b - 40.0 - 0.0)
            .map(|v| v.clamp(0.0, 255.0))
            .collect();
        let e = locate_edges(&p, 0.7, 5.0, Polarity::Either);
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(!e[0].rising && e[1].rising);
        assert!(
            (e[0].position - 28.5).abs() < 0.1,
            "falling at {}",
            e[0].position
        );
        assert!(
            (e[1].position - 31.5).abs() < 0.1,
            "rising at {}",
            e[1].position
        );
    }

    /// Opposite-sign neighbours count as zero (a hand-built gradient would be [1, 8, -9, 2]).
    #[test]
    fn opposite_sign_neighbour_is_ignored_in_refinement() {
        // 0.0 smoothing: profile whose gradient is exactly [., 8, -9, ., .] at samples 2 and 3
        let p = [0.0, 0.0, 0.0, 16.0, 0.0, 0.0, 0.0];
        let rising = locate_edges_with(&p, 0.0, 5.0, Polarity::Rising, Refine::Parabola);
        let falling = locate_edges_with(&p, 0.0, 5.0, Polarity::Falling, Refine::Parabola);
        assert_eq!((rising.len(), falling.len()), (1, 1));
        // with the opposite lobe treated as zero, each lobe is a lone 1-sample peak: d = 0
        assert!(
            (rising[0].position - 2.0).abs() < 1e-12,
            "{}",
            rising[0].position
        );
        assert!(
            (falling[0].position - 4.0).abs() < 1e-12,
            "{}",
            falling[0].position
        );
    }

    /// Many scan lines can share one window built for the ROI, and give the same numbers
    /// as per-line windows.
    #[test]
    fn shared_window_matches_per_line_windows() {
        let (w, h) = (80usize, 60usize);
        let px: Vec<f32> = (0..w * h)
            .map(|i| {
                (20.0 + 180.0 * 0.5 * (1.0 + erf(((i % w) as f64 - 30.4) / (1.2 * 2f64.sqrt()))))
                    as f32
            })
            .collect();
        let img = ImageView::new(w, h, &px).unwrap();
        let win = SplineWindow::new(&img, 10.0, 50.0, 10.0, 50.0).unwrap();
        for y in [12.0, 25.5, 40.25, 49.0] {
            let a = Profile::from_line(&img, (10.0, y), (50.0, y)).unwrap();
            let b = Profile::from_line_in(&win, (10.0, y), (50.0, y)).unwrap();
            assert_eq!(a.values.len(), b.values.len());
            for (u, v) in a.values.iter().zip(&b.values) {
                assert!((u - v).abs() < 1e-9);
            }
        }
        assert!(
            Profile::from_line_in(&win, (10.0, 12.0), (70.0, 12.0)).is_none(),
            "leaves the window"
        );
    }

    /// Worst location error of the two edges of a dark line `sep` samples wide, over 40 phases.
    fn pair_error(sep: f64, sigma: f64, blur: f64) -> f64 {
        let mut worst: f64 = 0.0;
        for k in 0..40 {
            let a = 30.0 + k as f64 * 0.025;
            let b = a + sep;
            let p: Vec<f64> = step(80, a, 200.0, 40.0, blur)
                .iter()
                .zip(step(80, b, 40.0, 200.0, blur))
                .map(|(u, v)| u + v - 40.0)
                .collect();
            let e = locate_edges(&p, sigma, 5.0, Polarity::Either);
            assert_eq!(e.len(), 2, "sep {sep} sigma {sigma}: {e:?}");
            worst = worst
                .max((e[0].position - a).abs())
                .max((e[1].position - b).abs());
        }
        worst
    }

    /// Edge bias vs separation of two opposite edges (smoothing sigma 1.0, optical blur 1.2).
    /// Measured worst case: 0.0004 px at 8 px, 0.018 px at 6 px, 0.25 px at 4 px.
    /// The bias grows as the edges get closer than ~4 smoothing sigmas; the thread tool must
    /// treat roots/crests narrower than that as unreliable. The numbers asserted are the
    /// measured worst cases, with margin.
    #[test]
    fn close_opposite_edges_bias_shrinks_with_separation() {
        let limits = [(8.0, 0.01), (6.0, 0.03), (4.0, 0.30)]; // (separation px, max bias px)
        for (sep, max) in limits {
            let e = pair_error(sep, 1.0, 1.2);
            println!("PAIR sep {sep}: worst error {e:.4} px");
            assert!(e < max, "sep {sep}: error {e:.4} px exceeds {max}");
        }
    }

    /// The axis-aligned fast path must equal the spline path at integer coordinates.
    #[test]
    fn axis_aligned_fast_path_equals_the_spline_path() {
        let (w, h) = (90usize, 70usize);
        let px: Vec<f32> = (0..w * h)
            .map(|i| ((i * 37 % 101) as f32) + (i % w) as f32 * 0.5)
            .collect();
        let img = ImageView::new(w, h, &px).unwrap();
        let a = Profile::along_axis(&img, true, 30, 5, 80).unwrap();
        let b = Profile::from_line(&img, (5.0, 30.0), (79.0, 30.0)).unwrap();
        assert_eq!(a.values.len(), b.values.len());
        for (u, v) in a.values.iter().zip(&b.values) {
            assert!((u - v).abs() < 1e-9, "{u} vs {v}");
        }
        let c = Profile::along_axis(&img, false, 40, 3, 60).unwrap();
        let d = Profile::from_line(&img, (40.0, 3.0), (40.0, 59.0)).unwrap();
        for (u, v) in c.values.iter().zip(&d.values) {
            assert!((u - v).abs() < 1e-9);
        }
        assert_eq!((c.origin, c.direction), ((40.0, 3.0), (0.0, 1.0)));
        assert!(
            Profile::along_axis(&img, true, 70, 0, 10).is_none(),
            "row out of range"
        );
        assert!(
            Profile::along_axis(&img, true, 0, 0, 91).is_none(),
            "range out of image"
        );
        assert!(
            Profile::along_axis(&img, true, 0, 5, 6).is_none(),
            "too short"
        );
    }

    fn shaft_profile(n: usize, a: f64, b: f64, blur: f64, bright: f64, dark: f64) -> Vec<f64> {
        (0..n)
            .map(|i| {
                let x = i as f64;
                let down = 0.5 * (1.0 + erf((x - a) / (blur * 2f64.sqrt())));
                let up = 0.5 * (1.0 + erf((x - b) / (blur * 2f64.sqrt())));
                bright - (bright - dark) * (down - up)
            })
            .collect()
    }

    /// The 50 % crossing is the boundary for sharp and for very soft (12 px) edges alike, at any
    /// sub-pixel phase, and independent of the absolute bright/dark levels.
    #[test]
    fn silhouette_edges_hit_the_true_boundary_for_sharp_and_soft_edges() {
        for (blur, tol) in [(1.2, 0.03), (4.0, 0.03)] {
            for k in 0..10 {
                let (a, b) = (283.0 + k as f64 * 0.1, 657.0 + k as f64 * 0.1);
                for (bright, dark) in [(195.0, 2.0), (120.0, 30.0)] {
                    let v = shaft_profile(880, a, b, blur, bright, dark);
                    let (l, r) = silhouette_edges(&v).unwrap();
                    assert!(
                        (l - a).abs() < tol && (r - b).abs() < tol,
                        "blur {blur} k {k}: {l} {r} vs {a} {b}"
                    );
                }
            }
        }
    }

    /// Dust specks outside the shaft (narrow dips, even deeper than the shaft's soft edges are
    /// steep) must not be taken for the silhouette.
    #[test]
    fn silhouette_edges_skip_dust_specks() {
        let mut v = shaft_profile(880, 283.4, 657.2, 4.0, 195.0, 3.0);
        for x in (98..101).chain(700..703).chain(30..33) {
            v[x] = 20.0;
        }
        let (l, r) = silhouette_edges(&v).unwrap();
        assert!(
            (l - 283.4).abs() < 0.05 && (r - 657.2).abs() < 0.05,
            "{l} {r}"
        );
    }

    #[test]
    fn silhouette_edges_need_a_real_dark_region() {
        assert!(silhouette_edges(&vec![190.0; 400]).is_none(), "no part");
        assert!(silhouette_edges(&[0.0; 50]).is_none(), "too short");
        let mut speck = vec![190.0; 400];
        speck[200..203].fill(20.0);
        assert!(silhouette_edges(&speck).is_none(), "only a speck");
        let low_contrast = shaft_profile(400, 100.0, 300.0, 2.0, 120.0, 110.0);
        assert!(
            silhouette_edges(&low_contrast).is_none(),
            "contrast below 20 gray levels"
        );
        let mut nan = shaft_profile(400, 100.0, 300.0, 2.0, 195.0, 3.0);
        nan[50] = f64::NAN;
        assert!(silhouette_edges(&nan).is_none());
    }
}
