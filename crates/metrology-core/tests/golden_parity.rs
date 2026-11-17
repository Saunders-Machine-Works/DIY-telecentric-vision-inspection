//! Parity of the Rust core with scipy/numpy reference vectors produced by
//! tools/ref/gen_golden.py (checked in under tests/golden). Tolerances are ~1e-9.
//!
//! Independence: the spline, Gaussian filter and gradient references come straight from
//! scipy/numpy. The edge-locator reference is a line-for-line Python restatement of the
//! algorithm, so it only proves Rust and Python agree; the accuracy of the locator rests on
//! the analytic blurred-step and noise tests in src/profile.rs.

use metrology_core::image::ImageView;
use metrology_core::profile::{gaussian_smooth, gradient, locate_edges_with, Polarity, Refine};
use metrology_core::spline::SplineWindow;
use std::collections::HashMap;

fn load(text: &str) -> HashMap<String, Vec<f64>> {
    text.lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| {
            (
                k.to_string(),
                v.split_whitespace().map(|x| x.parse().unwrap()).collect(),
            )
        })
        .collect()
}

fn assert_close(got: &[f64], want: &[f64], tol: f64, what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g - w).abs() <= tol,
            "{what}[{i}]: got {g} want {w} diff {}",
            (g - w).abs()
        );
    }
}

/// Rust and scipy see identical f32 pixels; the only difference is the algorithm.
/// Inside the image and up to 4 px beyond it the match is ~1e-11.
const SPLINE_TOL: f64 = 1e-9;
/// Far outside the image (> 4 px) scipy differs from an infinite replicate extension by
/// ~5e-7 gray levels. Measurement tools never sample there; this documents the behaviour.
const FAR_OUTSIDE_TOL: f64 = 1e-5;

/// How far (px) a point lies beyond the image border; 0 inside.
fn beyond_border(y: f64, x: f64, h: usize, w: usize) -> f64 {
    let d = |v: f64, n: usize| (-v).max(v - (n as f64 - 1.0)).max(0.0);
    d(y, h).max(d(x, w))
}

fn tol_for(y: f64, x: f64, h: usize, w: usize) -> f64 {
    if beyond_border(y, x, h, w) > 4.0 {
        FAR_OUTSIDE_TOL
    } else {
        SPLINE_TOL
    }
}

struct SplineCase {
    img: Vec<f32>,
    w: usize,
    h: usize,
    pts: Vec<f64>,
    expected: Vec<f64>,
}

fn spline_case(text: &str) -> SplineCase {
    let m = load(text);
    // The generator rounds the image to f32, so this cast is exact.
    let img: Vec<f32> = m["image"].iter().map(|&v| v as f32).collect();
    SplineCase {
        img,
        w: m["width"][0] as usize,
        h: m["height"][0] as usize,
        pts: m["points_yx"].clone(),
        expected: m["expected"].clone(),
    }
}

const SPLINE_FILES: [(&str, &str); 2] = [
    ("small 40x50", include_str!("golden/spline.txt")),
    (
        "large 200x200 incl. far outside points",
        include_str!("golden/spline_large.txt"),
    ),
];

#[test]
fn spline_matches_scipy_map_coordinates_nearest_per_point_windows() {
    for (name, text) in SPLINE_FILES {
        let c = spline_case(text);
        let img = ImageView::new(c.w, c.h, &c.img).unwrap();
        let (mut worst, mut worst_far) = (0.0f64, 0.0f64);
        for (k, p) in c.pts.chunks(2).enumerate() {
            let (y, x) = (p[0], p[1]);
            let win = SplineWindow::new(&img, x, x, y, y).unwrap();
            let d = (win.sample(x, y).unwrap() - c.expected[k]).abs();
            if tol_for(y, x, c.h, c.w) == SPLINE_TOL {
                worst = worst.max(d);
            } else {
                worst_far = worst_far.max(d);
            }
            assert!(
                d <= tol_for(y, x, c.h, c.w),
                "{name} point {k} ({y:.3},{x:.3}): diff {d}"
            );
        }
        println!(
            "{name}: worst diff vs scipy {worst:e} (inside/near), {worst_far:e} (far outside), {} points",
            c.pts.len() / 2
        );
    }
}

/// One window over all points that lie within 60 px of the image (the window must stay
/// reasonably small); covers windows cut off inside a larger image in both axes.
#[test]
fn spline_single_window_over_nearby_points_matches_too() {
    for (name, text) in SPLINE_FILES {
        let c = spline_case(text);
        let img = ImageView::new(c.w, c.h, &c.img).unwrap();
        let near = |y: f64, x: f64| {
            y > -60.0 && y < c.h as f64 + 60.0 && x > -60.0 && x < c.w as f64 + 60.0
        };
        let sel: Vec<(usize, f64, f64)> = c
            .pts
            .chunks(2)
            .enumerate()
            .filter(|(_, p)| near(p[0], p[1]))
            .map(|(k, p)| (k, p[0], p[1]))
            .collect();
        let mn = |f: fn(&(usize, f64, f64)) -> f64| sel.iter().map(f).fold(f64::INFINITY, f64::min);
        let mx =
            |f: fn(&(usize, f64, f64)) -> f64| sel.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        let win =
            SplineWindow::new(&img, mn(|s| s.2), mx(|s| s.2), mn(|s| s.1), mx(|s| s.1)).unwrap();
        assert!(
            sel.len() > 100,
            "{name}: expected many nearby points, got {}",
            sel.len()
        );
        for (k, y, x) in sel {
            let d = (win.sample(x, y).unwrap() - c.expected[k]).abs();
            assert!(d <= tol_for(y, x, c.h, c.w), "{name} point {k}: diff {d}");
        }
    }
}

#[test]
fn gaussian_and_gradient_match_scipy_numpy() {
    let m = load(include_str!("golden/filters.txt"));
    let sig = &m["signal"];
    for sigma in ["0.7", "1.0", "1.5"] {
        let s: f64 = sigma.parse().unwrap();
        assert_close(
            &gaussian_smooth(sig, s),
            &m[&format!("gauss_{sigma}")],
            1e-12,
            &format!("gauss {sigma}"),
        );
    }
    assert_close(&gradient(sig), &m["gradient"], 1e-12, "gradient");
}

#[test]
fn edge_locator_matches_reference_on_all_cases_for_both_refinements() {
    let m = load(include_str!("golden/edges.txt"));
    let count = m["count"][0] as usize;
    let params = [
        (1.0, 5.0, 0, Polarity::Either),
        (1.0, 5.0, 1, Polarity::Rising),
        (1.5, 8.0, -1, Polarity::Falling),
    ];
    let refines = [
        ("parabola", Refine::Parabola),
        ("gaussian", Refine::Gaussian),
    ];
    let mut total_edges = 0;
    for k in 0..count {
        let prof = &m[&format!("profile_{k}")];
        for (sigma, thr, pol, polarity) in params {
            for (rname, refine) in refines {
                let want = &m[&format!("edges_{k}_s{sigma:?}_t{thr:?}_p{pol}_{rname}")];
                let got = locate_edges_with(prof, sigma, thr, polarity, refine);
                assert_eq!(
                    got.len() * 3,
                    want.len(),
                    "case {k} sigma {sigma} pol {pol} {rname}: edge count"
                );
                for (e, w) in got.iter().zip(want.chunks(3)) {
                    assert!(
                        (e.position - w[0]).abs() < 1e-9,
                        "case {k} {rname} position {} vs {}",
                        e.position,
                        w[0]
                    );
                    assert!(
                        (e.strength - w[1]).abs() < 1e-9,
                        "case {k} {rname} strength"
                    );
                    assert_eq!(e.rising, w[2] > 0.0, "case {k} polarity");
                }
                total_edges += got.len();
            }
        }
    }
    assert!(
        total_edges >= 30,
        "the fixture must actually contain edges, found {total_edges}"
    );
}
