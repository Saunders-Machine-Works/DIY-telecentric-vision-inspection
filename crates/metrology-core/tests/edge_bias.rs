//! Bias of the silhouette 50 % edge crossing against analytic truth.
//!
//! Method (after the threshold-accuracy study in the third-party Measurific repository, MIT, and
//! the analytic-edge accuracy tests in Loupe, Apache-2.0: ideas only, no code taken): render a
//! dark bar of known width on a bright background as the exact pixel integral of a Gaussian-blurred
//! step, at many sub-pixel phases and blur sigmas, run `silhouette_edges`, and compare the
//! measured edge positions and width with the truth. A pixel-locked or phase-dependent error shows
//! up as a spread across phases.
use metrology_core::profile::silhouette_edges;

/// erf by the Abramowitz-Stegun 7.1.26 form is too coarse (1.5e-7) for 1e-3 px work only when
/// summed; use a series/continued-fraction split that is good to ~1e-15.
fn erf(x: f64) -> f64 {
    let ax = x.abs();
    let v = if ax < 2.5 {
        // Maclaurin series
        let (mut term, mut sum, mut n) = (ax, ax, 0.0);
        while term.abs() > 1e-17 * sum.abs() {
            n += 1.0;
            term *= -ax * ax / n;
            sum += term / (2.0 * n + 1.0);
        }
        2.0 / std::f64::consts::PI.sqrt() * sum
    } else {
        // continued fraction for erfc
        let mut f = 0.0;
        for k in (1..=60).rev() {
            f = f64::from(k) * 0.5 / (ax + f);
        }
        1.0 - (-ax * ax).exp() / (std::f64::consts::PI.sqrt() * (ax + f))
    };
    v.copysign(x)
}

/// Integral of erf(z) dz.
fn erf_int(z: f64) -> f64 {
    z * erf(z) + (-z * z).exp() / std::f64::consts::PI.sqrt()
}

/// Mean over the pixel [x-0.5, x+0.5] of the blurred step 0.5 (1 + erf((t - e) / (s sqrt 2))).
fn pixel_step(x: f64, e: f64, s: f64) -> f64 {
    let k = s * std::f64::consts::SQRT_2;
    let (a, b) = ((x - 0.5 - e) / k, (x + 0.5 - e) / k);
    0.5 + 0.5 * k * (erf_int(b) - erf_int(a))
}

/// Intensity 0..255 of a dark bar from `a` to `b` on a bright field.
fn bar(n: usize, a: f64, b: f64, s: f64, round: bool) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let x = i as f64;
            let v = 255.0 * (1.0 - (pixel_step(x, a, s) - pixel_step(x, b, s)));
            if round {
                v.round()
            } else {
                v
            }
        })
        .collect()
}

/// Widths with different fractional parts put the two edges at different sub-pixel phases, which
/// is where a phase-dependent interpolation error stops cancelling in a diameter.
const WIDTHS: [f64; 4] = [200.0, 200.25, 200.5, 200.75];

/// (worst |position error| of either edge, worst |width error|) over 40 phases and `WIDTHS`, px.
fn sweep(s: f64, round: bool) -> (f64, f64) {
    let (mut pos, mut wid) = (0.0_f64, 0.0_f64);
    for width in WIDTHS {
        for k in 0..40 {
            let a = 150.0 + f64::from(k) / 40.0;
            let b = a + width;
            let (l, r) = silhouette_edges(&bar(500, a, b, s, round)).expect("edges");
            pos = pos.max((l - a).abs()).max((r - b).abs());
            wid = wid.max(((r - l) - width).abs());
        }
    }
    (pos, wid)
}

/// A 200 px wide bar (a 6 mm pin at 30 um/px). From blur sigma 0.8 px up (a well focused
/// telecentric image) the edges are within 0.01 px (0.3 um) and the width within 0.02 px (0.6 um)
/// at every phase. Measurific's threshold study finds linear interpolation 0.031 px worst-case
/// in width at sigma 1.0 and a cubic spline 0.0033 px; the old 4-point Catmull-Rom path here
/// gave 0.031 px, the spline path gives 0.0066 px.
#[test]
fn bar_edges_and_widths_are_unbiased_at_every_phase() {
    for s in [0.8, 1.0, 1.5, 2.0] {
        let (pos, wid) = sweep(s, false);
        assert!(pos < 0.01, "sigma {s}: edge error {pos} px");
        assert!(wid < 0.02, "sigma {s}: width error {wid} px");
    }
}

/// Known limit: a very sharp edge (sigma 0.6 px) is under-sampled and no interpolation recovers
/// it fully; the residual is bounded here so a regression shows.
#[test]
fn sharp_edges_stay_within_a_twentieth_of_a_pixel() {
    let (pos, wid) = sweep(0.6, false);
    assert!(pos < 0.025, "edge error {pos} px");
    assert!(wid < 0.05, "width error {wid} px");
}

/// Rounding to 8 bits is what a Mono8 frame does; one frame's quantisation adds scatter that
/// must stay within 0.02 px (0.6 um) per edge and 0.03 px in width at every phase.
#[test]
fn eight_bit_quantisation_stays_within_a_fiftieth_of_a_pixel() {
    for s in [1.0, 1.5] {
        let (pos, wid) = sweep(s, true);
        assert!(pos < 0.02, "sigma {s}: edge error {pos} px");
        assert!(wid < 0.03, "sigma {s}: width error {wid} px");
    }
}

/// The generator itself: the pixel integral of a step puts 50 % exactly at the edge, and a bar
/// has its mean intensity at 255 minus its width (a check independent of `silhouette_edges`).
#[test]
fn generator_is_exact() {
    assert!((pixel_step(10.0, 10.0, 1.0) - 0.5).abs() < 1e-12);
    assert!((erf(0.5) - 0.520_499_877_813_046_5).abs() < 1e-12);
    assert!((erf(3.0) - 0.999_977_909_503_001_4).abs() < 1e-12);
    let w = 37.3;
    let v = bar(300, 100.2, 100.2 + w, 1.0, false);
    let dark: f64 = v.iter().map(|x| 255.0 - x).sum::<f64>() / 255.0;
    assert!((dark - w).abs() < 1e-9, "{dark}");
}
