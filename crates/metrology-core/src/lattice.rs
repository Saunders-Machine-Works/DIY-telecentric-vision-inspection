//! Reticle (checkerboard) lattice fit: system scale and apparent tilt from a PT-series
//! chrome-on-glass board, with no corner detector.
//!
//! The board's vertical grid lines are found by scanning rows for the regular train of
//! alternating edges (and horizontal grid lines by scanning columns), then fitted as
//!   vertical lines   x = x0 + i*p + t*y      horizontal lines   y = y0 + j*q + u*x
//! giving the lattice basis (per board square, in pixels)
//!   a1 = p/(1 - t*u) * (1, u)      a2 = q/(1 - t*u) * (t, 1).
//!
//! Orthographic (bi-telecentric) projection of a board tilted by theta maps squares through
//! rotation * diag(m, m*cos(theta)) * rotation, so the singular values of [a1 a2] are
//! (m*s, m*s*cos(theta)) for square size s: the *largest* is the true magnification and the
//! ratio gives the tilt. That holds only for tilt. Pixel aspect or lens anisotropy fixed in
//! image axes looks identical, and then sigma_max overstates the scale along one image axis
//! by up to the anisotropy (a 1.1 deg apparent tilt is a 1.8e-4 split). To tell the two
//! apart, refit with the board rotated by 90 degrees: real tilt turns with the board,
//! anisotropy stays in image axes. Until then use the full matrix [a1 a2], not just sigma_max.
//!
//! Each family also carries a rising/falling edge offset (bright-vs-dark edge-width bias
//! shifts the two in opposite directions); it is fitted and reported, and does not enter the
//! pitch. The indexed edge points are returned so a distortion map can use the residuals.

use crate::image::{ImageView, Roi};
use crate::linalg::least_squares;
use crate::periodic::{longest_regular_run, PeriodicError};
use crate::profile::{contrast_threshold, locate_edges, Polarity, Profile};

#[derive(Debug, Clone, Copy)]
pub struct LatticeParams {
    /// Expected square size in pixels (used to reject harmonics and irregular edges).
    pub nominal_pitch_px: f64,
    /// Distance between scan lines, pixels.
    pub line_step: usize,
    /// Allowed fractional deviation of an edge gap from the nominal pitch.
    pub tol: f64,
    /// Minimum edges in a run for a scan line to count.
    pub min_edges: usize,
    /// Edge threshold as a fraction of the line's 5-95 % intensity range.
    pub contrast_fraction: f64,
    /// Gaussian smoothing sigma for edge location, samples.
    pub sigma: f64,
    /// Only scan inside this region (default: the whole image). An invalid region is an
    /// error, never silently replaced by the whole image.
    pub roi: Option<Roi>,
}

impl Default for LatticeParams {
    fn default() -> Self {
        LatticeParams {
            nominal_pitch_px: 20.0,
            line_step: 12,
            tol: 0.2,
            min_edges: 10,
            contrast_fraction: 0.15,
            sigma: 1.0,
            roi: None,
        }
    }
}

/// One edge assigned to a grid line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatticePoint {
    /// Position along the scan direction, image pixels.
    pub s: f64,
    /// Scan-line coordinate across it, image pixels.
    pub c: f64,
    /// Grid-line index (integer, relative to an arbitrary origin).
    pub index: i64,
    pub rising: bool,
    /// Edge position minus the fitted grid line, pixels.
    pub residual: f64,
    /// Used by the linear lattice fit (false: kept for the distortion map only).
    pub used: bool,
}

/// Fit of one family of parallel grid lines:
/// s = offset + i*pitch + slope*(c - c_mean) + edge_bias*(+1 rising, -1 falling).
#[derive(Debug, Clone, PartialEq)]
pub struct AxisFit {
    /// Position of grid line 0 at the mean scan-line coordinate.
    pub offset: f64,
    /// Spacing between adjacent grid lines along the scan direction, pixels.
    pub pitch: f64,
    /// Drift of a grid line per pixel of scan-line coordinate (tan of its tilt).
    pub slope: f64,
    /// Half the shift of rising edges relative to falling edges, pixels.
    pub edge_bias: f64,
    /// RMS residual of the edge positions about the fitted lines, pixels.
    pub rms: f64,
    /// Edges used by the linear fit (the rest of `samples` were clipped from it).
    pub points: usize,
    pub clipped: usize,
    pub lines: usize,
    /// Mean scan-line coordinate the slope is referenced to.
    pub c_mean: f64,
    /// Every edge assigned to a grid line (within 0.3 pitch), with index and residual.
    pub samples: Vec<LatticePoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LatticeFit {
    /// Vertical grid lines, found by scanning along x.
    pub vertical: AxisFit,
    /// Horizontal grid lines, found by scanning along y.
    pub horizontal: AxisFit,
    /// Lattice basis vectors, pixels per board square.
    pub a1: (f64, f64),
    pub a2: (f64, f64),
    /// Largest / smallest singular value of [a1 a2], pixels per square.
    pub sigma_max: f64,
    pub sigma_min: f64,
    /// Apparent tilt from foreshortening, degrees (also absorbs pixel/lens anisotropy).
    pub tilt_deg: f64,
    /// Direction of a1 against the image x axis, degrees.
    pub rotation_deg: f64,
}

impl LatticeFit {
    /// Micrometres per pixel for a board whose squares are `square_mm` wide, assuming the
    /// apparent tilt is real (see the module docs).
    pub fn um_per_px(&self, square_mm: f64) -> f64 {
        square_mm * 1000.0 / self.sigma_max
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LatticeError {
    /// Not enough regular scan lines in a family.
    NotEnoughLines {
        family: &'static str,
        usable: usize,
        harmonic: usize,
        rejected: usize,
    },
    /// Could not estimate how the grid lines drift between scan lines.
    NoSlope { family: &'static str },
    /// The grid lines drift so far between scan lines that their indices would alias.
    SlopeAliased { family: &'static str, drift_px: f64 },
    /// The requested region is empty or outside the image.
    InvalidRoi,
    /// The parameters cannot work (e.g. scan lines further apart than 1.5 squares, which
    /// lets grid-line indices alias between lines).
    InvalidParams { reason: &'static str },
    /// Fewer than 20 edges survived index assignment and clipping.
    TooFewPoints { family: &'static str, kept: usize },
    /// The edges span too few distinct grid lines for a meaningful pitch.
    TooFewGridLines { family: &'static str, found: usize },
    /// The two families are degenerate (parallel) or the fit is singular.
    Degenerate,
}

struct Line {
    c: f64,
    s: Vec<f64>,
    rising: Vec<bool>,
    pitch: f64,
}

/// Minimum distinct grid lines a family must span.
const MIN_GRID_LINES: usize = 8;

/// Scan one family. `along_x`: scan rows (finding vertical grid lines) else columns.
fn scan_family(
    img: &ImageView,
    along_x: bool,
    p: &LatticeParams,
    r: Roi,
) -> Result<Vec<Line>, LatticeError> {
    let family = if along_x {
        "vertical grid lines (row scans)"
    } else {
        "horizontal grid lines (column scans)"
    };
    // s runs along the scan (from s_lo to s_hi), c across it (from c_lo to c_hi)
    let ((s_lo, s_hi), (c_lo, c_hi)) = if along_x {
        ((r.x0, r.x1), (r.y0, r.y1))
    } else {
        ((r.y0, r.y1), (r.x0, r.x1))
    };
    let (mut lines, mut harmonic, mut rejected) = (Vec::new(), 0usize, 0usize);
    let mut c = c_lo + p.line_step.max(1);
    while c + p.line_step < c_hi {
        if let Some(prof) = Profile::along_axis(img, along_x, c, s_lo, s_hi) {
            let thr = contrast_threshold(&prof.values, p.contrast_fraction);
            let edges: Vec<_> = locate_edges(&prof.values, p.sigma, thr, Polarity::Either)
                .into_iter()
                .filter(|e| !e.near_end)
                .collect();
            let pos: Vec<f64> = edges.iter().map(|e| e.position + s_lo as f64).collect();
            let rising: Vec<bool> = edges.iter().map(|e| e.rising).collect();
            match longest_regular_run(&pos, &rising, Some(p.nominal_pitch_px), p.tol, p.min_edges) {
                Ok(run) => lines.push(Line {
                    c: c as f64,
                    s: pos[run.first..=run.last].to_vec(),
                    rising: rising[run.first..=run.last].to_vec(),
                    pitch: run.pitch,
                }),
                Err(PeriodicError::Harmonic { .. }) => harmonic += 1,
                Err(_) => rejected += 1,
            }
        }
        c += p.line_step;
    }
    if lines.len() < 5 {
        return Err(LatticeError::NotEnoughLines {
            family,
            usable: lines.len(),
            harmonic,
            rejected,
        });
    }
    Ok(lines)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// Raw edge: (s, c, rising).
type RawPoint = (f64, f64, bool);

/// Model parameters: s = o + i*p + t*(c - cbar) + d*pol, with i taken relative to `ibar`.
#[derive(Clone, Copy)]
struct Model {
    o: f64,
    p: f64,
    t: f64,
    d: f64,
    ibar: f64,
}

impl Model {
    fn predict(&self, i: f64, c: f64, cbar: f64, rising: bool) -> f64 {
        self.o
            + (i - self.ibar) * self.p
            + self.t * (c - cbar)
            + if rising { self.d } else { -self.d }
    }
}

fn fit_family(
    lines: &[Line],
    p: &LatticeParams,
    family: &'static str,
) -> Result<AxisFit, LatticeError> {
    // 1. drift of grid lines between neighbouring scan lines (median over all matched edges)
    let max_dc = 2.5 * p.line_step as f64;
    let mut slopes = Vec::new();
    for w in lines.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let dc = b.c - a.c;
        if dc <= 0.0 || dc > max_dc {
            continue;
        }
        for &sa in &a.s {
            let nearest =
                b.s.iter()
                    .copied()
                    .min_by(|x, y| (x - sa).abs().total_cmp(&(y - sa).abs()));
            if let Some(sb) = nearest.filter(|sb| (sb - sa).abs() < 0.3 * p.nominal_pitch_px) {
                slopes.push((sb - sa) / dc);
            }
        }
    }
    if slopes.len() < 20 {
        return Err(LatticeError::NoSlope { family });
    }
    let t0 = median(&mut slopes);
    let drift = t0.abs() * max_dc;
    if drift > 0.3 * p.nominal_pitch_px {
        return Err(LatticeError::SlopeAliased {
            family,
            drift_px: drift,
        });
    }
    // 2. integer index of every edge relative to the longest line's first edge
    let mut pitches: Vec<f64> = lines.iter().map(|l| l.pitch).collect();
    let p0 = median(&mut pitches);
    let cbar = lines.iter().map(|l| l.c).sum::<f64>() / lines.len() as f64;
    let reference = lines.iter().max_by_key(|l| l.s.len()).expect("non-empty");
    let raw: Vec<RawPoint> = lines
        .iter()
        .flat_map(|l| l.s.iter().zip(&l.rising).map(move |(&s, &r)| (s, l.c, r)))
        .collect();
    let mut m = Model {
        o: reference.s[0] - t0 * (reference.c - cbar),
        p: p0,
        t: t0,
        d: 0.0,
        ibar: 0.0,
    };
    let mut kept: Vec<(RawPoint, f64)> = Vec::new();
    let mut last_gate = 0.3 * m.p;
    for iter in 0..12 {
        // gate: wide while the model is rough, then a robust clip around the current fit
        let gate = if iter < 3 {
            0.3 * m.p
        } else {
            let mut r: Vec<f64> = kept
                .iter()
                .map(|&(q, i)| (q.0 - m.predict(i, q.1, cbar, q.2)).abs())
                .collect();
            (5.0 * 1.4826 * median(&mut r)).max(0.05)
        };
        last_gate = gate;
        let prev = (m.p, kept.len());
        kept = raw
            .iter()
            .filter_map(|&q| {
                let i = ((q.0 - m.predict(0.0, q.1, cbar, q.2)) / m.p).round();
                ((q.0 - m.predict(i, q.1, cbar, q.2)).abs() < gate).then_some((q, i))
            })
            .collect();
        if kept.len() < 20 {
            return Err(LatticeError::TooFewPoints {
                family,
                kept: kept.len(),
            });
        }
        let ibar = kept.iter().map(|&(_, i)| i).sum::<f64>() / kept.len() as f64;
        let rows: Vec<Vec<f64>> = kept
            .iter()
            .map(|&(q, i)| vec![1.0, i - ibar, q.1 - cbar, if q.2 { 1.0 } else { -1.0 }])
            .collect();
        let ys: Vec<f64> = kept.iter().map(|&(q, _)| q.0).collect();
        let x = least_squares(&rows, &ys).ok_or(LatticeError::Degenerate)?;
        m = Model {
            o: x[0],
            p: x[1],
            t: x[2],
            d: x[3],
            ibar,
        };
        if iter >= 3 && (m.p - prev.0).abs() < 1e-10 && kept.len() == prev.1 {
            break;
        }
    }
    let indices: std::collections::BTreeSet<i64> = kept.iter().map(|&(_, i)| i as i64).collect();
    if indices.len() < MIN_GRID_LINES {
        return Err(LatticeError::TooFewGridLines {
            family,
            found: indices.len(),
        });
    }
    // Every edge that fits a grid line within 0.3 pitch is returned, flagged `used` if it also
    // survived the tight clip of the linear fit. The distortion map needs the clipped-out edges
    // (distortion makes the outer ones depart from a linear model) and clips on its own.
    let samples: Vec<LatticePoint> = raw
        .iter()
        .filter_map(|&q| {
            let i = ((q.0 - m.predict(0.0, q.1, cbar, q.2)) / m.p).round();
            let residual = q.0 - m.predict(i, q.1, cbar, q.2);
            (residual.abs() < 0.3 * m.p).then_some(LatticePoint {
                s: q.0,
                c: q.1,
                index: i as i64,
                rising: q.2,
                residual,
                used: residual.abs() < last_gate,
            })
        })
        .collect();
    let used = samples.iter().filter(|q| q.used).count().max(1);
    let rms = (samples
        .iter()
        .filter(|q| q.used)
        .map(|q| q.residual * q.residual)
        .sum::<f64>()
        / used as f64)
        .sqrt();
    Ok(AxisFit {
        offset: m.o - m.ibar * m.p,
        pitch: m.p,
        slope: m.t,
        edge_bias: m.d,
        rms,
        points: used,
        clipped: samples.len() - used,
        lines: lines.len(),
        c_mean: cbar,
        samples,
    })
}
/// Fit the checkerboard lattice in `img`.
///
/// Limits: `line_step` must be at least 1 and below 1.5 squares. The board may be rotated by
/// at most about atan(1 / min_edges) (~5.7 degrees for the default 10 edges): a scan line must
/// stay inside one row of squares for `min_edges` squares, and a rotated board makes it cross
/// into the next row sooner, so no usable run is found beyond that and the fit refuses with
/// `NotEnoughLines`. If runs are found but the grid lines drift more than 0.3 pitch between
/// scan lines (possible with a small `min_edges`), `SlopeAliased` is returned instead of risking
/// aliased grid-line indices. Sample indices are only meaningful within one fit (arbitrary
/// origin per fit and family) and residuals are along the scan direction.
pub fn fit_lattice(img: &ImageView, params: &LatticeParams) -> Result<LatticeFit, LatticeError> {
    if params.line_step == 0 || params.line_step as f64 >= 1.5 * params.nominal_pitch_px {
        return Err(LatticeError::InvalidParams {
            reason: "line_step must be 1..1.5 squares",
        });
    }
    let r = match params.roi {
        Some(r) if r.fits(img) => r,
        Some(_) => return Err(LatticeError::InvalidRoi),
        None => Roi::new(0, 0, img.width(), img.height()),
    };
    let v = fit_family(
        &scan_family(img, true, params, r)?,
        params,
        "vertical grid lines",
    )?;
    let h = fit_family(
        &scan_family(img, false, params, r)?,
        params,
        "horizontal grid lines",
    )?;
    let (p, t, q, u) = (v.pitch, v.slope, h.pitch, h.slope);
    let k = 1.0 - t * u;
    if k.abs() < 1e-6 {
        return Err(LatticeError::Degenerate);
    }
    let (a1, a2) = ((p / k, p / k * u), (q / k * t, q / k));
    let s = a1.0 * a1.0 + a1.1 * a1.1 + a2.0 * a2.0 + a2.1 * a2.1;
    let det = (a1.0 * a2.1 - a1.1 * a2.0).abs();
    let disc = (s * s - 4.0 * det * det).max(0.0).sqrt();
    let (sigma_max, sigma_min) = (
        ((s + disc) / 2.0).sqrt(),
        ((s - disc) / 2.0).max(0.0).sqrt(),
    );
    Ok(LatticeFit {
        vertical: v,
        horizontal: h,
        a1,
        a2,
        sigma_max,
        sigma_min,
        tilt_deg: (sigma_min / sigma_max).clamp(0.0, 1.0).acos().to_degrees(),
        rotation_deg: a1.1.atan2(a1.0).to_degrees(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{basis, render, render_g};

    fn fit(a1: (f64, f64), a2: (f64, f64), noise: f64) -> Result<LatticeFit, LatticeError> {
        let (w, h) = (700, 520);
        let px = render(w, h, a1, a2, (350.0, 260.0), (28.0, 20.0), 1.0, noise);
        let img = ImageView::new(w, h, &px).unwrap();
        fit_lattice(
            &img,
            &LatticeParams {
                nominal_pitch_px: 14.0,
                line_step: 8,
                ..Default::default()
            },
        )
    }

    #[test]
    fn rotated_untilted_board_gives_true_pitch_and_zero_tilt() {
        let (a1, a2) = basis(14.0, 1.5, 0.0, 0.0);
        let f = fit(a1, a2, 0.6).unwrap();
        assert!(
            (f.sigma_max - 14.0).abs() / 14.0 < 3e-4,
            "sigma_max {}",
            f.sigma_max
        );
        assert!(f.tilt_deg < 0.5, "tilt {}", f.tilt_deg);
        assert!(
            (f.rotation_deg - 1.5).abs() < 0.03,
            "rotation {}",
            f.rotation_deg
        );
        assert!(
            f.vertical.rms < 0.08 && f.horizontal.rms < 0.08,
            "rms {} {}",
            f.vertical.rms,
            f.horizontal.rms
        );
    }

    /// 4 degrees of tilt about an oblique axis compresses one direction by cos(4 deg) = 0.99756.
    /// The true scale is the largest singular value, not the mean of the x and y pitches.
    #[test]
    fn tilted_board_recovers_true_scale_and_tilt() {
        let (a1, a2) = basis(14.0, 1.5, 4.0, 25.0);
        let f = fit(a1, a2, 0.6).unwrap();
        assert!(
            (f.sigma_max - 14.0).abs() / 14.0 < 3e-4,
            "sigma_max {}",
            f.sigma_max
        );
        assert!((f.tilt_deg - 4.0).abs() < 0.5, "tilt {}", f.tilt_deg);
        let naive = 0.5 * (f.vertical.pitch + f.horizontal.pitch);
        assert!(
            (naive - 14.0).abs() > 0.005,
            "the naive mean pitch {naive} is biased by the tilt"
        );
    }

    #[test]
    fn single_frame_noise_does_not_break_the_fit() {
        let (a1, a2) = basis(14.0, -2.0, 2.0, 70.0);
        let f = fit(a1, a2, 1.6).unwrap();
        assert!(
            (f.sigma_max - 14.0).abs() / 14.0 < 5e-4,
            "sigma_max {}",
            f.sigma_max
        );
    }

    #[test]
    fn a_wrong_nominal_pitch_is_rejected_as_a_harmonic_not_accepted() {
        let (a1, a2) = basis(14.0, 0.5, 0.0, 0.0);
        let (w, h) = (700, 520);
        let px = render(w, h, a1, a2, (350.0, 260.0), (28.0, 20.0), 1.0, 0.5);
        let img = ImageView::new(w, h, &px).unwrap();
        let err = fit_lattice(
            &img,
            &LatticeParams {
                nominal_pitch_px: 28.0,
                line_step: 8,
                ..Default::default()
            },
        )
        .unwrap_err();
        match err {
            LatticeError::NotEnoughLines {
                harmonic, usable, ..
            } => assert!(harmonic > 10 && usable == 0, "{harmonic} {usable}"),
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn a_blank_image_has_no_lattice() {
        let px = vec![200.0f32; 300 * 200];
        let img = ImageView::new(300, 200, &px).unwrap();
        assert!(matches!(
            fit_lattice(&img, &LatticeParams::default()),
            Err(LatticeError::NotEnoughLines { usable: 0, .. })
        ));
    }

    #[test]
    fn general_solver_and_least_squares_recover_known_parameters() {
        // s = 5 + 2*i + 0.1*(c - 10)
        let pts: Vec<(f64, f64, f64)> = (0..10)
            .flat_map(|i| {
                (0..5).map(move |k| {
                    (
                        5.0 + 2.0 * i as f64 + 0.1 * (k as f64 * 3.0),
                        k as f64 * 3.0 + 10.0,
                        i as f64,
                    )
                })
            })
            .collect();
        let rows: Vec<Vec<f64>> = pts.iter().map(|q| vec![1.0, q.2, q.1 - 10.0]).collect();
        let ys: Vec<f64> = pts.iter().map(|q| q.0).collect();
        let x = least_squares(&rows, &ys).unwrap();
        assert!(
            (x[0] - 5.0).abs() < 1e-9 && (x[1] - 2.0).abs() < 1e-9 && (x[2] - 0.1).abs() < 1e-9,
            "{x:?}"
        );
    }

    #[test]
    fn an_invalid_region_is_an_error_not_the_whole_image() {
        let (a1, a2) = basis(14.0, 0.5, 0.0, 0.0);
        let (w, h) = (700, 520);
        let px = render(w, h, a1, a2, (350.0, 260.0), (28.0, 20.0), 1.0, 0.5);
        let img = ImageView::new(w, h, &px).unwrap();
        let base = LatticeParams {
            nominal_pitch_px: 14.0,
            line_step: 8,
            ..Default::default()
        };
        for bad in [
            Roi::new(0, 0, 701, 100),
            Roi::new(50, 50, 50, 300),
            Roi::new(900, 10, 950, 200),
        ] {
            let r = fit_lattice(
                &img,
                &LatticeParams {
                    roi: Some(bad),
                    ..base
                },
            );
            assert_eq!(r, Err(LatticeError::InvalidRoi), "{bad:?}");
        }
    }

    /// Rotation limit with the default min_edges = 10 (14 px squares, scan every 8 px): a scan
    /// row must stay in one row of squares for 10 squares, so ~atan(1/10) = 5.7 degrees is the
    /// most that fits. 4 degrees fits; 9 and 30 degrees are refused as NotEnoughLines.
    #[test]
    fn rotation_limit_follows_min_edges() {
        let (a1, a2) = basis(14.0, 4.0, 0.0, 0.0);
        let ok = fit(a1, a2, 0.5).unwrap();
        assert!(
            (ok.sigma_max - 14.0).abs() / 14.0 < 5e-4,
            "4 deg: {}",
            ok.sigma_max
        );
        assert!((ok.rotation_deg - 4.0).abs() < 0.1, "{}", ok.rotation_deg);
        for deg in [9.0, 30.0] {
            let (a1, a2) = basis(14.0, deg, 0.0, 0.0);
            let r = fit(a1, a2, 0.5);
            assert!(
                matches!(r, Err(LatticeError::NotEnoughLines { usable: 0, .. })),
                "{deg} deg: {r:?}"
            );
        }
    }

    /// With a small min_edges, runs are found at larger rotations; then the grid lines drift
    /// 4.6 px (> 0.3 pitch) between scan-line pairs at 13 degrees and the guard must refuse.
    #[test]
    fn drift_guard_refuses_when_runs_exist_but_lines_would_alias() {
        let (a1, a2) = basis(14.0, 13.0, 0.0, 0.0);
        let (w, h) = (700, 520);
        let px = render(w, h, a1, a2, (350.0, 260.0), (28.0, 20.0), 1.0, 0.5);
        let img = ImageView::new(w, h, &px).unwrap();
        let p = LatticeParams {
            nominal_pitch_px: 14.0,
            line_step: 8,
            min_edges: 4,
            ..Default::default()
        };
        let r = fit_lattice(&img, &p);
        assert!(
            matches!(r, Err(LatticeError::SlopeAliased { drift_px, .. }) if drift_px > 4.2),
            "{r:?}"
        );
    }

    #[test]
    fn scan_lines_too_far_apart_are_rejected_up_front() {
        let px = vec![100.0f32; 100 * 100];
        let img = ImageView::new(100, 100, &px).unwrap();
        for bad in [0usize, 30, 21] {
            let r = fit_lattice(
                &img,
                &LatticeParams {
                    nominal_pitch_px: 14.0,
                    line_step: bad,
                    ..Default::default()
                },
            );
            assert!(
                matches!(r, Err(LatticeError::InvalidParams { .. })),
                "line_step {bad}: {r:?}"
            );
        }
    }

    #[test]
    fn indexed_samples_are_exposed_and_consistent() {
        let (a1, a2) = basis(14.0, 1.0, 0.0, 0.0);
        let f = fit(a1, a2, 0.5).unwrap();
        for fam in [&f.vertical, &f.horizontal] {
            // samples hold every gate-passed edge; `used` marks those in the linear fit
            assert_eq!(fam.samples.len(), fam.points + fam.clipped);
            assert_eq!(fam.samples.iter().filter(|s| s.used).count(), fam.points);
            let rms = (fam
                .samples
                .iter()
                .filter(|s| s.used)
                .map(|s| s.residual * s.residual)
                .sum::<f64>()
                / fam.points as f64)
                .sqrt();
            assert!((rms - fam.rms).abs() < 1e-12);
            let (lo, hi) = fam.samples.iter().fold((i64::MAX, i64::MIN), |a, s| {
                (a.0.min(s.index), a.1.max(s.index))
            });
            assert!(hi - lo >= 20, "index span {}", hi - lo);
            assert!(fam.samples.iter().any(|s| s.rising) && fam.samples.iter().any(|s| !s.rising));
        }
    }

    /// Dark squares grown by 0.05 square (0.7 px) shift rising and falling edges by 0.7 px in
    /// opposite directions. The fit must report that as edge_bias (~0.7 px) and keep the pitch.
    #[test]
    fn rising_falling_edge_bias_is_fitted_and_does_not_disturb_the_pitch() {
        let (a1, a2) = basis(14.0, 1.0, 0.0, 0.0);
        let (w, h) = (700, 520);
        let px = render_g(
            w,
            h,
            a1,
            a2,
            (350.0, 260.0),
            (28.0, 20.0),
            1.0,
            0.5,
            0.05,
            0.0,
        );
        let img = ImageView::new(w, h, &px).unwrap();
        let f = fit_lattice(
            &img,
            &LatticeParams {
                nominal_pitch_px: 14.0,
                line_step: 8,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            (f.sigma_max - 14.0).abs() / 14.0 < 3e-4,
            "sigma_max {}",
            f.sigma_max
        );
        assert!(
            (f.vertical.edge_bias.abs() - 0.7).abs() < 0.08,
            "bias {}",
            f.vertical.edge_bias
        );
        assert!(
            f.horizontal.edge_bias.abs() < 0.1,
            "other family unaffected: {}",
            f.horizontal.edge_bias
        );
        // Without the bias term this residual would be ~0.7 px; with it the remainder is edge
        // noise (a little higher than the plain board: grown dark squares leave narrower light ones).
        assert!(
            f.vertical.rms < 0.1,
            "bias no longer inflates the residual: {}",
            f.vertical.rms
        );
    }

    /// A region-limited fit must see only its region: the scan positions stay in full-image
    /// coordinates, so the same board gives the same pitch, and a region with no board has no lattice.
    #[test]
    fn region_limited_fit_matches_the_whole_board_and_respects_the_region() {
        let (a1, a2) = basis(14.0, 1.0, 0.0, 0.0);
        let (w, h) = (700, 520);
        let px = render(w, h, a1, a2, (350.0, 260.0), (28.0, 20.0), 1.0, 0.5);
        let img = ImageView::new(w, h, &px).unwrap();
        let base = LatticeParams {
            nominal_pitch_px: 14.0,
            line_step: 8,
            ..Default::default()
        };
        let whole = fit_lattice(&img, &base).unwrap();
        // the board spans x 154..546, y 120..400
        let left = fit_lattice(
            &img,
            &LatticeParams {
                roi: Some(Roi::new(160, 125, 350, 395)),
                ..base
            },
        )
        .unwrap();
        assert!(
            (left.sigma_max - whole.sigma_max).abs() < 0.01,
            "{} vs {}",
            left.sigma_max,
            whole.sigma_max
        );
        let all_x_in_region =
            left.vertical.points > 100 && left.vertical.points < whole.vertical.points;
        assert!(
            all_x_in_region,
            "region fit must use fewer points: {} vs {}",
            left.vertical.points, whole.vertical.points
        );
        let empty = fit_lattice(
            &img,
            &LatticeParams {
                roi: Some(Roi::new(10, 10, 140, 110)),
                ..base
            },
        );
        assert!(
            matches!(empty, Err(LatticeError::NotEnoughLines { usable: 0, .. })),
            "{empty:?}"
        );
    }
}
