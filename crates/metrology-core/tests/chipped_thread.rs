//! Regression on a real silhouette: a 1/2-13 thread turned with a chipped insert, captured on the
//! live camera (calibration.txt map, tunnel light, untouched 8-frame mean).
use metrology_core::thread_teeth::{inspect, Defect, Side, ThreadSpec};

type Pts = Vec<(f64, f64)>;

fn load(text: &str, rows: std::ops::RangeInclusive<usize>) -> (Pts, Pts) {
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for line in text.lines().skip(1) {
        let v: Vec<f64> = line.split(',').map(|x| x.parse().unwrap()).collect();
        if rows.contains(&(v[0] as usize)) {
            l.push((v[1], v[2]));
            r.push((v[3], v[4]));
        }
    }
    (l, r)
}

fn report() -> metrology_core::thread_teeth::TeethReport {
    let (l, r) = load(include_str!("data/chipped_half_inch_13.csv"), 1060..=1300); // the whole thread, nothing of the neck above or below
    inspect(&l, &r, &ThreadSpec::unified(13.0)).unwrap()
}

/// The chipped insert left every tooth with a 52-60 degree ramp on its upper flank (the one
/// facing the shoulder, rising radius with image y) while the lower flanks are a healthy 30
/// degrees, and the bottom-most left tooth (left slot 2) is broken off.
#[test]
fn every_rising_flank_is_a_ramp_and_the_last_left_tooth_is_gone() {
    let rep = report();
    assert_eq!(rep.teeth.len(), 6);
    for t in &rep.teeth {
        if t.defects == [Defect::Missing] {
            assert_eq!((t.side, t.index), (Side::Left, 2));
            continue;
        }
        let rise = t.rise_half_angle_deg.unwrap();
        let fall = t.fall_half_angle_deg.unwrap();
        assert!((50.0..61.0).contains(&rise), "{:?} rise {rise}", t.side);
        assert!((29.5..31.0).contains(&fall), "{:?} fall {fall}", t.side);
        assert!(
            matches!(
                t.defects.as_slice(),
                [Defect::BadFlank { falling: false, .. }]
            ),
            "{:?}",
            t.defects
        );
        assert!(t.crest_r_mm > 6.2, "crest {}", t.crest_r_mm);
    }
    assert!(rep.axis_refined);
    assert!((rep.reference_crest_r_mm - 6.247).abs() < 0.01);
    assert!((rep.reference_root_r_mm - 5.336).abs() < 0.01);
}

/// Control: the certified 1/2-13 GO gage (same camera, same map) must not be flagged as
/// defective. Per-tooth flank half angles scatter 24.6 to 31.2 degrees on this real gage.
#[test]
fn healthy_go_gage_has_no_defects() {
    let (l, r) = load(include_str!("data/go_gage_half_inch_13.csv"), 940..=1500);
    let rep = inspect(&l, &r, &ThreadSpec::unified(13.0)).unwrap();
    assert_eq!(rep.teeth.len(), 15);
    assert!(rep.is_clean(), "{:?}", rep.flagged().collect::<Vec<_>>());
    assert!((rep.reference_crest_r_mm - 6.356).abs() < 0.01);
    assert!((rep.reference_root_r_mm - 5.030).abs() < 0.02);
    let angles: Vec<f64> = rep
        .teeth
        .iter()
        .flat_map(|t| [t.rise_half_angle_deg, t.fall_half_angle_deg])
        .map(Option::unwrap)
        .collect();
    assert!(
        angles.iter().all(|a| (24.0..32.0).contains(a)),
        "{angles:?}"
    );
}
