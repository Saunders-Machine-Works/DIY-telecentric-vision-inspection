//! Thread dimensions from the two silhouette sides of a threaded shaft.
//!
//! Input: the left and right edge points of the silhouette in millimetres (one pair per scan
//! row), already converted through the field map. The shaft axis is fitted from the midpoints;
//! each point becomes (axial position t, radius r) about that axis.
//!
//! Silhouette-defined quantities (per side, over whole pitches only):
//!   silhouette level       = the radius at which the silhouette spends half the period above it
//!                            (the median of r). Not a gauge pitch diameter, see below.
//!   major / minor radius   = mean of the outermost / innermost 8 % of the samples (a robust
//!                            stand-in for the crest and root flats).
//!   pitch                  = slope of the crossing positions of the pitch level against
//!                            crossing number (2 crossings per pitch).
//! Diameters are the sums of the left and right radii, so an off-centre axis cancels.
//!
//! `pd_silhouette_mm` is only a statistic of the silhouette: the silhouette of a helical thread is
//! the union of its azimuthal sections, not the axial profile, and it reads high by twice the
//! helix-envelope raise (see `thread_flank`). The pitch diameter to compare with a certificate is
//! `pd_mm`, from straight-line fits to the flanks with that raise removed. Neither applies an
//! edge-bias correction: edges are the 50 % crossings of the silhouette, which a backlit,
//! narrow-angle-lit plain pin showed to be unbiased to about 1 um.

use crate::linalg::least_squares;
use crate::thread_flank::{self, FlankError, FlankFit};

/// Smallest number of full pitches (per side) we accept.
pub const MIN_PERIODS: usize = 3;
/// Fraction of samples averaged for the crest and root levels.
const EXTREME_FRACTION: f64 = 0.08;

#[derive(Debug, Clone, PartialEq)]
pub enum ThreadError {
    /// Left and right must hold the same number (>= 20) of rows.
    BadInput,
    /// No usable axis (degenerate midpoints).
    NoAxis,
    /// A side has fewer than MIN_PERIODS full pitches.
    TooFewPeriods { side: &'static str, found: usize },
    /// The two sides disagree about the pitch by more than 2 %.
    PitchMismatch { left_mm: f64, right_mm: f64 },
    /// The flank fit of a side failed (no usable straight flanks).
    Flank { side: &'static str, why: FlankError },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SideStats {
    pub pd_radius_mm: f64,
    pub major_radius_mm: f64,
    pub minor_radius_mm: f64,
    pub pitch_mm: f64,
    /// Full pitches analysed on this side.
    pub periods: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreadFit {
    pub major_mm: f64,
    pub minor_mm: f64,
    /// Silhouette-defined statistic, NOT a pitch diameter (see the module docs).
    pub pd_silhouette_mm: f64,
    /// Pitch diameter from the flank fits with the helix-envelope raise removed, mm.
    pub pd_mm: f64,
    pub pitch_mm: f64,
    pub tpi: f64,
    /// Angle of the fitted axis from the image y axis, degrees.
    pub axis_tilt_deg: f64,
    pub left: SideStats,
    pub right: SideStats,
    pub left_flank: FlankFit,
    pub right_flank: FlankFit,
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

/// Positions where the profile crosses `level`, linearly interpolated, with the crossing direction.
fn crossings(t: &[f64], r: &[f64], level: f64) -> Vec<(f64, bool)> {
    let mut out = Vec::new();
    for k in 0..t.len() - 1 {
        let (a, b) = (r[k] - level, r[k + 1] - level);
        if (a < 0.0 && b >= 0.0) || (a >= 0.0 && b < 0.0) {
            out.push((t[k] + (t[k + 1] - t[k]) * (-a) / (b - a), b >= 0.0));
        }
    }
    out
}

/// One side's analysis: the statistics, and which samples form its crests.
struct SideAnalysis {
    stats: SideStats,
    /// Indices (into the side's arrays) of the samples near the top of each pitch.
    crest: Vec<usize>,
    /// Crossings of the pitch level (position, rising?), for the flank fit.
    crossings: Vec<(f64, bool)>,
}

fn analyse_side(side: &'static str, t: &[f64], r: &[f64]) -> Result<SideAnalysis, ThreadError> {
    // pitch level: median over everything first, refined over whole periods
    let mut level = median(r);
    let mut window = (t[0], t[t.len() - 1]);
    let (mut pitch, mut periods) = (0.0, 0);
    let mut cr = Vec::new();
    let mut last_cross = 0usize;
    for _ in 0..3 {
        cr = crossings(t, r, level);
        if cr.len() < 2 * MIN_PERIODS + 1 {
            return Err(ThreadError::TooFewPeriods {
                side,
                found: cr.len() / 2,
            });
        }
        // crossings alternate in direction; an even span of them covers whole pitches
        let last = (cr.len() - 1) / 2 * 2;
        periods = last / 2;
        if periods < MIN_PERIODS {
            return Err(ThreadError::TooFewPeriods {
                side,
                found: periods,
            });
        }
        last_cross = last;
        window = (cr[0].0, cr[last].0);
        // pitch from the flank positions (robust to a tooth that never reaches the level)
        pitch = thread_flank::number_flanks(&thread_flank::debounce(&cr[..=last]))
            .map_err(|_| ThreadError::TooFewPeriods { side, found: 0 })?
            .1;
        let inside: Vec<f64> = t
            .iter()
            .zip(r)
            .filter(|(tt, _)| **tt >= window.0 && **tt <= window.1)
            .map(|(_, rr)| *rr)
            .collect();
        if inside.is_empty() {
            return Err(ThreadError::TooFewPeriods { side, found: 0 });
        }
        level = median(&inside);
    }
    let mut inside: Vec<f64> = t
        .iter()
        .zip(r)
        .filter(|(tt, _)| **tt >= window.0 && **tt <= window.1)
        .map(|(_, rr)| *rr)
        .collect();
    inside.sort_by(f64::total_cmp);
    let k = ((inside.len() as f64 * EXTREME_FRACTION).ceil() as usize).max(1);
    let mean = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
    // crest samples: within each pitch, the samples in the top 10 % of that pitch's own range,
    // so a slightly wrong axis (radius drifting along the shaft) cannot starve the crests of
    // some pitches
    let mut crest = Vec::new();
    for j in (0..last_cross).step_by(2) {
        let (lo, hi) = (cr[j].0, cr[j + 2].0);
        // t ascends with the scan rows
        let idx: Vec<usize> =
            (t.partition_point(|&x| x < lo)..t.partition_point(|&x| x < hi)).collect();
        if idx.is_empty() {
            continue;
        }
        let (mn, mx) = idx
            .iter()
            .fold((f64::MAX, f64::MIN), |a, &i| (a.0.min(r[i]), a.1.max(r[i])));
        crest.extend(idx.into_iter().filter(|&i| r[i] >= mx - 0.1 * (mx - mn)));
    }
    Ok(SideAnalysis {
        stats: SideStats {
            pd_radius_mm: level,
            major_radius_mm: mean(&inside[inside.len() - k..]),
            minor_radius_mm: mean(&inside[..k]),
            pitch_mm: pitch,
            periods,
        },
        crest,
        crossings: cr,
    })
}

/// Axis X = a + b*Y as an (a, b) pair.
pub(crate) type Axis = (f64, f64);

/// Project points about `axis`: (axial position, radius), radius positive away from the axis
/// on the side given by `sign` (-1 left, +1 right).
pub(crate) fn project(pts: &[(f64, f64)], axis: Axis, sign: f64) -> (Vec<f64>, Vec<f64>) {
    let norm = (1.0 + axis.1 * axis.1).sqrt();
    let (d, n) = ((axis.1 / norm, 1.0 / norm), (1.0 / norm, -axis.1 / norm));
    pts.iter()
        .map(|p| {
            let (dx, dy) = (p.0 - axis.0, p.1);
            (dx * d.0 + dy * d.1, sign * (dx * n.0 + dy * n.1))
        })
        .unzip()
}

/// Least-squares line X = c0 + c1*Y through points, `None` if degenerate.
pub(crate) fn fit_line(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    let rows: Vec<Vec<f64>> = pts.iter().map(|p| vec![1.0, p.1]).collect();
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    least_squares(&rows, &xs).map(|c| (c[0], c[1]))
}

/// Axis from the crest points of the two sides, which are parallel to the axis and free of the
/// helix ripple that biases the midpoint line: the mean slope of the two crest lines, centred
/// between them. `None` with fewer than 10 points on a side or a degenerate fit.
pub(crate) fn axis_from_crests(lc: &[(f64, f64)], rc: &[(f64, f64)]) -> Option<Axis> {
    if lc.len() < 10 || rc.len() < 10 {
        return None;
    }
    let (l, r) = (fit_line(lc)?, fit_line(rc)?);
    let b = 0.5 * (l.1 + r.1);
    let ymean = |v: &[(f64, f64)]| v.iter().map(|p| p.1).sum::<f64>() / v.len() as f64;
    let (ymean_l, ymean_r) = (ymean(lc), ymean(rc));
    let a_l = l.0 + l.1 * ymean_l - b * ymean_l;
    let a_r = r.0 + r.1 * ymean_r - b * ymean_r;
    Some((0.5 * (a_l + a_r), b))
}

/// Measure a vertical-ish threaded shaft from its left and right silhouette points (mm), one
/// pair per scan row in the same order (ascending image y).
///
/// The axis is first taken from the row midpoints, which carry a ripple because the two sides of
/// a helical thread are half a pitch out of phase (biased unless the window spans whole
/// pitches); it is then refined from the crest lines of the two sides, which are parallel to the
/// axis and free of that ripple, and everything is recomputed about the refined axis.
pub fn measure_thread(left: &[(f64, f64)], right: &[(f64, f64)]) -> Result<ThreadFit, ThreadError> {
    if left.len() != right.len()
        || left.len() < 20
        || left
            .iter()
            .chain(right)
            .any(|p| !p.0.is_finite() || !p.1.is_finite())
    {
        return Err(ThreadError::BadInput);
    }
    let mid: Vec<(f64, f64)> = left
        .iter()
        .zip(right)
        .map(|(l, r)| (0.5 * (l.0 + r.0), 0.5 * (l.1 + r.1)))
        .collect();
    let mut axis = fit_line(&mid).ok_or(ThreadError::NoAxis)?;
    for _ in 0..3 {
        let ((tl, rl), (tr, rr)) = (project(left, axis, -1.0), project(right, axis, 1.0));
        let (la, ra) = (
            analyse_side("left", &tl, &rl)?,
            analyse_side("right", &tr, &rr)?,
        );
        let lc: Vec<(f64, f64)> = la.crest.iter().map(|&i| left[i]).collect();
        let rc: Vec<(f64, f64)> = ra.crest.iter().map(|&i| right[i]).collect();
        let next = axis_from_crests(&lc, &rc).unwrap_or(axis);
        let moved = (next.1 - axis.1).abs();
        axis = next;
        if moved < 1e-9 {
            break;
        }
    }
    // final statistics about the converged axis
    let ((tl, rl), (tr, rr)) = (project(left, axis, -1.0), project(right, axis, 1.0));
    let (la, ra) = (
        analyse_side("left", &tl, &rl)?,
        analyse_side("right", &tr, &rr)?,
    );
    let (ls, rs) = (la.stats, ra.stats);
    if (ls.pitch_mm - rs.pitch_mm).abs() > 0.02 * ls.pitch_mm.abs() {
        return Err(ThreadError::PitchMismatch {
            left_mm: ls.pitch_mm,
            right_mm: rs.pitch_mm,
        });
    }
    // the crossings were found at the level of the last refinement pass, one step before the
    // final median level in `stats`; the flank fit only uses them to number the flanks
    let flank = |side: &'static str, t: &[f64], r: &[f64], a: &SideAnalysis| {
        thread_flank::fit(
            t,
            r,
            &a.crossings,
            a.stats.minor_radius_mm,
            a.stats.major_radius_mm,
        )
        .map_err(|why| ThreadError::Flank { side, why })
    };
    let (left_flank, right_flank) = (
        flank("left", &tl, &rl, &la)?,
        flank("right", &tr, &rr, &ra)?,
    );
    let pitch = 0.5 * (ls.pitch_mm + rs.pitch_mm);
    Ok(ThreadFit {
        major_mm: ls.major_radius_mm + rs.major_radius_mm,
        minor_mm: ls.minor_radius_mm + rs.minor_radius_mm,
        pd_silhouette_mm: ls.pd_radius_mm + rs.pd_radius_mm,
        pd_mm: left_flank.pd_radius_mm + right_flank.pd_radius_mm,
        pitch_mm: pitch,
        tpi: 25.4 / pitch,
        axis_tilt_deg: axis.1.atan().to_degrees(),
        left: ls,
        right: rs,
        left_flank,
        right_flank,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_synth::{
        analytic_pd_radius, helical_shaft, helical_shaft_with, pd_radius_of, shaft,
    };

    /// The flank-based pitch diameter recovers the true pitch diameter of a helical thread, whose
    /// silhouette statistic reads about twice the helix raise (~50 um on 1/2-13) too high.
    #[test]
    fn helical_silhouette_pd_is_recovered_by_the_flank_fit() {
        // (pitch, major radius, minor radius, row spacing): the last two are small threads (#10-32
        // and M5-like) at the 30 um row spacing the camera gives
        for (p, rmaj, rmin, dy) in [
            (25.4 / 13.0, 6.35, 5.25, 0.01),
            (1.5, 6.0, 5.1, 0.01),
            (25.4 / 32.0, 2.4, 2.0, 0.03),
            (0.8, 2.5, 2.1, 0.03),
        ] {
            // a window that is not a whole number of pitches
            let rows = (11.37 * p / dy) as usize;
            let (l, r) = helical_shaft(p, rmaj, rmin, 0.0, rows, dy, 0.0);
            let f = measure_thread(&l, &r).unwrap();
            let truth = 2.0 * analytic_pd_radius(p, rmaj, rmin);
            let raised = f.pd_silhouette_mm - truth;
            assert!(
                raised > 0.5 * (f.left_flank.helix_shift_mm + f.right_flank.helix_shift_mm),
                "silhouette PD not raised: {raised}"
            );
            // the helix raise is 25 / 19 / 9 um per side here; the flank fit leaves < 1 um
            assert!(
                (f.pd_mm - truth).abs() < 1.0e-3,
                "p {p}: flank PD {} vs truth {truth}",
                f.pd_mm
            );
            assert!((f.pitch_mm - p).abs() < 3e-4);
        }
    }

    /// One tooth on one side never reaches the pitch level (nick, burr): the measurement still
    /// works end to end, with the right pitch and pitch diameter, instead of being refused.
    #[test]
    fn a_notched_tooth_on_one_side_is_measured_through() {
        let (p, rmaj, rmin) = (25.4 / 13.0, 6.35, 5.25);
        let (mut l, r) = helical_shaft(p, rmaj, rmin, 0.0, 700, 0.02, 0.0);
        // flatten the left side to the root over one whole tooth (rows 0.02 mm apart, axis x = 5)
        let (c0, c1) = (3.0 * p - 0.5 * p, 3.0 * p + 0.5 * p);
        for pt in l.iter_mut().filter(|pt| pt.1 > c0 && pt.1 < c1) {
            pt.0 = 5.0 - rmin;
        }
        let f = measure_thread(&l, &r).unwrap();
        let truth = 2.0 * analytic_pd_radius(p, rmaj, rmin);
        assert!(
            (f.pd_mm - truth).abs() < 2e-3,
            "flank PD {} vs truth {truth}",
            f.pd_mm
        );
        assert!((f.pitch_mm - p).abs() < 3e-4, "pitch {} vs {p}", f.pitch_mm);
    }

    /// Tilt and noise (5 um, ~10x the measured edge noise) must not move the flank PD either.
    #[test]
    fn flank_pd_survives_tilt_and_noise() {
        let (p, rmaj, rmin) = (25.4 / 13.0, 6.35, 5.25);
        let truth = 2.0 * analytic_pd_radius(p, rmaj, rmin);
        for (tilt, noise) in [(2.0, 0.0), (-1.5, 0.0), (0.5, 0.005)] {
            let (l, r) = helical_shaft(p, rmaj, rmin, tilt, 700, 0.02, noise);
            let f = measure_thread(&l, &r).unwrap();
            assert!(
                (f.pd_mm - truth).abs() < 3e-3,
                "tilt {tilt} noise {noise}: flank PD {} vs truth {truth}",
                f.pd_mm
            );
        }
    }

    /// A buttress-like profile (19.5 and 44.3 degree flanks): each flank gets its own angle and its
    /// own helix correction, and the rising/falling flanks are not swapped.
    #[test]
    fn asymmetric_flanks_are_measured_separately() {
        let p = 2.0;
        let (rmaj, rmin) = (6.0, 4.9); // depth 1.1 mm
                                       // per pitch: root flat 0.15, rising flank 0.2, crest flat 0.1, falling flank 0.55
        let prof = move |z: f64| {
            let u = z.rem_euclid(p) / p;
            if u < 0.15 {
                rmin
            } else if u < 0.35 {
                rmin + (rmaj - rmin) * (u - 0.15) / 0.2
            } else if u < 0.45 {
                rmaj
            } else {
                rmaj - (rmaj - rmin) * (u - 0.45) / 0.55
            }
        };
        let (l, r) = helical_shaft_with(p, &prof, 0.0, 800, 0.0125, 0.0);
        let f = measure_thread(&l, &r).unwrap();
        let truth = 2.0 * pd_radius_of(&prof, p);
        let (rise, fall) = (
            (0.2 * p / 1.1_f64).atan().to_degrees(),
            (0.55 * p / 1.1_f64).atan().to_degrees(),
        );
        for side in [f.left_flank, f.right_flank] {
            assert!(
                (side.rise_half_angle_deg - rise).abs() < 1.5,
                "rise {} vs {rise}",
                side.rise_half_angle_deg
            );
            assert!(
                (side.fall_half_angle_deg - fall).abs() < 1.5,
                "fall {} vs {fall}",
                side.fall_half_angle_deg
            );
        }
        assert!(
            (f.pd_mm - truth).abs() < 4e-3,
            "flank PD {} vs truth {truth}",
            f.pd_mm
        );
    }

    /// A crest flat wider than the tooth/groove balance puts the pitch radius outside the flanks;
    /// refuse it instead of extrapolating the lines.
    #[test]
    fn a_pitch_radius_outside_the_flanks_is_refused() {
        let p = 1.5;
        // crest flat 0.7 P, flanks 0.1 P each, root flat 0.1 P: the tooth is always wider than P/2
        let prof = move |z: f64| {
            let u = z.rem_euclid(p) / p;
            if u < 0.7 {
                6.0
            } else if u < 0.8 {
                6.0 - (u - 0.7) / 0.1
            } else if u < 0.9 {
                5.0
            } else {
                5.0 + (u - 0.9) / 0.1
            }
        };
        let (l, r) = helical_shaft_with(p, &prof, 0.0, 700, 0.02, 0.0);
        assert!(
            matches!(
                measure_thread(&l, &r),
                Err(ThreadError::Flank {
                    why: FlankError::NoPitchCylinder,
                    ..
                })
            ),
            "{:?}",
            measure_thread(&l, &r)
        );
    }

    /// Flank angles come out as those of the profile (here 29.x degrees, the generator's slope).
    #[test]
    fn flank_angles_match_the_profile() {
        let (p, rmaj, rmin) = (25.4 / 13.0, 6.35, 5.25);
        let (l, r) = helical_shaft(p, rmaj, rmin, 0.0, 700, 0.02, 0.0);
        let f = measure_thread(&l, &r).unwrap();
        let want = (0.3125 * p / (rmaj - rmin)).atan().to_degrees();
        for a in [
            f.left_flank.rise_half_angle_deg,
            f.left_flank.fall_half_angle_deg,
            f.right_flank.rise_half_angle_deg,
            f.right_flank.fall_half_angle_deg,
        ] {
            assert!((a - want).abs() < 1.0, "flank angle {a} vs {want}");
        }
    }

    /// A profile with no straight flanks (a square wave) is refused rather than given a PD.
    #[test]
    fn a_thread_without_flanks_is_refused() {
        let p = 1.5;
        let sq = |z: f64| if z.rem_euclid(p) < 0.5 * p { 6.0 } else { 5.0 };
        let (mut l, mut r) = (Vec::new(), Vec::new());
        for k in 0..600 {
            let t = k as f64 * 0.02;
            l.push((5.0 - sq(t), t));
            r.push((5.0 + sq(t + 0.5 * p), t));
        }
        assert_eq!(
            measure_thread(&l, &r),
            Err(ThreadError::Flank {
                side: "left",
                why: FlankError::TooFewSamples {
                    rising: 0,
                    falling: 0
                }
            })
        );
    }

    /// 1/2-13: pitch 1.9538 mm, major 12.7 mm, minor about 10.5 mm (radii 6.35 / 5.25).
    #[test]
    fn recovers_major_minor_pitch_diameter_pitch_and_tpi_of_an_ideal_thread() {
        let p = 25.4 / 13.0;
        let (rmaj, rmin) = (6.35, 5.25);
        let (l, r) = shaft(p, rmaj, rmin, 0.0, 700, 0.02, 0.0);
        let f = measure_thread(&l, &r).unwrap();
        let pd = 2.0 * analytic_pd_radius(p, rmaj, rmin);
        assert!((f.major_mm - 12.7).abs() < 2e-3, "major {}", f.major_mm);
        // major/minor are means of the top/bottom 8 % of the samples: with discrete rows and a
        // window that is not exactly whole pitches they sit ~1.4 / 2.5 um off the flats
        assert!((f.minor_mm - 10.5).abs() < 3.5e-3, "minor {}", f.minor_mm);
        assert!(
            (f.pd_silhouette_mm - pd).abs() < 2e-3,
            "pd {} vs analytic {pd}",
            f.pd_silhouette_mm
        );
        assert!((f.pitch_mm - p).abs() < 2e-4, "pitch {}", f.pitch_mm);
        assert!((f.tpi - 13.0).abs() < 2e-3, "tpi {}", f.tpi);
        assert!(f.left.periods >= 6 && f.right.periods >= 6);
    }

    /// Tilting the shaft must not change any diameter (they are measured about the fitted axis).
    #[test]
    fn a_tilted_axis_changes_nothing() {
        let p = 25.4 / 13.0;
        let (l0, r0) = shaft(p, 6.35, 5.25, 0.0, 700, 0.02, 0.0);
        let f0 = measure_thread(&l0, &r0).unwrap();
        for tilt in [-2.0, 0.7, 3.0] {
            let (l, r) = shaft(p, 6.35, 5.25, tilt, 700, 0.02, 0.0);
            let f = measure_thread(&l, &r).unwrap();
            assert!(
                (f.axis_tilt_deg - tilt).abs() < 0.05,
                "tilt {} vs {tilt}",
                f.axis_tilt_deg
            );
            for (a, b, what) in [
                (f.major_mm, f0.major_mm, "major"),
                (f.minor_mm, f0.minor_mm, "minor"),
                (f.pd_silhouette_mm, f0.pd_silhouette_mm, "pd"),
                (f.pitch_mm, f0.pitch_mm, "pitch"),
            ] {
                assert!((a - b).abs() < 3e-3, "{what} at tilt {tilt}: {a} vs {b}");
            }
        }
    }

    /// Scaling the radii scales the diameters (a direct check on units and on summing both sides).
    #[test]
    fn different_thread_sizes_give_their_own_diameters() {
        for (p, rmaj, rmin) in [(25.4 / 32.0, 2.4, 1.9), (1.5, 5.0, 4.0), (1.0, 3.0, 2.2)] {
            let rows = (14.0 * p / 0.01) as usize;
            let (l, r) = shaft(p, rmaj, rmin, 0.0, rows, 0.01, 0.0);
            let f = measure_thread(&l, &r).unwrap();
            assert!(
                (f.major_mm - 2.0 * rmaj).abs() < 4e-3 && (f.minor_mm - 2.0 * rmin).abs() < 4e-3,
                "{f:?}"
            );
            assert!((f.pitch_mm - p).abs() < 3e-4, "pitch {} vs {p}", f.pitch_mm);
        }
    }

    #[test]
    fn noise_does_not_move_the_diameters_much() {
        let p = 25.4 / 13.0;
        let (l, r) = shaft(p, 6.35, 5.25, 0.5, 700, 0.02, 0.005);
        let f = measure_thread(&l, &r).unwrap();
        let pd = 2.0 * analytic_pd_radius(p, 6.35, 5.25);
        // 5 um of noise is ~10x the measured real edge noise (0.45 um). Measured effect: PD 4 um,
        // major +3.6 um, minor -6.8 um (selection bias of the extreme 8 %).
        assert!(
            (f.pd_silhouette_mm - pd).abs() < 6e-3,
            "pd {}",
            f.pd_silhouette_mm
        );
        assert!((f.major_mm - 12.7).abs() < 8e-3, "major {}", f.major_mm);
        assert!((f.minor_mm - 10.5).abs() < 1.2e-2, "minor {}", f.minor_mm);
    }

    #[test]
    fn bad_input_and_too_few_periods_are_errors() {
        let p = 25.4 / 13.0;
        let (l, r) = shaft(p, 6.35, 5.25, 0.0, 700, 0.02, 0.0);
        assert_eq!(
            measure_thread(&l[..10], &r[..10]),
            Err(ThreadError::BadInput)
        );
        assert_eq!(measure_thread(&l, &r[..600]), Err(ThreadError::BadInput));
        let (ls, rs) = shaft(p, 6.35, 5.25, 0.0, 130, 0.02, 0.0); // 2.6 mm: ~1.3 pitches
        assert!(
            matches!(
                measure_thread(&ls, &rs),
                Err(ThreadError::TooFewPeriods { .. })
            ),
            "{:?}",
            measure_thread(&ls, &rs)
        );
    }

    /// A missing edge (NaN or infinite point) is a bad input, not a thread with a strange crest.
    #[test]
    fn non_finite_points_are_bad_input() {
        let (mut l, r) = shaft(25.4 / 13.0, 6.35, 5.25, 0.0, 700, 0.02, 0.0);
        l[100].0 = f64::NAN;
        assert_eq!(measure_thread(&l, &r), Err(ThreadError::BadInput));
        l[100].0 = f64::INFINITY;
        assert_eq!(measure_thread(&l, &r), Err(ThreadError::BadInput));
    }

    /// A smooth cylinder has no thread: refuse rather than invent a pitch.
    #[test]
    fn a_plain_cylinder_is_not_a_thread() {
        let l: Vec<(f64, f64)> = (0..400).map(|k| (-6.0, k as f64 * 0.02)).collect();
        let r: Vec<(f64, f64)> = (0..400).map(|k| (6.0, k as f64 * 0.02)).collect();
        assert!(matches!(
            measure_thread(&l, &r),
            Err(ThreadError::TooFewPeriods { .. })
        ));
    }
}
