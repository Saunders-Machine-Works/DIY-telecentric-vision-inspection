//! Tooth-by-tooth inspection of a turned thread: finds chipped, missing, shifted, narrow or wide
//! teeth and shallow roots, even when the whole-thread fit of `thread_measure` refuses the part.
//!
//! Input is the same as `measure_thread`: left and right silhouette points (mm), one pair per
//! scan row in ascending image y, covering the threaded section only (the caller picks the
//! window; slots that would stick out of it are not judged). The axis is the line through the
//! row midpoints, refined from the flat tops of the well-formed teeth; every point becomes
//! (axial position t, radius r).
//!
//! Each side is judged on its own. The two sides of a helical thread are half a pitch out of
//! phase, so each side gets its own tooth grid: slots one pitch apart, positioned on the median
//! phase of the tallest well-formed peaks (a few damaged teeth cannot move it). A slot is the
//! stretch of t within half a pitch of its centre. For every slot we report
//!   * crest radius (highest r) and root radius (the higher of the lowest r within 0.75 pitch
//!     before and after the crest, so a filled root on either side shows),
//!   * the width of the tooth where it crosses the half-depth level, and its centre offset
//!     from the slot centre,
//!   * the half angle of each flank.
//!
//! The grid keeps the nominal pitch, so a pitch or scale error of e accumulates: over a window
//! of N turns the end slots sit N * e pitches off, and N * e above 0.1 flags them as Shifted.
//! Teeth are compared with the part's own good teeth (crest: upper quartile of all crests; root:
//! median of all roots), because a thread's absolute crest and root depend on its class and
//! allowance, which this module does not judge; the caller compares the reported reference
//! radii with the drawing.
//!
//! A tooth is flagged when (D = reference crest - reference root):
//!   Missing      its crest never reaches the half-depth level
//!   LowCrest     crest more than 8 % of D below the reference crest
//!   ShallowRoot  root more than 8 % of D above the reference root
//!   Shifted      centre more than 10 % of a pitch from the slot centre
//!   Narrow/Wide  width at half depth more than 20 % of a pitch from half a pitch
//!                (a helical thread's silhouette is about 0.44 to 0.57 of a pitch wide here)
//!   BadFlank     a flank whose half angle (straight-line fit over the middle half of its
//!                depth, against the axis normal) is more than 8 degrees from the spec's;
//!                a chipped insert leaves a long ramp instead of a 30 degree flank
//!   NoFlank      a flank of a tooth that reaches half depth but has too few samples in its
//!                middle-half band to fit (a near-radial wall, e.g. a chip edge)
//! Thresholds are constants below; real healthy teeth scatter far less (a few micrometres).
//!
//! Judgements depend on the window the caller passes: the crest and root references are one
//! global quartile/median, so a taper, a run-out or a stretch of neck inside the window makes
//! its slots read Missing, LowCrest or ShallowRoot, and a slot cut by the window end has a
//! truncated root search. Treat flags on the first and last slot of a side as suspect.

use crate::thread_measure::{axis_from_crests, fit_line, project};

/// Smallest peak height above the surrounding valleys counted as a tooth, fraction of the
/// nominal depth.
const PROMINENCE: f64 = 0.3;
/// A peak is a reference tooth if within this fraction of the nominal depth of the upper
/// quartile of peak heights (one burr cannot take that place).
const REFERENCE_BAND: f64 = 0.15;
/// Reference teeth needed per side to place the tooth grid.
pub const MIN_REFERENCE_TEETH: usize = 2;
const LOW_CREST: f64 = 0.08;
const SHALLOW_ROOT: f64 = 0.08;
const MAX_SHIFT: f64 = 0.10;
const WIDTH_TOLERANCE: f64 = 0.20;
/// A healthy GO gage scatters up to 5.4 degrees per tooth (one fit per flank); a chipped insert
/// leaves 20 to 30 degrees.
const FLANK_TOLERANCE_DEG: f64 = 8.0;
/// Samples a flank needs inside its middle-half depth band to be fitted: a tenth of the rows per
/// pitch (a 60 degree flank's band spans about 0.17 pitch), limited to this range.
const FLANK_SAMPLES: (f64, f64) = (3.0, 8.0);
/// Rows per pitch below which the spec cannot be judged (a flank would have under 3 samples).
const MIN_ROWS_PER_PITCH: f64 = 20.0;
/// Upper bound on slots per side; a sane window holds a few dozen.
const MAX_SLOTS: usize = 10_000;
/// How far beyond the flat top the half-depth crossings are searched, in pitches.
const WIDTH_REACH: f64 = 0.75;

/// What the thread should be: pitch and nominal depth, from the drawing or the identified thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreadSpec {
    pub pitch_mm: f64,
    /// Nominal radial depth, crest to root, mm (about 0.6 P for a basic 60 degree thread).
    pub depth_mm: f64,
    /// Nominal flank half angle, degrees (30 for a 60 degree thread).
    pub half_angle_deg: f64,
}

impl ThreadSpec {
    /// A basic 60 degree thread of the given threads per inch: depth 0.6134 P (external, ASME).
    pub fn unified(tpi: f64) -> Self {
        let pitch_mm = 25.4 / tpi;
        ThreadSpec {
            pitch_mm,
            depth_mm: 0.613_435 * pitch_mm,
            half_angle_deg: 30.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TeethError {
    /// Left and right must hold the same number (>= 20) of finite rows with ascending image y,
    /// and the spec be finite and positive with a pitch of at least 8 rows.
    BadInput,
    /// No usable axis (degenerate midpoints).
    NoAxis,
    /// Fewer than MIN_REFERENCE_TEETH well-formed teeth on a side to place the tooth grid.
    NoReferenceTeeth { side: Side, found: usize },
    /// The window holds no complete tooth slot on either side.
    NoTeeth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Defect {
    /// The tooth never reaches half depth (crest radius reported on the tooth).
    Missing,
    LowCrest {
        short_mm: f64,
    },
    ShallowRoot {
        high_mm: f64,
    },
    Shifted {
        offset_mm: f64,
    },
    Narrow {
        width_mm: f64,
    },
    /// `None`: the tooth stays above half depth for more than the search reach on one side.
    Wide {
        width_mm: Option<f64>,
    },
    /// A flank whose half angle is off the spec by more than the tolerance.
    BadFlank {
        falling: bool,
        half_angle_deg: f64,
    },
    /// A flank with too few samples to fit on a tooth that otherwise reaches half depth.
    NoFlank {
        falling: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tooth {
    pub side: Side,
    /// Slot number on its side, counted from the top of the window (lowest image y) starting at 0.
    pub index: usize,
    /// Axial position of the slot centre, mm (same axis origin for both sides).
    pub t_mm: f64,
    pub crest_r_mm: f64,
    pub root_r_mm: f64,
    /// Width and centre offset at half depth; `None` when the tooth is missing or too wide.
    pub width_mm: Option<f64>,
    pub centre_offset_mm: Option<f64>,
    /// Half angles of the flank before (rising r) and after (falling r) the crest, degrees;
    /// `None` when the flank has too few samples in its middle-half band.
    pub rise_half_angle_deg: Option<f64>,
    pub fall_half_angle_deg: Option<f64>,
    pub defects: Vec<Defect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeethReport {
    pub teeth: Vec<Tooth>,
    pub reference_crest_r_mm: f64,
    pub reference_root_r_mm: f64,
    pub axis_tilt_deg: f64,
    /// False when too few good-tooth samples were found to refine the axis, so the midpoint
    /// line (biased by the helix ripple and by damage) was kept.
    pub axis_refined: bool,
}

impl TeethReport {
    pub fn is_clean(&self) -> bool {
        self.teeth.iter().all(|t| t.defects.is_empty())
    }

    /// Every flagged tooth with its defects.
    pub fn flagged(&self) -> impl Iterator<Item = &Tooth> {
        self.teeth.iter().filter(|t| !t.defects.is_empty())
    }
}

/// Index range of the samples with lo <= t <= hi (t ascending).
fn span(t: &[f64], lo: f64, hi: f64) -> std::ops::Range<usize> {
    t.partition_point(|&x| x < lo)..t.partition_point(|&x| x <= hi)
}

fn max_of(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::MIN, f64::max)
}

fn min_of(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::MAX, f64::min)
}

/// The contiguous samples around `i` within 5 % of the depth of its height: the flat top.
fn plateau(t: &[f64], r: &[f64], i: usize, depth: f64) -> std::ops::Range<usize> {
    let floor = r[i] - 0.05 * depth;
    let (mut lo, mut hi) = (i, i);
    while lo > 0 && r[lo - 1] >= floor {
        lo -= 1;
    }
    while hi + 1 < t.len() && r[hi + 1] >= floor {
        hi += 1;
    }
    lo..hi + 1
}

/// Axial centre of the flat top around sample `i`.
fn crest_centre(t: &[f64], r: &[f64], i: usize, depth: f64) -> f64 {
    let flat = plateau(t, r, i, depth);
    0.5 * (t[flat.start] + t[flat.end - 1])
}

/// Peaks of r(t): the first sample of each local maximum over +-0.3 pitch that stands at least
/// `PROMINENCE * depth` above the lowest point within +-0.5 pitch.
fn peaks(t: &[f64], r: &[f64], spec: &ThreadSpec) -> Vec<usize> {
    let mut out = Vec::new();
    for i in 0..t.len() {
        let near = span(t, t[i] - 0.3 * spec.pitch_mm, t[i] + 0.3 * spec.pitch_mm);
        if r[i] < max_of(&r[near.clone()]) || (i > 0 && near.contains(&(i - 1)) && r[i - 1] == r[i])
        {
            continue;
        }
        let far = span(t, t[i] - 0.5 * spec.pitch_mm, t[i] + 0.5 * spec.pitch_mm);
        if r[i] - min_of(&r[far]) >= PROMINENCE * spec.depth_mm {
            out.push(i);
        }
    }
    out
}

/// The well-formed teeth: peaks within `REFERENCE_BAND` of the upper quartile of peak heights.
fn reference_peaks(t: &[f64], r: &[f64], spec: &ThreadSpec) -> Vec<usize> {
    let found = peaks(t, r, spec);
    if found.is_empty() {
        return found;
    }
    let mut heights: Vec<f64> = found.iter().map(|&i| r[i]).collect();
    heights.sort_by(f64::total_cmp);
    let top = quantile(&heights, 0.75);
    found
        .into_iter()
        .filter(|&i| (r[i] - top).abs() <= REFERENCE_BAND * spec.depth_mm)
        .collect()
}

/// Signed distance from `a` to `b` on a circle of circumference `p`, in (-p/2, p/2].
fn circ_diff(b: f64, a: f64, p: f64) -> f64 {
    let d = (b - a).rem_euclid(p);
    if d > 0.5 * p {
        d - p
    } else {
        d
    }
}

/// Phase (position mod pitch) of the tooth grid: the circular median of the reference peaks,
/// then the mean of those within a tenth of a pitch of it.
fn grid_phase(positions: &[f64], p: f64) -> f64 {
    let cost = |c: f64| {
        positions
            .iter()
            .map(|&x| circ_diff(x, c, p).abs())
            .sum::<f64>()
    };
    let median = positions
        .iter()
        .map(|&x| x.rem_euclid(p))
        .min_by(|&a, &b| cost(a).total_cmp(&cost(b)))
        .unwrap_or(0.0);
    let near: Vec<f64> = positions
        .iter()
        .map(|&x| circ_diff(x, median, p))
        .filter(|d| d.abs() <= 0.1 * p)
        .collect();
    (median + near.iter().sum::<f64>() / near.len() as f64).rem_euclid(p)
}

/// Slot measurements before the references are known.
struct Slot {
    index: usize,
    t: f64,
    crest_i: usize,
    crest_r: f64,
    root_r: f64,
    /// Sample ranges of the rising and falling flank: from the lowest point on that side of the
    /// crest to the flat top, so a neighbour's flank cannot enter.
    rise: std::ops::Range<usize>,
    fall: std::ops::Range<usize>,
}

fn slots(side: Side, t: &[f64], r: &[f64], spec: &ThreadSpec) -> Result<Vec<Slot>, TeethError> {
    let p = spec.pitch_mm;
    let reference: Vec<f64> = reference_peaks(t, r, spec)
        .iter()
        .map(|&i| crest_centre(t, r, i, spec.depth_mm))
        .collect();
    if reference.len() < MIN_REFERENCE_TEETH {
        return Err(TeethError::NoReferenceTeeth {
            side,
            found: reference.len(),
        });
    }
    let phase = grid_phase(&reference, p);
    let (t_lo, t_hi) = (t[0], t[t.len() - 1]);
    // a slot counts as inside the window if it sticks out by no more than two row spacings
    let slack = 2.0 * (t_hi - t_lo) / (t.len() - 1) as f64;
    let first = ((t_lo - slack + 0.5 * p - phase) / p).ceil();
    let mut out = Vec::new();
    for index in 0..MAX_SLOTS {
        let centre = phase + (first + index as f64) * p;
        if centre + 0.5 * p > t_hi + slack {
            break;
        }
        let range = span(t, centre - 0.5 * p, centre + 0.5 * p);
        if range.is_empty() {
            continue;
        }
        let crest_i = range
            .clone()
            .max_by(|&a, &b| r[a].total_cmp(&r[b]))
            .expect("non-empty slot");
        let flat = plateau(t, r, crest_i, spec.depth_mm);
        // roots are searched 0.75 pitch either side of the centre: a damaged tooth is asymmetric
        // and its ramp can run past the slot edge, but no neighbour's flank is lower than a root
        let wide = span(t, centre - 0.75 * p, centre + 0.75 * p);
        let lowest = |from: std::ops::Range<usize>, last: bool| -> Option<usize> {
            let mut best: Option<usize> = None;
            for i in from {
                if best.is_none_or(|b| if last { r[i] <= r[b] } else { r[i] < r[b] }) {
                    best = Some(i);
                }
            }
            best
        };
        let before = lowest(wide.start..flat.start.clamp(wide.start, wide.end), true);
        let after = lowest(flat.end.clamp(wide.start, wide.end)..wide.end, false);
        out.push(Slot {
            index,
            t: centre,
            crest_i,
            crest_r: r[crest_i],
            // the higher of the two root minima: a root filled on either side shows
            root_r: match (before, after) {
                (Some(b), Some(a)) => r[b].max(r[a]),
                (Some(i), None) | (None, Some(i)) => r[i],
                (None, None) => r[crest_i],
            },
            rise: before.map_or(0..0, |b| b..flat.start),
            fall: after.map_or(0..0, |a| flat.end..a + 1),
        });
    }
    Ok(out)
}

/// Width and centre of the part of the tooth with flat top `flat` that stands above `level`,
/// `None` if it does not end on both sides within `reach` of the flat top.
fn width_at(
    t: &[f64],
    r: &[f64],
    flat: std::ops::Range<usize>,
    level: f64,
    reach: f64,
) -> Option<(f64, f64)> {
    let cross = |from: usize, step: isize| -> Option<f64> {
        let mut i = from;
        loop {
            let j = i.checked_add_signed(step).filter(|&j| j < t.len())?;
            if (t[j] - t[from]).abs() > reach {
                return None;
            }
            if r[j] < level {
                // interpolate between the last sample above and the first below
                return Some(t[i] + (t[j] - t[i]) * (r[i] - level) / (r[i] - r[j]));
            }
            i = j;
        }
    };
    let (lo, hi) = (cross(flat.start, -1)?, cross(flat.end - 1, 1)?);
    Some((hi - lo, 0.5 * (hi + lo)))
}

/// Half angle (degrees from the axis normal) of the flank in `range`: a straight-line fit of t
/// against r over the samples whose r lies in `band`. `None` with too few samples.
fn flank_half_angle(
    t: &[f64],
    r: &[f64],
    range: std::ops::Range<usize>,
    band: (f64, f64),
    min_samples: usize,
) -> Option<f64> {
    let pts: Vec<(f64, f64)> = range
        .filter(|&i| r[i] >= band.0 && r[i] <= band.1)
        .map(|i| (r[i], t[i]))
        .collect();
    if pts.len() < min_samples {
        return None;
    }
    let n = pts.len() as f64;
    let (mr, mt) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let var: f64 = pts.iter().map(|p| (p.0 - mr).powi(2)).sum();
    let cov: f64 = pts.iter().map(|p| (p.0 - mr) * (p.1 - mt)).sum();
    (var > 0.0).then(|| (cov / var).abs().atan().to_degrees())
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// Inspect every tooth of the threaded window. See the module docs.
pub fn inspect(
    left: &[(f64, f64)],
    right: &[(f64, f64)],
    spec: &ThreadSpec,
) -> Result<TeethReport, TeethError> {
    let finite = |p: &(f64, f64)| p.0.is_finite() && p.1.is_finite();
    let spec_ok = [spec.pitch_mm, spec.depth_mm, spec.half_angle_deg]
        .iter()
        .all(|v| v.is_finite())
        && spec.pitch_mm > 0.0
        && spec.depth_mm > 0.0
        && spec.half_angle_deg > 0.0
        && spec.half_angle_deg < 90.0;
    if left.len() != right.len()
        || left.len() < 20
        || !left.iter().chain(right).all(finite)
        || !spec_ok
        || !left.windows(2).all(|w| w[1].1 > w[0].1)
    {
        return Err(TeethError::BadInput);
    }
    let row_spacing = (left[left.len() - 1].1 - left[0].1) / (left.len() - 1) as f64;
    if spec.pitch_mm < MIN_ROWS_PER_PITCH * row_spacing {
        return Err(TeethError::BadInput);
    }
    let mid: Vec<(f64, f64)> = left
        .iter()
        .zip(right)
        .map(|(l, r)| (0.5 * (l.0 + r.0), 0.5 * (l.1 + r.1)))
        .collect();
    let mut axis = fit_line(&mid).ok_or(TeethError::NoAxis)?;
    let mut axis_refined = false;
    // A damaged tooth drags the midpoint line off the axis, so refine it from the flat tops of
    // the well-formed teeth only (as `measure_thread` does from its crest samples).
    for _ in 0..3 {
        let flat_tops = |pts: &[(f64, f64)], sign: f64| -> Vec<(f64, f64)> {
            let (t, r) = project(pts, axis, sign);
            reference_peaks(&t, &r, spec)
                .into_iter()
                .flat_map(|i| plateau(&t, &r, i, spec.depth_mm))
                .map(|i| pts[i])
                .collect()
        };
        let next = axis_from_crests(&flat_tops(left, -1.0), &flat_tops(right, 1.0));
        match next {
            Some(a) if (a.1 - axis.1).abs() > 1e-9 || (a.0 - axis.0).abs() > 1e-9 => {
                axis = a;
                axis_refined = true;
            }
            Some(_) => break,
            None => break,
        }
    }
    let sides = [
        (Side::Left, project(left, axis, -1.0)),
        (Side::Right, project(right, axis, 1.0)),
    ];
    let mut raw = Vec::new();
    for (side, (t, r)) in &sides {
        raw.push(slots(*side, t, r, spec)?);
    }
    let mut crests: Vec<f64> = raw.iter().flatten().map(|s| s.crest_r).collect();
    let mut roots: Vec<f64> = raw.iter().flatten().map(|s| s.root_r).collect();
    if crests.is_empty() {
        return Err(TeethError::NoTeeth);
    }
    crests.sort_by(f64::total_cmp);
    roots.sort_by(f64::total_cmp);
    let (ref_crest, ref_root) = (quantile(&crests, 0.75), quantile(&roots, 0.5));
    let depth = ref_crest - ref_root;
    let level = 0.5 * (ref_crest + ref_root);
    let p = spec.pitch_mm;
    let band = (ref_root + 0.25 * depth, ref_root + 0.75 * depth);
    let min_samples = (0.1 * p / row_spacing)
        .round()
        .clamp(FLANK_SAMPLES.0, FLANK_SAMPLES.1) as usize;
    let mut teeth = Vec::new();
    for ((side, (t, r)), slot_list) in sides.iter().zip(&raw) {
        for s in slot_list {
            let mut defects = Vec::new();
            let (mut width_mm, mut centre_offset_mm) = (None, None);
            let (mut rise, mut fall) = (None, None);
            if s.crest_r < level {
                defects.push(Defect::Missing);
            } else {
                if ref_crest - s.crest_r > LOW_CREST * depth {
                    defects.push(Defect::LowCrest {
                        short_mm: ref_crest - s.crest_r,
                    });
                }
                if s.root_r - ref_root > SHALLOW_ROOT * depth {
                    defects.push(Defect::ShallowRoot {
                        high_mm: s.root_r - ref_root,
                    });
                }
                match width_at(
                    t,
                    r,
                    plateau(t, r, s.crest_i, spec.depth_mm),
                    level,
                    WIDTH_REACH * p,
                ) {
                    Some((w, c)) => {
                        (width_mm, centre_offset_mm) = (Some(w), Some(c - s.t));
                        if (c - s.t).abs() > MAX_SHIFT * p {
                            defects.push(Defect::Shifted { offset_mm: c - s.t });
                        }
                        if w < (0.5 - WIDTH_TOLERANCE) * p {
                            defects.push(Defect::Narrow { width_mm: w });
                        } else if w > (0.5 + WIDTH_TOLERANCE) * p {
                            defects.push(Defect::Wide { width_mm: Some(w) });
                        }
                    }
                    None => defects.push(Defect::Wide { width_mm: None }),
                }
                rise = flank_half_angle(t, r, s.rise.clone(), band, min_samples);
                fall = flank_half_angle(t, r, s.fall.clone(), band, min_samples);
                for (angle, falling) in [(rise, false), (fall, true)] {
                    match angle {
                        Some(a) if (a - spec.half_angle_deg).abs() > FLANK_TOLERANCE_DEG => {
                            defects.push(Defect::BadFlank {
                                falling,
                                half_angle_deg: a,
                            });
                        }
                        Some(_) => {}
                        None => defects.push(Defect::NoFlank { falling }),
                    }
                }
            }
            teeth.push(Tooth {
                side: *side,
                index: s.index,
                t_mm: s.t,
                crest_r_mm: s.crest_r,
                root_r_mm: s.root_r,
                width_mm,
                centre_offset_mm,
                rise_half_angle_deg: rise,
                fall_half_angle_deg: fall,
                defects,
            });
        }
    }
    Ok(TeethReport {
        teeth,
        reference_crest_r_mm: ref_crest,
        reference_root_r_mm: ref_root,
        axis_tilt_deg: axis.1.atan().to_degrees(),
        axis_refined,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_synth::radius;

    type Pts = Vec<(f64, f64)>;

    const P: f64 = 1.954;
    const R_MAJ: f64 = 6.25;
    const R_MIN: f64 = 5.35;
    const DY: f64 = 0.03;
    const TURNS: usize = 12;
    /// Flank half angle of the synthetic profile: 0.3125 P of axial run over 0.9 mm of depth.
    const FLANK_DEG: f64 = 34.15;

    fn spec() -> ThreadSpec {
        ThreadSpec {
            pitch_mm: P,
            depth_mm: R_MAJ - R_MIN,
            half_angle_deg: FLANK_DEG,
        }
    }

    /// Left/right points of an untilted shaft; `left_fix(t, r)` may alter the left radius.
    fn shaft(fix: &dyn Fn(f64, f64) -> f64) -> (Pts, Pts) {
        let rows = (TURNS as f64 * P / DY) as usize;
        let (mut l, mut r) = (Vec::new(), Vec::new());
        for k in 0..rows {
            let t = k as f64 * DY;
            let a = fix(t, radius(t, P, R_MAJ, R_MIN));
            let b = radius(t + 0.5 * P, P, R_MAJ, R_MIN);
            l.push((5.0 - a, t));
            r.push((5.0 + b, t));
        }
        (l, r)
    }

    /// Smooth warp weight, 1 at `centre`, 0 half a pitch away.
    fn bump(t: f64, centre: f64) -> f64 {
        let u = (t - centre) / P;
        if u.abs() >= 0.5 {
            0.0
        } else {
            (std::f64::consts::PI * u).cos().powi(2)
        }
    }

    const BAD: f64 = 5.0 * P; // the damaged left tooth (a crest sits at every k * P)

    fn flagged_defects(rep: &TeethReport) -> Vec<(Side, f64, Vec<Defect>)> {
        rep.flagged()
            .map(|t| (t.side, t.t_mm, t.defects.clone()))
            .collect()
    }

    #[test]
    fn clean_thread_has_no_flags_and_reports_real_values() {
        let (l, r) = shaft(&|_, a| a);
        let rep = inspect(&l, &r, &spec()).unwrap();
        assert!(rep.is_clean(), "{:?}", flagged_defects(&rep));
        let count = |s| rep.teeth.iter().filter(|t| t.side == s).count();
        assert_eq!((count(Side::Left), count(Side::Right)), (11, 12));
        // the midpoint axis carries a few micrometres of ripple from the half-pitch phase offset
        assert!((rep.reference_crest_r_mm - R_MAJ).abs() < 0.005, "{rep:?}");
        assert!((rep.reference_root_r_mm - R_MIN).abs() < 0.005);
        // half-depth width of the synthetic profile is 0.4375 P
        for t in &rep.teeth {
            let w = t.width_mm.unwrap();
            assert!((w - 0.4375 * P).abs() < 0.01, "{w}");
            assert!(t.centre_offset_mm.unwrap().abs() < 0.01);
        }
        assert!(rep.axis_tilt_deg.abs() < 0.05, "{}", rep.axis_tilt_deg);
        assert!(rep.axis_refined);
        for t in &rep.teeth {
            for a in [t.rise_half_angle_deg, t.fall_half_angle_deg] {
                assert!((a.unwrap() - FLANK_DEG).abs() < 0.5, "{a:?}");
            }
        }
    }

    #[test]
    fn chipped_crest_is_the_only_flag() {
        let cap = 5.9; // above half depth (5.8), 0.35 mm short
        let (l, r) = shaft(&|t, a| {
            if (t - BAD).abs() < 0.5 * P {
                a.min(cap)
            } else {
                a
            }
        });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].0, Side::Left);
        assert!((f[0].1 - BAD).abs() < 0.05);
        match f[0].2.as_slice() {
            [Defect::LowCrest { short_mm }, ..] => assert!((short_mm - 0.35).abs() < 0.01),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tooth_that_never_reaches_half_depth_is_missing() {
        let (l, r) = shaft(&|t, a| if (t - BAD).abs() < 0.5 * P { R_MIN } else { a });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].2, vec![Defect::Missing]);
        assert!((f[0].1 - BAD).abs() < 0.05);
    }

    #[test]
    fn displaced_tooth_is_shifted() {
        let s = 0.25 * P;
        let (l, r) = shaft(&|t, _| radius(t - s * bump(t, BAD), P, R_MAJ, R_MIN));
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        let offset = f[0].2.iter().find_map(|d| match d {
            Defect::Shifted { offset_mm } => Some(*offset_mm),
            _ => None,
        });
        let o = offset.expect("shifted flag");
        assert!(o > 0.14 * P && o < 0.19 * P, "offset {o}");
    }

    #[test]
    fn squeezed_tooth_is_narrow() {
        let (l, r) = shaft(&|t, _| {
            let u = (t - BAD) / P;
            radius(BAD + u * (1.0 + 0.8 * bump(t, BAD)) * P, P, R_MAJ, R_MIN)
        });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        // squeezing a tooth also steepens its flanks, so both flags are right
        let narrow = f[0].2.iter().find_map(|d| match d {
            Defect::Narrow { width_mm } => Some(*width_mm),
            _ => None,
        });
        assert!(narrow.is_some_and(|w| w < 0.3 * P), "{:?}", f[0]);
        assert!(f[0].2.iter().all(|d| matches!(
            d,
            Defect::Narrow { .. } | Defect::BadFlank { .. } | Defect::NoFlank { .. }
        )));
    }

    #[test]
    fn ramped_rising_flank_is_a_bad_flank_on_its_tooth_only() {
        // the rising flank becomes a straight ramp over 0.4375 P: half angle atan(0.4375 P / 0.9)
        let (l, r) = shaft(&|t, a| {
            let start = BAD - 0.5 * P;
            if t >= start && t < BAD - 0.0625 * P {
                R_MIN + (R_MAJ - R_MIN) * (t - start) / (0.4375 * P)
            } else {
                a
            }
        });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        match f[0].2.as_slice() {
            [Defect::BadFlank {
                falling: false,
                half_angle_deg,
            }] => assert!((half_angle_deg - 43.5).abs() < 1.5, "{half_angle_deg}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn root_filled_on_one_side_only_is_still_flagged() {
        // fill the root after the bad crest (and so the same root seen by the next tooth) to 5.55
        let (l, r) = shaft(&|t, a| {
            if t > BAD + 0.3 * P && t < BAD + 0.7 * P {
                a.max(5.55)
            } else {
                a
            }
        });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 2, "{f:?}");
        for (k, (side, t, defects)) in f.iter().enumerate() {
            assert_eq!(*side, Side::Left);
            assert!((t - (BAD + k as f64 * P)).abs() < 0.05, "{t}");
            assert!(
                matches!(defects.as_slice(), [Defect::ShallowRoot { high_mm }] if (high_mm - 0.2).abs() < 0.01),
                "{defects:?}"
            );
        }
    }

    #[test]
    fn one_burr_taller_than_the_crests_does_not_become_the_reference() {
        // 0.3 mm spike (more than 0.15 of the depth) on the top of one left crest
        let (l, r) = shaft(&|t, a| if (t - BAD).abs() < 0.03 { a + 0.3 } else { a });
        let rep = inspect(&l, &r, &spec()).unwrap();
        assert!(rep.is_clean(), "{:?}", flagged_defects(&rep));
        assert!((rep.reference_crest_r_mm - R_MAJ).abs() < 0.005);
    }

    #[test]
    fn widened_tooth_is_wide() {
        // a broad flat top 0.76 P wide with short steep shoulders (about 15 degrees from radial)
        let (l, r) = shaft(&|t, a| {
            let d = (t - BAD).abs();
            if d >= 0.5 * P {
                a
            } else if d < 0.38 * P {
                R_MAJ
            } else {
                R_MIN + (R_MAJ - R_MIN) * (0.5 * P - d) / (0.12 * P)
            }
        });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        let wide = f[0].2.iter().find_map(|d| match d {
            Defect::Wide { width_mm } => *width_mm,
            _ => None,
        });
        assert!(wide.is_some_and(|w| w > 0.7 * P), "{:?}", f[0]);
    }

    #[test]
    fn square_tooth_has_no_measurable_flanks() {
        // vertical walls leave no samples in the middle half of the depth
        let (l, r) = shaft(&|t, a| {
            if (t - BAD).abs() < 0.5 * P {
                if (t - BAD).abs() < 0.25 * P {
                    R_MAJ
                } else {
                    R_MIN
                }
            } else {
                a
            }
        });
        let rep = inspect(&l, &r, &spec()).unwrap();
        let f = flagged_defects(&rep);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!((f[0].1 - BAD).abs() < 0.05);
        assert_eq!(
            f[0].2,
            vec![
                Defect::NoFlank { falling: false },
                Defect::NoFlank { falling: true }
            ]
        );
    }

    #[test]
    fn tilted_shaft_is_axis_corrected_and_clean() {
        let (mut l, mut r) = shaft(&|_, a| a);
        for p in l.iter_mut().chain(r.iter_mut()) {
            p.0 += 0.01 * p.1; // 0.573 degrees
        }
        let rep = inspect(&l, &r, &spec()).unwrap();
        assert!(rep.is_clean(), "{:?}", flagged_defects(&rep));
        assert!(rep.axis_refined);
        assert!(
            (rep.axis_tilt_deg - 0.01_f64.atan().to_degrees()).abs() < 0.03,
            "{}",
            rep.axis_tilt_deg
        );
        assert!((rep.reference_crest_r_mm - R_MAJ).abs() < 0.005);
    }

    #[test]
    fn coarser_rows_keep_healthy_flanks_measurable() {
        // every 2nd row: 0.06 mm spacing, 32 rows per pitch, ~5 samples per flank
        let (l, r) = shaft(&|_, a| a);
        let half = |v: &Pts| v.iter().step_by(2).copied().collect::<Pts>();
        let rep = inspect(&half(&l), &half(&r), &spec()).unwrap();
        assert!(rep.is_clean(), "{:?}", flagged_defects(&rep));
        for t in &rep.teeth {
            assert!((t.rise_half_angle_deg.unwrap() - FLANK_DEG).abs() < 1.0);
        }
        // every 4th row: 16 rows per pitch is too coarse to judge flanks at all
        let quarter = |v: &Pts| v.iter().step_by(4).copied().collect::<Pts>();
        assert_eq!(
            inspect(&quarter(&l), &quarter(&r), &spec()),
            Err(TeethError::BadInput)
        );
    }

    #[test]
    fn unusable_rows_and_specs_are_bad_input() {
        let (l, r) = shaft(&|_, a| a);
        let rev = |v: &Pts| v.iter().rev().copied().collect::<Pts>();
        assert_eq!(
            inspect(&rev(&l), &rev(&r), &spec()),
            Err(TeethError::BadInput)
        );
        for pitch_mm in [f64::INFINITY, f64::NAN, 0.1] {
            let bad = ThreadSpec { pitch_mm, ..spec() };
            assert_eq!(
                inspect(&l, &r, &bad),
                Err(TeethError::BadInput),
                "{pitch_mm}"
            );
        }
        for half_angle_deg in [0.0, 90.0, f64::NAN] {
            let bad = ThreadSpec {
                half_angle_deg,
                ..spec()
            };
            assert_eq!(inspect(&l, &r, &bad), Err(TeethError::BadInput));
        }
    }

    #[test]
    fn rejects_bad_input_and_flat_shafts() {
        let (l, r) = shaft(&|_, a| a);
        assert_eq!(
            inspect(&l[..10], &r[..10], &spec()),
            Err(TeethError::BadInput)
        );
        assert_eq!(inspect(&l, &r[1..], &spec()), Err(TeethError::BadInput));
        let bad = ThreadSpec {
            pitch_mm: 0.0,
            ..spec()
        };
        assert_eq!(inspect(&l, &r, &bad), Err(TeethError::BadInput));
        let (mut nan_l, _) = shaft(&|_, a| a);
        nan_l[3].0 = f64::NAN;
        assert_eq!(inspect(&nan_l, &r, &spec()), Err(TeethError::BadInput));
        let (fl, fr) = shaft(&|_, _| R_MAJ);
        let flat_r: Vec<(f64, f64)> = fr.iter().map(|p| (5.0 + R_MAJ, p.1)).collect();
        assert_eq!(
            inspect(&fl, &flat_r, &spec()),
            Err(TeethError::NoReferenceTeeth {
                side: Side::Left,
                found: 0
            })
        );
    }

    #[test]
    fn unified_spec_gives_the_13_tpi_pitch() {
        let s = ThreadSpec::unified(13.0);
        assert!((s.pitch_mm - 1.953_846).abs() < 1e-6);
        assert!((s.depth_mm - 1.198_6).abs() < 1e-3);
    }
}
