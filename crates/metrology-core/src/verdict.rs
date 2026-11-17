//! Whether a measurement may be believed, and the single verdict of a thread inspection.
//!
//! A reading taken in uneven light, with many rows missing or with ragged edges is not a
//! measurement of the part, so it gets no verdict at all (never PASS). A thread verdict combines
//! everything the inspection found: untrusted input, a tooth inspection or whole-thread fit that
//! could not run, defective teeth, and diameters or pitch diameter outside the limits. The order
//! of precedence is: untrusted or impossible to judge (NO VERDICT), then any failure (FAIL),
//! then PASS.

use crate::diameter::{Diameter, EdgeScan};
use crate::thread_standard::{fit as fit_range, Fit, ThreadLimits};
use crate::thread_teeth::TeethReport;

/// Background brightness difference beside the part above which the result is not trusted. A 5 %
/// ramp moved a 0.5 in pin by 6 um on the bench; 3 % keeps that under about 4 um.
pub const MAX_IMBALANCE: f64 = 0.03;
/// Most of the requested rows that may fail to measure.
pub const MAX_MISSING_FRACTION: f64 = 0.2;
/// Largest row-to-row spread of a plain diameter, mm; more means ragged edges or a part that is
/// not a clean cylinder in the window (good pins read 0.001 to 0.002 mm).
pub const MAX_ROW_SD_MM: f64 = 0.01;
/// Largest relative difference between the measured pitch and the selected thread's.
pub const MAX_PITCH_ERROR: f64 = 0.02;

/// True if `value` is above `limit` or not a number (a NaN reading is never within a limit).
fn exceeds(value: f64, limit: f64) -> bool {
    value.is_nan() || value > limit
}

#[derive(Debug, Clone, PartialEq)]
pub enum Untrusted {
    UnevenLight {
        imbalance: f64,
    },
    /// No background margin wide enough beside the part to judge the light.
    LightNotJudged,
    MissingRows {
        used: usize,
        requested: usize,
    },
    RaggedEdges {
        sd_mm: f64,
    },
}

impl std::fmt::Display for Untrusted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Untrusted::UnevenLight { imbalance } => write!(
                f,
                "uneven light: the background beside the part differs by {:.0} %, centre the part in the slot",
                imbalance.abs() * 200.0
            ),
            Untrusted::LightNotJudged => write!(
                f,
                "the light beside the part could not be judged: widen the scan window so it holds background on both sides"
            ),
            Untrusted::MissingRows { used, requested } => write!(
                f,
                "only {used} of {requested} rows found the part: check the scan window"
            ),
            Untrusted::RaggedEdges { sd_mm } => write!(
                f,
                "the width varies {:.1} um between rows: dirt, a burr or not a plain cylinder here",
                sd_mm * 1000.0
            ),
        }
    }
}

/// What makes a scan untrusted (empty if nothing does).
pub fn assess_scan(scan: &EdgeScan) -> Vec<Untrusted> {
    let mut out = Vec::new();
    match scan.illumination_imbalance {
        None => out.push(Untrusted::LightNotJudged),
        Some(i) if exceeds(i.abs(), MAX_IMBALANCE) => {
            out.push(Untrusted::UnevenLight { imbalance: i });
        }
        Some(_) => {}
    }
    let (used, requested) = (scan.rows.len(), scan.rows_requested);
    if (used as f64) < (1.0 - MAX_MISSING_FRACTION) * requested as f64 {
        out.push(Untrusted::MissingRows { used, requested });
    }
    out
}

/// What makes a plain-diameter measurement untrusted (empty if nothing does).
pub fn assess_diameter(d: &Diameter) -> Vec<Untrusted> {
    let mut out = Vec::new();
    match d.illumination_imbalance {
        None => out.push(Untrusted::LightNotJudged),
        Some(i) if exceeds(i.abs(), MAX_IMBALANCE) => {
            out.push(Untrusted::UnevenLight { imbalance: i });
        }
        Some(_) => {}
    }
    let requested = d.rows_used + d.rows_skipped;
    if (d.rows_used as f64) < (1.0 - MAX_MISSING_FRACTION) * requested as f64 {
        out.push(Untrusted::MissingRows {
            used: d.rows_used,
            requested,
        });
    }
    if exceeds(d.row_sd_mm, MAX_ROW_SD_MM) {
        out.push(Untrusted::RaggedEdges { sd_mm: d.row_sd_mm });
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThreadVerdict {
    Pass,
    Fail(Vec<String>),
    /// Cannot be judged: the reasons.
    NoVerdict(Vec<String>),
}

/// The verdict of a thread inspection. `fit` is the whole-thread fit's (pitch diameter, pitch) in
/// mm (`Err` text if it was refused); `teeth` the per-tooth report (or why it failed). A measured
/// pitch more than 2 % from the selected thread's means the wrong size is selected.
pub fn thread_verdict(
    limits: &ThreadLimits,
    scan: &EdgeScan,
    fit: Result<(f64, f64), &str>,
    teeth: Result<&TeethReport, &str>,
) -> ThreadVerdict {
    let mut no_verdict: Vec<String> = assess_scan(scan).iter().map(ToString::to_string).collect();
    let mut failures = Vec::new();
    match teeth {
        Err(e) => no_verdict.push(format!("the teeth could not be inspected ({e})")),
        Ok(rep) => {
            let flagged = rep.flagged().count();
            if flagged > 0 {
                failures.push(format!(
                    "{flagged} of {} teeth are defective",
                    rep.teeth.len()
                ));
            }
            for (what, d_mm, lo, hi) in [
                (
                    "major diameter",
                    2.0 * rep.reference_crest_r_mm,
                    limits.major_min_mm,
                    limits.major_max_mm,
                ),
                (
                    "minor diameter",
                    2.0 * rep.reference_root_r_mm,
                    limits.minor_min_mm,
                    limits.minor_max_mm,
                ),
            ] {
                match fit_range(d_mm, lo, hi) {
                    Fit::Within => {}
                    Fit::Below => failures.push(format!("{what} below its minimum")),
                    Fit::Above => failures.push(format!("{what} above its maximum")),
                    Fit::Invalid => no_verdict.push(format!("the {what} limits are not valid")),
                }
            }
        }
    }
    match fit {
        Ok((pd, pitch)) => {
            if (pitch / limits.pitch_mm - 1.0).abs() > MAX_PITCH_ERROR || !pitch.is_finite() {
                no_verdict.push(format!(
                    "the measured pitch {:.3} TPI is not the selected thread's {:.3} TPI: select the right size",
                    25.4 / pitch,
                    limits.tpi()
                ));
            }
            match fit_range(pd, limits.pd_min_mm, limits.pd_max_mm) {
                Fit::Within => {}
                Fit::Below => failures.push("pitch diameter below its minimum".to_string()),
                Fit::Above => failures.push("pitch diameter above its maximum".to_string()),
                Fit::Invalid => {
                    no_verdict.push("the pitch diameter limits are not valid".to_string());
                }
            }
        }
        // a thread already failing on its teeth need not also fit as a whole; otherwise the
        // missing pitch diameter means the thread cannot be passed
        Err(e) if failures.is_empty() => {
            no_verdict.push(format!(
                "no pitch diameter: the whole-thread fit was refused ({e})"
            ));
        }
        Err(_) => {}
    }
    if !no_verdict.is_empty() {
        ThreadVerdict::NoVerdict(no_verdict)
    } else if !failures.is_empty() {
        ThreadVerdict::Fail(failures)
    } else {
        ThreadVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_teeth::{Defect, Side, Tooth};

    fn scan(imbalance: Option<f64>, used: usize, requested: usize) -> EdgeScan {
        EdgeScan {
            rows: (0..used).collect(),
            left_px: Vec::new(),
            right_px: Vec::new(),
            left_mm: Vec::new(),
            right_mm: Vec::new(),
            rows_requested: requested,
            illumination_imbalance: imbalance,
        }
    }

    fn diameter(imbalance: Option<f64>, sd: f64, used: usize, skipped: usize) -> Diameter {
        Diameter {
            diameter_mm: 12.7,
            row_sd_mm: sd,
            rows_used: used,
            rows_skipped: skipped,
            taper: 0.0,
            tilt_deg: 0.0,
            illumination_imbalance: imbalance,
            left_px: Vec::new(),
            right_px: Vec::new(),
        }
    }

    fn tooth(defects: Vec<Defect>) -> Tooth {
        Tooth {
            side: Side::Left,
            index: 0,
            t_mm: 0.0,
            crest_r_mm: 6.3,
            root_r_mm: 5.1,
            width_mm: Some(1.0),
            centre_offset_mm: Some(0.0),
            rise_half_angle_deg: Some(30.0),
            fall_half_angle_deg: Some(30.0),
            defects,
        }
    }

    /// Crest diameter 12.6 mm (0.4961 in), root 10.2 mm (0.4016 in): inside 1/2-13 2A limits.
    fn report(defects: Vec<Defect>) -> TeethReport {
        TeethReport {
            teeth: vec![tooth(defects)],
            reference_crest_r_mm: 6.3,
            reference_root_r_mm: 5.1,
            axis_tilt_deg: 0.0,
            axis_refined: true,
        }
    }

    fn limits() -> ThreadLimits {
        ThreadLimits::unified_2a("1/2-13", 0.5, 13.0)
    }

    fn pd_in_limits() -> (f64, f64) {
        (
            0.5 * (limits().pd_min_mm + limits().pd_max_mm),
            limits().pitch_mm,
        )
    }

    #[test]
    fn clean_even_light_diameter_is_trusted() {
        assert!(assess_diameter(&diameter(Some(0.01), 0.002, 500, 20)).is_empty());
    }

    #[test]
    fn an_unjudged_light_balance_is_not_trusted() {
        let d = assess_diameter(&diameter(None, 0.002, 500, 0));
        assert_eq!(d, vec![Untrusted::LightNotJudged]);
        assert_eq!(
            assess_scan(&scan(None, 500, 500)),
            vec![Untrusted::LightNotJudged]
        );
        assert!(d[0].to_string().contains("widen the scan window"));
    }

    #[test]
    fn each_untrusted_cause_is_named_with_its_numbers() {
        let u = assess_diameter(&diameter(Some(-0.05), 0.02, 300, 300));
        assert_eq!(u.len(), 3, "{u:?}");
        assert_eq!(u[0], Untrusted::UnevenLight { imbalance: -0.05 });
        assert_eq!(
            u[1],
            Untrusted::MissingRows {
                used: 300,
                requested: 600
            }
        );
        assert_eq!(u[2], Untrusted::RaggedEdges { sd_mm: 0.02 });
        assert!(u[0].to_string().contains("10 %"), "{}", u[0]);
    }

    #[test]
    fn nan_measurements_are_untrusted_not_passed() {
        let u = assess_diameter(&diameter(Some(f64::NAN), f64::NAN, 500, 0));
        assert_eq!(u.len(), 2, "{u:?}");
        assert_eq!(assess_scan(&scan(Some(f64::NAN), 500, 500)).len(), 1);
    }

    #[test]
    fn scan_trust_checks_light_and_missing_rows() {
        assert!(assess_scan(&scan(Some(0.02), 450, 500)).is_empty());
        let u = assess_scan(&scan(Some(0.04), 350, 500));
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn a_clean_thread_inside_limits_passes() {
        let (l, s) = (limits(), scan(Some(0.0), 500, 500));
        let r = report(vec![]);
        assert_eq!(
            thread_verdict(&l, &s, Ok(pd_in_limits()), Ok(&r)),
            ThreadVerdict::Pass
        );
    }

    #[test]
    fn defective_teeth_fail_even_when_the_whole_fit_was_refused() {
        let (l, s) = (limits(), scan(Some(0.0), 500, 500));
        let r = report(vec![Defect::Missing]);
        match thread_verdict(&l, &s, Err("PitchMismatch"), Ok(&r)) {
            ThreadVerdict::Fail(why) => assert_eq!(why, vec!["1 of 1 teeth are defective"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_clean_thread_without_a_pitch_diameter_has_no_verdict() {
        let (l, s) = (limits(), scan(Some(0.0), 500, 500));
        let r = report(vec![]);
        match thread_verdict(&l, &s, Err("TooFewPeriods"), Ok(&r)) {
            ThreadVerdict::NoVerdict(why) => assert!(why[0].contains("TooFewPeriods"), "{why:?}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn failed_teeth_inspection_and_uneven_light_give_no_verdict() {
        let l = limits();
        let even = scan(Some(0.0), 500, 500);
        assert!(matches!(
            thread_verdict(&l, &even, Ok(pd_in_limits()), Err("NoTeeth")),
            ThreadVerdict::NoVerdict(_)
        ));
        let uneven = scan(Some(0.06), 500, 500);
        let r = report(vec![Defect::Missing]);
        // untrusted input outranks a failure: the defect may be an artefact
        assert!(matches!(
            thread_verdict(&l, &uneven, Ok(pd_in_limits()), Ok(&r)),
            ThreadVerdict::NoVerdict(_)
        ));
    }

    #[test]
    fn pitch_diameter_and_diameters_outside_the_limits_fail() {
        let (l, s) = (limits(), scan(Some(0.0), 500, 500));
        let r = report(vec![]);
        match thread_verdict(&l, &s, Ok((l.pd_max_mm + 0.01, l.pitch_mm)), Ok(&r)) {
            ThreadVerdict::Fail(why) => assert_eq!(why, vec!["pitch diameter above its maximum"]),
            other => panic!("{other:?}"),
        }
        let mut shallow = report(vec![]);
        shallow.reference_root_r_mm = 5.35; // root diameter 10.7 mm, above the 10.27 mm maximum
        match thread_verdict(&l, &s, Ok(pd_in_limits()), Ok(&shallow)) {
            ThreadVerdict::Fail(why) => assert_eq!(why, vec!["minor diameter above its maximum"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_measured_pitch_that_is_not_the_selected_threads_has_no_verdict() {
        let (l, s, r) = (limits(), scan(Some(0.0), 500, 500), report(vec![]));
        let (pd, pitch) = pd_in_limits();
        // 3 % coarse: the 13 TPI thread is selected but about 12.6 TPI was measured
        match thread_verdict(&l, &s, Ok((pd, pitch * 1.03)), Ok(&r)) {
            ThreadVerdict::NoVerdict(why) => assert!(why[0].contains("12.62"), "{why:?}"),
            other => panic!("{other:?}"),
        }
        // 1 % is inside the tolerance
        assert_eq!(
            thread_verdict(&l, &s, Ok((pd, pitch * 1.01)), Ok(&r)),
            ThreadVerdict::Pass
        );
    }

    #[test]
    fn invalid_limits_give_no_verdict_never_a_pass() {
        let (s, r) = (scan(Some(0.0), 500, 500), report(vec![]));
        let mut nan = limits();
        nan.major_max_mm = f64::NAN;
        assert!(matches!(
            thread_verdict(&nan, &s, Ok(pd_in_limits()), Ok(&r)),
            ThreadVerdict::NoVerdict(_)
        ));
        let mut swapped = limits();
        std::mem::swap(&mut swapped.pd_min_mm, &mut swapped.pd_max_mm);
        assert!(matches!(
            thread_verdict(&swapped, &s, Ok(pd_in_limits()), Ok(&r)),
            ThreadVerdict::NoVerdict(_)
        ));
    }
}
