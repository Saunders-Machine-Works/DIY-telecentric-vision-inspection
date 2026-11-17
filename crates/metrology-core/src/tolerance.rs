//! Tolerance verdicts for a measured dimension: PASS, WARN (inside the limits but close to one)
//! or FAIL, with an optional guard band.
//!
//! A `Spec` is `nominal` with a lower and an upper deviation (`lower <= 0 <= upper`, the usual
//! drawing form `nominal +upper / -lower`). The deviation is `value - nominal`. Without a guard
//! band the limits are inclusive. A guard band `g` (ISO 14253-1 style, from the measurement
//! uncertainty) shrinks the PASS zone by `g` at each limit, so a reading inside the limits but
//! within `g` of one cannot be called a clean pass; it becomes WARN, and only a reading beyond
//! the limit itself is FAIL. The WARN zone is also at least the outer `WARN_FRACTION` of the
//! tolerance band on the side where the deviation falls (the shop habit of flagging a part that
//! uses up most of its tolerance), whichever is wider.

/// A deviation beyond this fraction of the limit on its side is a warning.
pub const WARN_FRACTION: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    pub nominal: f64,
    /// Upper deviation, >= 0.
    pub upper: f64,
    /// Lower deviation, <= 0.
    pub lower: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Warn,
    Fail,
}

impl Spec {
    /// `nominal +upper / -lower_magnitude`; `None` if the numbers are not finite, a magnitude
    /// is negative, or both are zero.
    pub fn new(nominal: f64, upper: f64, lower_magnitude: f64) -> Option<Spec> {
        let ok = [nominal, upper, lower_magnitude]
            .iter()
            .all(|v| v.is_finite())
            && upper >= 0.0
            && lower_magnitude >= 0.0
            && upper + lower_magnitude > 0.0;
        ok.then_some(Spec {
            nominal,
            upper,
            lower: -lower_magnitude,
        })
    }

    /// Limits as absolute values, (lowest, highest).
    pub fn limits(&self) -> (f64, f64) {
        (self.nominal + self.lower, self.nominal + self.upper)
    }

    /// Deviation of `value` from the nominal.
    pub fn deviation(&self, value: f64) -> f64 {
        value - self.nominal
    }

    /// Verdict for `value`, with `guard` (>= 0, the same unit) held back at each limit.
    pub fn verdict(&self, value: f64, guard: f64) -> Verdict {
        if !value.is_finite() {
            return Verdict::Fail;
        }
        let dev = self.deviation(value);
        if dev > self.upper || dev < self.lower {
            return Verdict::Fail;
        }
        if !guard.is_finite() {
            return Verdict::Fail;
        }
        let guard = guard.max(0.0);
        let (side, limit) = if dev >= 0.0 {
            (dev, self.upper)
        } else {
            (-dev, -self.lower)
        };
        let warn_from = (WARN_FRACTION * limit).min(limit - guard);
        if side > warn_from {
            Verdict::Warn
        } else {
            Verdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin() -> Spec {
        // 0.5000 +0.0005 / -0.0005
        Spec::new(0.5, 0.0005, 0.0005).unwrap()
    }

    #[test]
    fn limits_and_deviation_have_real_values() {
        let s = Spec::new(0.4985, 0.0, 0.0109).unwrap();
        let (lo, hi) = s.limits();
        assert!((lo - 0.4876).abs() < 1e-12 && (hi - 0.4985).abs() < 1e-12);
        assert!((s.deviation(0.4900) - (-0.0085)).abs() < 1e-12);
    }

    #[test]
    fn pass_warn_fail_zones_for_a_symmetric_pin() {
        let s = pin();
        assert_eq!(s.verdict(0.50000, 0.0), Verdict::Pass);
        assert_eq!(s.verdict(0.50039, 0.0), Verdict::Pass); // 78 % of the limit
        assert_eq!(s.verdict(0.50041, 0.0), Verdict::Warn); // 82 %
        assert_eq!(s.verdict(0.49959, 0.0), Verdict::Warn);
        assert_eq!(s.verdict(0.50049, 0.0), Verdict::Warn); // just inside the limit
        assert_eq!(s.verdict(0.50051, 0.0), Verdict::Fail);
        assert_eq!(s.verdict(0.49949, 0.0), Verdict::Fail);
    }

    #[test]
    fn one_sided_tolerances_only_warn_toward_the_toleranced_side() {
        // a major diameter: max 0.4985, no plus tolerance, 0.0109 minus
        let s = Spec::new(0.4985, 0.0, 0.0109).unwrap();
        assert_eq!(s.verdict(0.4985, 0.0), Verdict::Pass); // on the nominal
        assert_eq!(s.verdict(0.4986, 0.0), Verdict::Fail); // any plus deviation is out
        assert_eq!(s.verdict(0.4900, 0.0), Verdict::Pass);
        assert_eq!(s.verdict(0.4890, 0.0), Verdict::Warn); // 0.0095 of 0.0109
        assert_eq!(s.verdict(0.4870, 0.0), Verdict::Fail);
    }

    #[test]
    fn a_guard_band_turns_near_limit_readings_into_warnings() {
        let s = pin();
        // 0.0001 guard: PASS only up to 0.0004 from nominal (the 80 % rule gives 0.0004 as well)
        assert_eq!(s.verdict(0.50035, 0.0001), Verdict::Pass);
        let wide = Spec::new(0.5, 0.005, 0.005).unwrap(); // 80 % = 0.004
        assert_eq!(wide.verdict(0.5039, 0.0), Verdict::Pass);
        assert_eq!(wide.verdict(0.5040, 0.0015), Verdict::Warn); // guard reaches 0.0035
        assert_eq!(wide.verdict(0.5051, 0.0015), Verdict::Fail); // a guard never fails in-limit
        assert_eq!(wide.verdict(0.5049, 0.0015), Verdict::Warn);
    }

    #[test]
    fn bad_numbers_never_pass() {
        assert_eq!(pin().verdict(f64::NAN, 0.0), Verdict::Fail);
        assert_eq!(pin().verdict(f64::INFINITY, 0.0), Verdict::Fail);
        assert!(Spec::new(0.5, -0.1, 0.1).is_none());
        assert!(Spec::new(0.5, 0.1, -0.1).is_none());
        assert!(Spec::new(0.5, 0.0, 0.0).is_none());
        assert!(Spec::new(f64::NAN, 0.1, 0.1).is_none());
        // a negative guard is treated as none; a guard that is not a number never passes
        assert_eq!(pin().verdict(0.50000, -1.0), Verdict::Pass);
        assert_eq!(pin().verdict(0.50000, f64::NAN), Verdict::Fail);
        assert_eq!(pin().verdict(0.50000, f64::INFINITY), Verdict::Fail);
    }
}
