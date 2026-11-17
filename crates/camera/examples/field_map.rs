//! Distortion map on the real reticle, validated on edges the map never saw.
//!
//! The map is fitted only on edges inside 80 px blocks of one checkerboard parity (including the
//! edges the linear lattice fit clipped). Chords of exactly N squares (N * 0.6 mm) are then
//! measured between edges that both lie in the OTHER parity's blocks, i.e. in regions the map
//! was never trained on, and compared with the truth. Truth is "N squares in the board plane",
//! so only the BoardPlane policy is meaningful here.
//!
//! It also compares the direction of the local-scale gradient with the direction in which the
//! board is foreshortened: a keystone effect from tilt would make them line up.
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::field::{FieldMap, TiltPolicy};
use metrology_core::image::ImageView;
use metrology_core::lattice::{fit_lattice, LatticeFit, LatticeParams, LatticePoint};
use std::collections::HashMap;

const SQUARE_MM: f64 = 0.6;
const BLOCK: f64 = 80.0;

/// Block parity of an image position (0 = training, 1 = test).
fn parity(x: f64, y: f64) -> usize {
    (((x / BLOCK).floor() as i64 + (y / BLOCK).floor() as i64) & 1) as usize
}

fn image_pos(q: &LatticePoint, vertical: bool) -> (f64, f64) {
    if vertical {
        (q.s, q.c)
    } else {
        (q.c, q.s)
    }
}

/// Chord errors (ppm of the true length) for chords of `k` squares whose ends are both in
/// test blocks.
fn chord_errors(map: &FieldMap, lat: &LatticeFit, k: i64) -> Vec<f64> {
    let mut out = Vec::new();
    for (fam, vertical) in [(&lat.vertical, true), (&lat.horizontal, false)] {
        // (scan line, grid-line index) -> mean edge position along the scan; test blocks only
        let mut by_line: HashMap<(i64, i64), Vec<f64>> = HashMap::new();
        for q in &fam.samples {
            let (x, y) = image_pos(q, vertical);
            if parity(x, y) == 1 {
                by_line.entry((q.c as i64, q.index)).or_default().push(q.s);
            }
        }
        let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
        for (&(c, i), a) in &by_line {
            if let Some(b) = by_line.get(&(c, i + k)) {
                let (sa, sb) = (mean(a), mean(b));
                let (pa, pb) = if vertical {
                    ((sa, c as f64), (sb, c as f64))
                } else {
                    ((c as f64, sa), (c as f64, sb))
                };
                // The map refuses (`distance_mm` -> None) where it had no training edges, which
                // is exactly the held-out blocks, so evaluate the polynomial directly.
                let (ga, gb) = (map.grid_coords(pa.0, pa.1), map.grid_coords(pb.0, pb.1));
                let d = (ga.0 - gb.0).hypot(ga.1 - gb.1) * SQUARE_MM;
                let want = k as f64 * SQUARE_MM;
                out.push((d - want) / want * 1e6);
            }
        }
    }
    out
}

fn stats(v: &[f64]) -> String {
    if v.is_empty() {
        return "no chords".into();
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let rms = (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt();
    let worst = v.iter().fold(0.0f64, |a, x| a.max(x.abs()));
    format!(
        "n {:5}  mean {:+6.0} ppm  rms {:5.0} ppm  worst {:5.0} ppm",
        v.len(),
        mean,
        rms,
        worst
    )
}

fn main() {
    let mut cam = PylonCamera::open(0).expect("open camera");
    let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
    let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
    let lat = fit_lattice(&img, &LatticeParams::default()).expect("lattice fit");
    println!(
        "lattice: {:.5} px/sq = {:.4} um/px; edges {} / {} (linear fit clipped {} / {})",
        lat.sigma_max,
        lat.um_per_px(SQUARE_MM),
        lat.vertical.samples.len(),
        lat.horizontal.samples.len(),
        lat.vertical.clipped,
        lat.horizontal.clipped
    );
    let mut train = lat.clone();
    for (fam, vertical) in [(&mut train.vertical, true), (&mut train.horizontal, false)] {
        fam.samples.retain(|q| {
            let (x, y) = image_pos(q, vertical);
            parity(x, y) == 0
        });
    }
    // 16 squares = ~4 blocks (same parity at both ends); 20 would be ~5 blocks (opposite parity)
    for k in [10i64, 16, 30] {
        println!(
            "\nchords of {k} squares ({:.1} mm), both ends in held-out blocks:",
            k as f64 * SQUARE_MM
        );
        for degree in 1..=4 {
            let map = FieldMap::fit(&train, SQUARE_MM, degree, TiltPolicy::BoardPlane)
                .expect("field fit");
            println!("  degree {degree}: {}", stats(&chord_errors(&map, &lat, k)));
        }
    }

    println!("\nfull-data maps (in-sample rms px, spatial-CV rms px, clipped):");
    for degree in 1..=5 {
        let map =
            FieldMap::fit(&lat, SQUARE_MM, degree, TiltPolicy::BoardPlane).expect("field fit");
        println!(
            "  degree {degree}: rms ({:.3}, {:.3})  cv ({:.3}, {:.3})  clipped {:?}  apparent tilt {:.2} deg",
            map.rms_px.0, map.rms_px.1, map.cv_rms_px.0, map.cv_rms_px.1, map.clipped, map.apparent_tilt_deg()
        );
    }

    // local scale over the calibrated region (degree 3) and the direction of its gradient
    let map = FieldMap::fit(&lat, SQUARE_MM, 3, TiltPolicy::BoardPlane).unwrap();
    let (x0, x1, y0, y1) = map.region();
    println!("\ncalibrated region x {x0:.0}..{x1:.0}, y {y0:.0}..{y1:.0} (bounding box); local um/px along x / along y:");
    let mut pts = Vec::new();
    for fy in [0.12, 0.5, 0.88] {
        for fx in [0.12, 0.5, 0.88] {
            let (x, y) = (x0 + fx * (x1 - x0), y0 + fy * (y1 - y0));
            if let (Some(sx), Some(sy)) = (
                map.local_um_per_px(x, y, (1.0, 0.0)),
                map.local_um_per_px(x, y, (0.0, 1.0)),
            ) {
                println!("  ({x:6.0},{y:6.0})  {sx:.4} / {sy:.4}");
                pts.push((x, y, 0.5 * (sx + sy)));
            }
        }
    }
    // least-squares plane s = s0 + gx*x + gy*y
    let n = pts.len() as f64;
    let (mx, my, ms) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
        pts.iter().map(|p| p.2).sum::<f64>() / n,
    );
    let (sxx, syy, sxy, sxs, sys) = pts.iter().fold((0.0, 0.0, 0.0, 0.0, 0.0), |a, p| {
        let (dx, dy, ds) = (p.0 - mx, p.1 - my, p.2 - ms);
        (
            a.0 + dx * dx,
            a.1 + dy * dy,
            a.2 + dx * dy,
            a.3 + dx * ds,
            a.4 + dy * ds,
        )
    });
    let det = sxx * syy - sxy * sxy;
    let (gx, gy) = ((sxs * syy - sys * sxy) / det, (sys * sxx - sxs * sxy) / det);
    let ang = gy.atan2(gx).to_degrees();
    println!(
        "scale gradient: {:.1} ppm per 1000 px toward {:.0} deg (0 = +x, 90 = +y; image y points down)",
        gx.hypot(gy) / ms * 1e6 * 1000.0,
        ang
    );
    // direction in which the lattice is compressed (smallest singular direction of [a1 a2])
    let (a1, a2) = (lat.a1, lat.a2);
    let (pxx, pyy, pxy) = (
        a1.0 * a1.0 + a2.0 * a2.0,
        a1.1 * a1.1 + a2.1 * a2.1,
        a1.0 * a1.1 + a2.0 * a2.1,
    );
    let theta_max = 0.5 * (2.0 * pxy).atan2(pxx - pyy);
    let compress = (theta_max.to_degrees() + 90.0).rem_euclid(180.0);
    println!(
        "board foreshortening (apparent tilt {:.2} deg, noise level ~1.5 deg) acts along {:.0} deg (mod 180)",
        lat.tilt_deg, compress
    );
    let diff = ((ang - compress).rem_euclid(180.0)).min(180.0 - (ang - compress).rem_euclid(180.0));
    println!("angle between gradient and foreshortening axes: {diff:.0} deg (0 or 90 would be suspicious for keystone only if the tilt were significant)");
}
