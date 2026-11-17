//! Pitch diameter from the flanks of one silhouette side.
//!
//! The silhouette of a helical thread is not its axial profile: at axial position z it is
//! `max over azimuth phi of R(z - P*phi/2pi) * cos(phi)`. On a flank of slope m = dr/dz that raises
//! the silhouette radially by `(m*P/2pi)^2 / (2 r)` (about 25 um per side on a 1/2-13 thread) and
//! leaves crests and roots alone, so statistics of the whole silhouette (its median level, say)
//! read high. This module instead:
//!   1. numbers the flanks from the positions of the pitch-level crossings,
//!   2. takes only the middle of the flanks (clear of the rounded crest/root corners),
//!   3. fits one straight line to all the rising flanks and one to all the falling flanks,
//!      `z = c + s * (r - r_ref) + k * P` (k = flank number, P fitted jointly),
//!   4. finds the radius where the tooth between a rising and the next falling flank is P/2 wide
//!      (the definition of the pitch cylinder; no assumption about crest or root flats),
//!   5. removes the helix-envelope raise of each flank, computed from its own fitted slope.
//!
//! The axial profile of a symmetric thread has equal flank angles; all are reported as a check.
//! A thread whose pitch radius lies well outside the fitted band (crest or root flat wider than
//! about a third of the pitch) is refused rather than extrapolated. The sample selection (middle
//! half of the depth, within a quarter pitch of a crossing) assumes flanks spanning at most about
//! 0.55 P axially: tested on 60 degree V forms and a 19/44 degree buttress-like form, not on
//! shallow Acme-like or square forms.

use crate::linalg::least_squares;
use std::collections::BTreeSet;

/// Part of the minor-to-major radius range used for flank samples (fractions of the depth).
const BAND: (f64, f64) = (0.25, 0.75);
/// The pitch radius may lie this far (fraction of the depth) outside that band.
const BAND_SLACK: f64 = 0.1;
/// A sample belongs to a crossing if it lies within this fraction of a pitch of it.
const NEAR: f64 = 0.25;
/// Fewest flank samples per direction, and fewest distinct flanks contributing them.
pub const MIN_FLANK_SAMPLES: usize = 12;
pub const MIN_FLANKS: usize = 3;
/// Flanks flatter than this (dz/dr, about 3 degrees from the radial direction) are not flanks:
/// a square-thread-like profile or a degenerate fit, and the helix correction divides by the slope.
const MIN_SLOPE: f64 = 0.05;
/// A flank fit with a larger axial rms is not a clean thread (damage, wrong axis, noise): the
/// smaller of this (mm) and `MAX_RMS_PITCH` of the pitch. Real gages run at 0.5 % of the pitch.
pub const MAX_RMS_MM: f64 = 0.03;
pub const MAX_RMS_PITCH: f64 = 0.02;

#[derive(Debug, Clone, PartialEq)]
pub enum FlankError {
    /// The crossings do not give a pitch (too few of one direction).
    NoPitch,
    /// Too few flank samples; the profile has no usable straight flanks in the sampled band.
    TooFewSamples { rising: usize, falling: usize },
    /// Samples come from fewer than `MIN_FLANKS` distinct flanks of a direction.
    TooFewFlanks { rising: usize, falling: usize },
    /// The fitted flanks do not describe a thread (slopes of the wrong sign, or no pitch radius
    /// near the fitted band where the tooth is P/2 wide).
    NoPitchCylinder,
    /// The flank lines fit the samples worse than the limit above.
    PoorFit { rms_mm: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlankFit {
    /// Pitch radius of the axial profile (helix raise removed), mm.
    pub pd_radius_mm: f64,
    /// Same radius of the silhouette itself, before the helix correction, mm.
    pub pd_radius_raw_mm: f64,
    /// Effective helix-envelope raise that was removed, mm.
    pub helix_shift_mm: f64,
    /// Flank angles from the radial direction, degrees (30 for a 60 degree thread).
    pub rise_half_angle_deg: f64,
    pub fall_half_angle_deg: f64,
    /// Pitch from the joint flank fit, mm.
    pub pitch_mm: f64,
    /// RMS axial residual of the flank samples about their lines, mm.
    pub rms_mm: f64,
    /// Samples used on rising / falling flanks.
    pub samples: (usize, usize),
}

/// Median gap between consecutive crossings of the same direction: a pitch to round with. `None`
/// unless both directions occur and the gaps are positive.
fn rough_pitch(crossings: &[(f64, bool)]) -> Option<f64> {
    let mut gaps = Vec::new();
    for dir in [true, false] {
        let pos: Vec<f64> = crossings
            .iter()
            .filter(|c| c.1 == dir)
            .map(|c| c.0)
            .collect();
        if pos.is_empty() {
            return None;
        }
        gaps.extend(pos.windows(2).map(|w| w[1] - w[0]));
    }
    gaps.sort_by(f64::total_cmp);
    gaps.get(gaps.len() / 2).copied().filter(|g| *g > 0.0)
}

/// Remove edge-noise chatter: an opposite-direction pair of crossings closer than a tenth of a
/// pitch is the profile touching the level and falling back, not a flank, so both are dropped
/// (leaving the one real crossing of a rising-falling-rising triple).
pub fn debounce(crossings: &[(f64, bool)]) -> Vec<(f64, bool)> {
    let Some(rough) = rough_pitch(crossings) else {
        return crossings.to_vec();
    };
    let mut out = Vec::with_capacity(crossings.len());
    let mut i = 0;
    while i < crossings.len() {
        match crossings.get(i + 1) {
            Some(n) if n.1 != crossings[i].1 && n.0 - crossings[i].0 < 0.1 * rough => i += 2,
            _ => {
                out.push(crossings[i]);
                i += 1;
            }
        }
    }
    out
}

/// Flank number of every crossing (counted separately for rising and falling, from the first of
/// that direction) and the pitch they imply.
///
/// Each number is the previous one of the same direction plus the whole pitches in the gap to it,
/// so a tooth that never reaches the pitch level (nick, burr) costs one jump of 2, not an offset
/// in every later flank, and a pitch error cannot accumulate along the window. The rough pitch
/// for rounding is the median same-direction gap; the returned pitch is a joint least-squares fit
/// of `position = c_direction + k * P`.
pub fn number_flanks(crossings: &[(f64, bool)]) -> Result<(Vec<f64>, f64), FlankError> {
    let rough = rough_pitch(crossings).ok_or(FlankError::NoPitch)?;
    let mut last: [Option<(f64, f64)>; 2] = [None, None];
    let ord: Vec<f64> = crossings
        .iter()
        .map(|&(pos, rising)| {
            let slot = &mut last[rising as usize];
            let k = slot.map_or(0.0, |(p, k)| k + ((pos - p) / rough).round().max(1.0));
            *slot = Some((pos, k));
            k
        })
        .collect();
    let rows: Vec<Vec<f64>> = crossings
        .iter()
        .zip(&ord)
        .map(|(c, &k)| {
            if c.1 {
                vec![1.0, 0.0, k]
            } else {
                vec![0.0, 1.0, k]
            }
        })
        .collect();
    let ys: Vec<f64> = crossings.iter().map(|c| c.0).collect();
    let c = least_squares(&rows, &ys).ok_or(FlankError::NoPitch)?;
    if c[2].is_nan() || c[2] <= 0.0 {
        return Err(FlankError::NoPitch);
    }
    Ok((ord, c[2]))
}

/// Helix raise of a flank of axial slope `s = dz/dr` at radius `r`: (m P / 2 pi)^2 / (2 r).
fn helix_raise(s: f64, pitch: f64, r: f64) -> f64 {
    let b = pitch / (s.abs() * std::f64::consts::TAU);
    b * b / (2.0 * r)
}

/// One flank sample: axial position, flank number, radius offset from the band centre, direction.
type Sample = (f64, f64, f64, bool);

/// Flank samples between the first and last crossing: radius inside the band and within `NEAR`
/// pitches of a crossing. The band covers the middle half of the depth, so a sample of a flank is
/// at most about 0.14 P (a 0.55 P flank) from its own crossing, while the nearest crossing of the
/// other direction is over 0.35 P away: the samples of a flank whose own crossing is missing find
/// no crossing and are dropped, never filed under the opposite flank.
fn flank_samples(
    t: &[f64],
    r: &[f64],
    crossings: &[(f64, bool)],
    ord: &[f64],
    pitch: f64,
    (lo, hi): (f64, f64),
) -> Vec<Sample> {
    let (Some(first), Some(last)) = (crossings.first(), crossings.last()) else {
        return Vec::new();
    };
    let r_ref = 0.5 * (lo + hi);
    let mut out = Vec::new();
    for (&ti, &ri) in t.iter().zip(r) {
        if !(lo..=hi).contains(&ri) || ti < first.0 || ti > last.0 {
            continue;
        }
        let k = crossings.partition_point(|c| c.0 < ti);
        let dist = |j: usize| (crossings[j].0 - ti).abs();
        let near = match (k.checked_sub(1), (k < crossings.len()).then_some(k)) {
            (Some(a), Some(b)) => Some(if dist(a) <= dist(b) { a } else { b }),
            (a, b) => a.or(b),
        };
        let Some(j) = near.filter(|&j| dist(j) <= NEAR * pitch) else {
            continue;
        };
        out.push((ti, ord[j], ri - r_ref, crossings[j].1));
    }
    out
}

/// Fit the flanks of one side. `t`, `r`: axial position (ascending) and radius about the fitted
/// axis; `crossings`: (position, rising?) of the profile through its pitch level, sorted by
/// position; `minor_r`, `major_r`: robust root and crest radii of this side.
pub fn fit(
    t: &[f64],
    r: &[f64],
    crossings: &[(f64, bool)],
    minor_r: f64,
    major_r: f64,
) -> Result<FlankFit, FlankError> {
    let crossings = &debounce(crossings);
    let (ord, pitch_hint) = number_flanks(crossings)?;
    let depth = major_r - minor_r;
    let (lo, hi) = (minor_r + BAND.0 * depth, minor_r + BAND.1 * depth);
    let r_ref = 0.5 * (lo + hi);
    let samples = flank_samples(t, r, crossings, &ord, pitch_hint, (lo, hi));
    let count = |rising: bool| samples.iter().filter(|s| s.3 == rising).count();
    let (up, dn) = (count(true), count(false));
    if up < MIN_FLANK_SAMPLES || dn < MIN_FLANK_SAMPLES {
        return Err(FlankError::TooFewSamples {
            rising: up,
            falling: dn,
        });
    }
    let flanks = |rising: bool| {
        samples
            .iter()
            .filter(|s| s.3 == rising)
            .map(|s| s.1 as i64)
            .collect::<BTreeSet<_>>()
            .len()
    };
    let (fu, fd) = (flanks(true), flanks(false));
    if fu < MIN_FLANKS || fd < MIN_FLANKS {
        return Err(FlankError::TooFewFlanks {
            rising: fu,
            falling: fd,
        });
    }
    // flank numbers centred on the middle of the data, so the intercepts are mid-window values
    let k0 = samples.iter().map(|s| s.1).sum::<f64>() / samples.len() as f64;
    let rows: Vec<Vec<f64>> = samples
        .iter()
        .map(|&(_, k, x, rising)| {
            if rising {
                vec![1.0, x, 0.0, 0.0, k - k0]
            } else {
                vec![0.0, 0.0, 1.0, x, k - k0]
            }
        })
        .collect();
    let ys: Vec<f64> = samples.iter().map(|s| s.0).collect();
    let c = least_squares(&rows, &ys).ok_or(FlankError::NoPitchCylinder)?;
    let (c_up, s_up, c_dn, s_dn, pitch) = (c[0], c[1], c[2], c[3], c[4]);
    let rms = (rows
        .iter()
        .zip(&ys)
        .map(|(row, y)| (row.iter().zip(&c).map(|(a, b)| a * b).sum::<f64>() - y).powi(2))
        .sum::<f64>()
        / ys.len() as f64)
        .sqrt();
    if !(s_up > MIN_SLOPE && s_dn < -MIN_SLOPE && pitch > 0.0) {
        return Err(FlankError::NoPitchCylinder);
    }
    if rms.is_nan() || rms > MAX_RMS_MM.min(MAX_RMS_PITCH * pitch) {
        return Err(FlankError::PoorFit { rms_mm: rms });
    }

    // Tooth width at x = (c_dn + s_dn x) - (c_up + s_up x), taken modulo P (rising flank to the
    // next falling flank); a tooth P/2 wide is also a groove P/2 wide, so the tooth/groove pairing
    // cannot be wrong. Solutions are P/|ds| >= the depth apart (equal only when the two flanks
    // together span a whole pitch), so the one nearest the band centre (x = 0 by construction of
    // r_ref) is taken.
    let (dc, ds) = (c_dn - c_up, s_dn - s_up);
    let n = ((dc - 0.5 * pitch) / pitch).round();
    let raw = r_ref + (0.5 * pitch + n * pitch - dc) / ds;
    let slack = BAND_SLACK * depth;
    if raw < lo - slack || raw > hi + slack {
        return Err(FlankError::NoPitchCylinder);
    }
    // each flank's silhouette is raised by its own helix term: the true flank is r - raise, so
    // its line z(r) moves by -s * raise
    let (raise_up, raise_dn) = (helix_raise(s_up, pitch, raw), helix_raise(s_dn, pitch, raw));
    let dc_true = dc + s_dn * raise_dn - s_up * raise_up;
    let pd = r_ref + (0.5 * pitch + n * pitch - dc_true) / ds;
    if !pd.is_finite() {
        return Err(FlankError::NoPitchCylinder);
    }
    Ok(FlankFit {
        pd_radius_mm: pd,
        pd_radius_raw_mm: raw,
        helix_shift_mm: raw - pd,
        rise_half_angle_deg: s_up.atan().to_degrees(),
        fall_half_angle_deg: (-s_dn).atan().to_degrees(),
        pitch_mm: pitch,
        rms_mm: rms,
        samples: (up, dn),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_synth::{analytic_pd_radius, radius};

    const P: f64 = 25.4 / 13.0;
    const MAJ: f64 = 6.35;
    const MIN: f64 = 5.25;

    /// Axial-profile samples of an ideal thread (no helix effect) and their pitch-level crossings.
    fn data(
        p: f64,
        maj: f64,
        min: f64,
        z0: f64,
        z1: f64,
    ) -> (Vec<f64>, Vec<f64>, Vec<(f64, bool)>) {
        let dz = 0.01 * p / P; // ~0.01 mm on 1/2-13, proportionally finer on smaller threads
        let t: Vec<f64> = (0..)
            .map(|k| z0 + k as f64 * dz)
            .take_while(|&z| z < z1)
            .collect();
        let r: Vec<f64> = t.iter().map(|&z| radius(z, p, maj, min)).collect();
        let level = analytic_pd_radius(p, maj, min);
        let mut cr = Vec::new();
        for k in 0..t.len() - 1 {
            let (a, b) = (r[k] - level, r[k + 1] - level);
            if (a < 0.0) != (b < 0.0) {
                cr.push((t[k] + dz * (-a) / (b - a), b >= 0.0));
            }
        }
        (t, r, cr)
    }

    fn std_data(z0: f64, z1: f64) -> (Vec<f64>, Vec<f64>, Vec<(f64, bool)>) {
        data(P, MAJ, MIN, z0, z1)
    }

    /// On a profile with no helix effect the raw fit is the pitch radius to well under a micron,
    /// and the helix correction then subtracts a known, nonzero amount.
    #[test]
    fn raw_pitch_radius_of_an_ideal_profile_is_exact() {
        let (t, r, cr) = std_data(0.0, 14.0);
        let f = fit(&t, &r, &cr, MIN, MAJ).unwrap();
        let truth = analytic_pd_radius(P, MAJ, MIN);
        assert!(
            (f.pd_radius_raw_mm - truth).abs() < 5e-4,
            "{} vs {truth}",
            f.pd_radius_raw_mm
        );
        assert!((f.pitch_mm - P).abs() < 1e-4);
        assert!(
            (f.helix_shift_mm - 0.0272).abs() < 1e-3,
            "shift {}",
            f.helix_shift_mm
        );
        assert!(f.rms_mm < 1e-3);
    }

    /// Flank numbers follow the positions: a missing pair of crossings is one jump of 2, and a
    /// pitch hint that is a few percent off cannot drift the numbering along a long window.
    #[test]
    fn flank_numbers_follow_positions_through_a_gap() {
        let cr: Vec<(f64, bool)> = (0..20)
            .filter(|&j| j != 8 && j != 9) // one whole tooth missing
            .map(|j| (j as f64 * 0.5 * P, j % 2 == 0))
            .collect();
        let (ord, pitch) = number_flanks(&cr).unwrap();
        assert_eq!(
            ord,
            vec![
                0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 5.0, 5.0, 6.0, 6.0, 7.0, 7.0, 8.0, 8.0,
                9.0, 9.0
            ]
        );
        assert!((pitch - P).abs() < 1e-9, "pitch {pitch}");
    }

    /// A tooth that never reaches the pitch level loses a pair of crossings; the fit must still
    /// give the pitch radius.
    #[test]
    fn a_missing_tooth_does_not_shift_later_flanks() {
        let (t, mut r, mut cr) = std_data(0.0, 14.0);
        let truth = analytic_pd_radius(P, MAJ, MIN);
        let (c0, c1) = (3.0 * P - 0.45 * P, 3.0 * P + 0.45 * P);
        for (z, v) in t.iter().zip(r.iter_mut()) {
            if *z > c0 && *z < c1 {
                *v = MIN;
            }
        }
        cr.retain(|c| c.0 < c0 || c.0 > c1);
        let f = fit(&t, &r, &cr, MIN, MAJ).unwrap();
        assert!(
            (f.pd_radius_raw_mm - truth).abs() < 1e-3,
            "{} vs {truth}",
            f.pd_radius_raw_mm
        );
        assert!((f.pitch_mm - P).abs() < 1e-4, "pitch {}", f.pitch_mm);
    }

    /// A flank whose crossing is lost (the flank itself is still there) must not be filed under a
    /// neighbouring crossing of the other direction.
    #[test]
    fn a_dropped_crossing_leaves_the_fit_unchanged() {
        let (t, r, mut cr) = std_data(0.0, 14.0);
        let truth = analytic_pd_radius(P, MAJ, MIN);
        cr.remove(7);
        let f = fit(&t, &r, &cr, MIN, MAJ).unwrap();
        assert!(
            (f.pd_radius_raw_mm - truth).abs() < 1e-3,
            "{} vs {truth}",
            f.pd_radius_raw_mm
        );
    }

    /// Edge noise makes the profile touch the level and fall back on one flank (rising, falling,
    /// rising within a few rows): that must not become a phantom flank that offsets all later ones.
    #[test]
    fn a_chattering_crossing_is_not_a_phantom_flank() {
        let (t, r, mut cr) = std_data(0.0, 14.0);
        let truth = analytic_pd_radius(P, MAJ, MIN);
        let x = cr[7].0;
        assert!(cr[7].1 && !cr[6].1, "cr[7] is a rising crossing");
        cr.splice(7..8, [(x - 0.03, true), (x, false), (x + 0.03, true)]);
        let f = fit(&t, &r, &cr, MIN, MAJ).unwrap();
        assert!(
            (f.pd_radius_raw_mm - truth).abs() < 1e-3,
            "{} vs {truth}",
            f.pd_radius_raw_mm
        );
        assert!((f.pitch_mm - P).abs() < 1e-4, "pitch {}", f.pitch_mm);
    }

    /// Near-vertical flanks (about 2 degrees from radial) are not flanks: the helix correction
    /// divides by the slope and would explode, so the fit is refused.
    #[test]
    fn near_vertical_flanks_are_refused() {
        let p = 1.0;
        // crest 0.47, flank 0.03, root 0.47, flank 0.03 (dz/dr = 0.03 over a 1 mm depth)
        let prof = |z: f64| {
            let u = z.rem_euclid(p);
            match u {
                u if u < 0.47 => 6.0,
                u if u < 0.5 => 6.0 - (u - 0.47) / 0.03,
                u if u < 0.97 => 5.0,
                u => 5.0 + (u - 0.97) / 0.03,
            }
        };
        let t: Vec<f64> = (0..14_000).map(|k| k as f64 * 0.001).collect();
        let r: Vec<f64> = t.iter().map(|&z| prof(z)).collect();
        let cr: Vec<(f64, bool)> = (0..14)
            .flat_map(|k| [(k as f64 + 0.485, false), (k as f64 + 0.985, true)])
            .collect();
        assert_eq!(fit(&t, &r, &cr, 5.0, 6.0), Err(FlankError::NoPitchCylinder));
    }

    /// A window that starts and ends mid-flank must not change the answer.
    #[test]
    fn a_window_cut_through_flanks_gives_the_same_pitch_radius() {
        let truth = analytic_pd_radius(P, MAJ, MIN);
        for z0 in [0.0, 0.3, 0.77, 1.4] {
            let (t, r, cr) = std_data(z0, z0 + 11.3);
            let f = fit(&t, &r, &cr, MIN, MAJ).unwrap();
            assert!(
                (f.pd_radius_raw_mm - truth).abs() < 1e-3,
                "z0 {z0}: {} vs {truth}",
                f.pd_radius_raw_mm
            );
        }
    }

    /// A NaN radius is skipped, not fitted.
    #[test]
    fn a_nan_row_is_ignored() {
        let (t, mut r, cr) = std_data(0.0, 14.0);
        let truth = analytic_pd_radius(P, MAJ, MIN);
        for k in (10..r.len()).step_by(37) {
            r[k] = f64::NAN;
        }
        let f = fit(&t, &r, &cr, MIN, MAJ).unwrap();
        assert!((f.pd_radius_raw_mm - truth).abs() < 1e-3);
    }

    /// Scatter that is small in millimetres but large against the pitch is refused: about 16 um of
    /// axial rms is fine on 1/2-13 (limit 30 um) but 3 % of a 0.5 mm pitch (limit 10 um).
    #[test]
    fn the_scatter_limit_scales_with_the_pitch() {
        let ragged = |p: f64, maj: f64, min: f64, amp: f64| {
            let (t, mut r, cr) = data(p, maj, min, 0.0, 14.0 * p);
            let d = maj - min;
            for (k, v) in r.iter_mut().enumerate() {
                if *v > min + 0.3 * d && *v < maj - 0.3 * d {
                    *v += if k % 2 == 0 { amp } else { -amp };
                }
            }
            fit(&t, &r, &cr, min, maj)
        };
        // a 30 um radial ripple is ~17 um axial on the 1/2-13 flanks and ~16 um on the 0.5 mm one
        assert!(ragged(P, MAJ, MIN, 0.03).is_ok());
        assert!(matches!(
            ragged(0.5, 1.5, 1.2, 0.03),
            Err(FlankError::PoorFit { .. })
        ));
        assert!(matches!(
            ragged(P, MAJ, MIN, 0.12),
            Err(FlankError::PoorFit { .. })
        ));
    }

    /// Samples from only one or two flanks of a direction are not enough, however many there are.
    #[test]
    fn two_flanks_are_not_enough() {
        let (t, r, cr) = std_data(0.0, 1.9 * P); // ~2 rising and 2 falling flanks
        assert!(matches!(
            fit(&t, &r, &cr, MIN, MAJ),
            Err(FlankError::TooFewFlanks { .. })
        ));
    }

    #[test]
    fn no_crossings_or_samples_is_an_error() {
        assert_eq!(fit(&[], &[], &[], MIN, MAJ), Err(FlankError::NoPitch));
    }
}
