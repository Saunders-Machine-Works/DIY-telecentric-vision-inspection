//! Class 2A external Unified thread limits (ASME B1.1, as tabulated in Machinery's Handbook),
//! computed from the standard formulas, as editable data.
//!
//! The formulas (D = basic major diameter, P = pitch, both in inches):
//!   pitch diameter tolerance   Td2 = 0.0015 D^(1/3) + 0.0015 D^(1/2) + 0.015 P^(2/3)
//!                              (class 2A; length of engagement taken as D)
//!   allowance                  es  = 0.3 Td2
//!   major diameter             max = D - es,   min = max - 0.060 P^(2/3)
//!   pitch diameter             max = D - 0.649519 P - es,   min = max - Td2
//!   minor diameter             max = D - 1.226869 P - es  (the basic external minor less the
//!                              allowance; the tables give no minimum, so `minor_min` here is
//!                              the form-derived value `pd_min - 0.57735 P`)
//! These agree with the 1/4-20 and 1/2-13 class 2A limits in common tables to within 0.0001 in
//! (the tables round differently in the last digit). Every field is public so the user can edit
//! a limit to the drawing or to the handbook; nothing here is judged by this module.

pub const MM_PER_IN: f64 = 25.4;

/// Limits of one external thread, millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadLimits {
    pub name: String,
    pub pitch_mm: f64,
    pub major_max_mm: f64,
    pub major_min_mm: f64,
    pub pd_max_mm: f64,
    pub pd_min_mm: f64,
    pub minor_max_mm: f64,
    pub minor_min_mm: f64,
}

/// Where a measured diameter stands against a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Below,
    Within,
    Above,
    /// The value or a limit is not a number, or the minimum is above the maximum: nothing can be
    /// said, and it must never read as within.
    Invalid,
}

pub fn fit(value_mm: f64, min_mm: f64, max_mm: f64) -> Fit {
    if ![value_mm, min_mm, max_mm].iter().all(|v| v.is_finite()) || min_mm > max_mm {
        Fit::Invalid
    } else if value_mm < min_mm {
        Fit::Below
    } else if value_mm > max_mm {
        Fit::Above
    } else {
        Fit::Within
    }
}

impl ThreadLimits {
    /// Class 2A limits of the Unified thread of basic major diameter `major_in` (inches) and
    /// `tpi` threads per inch.
    pub fn unified_2a(name: &str, major_in: f64, tpi: f64) -> Self {
        let p = 1.0 / tpi;
        let td2 = 0.0015 * major_in.cbrt() + 0.0015 * major_in.sqrt() + 0.015 * p.powf(2.0 / 3.0);
        let es = 0.3 * td2;
        let major_max = major_in - es;
        let major_min = major_max - 0.060 * p.powf(2.0 / 3.0);
        let pd_max = major_in - 0.649_519 * p - es;
        let pd_min = pd_max - td2;
        let minor_max = major_in - 1.226_869 * p - es;
        let minor_min = pd_min - 0.577_35 * p;
        let mm = |v: f64| v * MM_PER_IN;
        ThreadLimits {
            name: name.to_string(),
            pitch_mm: mm(p),
            major_max_mm: mm(major_max),
            major_min_mm: mm(major_min),
            pd_max_mm: mm(pd_max),
            pd_min_mm: mm(pd_min),
            minor_max_mm: mm(minor_max),
            minor_min_mm: mm(minor_min),
        }
    }

    pub fn tpi(&self) -> f64 {
        MM_PER_IN / self.pitch_mm
    }
}

/// Basic major diameters (inches) of the numbered and fractional sizes #10 to 1/2 in.
const SIZES: [(&str, f64); 6] = [
    ("#10", 0.1900),
    ("1/4", 0.2500),
    ("5/16", 0.3125),
    ("3/8", 0.3750),
    ("7/16", 0.4375),
    ("1/2", 0.5000),
];

/// UNC and UNF threads per inch for each entry of `SIZES`.
const TPI: [(u32, u32); 6] = [(24, 32), (20, 28), (18, 24), (16, 24), (14, 20), (13, 20)];

/// Class 2A limits of the coarse (UNC) and fine (UNF) series from #10 to 1/2 in.
pub fn unified_catalog() -> Vec<ThreadLimits> {
    SIZES
        .iter()
        .zip(TPI)
        .flat_map(|(&(size, major), (unc, unf))| {
            [unc, unf]
                .map(|tpi| ThreadLimits::unified_2a(&format!("{size}-{tpi}"), major, tpi as f64))
        })
        .collect()
}

/// The catalogue entry with `tpi` threads per inch whose basic major diameter is nearest to
/// `measured_major_mm`.
pub fn nearest(
    catalog: &[ThreadLimits],
    tpi: f64,
    measured_major_mm: f64,
) -> Option<&ThreadLimits> {
    catalog
        .iter()
        .filter(|t| (t.tpi() - tpi).abs() < 0.05)
        .min_by(|a, b| {
            let d = |t: &ThreadLimits| (t.major_max_mm - measured_major_mm).abs();
            d(a).total_cmp(&d(b))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inches(l: &ThreadLimits) -> [f64; 6] {
        [
            l.major_max_mm,
            l.major_min_mm,
            l.pd_max_mm,
            l.pd_min_mm,
            l.minor_max_mm,
            l.minor_min_mm,
        ]
        .map(|v| v / MM_PER_IN)
    }

    /// 1/2-13 UNC 2A in common tables: major 0.4985 / 0.4876, pitch diameter 0.4485 / 0.4435,
    /// minor 0.4041 (max). The tables round the last digit differently: allow 0.0001.
    #[test]
    fn half_inch_13_matches_the_tables() {
        let l = ThreadLimits::unified_2a("1/2-13", 0.5, 13.0);
        let v = inches(&l);
        for (got, want) in v.iter().zip([0.4985, 0.4876, 0.4485, 0.4435, 0.4041]) {
            assert!((got - want).abs() < 1.1e-4, "{got} vs {want}");
        }
        assert!((l.pitch_mm - 1.953_846).abs() < 1e-5);
        assert!((l.tpi() - 13.0).abs() < 1e-9);
    }

    /// 1/4-20 UNC 2A: major 0.2489 / 0.2408, pitch diameter 0.2164 / 0.2127.
    #[test]
    fn quarter_20_matches_the_tables() {
        let v = inches(&ThreadLimits::unified_2a("1/4-20", 0.25, 20.0));
        for (got, want) in v.iter().zip([0.2489, 0.2408, 0.2164, 0.2127]) {
            assert!((got - want).abs() < 1.1e-4, "{got} vs {want}");
        }
    }

    #[test]
    fn catalog_has_both_series_for_every_size_and_nearest_picks_by_tpi_and_major() {
        let cat = unified_catalog();
        assert_eq!(cat.len(), 12);
        assert_eq!(cat[0].name, "#10-24");
        assert_eq!(cat[11].name, "1/2-20");
        // a crest diameter of 0.4918 in at 13 TPI is the 1/2-13, not 7/16-14
        let m = nearest(&cat, 13.0, 0.4918 * MM_PER_IN).unwrap();
        assert_eq!(m.name, "1/2-13");
        assert_eq!(
            nearest(&cat, 24.0, 0.186 * MM_PER_IN).unwrap().name,
            "#10-24"
        );
        assert!(nearest(&cat, 11.5, 12.0).is_none());
    }

    #[test]
    fn fit_classifies_against_a_range() {
        assert_eq!(fit(1.0, 2.0, 3.0), Fit::Below);
        assert_eq!(fit(2.0, 2.0, 3.0), Fit::Within);
        assert_eq!(fit(3.0, 2.0, 3.0), Fit::Within);
        assert_eq!(fit(3.1, 2.0, 3.0), Fit::Above);
    }

    #[test]
    fn nan_values_and_inverted_limits_are_invalid_not_within() {
        assert_eq!(fit(f64::NAN, 2.0, 3.0), Fit::Invalid);
        assert_eq!(fit(2.5, f64::NAN, 3.0), Fit::Invalid);
        assert_eq!(fit(2.5, 2.0, f64::INFINITY), Fit::Invalid);
        assert_eq!(fit(2.5, 3.0, 2.0), Fit::Invalid);
    }

    #[test]
    fn limits_are_editable_data() {
        let mut l = ThreadLimits::unified_2a("1/2-13", 0.5, 13.0);
        l.major_max_mm = 12.65;
        assert_eq!(fit(12.66, l.major_min_mm, l.major_max_mm), Fit::Above);
    }
}
