//! Measure a certified plain pin on the live camera through a saved calibration, to isolate the
//! silhouette edge offset from any thread geometry.
//! Usage: pin_measure <calibration.txt> <certified_in> [bursts] [y_lo] [y_hi] [x_lo] [x_hi] [h]
//! Pass `h` last for a horizontal pin (diameter measured along y; windows then mean: x_lo..x_hi
//! is the span of columns scanned, y_lo..y_hi the vertical window that contains both edges).
//! Prints the width per burst, its row-to-row spread, the left/right centre, and the implied
//! per-side edge offset (certified minus measured, halved).
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::field::FieldMap;
use metrology_core::image::ImageView;
use metrology_core::profile::{silhouette_edges, Profile};

const MM_PER_IN: f64 = 25.4;

fn mean_sd(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let sd = if v.len() > 1 {
        (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    (m, sd)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let cal = std::fs::read_to_string(a.first().expect("calibration.txt")).expect("read cal");
    let map = FieldMap::from_text(&cal).expect("parse calibration");
    let certified: f64 = a.get(1).expect("certified_in").parse().expect("number");
    let num = |i: usize, d: f64| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let bursts = num(2, 6.0) as usize;
    let (y_lo, y_hi, x_lo, x_hi) = (
        num(3, 900.0) as usize,
        num(4, 1500.0) as usize,
        num(5, 800.0) as usize,
        num(6, 1600.0) as usize,
    );
    let horizontal = a.get(7).is_some_and(|s| s == "h");
    let mut cam = PylonCamera::open(0).expect("open camera");
    let mut widths = Vec::new();
    for b in 0..bursts {
        let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
        let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
        let (mut w_in, mut lpx, mut rpx, mut ys) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut skipped = 0usize;
        // vertical pin: one horizontal scan per row in y_lo..y_hi across x_lo..x_hi;
        // horizontal pin: one vertical scan per column in x_lo..x_hi across y_lo..y_hi
        let (lines, (lo, hi)) = if horizontal {
            (x_lo..x_hi, (y_lo, y_hi))
        } else {
            (y_lo..y_hi, (x_lo, x_hi))
        };
        for c in lines {
            let Some(p) = Profile::along_axis(&img, !horizontal, c, lo, hi) else {
                skipped += 1;
                continue;
            };
            let Some((l, r)) = silhouette_edges(&p.values) else {
                skipped += 1;
                continue;
            };
            let (sl, sr) = (lo as f64 + l, lo as f64 + r);
            let at = |s: f64| {
                if horizontal {
                    (c as f64, s)
                } else {
                    (s, c as f64)
                }
            };
            let (Some(pl), Some(pr)) =
                (map.to_mm(at(sl).0, at(sl).1), map.to_mm(at(sr).0, at(sr).1))
            else {
                skipped += 1;
                continue;
            };
            w_in.push((pr.0 - pl.0).hypot(pr.1 - pl.1) / MM_PER_IN);
            lpx.push(sl);
            rpx.push(sr);
            ys.push(c as f64);
        }
        if w_in.len() < 50 {
            println!(
                "burst {b}: only {} rows measured ({skipped} skipped)",
                w_in.len()
            );
            continue;
        }
        let (w, sd) = mean_sd(&w_in);
        // slope of the width along the pin (taper) and of each edge (tilt), px per row
        let slope = |v: &[f64]| {
            let (my, mv) = (
                ys.iter().sum::<f64>() / ys.len() as f64,
                v.iter().sum::<f64>() / v.len() as f64,
            );
            let num: f64 = ys.iter().zip(v).map(|(y, x)| (y - my) * (x - mv)).sum();
            let den: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
            num / den
        };
        println!(
            "burst {b}: rows {} (skipped {skipped})  width {w:.5} in  row sd {sd:.5}  edge slopes L {:+.5} R {:+.5} px/row  centre x {:.2} px  (certified {certified}: {:+.5} in, {:+.2} um/side)",
            w_in.len(),
            slope(&lpx),
            slope(&rpx),
            0.5 * (lpx.iter().sum::<f64>() + rpx.iter().sum::<f64>()) / lpx.len() as f64,
            w - certified,
            (certified - w) * MM_PER_IN * 1000.0 / 2.0
        );
        widths.push(w);
    }
    if widths.len() >= 2 {
        let (m, sd) = mean_sd(&widths);
        println!(
            "\nover {} bursts: width {m:.5} in sd {sd:.5}; error vs certified {certified}: {:+.5} in = {:+.1} um on diameter, {:+.2} um per side",
            widths.len(),
            m - certified,
            (m - certified) * MM_PER_IN * 1000.0,
            (certified - m) * MM_PER_IN * 1000.0 / 2.0
        );
    }
}
