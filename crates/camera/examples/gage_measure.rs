//! Measure a vertical threaded gage on the live camera through a saved calibration.
//! Usage: gage_measure <calibration.txt> [bursts] [y_lo] [y_hi] [x_lo] [x_hi] [nominal_pd_in]
//! Each burst is an untouched 8-frame mean; the part is not moved between bursts.
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::field::FieldMap;
use metrology_core::image::ImageView;
use metrology_core::profile::{silhouette_edges, Profile};
use metrology_core::thread_measure::{measure_thread, ThreadFit};

const MM_PER_IN: f64 = 25.4;

type Mm = (f64, f64);

/// Left/right silhouette points (mm) for rows y_lo..y_hi step 2. Err names the first row that
/// could not be measured and why.
fn edge_points(
    img: &ImageView,
    map: &FieldMap,
    y: (usize, usize),
    x: (usize, usize),
) -> Result<(Vec<Mm>, Vec<Mm>), String> {
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for row in (y.0..y.1).step_by(2) {
        let prof = Profile::along_axis(img, true, row, x.0, x.1)
            .ok_or_else(|| format!("row {row}: bad scan range"))?;
        // a bright->dark edge into a large dark region on the left, dark->bright on the right;
        // dust specks and soft thread-flank edges are handled inside silhouette_edges
        let (l, r) = silhouette_edges(&prof.values)
            .ok_or_else(|| format!("row {row}: no silhouette (shaft) found in the scan window"))?;
        if r <= l + 50.0 {
            return Err(format!("row {row}: silhouette only {:.0} px wide", r - l));
        }
        let (xl, xr) = (x.0 as f64 + l, x.0 as f64 + r);
        let (pl, pr) = (
            map.to_mm(xl, row as f64).ok_or_else(|| {
                format!("row {row}: left edge x {xl:.1} is outside the calibrated region")
            })?,
            map.to_mm(xr, row as f64).ok_or_else(|| {
                format!("row {row}: right edge x {xr:.1} is outside the calibrated region")
            })?,
        );
        left.push(pl);
        right.push(pr);
    }
    Ok((left, right))
}

fn stats(v: &[f64]) -> (f64, f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let sd = if v.len() > 1 {
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    let range =
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
    (mean, sd, range)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let cal =
        std::fs::read_to_string(a.first().expect("calibration.txt")).expect("read calibration");
    let map = FieldMap::from_text(&cal).expect("parse calibration");
    let num = |i: usize, d: f64| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let bursts = num(1, 8.0) as usize;
    let (y_lo, y_hi, x_lo, x_hi) = (
        num(2, 940.0) as usize,
        num(3, 1500.0) as usize,
        num(4, 760.0) as usize,
        num(5, 1640.0) as usize,
    );
    let nominal_pd = num(6, 0.4500);
    println!("calibration: degree {} cv rms ({:.3}, {:.3}) px; rows {y_lo}..{y_hi}, x window {x_lo}..{x_hi}", map.degree(), map.cv_rms_px.0, map.cv_rms_px.1);
    let mut cam = PylonCamera::open(0).expect("open camera");
    let mut fits: Vec<ThreadFit> = Vec::new();
    for b in 0..bursts {
        let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
        let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
        let t = std::time::Instant::now();
        let result = edge_points(&img, &map, (y_lo, y_hi), (x_lo, x_hi))
            .and_then(|(l, r)| measure_thread(&l, &r).map_err(|e| format!("{e:?}")));
        match result {
            Ok(f) => {
                println!(
                    "burst {b}: {:?}  major {:.5} in  minor {:.5} in  PD(flank) {:.5} in ({:+.5} vs {nominal_pd})  [silhouette stat {:.5}, major-0.6495p {:.5}]  pitch {:.5} mm = {:.4} TPI  axis {:+.3} deg  flank angles L {:.2}/{:.2} R {:.2}/{:.2} deg  helix raise {:.1}/{:.1} um  rms {:.1}/{:.1} um",
                    t.elapsed(), f.major_mm / MM_PER_IN, f.minor_mm / MM_PER_IN, f.pd_mm / MM_PER_IN, f.pd_mm / MM_PER_IN - nominal_pd, f.pd_silhouette_mm / MM_PER_IN, (f.major_mm - 0.649_519 * f.pitch_mm) / MM_PER_IN, f.pitch_mm, f.tpi, f.axis_tilt_deg,
                    f.left_flank.rise_half_angle_deg, f.left_flank.fall_half_angle_deg, f.right_flank.rise_half_angle_deg, f.right_flank.fall_half_angle_deg,
                    f.left_flank.helix_shift_mm * 1e3, f.right_flank.helix_shift_mm * 1e3, f.left_flank.rms_mm * 1e3, f.right_flank.rms_mm * 1e3
                );
                if b == 0 {
                    println!(
                        "  left  side: {:?}\n  right side: {:?}\n  left  flank: {:?}\n  right flank: {:?}",
                        f.left, f.right, f.left_flank, f.right_flank
                    );
                }
                fits.push(f);
            }
            Err(e) => println!("burst {b}: FAILED {e}"),
        }
    }
    if fits.len() >= 2 {
        println!("\nover {} untouched bursts (inches):", fits.len());
        for (name, get) in [
            (
                "major",
                (|f: &ThreadFit| f.major_mm / MM_PER_IN) as fn(&ThreadFit) -> f64,
            ),
            ("minor", |f| f.minor_mm / MM_PER_IN),
            ("PD(flank)", |f| f.pd_mm / MM_PER_IN),
            ("pitch (mm)", |f| f.pitch_mm),
            ("TPI", |f| f.tpi),
        ] {
            let v: Vec<f64> = fits.iter().map(get).collect();
            let (mean, sd, range) = stats(&v);
            println!("  {name:<11} mean {mean:.5}  sd {sd:.5}  range {range:.5}");
        }
        let (mean, _, _) = stats(&fits.iter().map(|f| f.pd_mm / MM_PER_IN).collect::<Vec<_>>());
        println!(
            "  PD error vs nominal {nominal_pd} in: {:+.5} in (target +-0.0005)",
            mean - nominal_pd
        );
    }
}
