//! Synthetic threaded shafts for the thread tests.
//!
//! `shaft` samples the axial profile directly (a silhouette with no helix effect); `helical_shaft`
//! builds the true silhouette of a helical thread, `max over azimuth phi of R(z - P phi/2pi) cos phi`,
//! which is what a backlit camera sees.

type SidePts = Vec<(f64, f64)>;

/// Radius of an ideal 60 degree thread (UN basic profile) at axial position z: crest flat P/8 at
/// r_maj, root flat P/4 at r_min, straight flanks between.
pub fn radius(z: f64, p: f64, r_maj: f64, r_min: f64) -> f64 {
    let u = z.rem_euclid(p) / p; // 0..1 across one pitch, crest centred on 0
    let u = if u > 0.5 { u - 1.0 } else { u }; // -0.5..0.5
    let (crest, root) = (1.0 / 16.0, 1.0 / 8.0); // half flats as fractions of the pitch
    let a = u.abs();
    if a <= crest {
        r_maj
    } else if a >= 0.5 - root {
        r_min
    } else {
        r_maj - (r_maj - r_min) * (a - crest) / (0.5 - root - crest)
    }
}

/// Radius with half of the period above it: the pitch radius of the ideal profile above.
pub fn analytic_pd_radius(p: f64, r_maj: f64, r_min: f64) -> f64 {
    pd_radius_of(&|z| radius(z, p, r_maj, r_min), p)
}

/// Pitch radius (tooth width = groove width = P/2) of any axial profile of period `p`.
pub fn pd_radius_of(profile: &dyn Fn(f64) -> f64, p: f64) -> f64 {
    let mut v: Vec<f64> = (0..200_000)
        .map(|k| profile(k as f64 / 200_000.0 * p))
        .collect();
    v.sort_by(f64::total_cmp);
    0.5 * (v[v.len() / 2 - 1] + v[v.len() / 2])
}

/// Silhouette radius at axial position `t` of a helix whose profile is `profile`, seen from the
/// side whose azimuth is `phase` turns from the view normal (0 left, 0.5 right).
fn envelope(profile: &dyn Fn(f64) -> f64, p: f64, t: f64, phase: f64) -> f64 {
    const N: usize = 360;
    (0..=N)
        .map(|i| {
            let phi = (i as f64 / N as f64 - 0.5) * std::f64::consts::PI; // -90..90 degrees
            profile(t + phase * p - p * phi / std::f64::consts::TAU) * phi.cos()
        })
        .fold(f64::MIN, f64::max)
}

fn build(
    rl: &dyn Fn(f64) -> f64,
    rr: &dyn Fn(f64) -> f64,
    tilt_deg: f64,
    rows: usize,
    dy: f64,
    noise: f64,
) -> (SidePts, SidePts) {
    let (mut l, mut r) = (Vec::new(), Vec::new());
    let (s, c) = tilt_deg.to_radians().sin_cos();
    for k in 0..rows {
        let t = k as f64 * dy; // axial position along the axis
        let nz = |i: usize| noise * (((i * 7919 + 13) % 1000) as f64 / 500.0 - 1.0);
        let (a, b) = (rl(t) + nz(k), rr(t) + nz(k + 5000));
        // axis passes through (5, 0) with direction (s, c): tilted from +Y by tilt_deg
        let (ox, oy) = (5.0 + s * t, c * t);
        l.push((ox - a * c, oy + a * s));
        r.push((ox + b * c, oy - b * s));
    }
    (l, r)
}

/// Axial-profile silhouette, `rows` scan rows spaced `dy` mm, helix phase offset between sides,
/// optional alternating noise. Returns (left, right) points in mm.
pub fn shaft(
    p: f64,
    r_maj: f64,
    r_min: f64,
    tilt_deg: f64,
    rows: usize,
    dy: f64,
    noise: f64,
) -> (SidePts, SidePts) {
    build(
        &|t| radius(t, p, r_maj, r_min),
        &|t| radius(t + 0.5 * p, p, r_maj, r_min),
        tilt_deg,
        rows,
        dy,
        noise,
    )
}

/// Same, but the true silhouette of a helical thread (the sides are half a turn apart).
pub fn helical_shaft(
    p: f64,
    r_maj: f64,
    r_min: f64,
    tilt_deg: f64,
    rows: usize,
    dy: f64,
    noise: f64,
) -> (SidePts, SidePts) {
    let prof = move |z: f64| radius(z, p, r_maj, r_min);
    helical_shaft_with(p, &prof, tilt_deg, rows, dy, noise)
}

/// True silhouette of a helical thread with any axial `profile` of period `p`.
pub fn helical_shaft_with(
    p: f64,
    profile: &dyn Fn(f64) -> f64,
    tilt_deg: f64,
    rows: usize,
    dy: f64,
    noise: f64,
) -> (SidePts, SidePts) {
    build(
        &|t| envelope(profile, p, t, 0.0),
        &|t| envelope(profile, p, t, 0.5),
        tilt_deg,
        rows,
        dy,
        noise,
    )
}
