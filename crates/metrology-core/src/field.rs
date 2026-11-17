//! Field map (milestone I-4b): image position -> millimetres, undoing lens distortion with
//! the reticle's own grid.
//!
//! Every edge the lattice fit assigned to a grid line is a point (x, y) known to lie on grid
//! line i (vertical family) or j (horizontal family). A smooth polynomial i(x, y), j(x, y)
//! fitted to those points is the distortion-aware board coordinate of any image point.
//! How that becomes millimetres depends on what the lattice's small apparent anisotropy means
//! (`TiltPolicy`):
//!   BoardPlane    mm = (i, j) * square_mm                       (validated policy)
//!   Foreshortened mm = (a1*i + a2*j) * square_mm / sigma_max    (UNVALIDATED, see below)
//! with a1, a2 the columns of the inverse Jacobian of (i, j) at the centre of the region.
//!
//! WHY ONE CAPTURE CANNOT SETTLE THE POLICY. A tilted reticle, lens/pixel anisotropy, and a
//! small residual non-telecentricity (a tilted plane then also gets a keystone scale gradient
//! that the cubic absorbs as "distortion") all look alike. `Foreshortened` removes the
//! foreshortening of a real tilt but keeps any keystone, which a perpendicular part plane would
//! not have. A second capture with the board rotated 90 degrees separates them: real tilt and
//! keystone turn with the board, anisotropy and lens distortion stay fixed in image axes.
//! Until that is done, use `BoardPlane`.
//!
//! The map is only trusted where the reticle gave data: `contains` requires the point to lie in
//! a tile that holds edges from both families, never merely inside a bounding box.

use crate::lattice::{AxisFit, LatticeFit};
use crate::linalg::least_squares;
use std::collections::HashSet;

pub const MAX_DEGREE: usize = 5;
const MAX_TERMS: usize = (MAX_DEGREE + 1) * (MAX_DEGREE + 2) / 2;
/// Occupancy tile for the calibrated region, pixels.
const TILE_PX: f64 = 40.0;
/// Kept edges of EACH family a tile needs to count as calibrated (about 3 scan lines).
const MIN_EDGES_PER_TILE: usize = 3;
/// Cross-validation block (spatial hold-out), pixels.
const CV_BLOCK_PX: f64 = 80.0;

/// What the lattice anisotropy means physically (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TiltPolicy {
    /// The reticle is parallel to the part plane; anisotropy is lens/pixel and is corrected.
    BoardPlane,
    /// The apparent tilt is real (UNVALIDATED: keystone from a non-telecentric lens is kept).
    Foreshortened,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldError {
    /// Polynomial degree must be 1..=MAX_DEGREE.
    BadDegree,
    /// Too few edges for the requested degree.
    TooFewSamples {
        have: usize,
        need: usize,
    },
    Singular,
}

#[derive(Debug, Clone)]
pub struct FieldMap {
    degree: usize,
    terms: Vec<(i32, i32)>,
    center: (f64, f64),
    half: (f64, f64),
    ci: Vec<f64>,
    cj: Vec<f64>,
    policy: TiltPolicy,
    square_mm: f64,
    /// Columns of the lattice basis (image px per square) from the Jacobian at the centre.
    a1: (f64, f64),
    a2: (f64, f64),
    sigma_max: f64,
    tilt_deg: f64,
    /// (x_min, x_max, y_min, y_max) of all observations (before clipping), image pixels.
    region: (f64, f64, f64, f64),
    /// Tiles holding at least `MIN_EDGES_PER_TILE` kept edges of each family.
    occupied: HashSet<(i64, i64)>,
    /// In-sample RMS of the grid-line index fit, in pixels (vertical, horizontal family).
    pub rms_px: (f64, f64),
    /// Spatial two-fold cross-validation RMS in pixels: fit on the checkerboard of 80 px
    /// blocks of one parity, test on the other (the test edges are never next to the same grid
    /// line's training edges), then swap and average.
    pub cv_rms_px: (f64, f64),
    /// Edges removed by the polynomial's own robust clip (vertical, horizontal family).
    pub clipped: (usize, usize),
}

fn terms_of(degree: usize) -> Vec<(i32, i32)> {
    (0..=degree as i32)
        .flat_map(|a| (0..=degree as i32 - a).map(move |b| (a, b)))
        .collect()
}

/// Monomials x^a y^b for each term into `out`; returns the number of terms.
fn basis_into(terms: &[(i32, i32)], x: f64, y: f64, out: &mut [f64; MAX_TERMS]) -> usize {
    let (mut px, mut py) = ([1.0; MAX_DEGREE + 1], [1.0; MAX_DEGREE + 1]);
    for k in 1..=MAX_DEGREE {
        (px[k], py[k]) = (px[k - 1] * x, py[k - 1] * y);
    }
    for (o, &(a, b)) in out.iter_mut().zip(terms) {
        *o = px[a as usize] * py[b as usize];
    }
    terms.len()
}

/// d/dx and d/dy of each monomial (with respect to the normalised coordinates).
fn basis_grad(terms: &[(i32, i32)], x: f64, y: f64) -> (Vec<f64>, Vec<f64>) {
    let pw = |v: f64, k: i32| if k < 0 { 0.0 } else { v.powi(k) };
    let dx = terms
        .iter()
        .map(|&(a, b)| a as f64 * pw(x, a - 1) * pw(y, b))
        .collect();
    let dy = terms
        .iter()
        .map(|&(a, b)| b as f64 * pw(x, a) * pw(y, b - 1))
        .collect();
    (dx, dy)
}

/// One observation: image point and the grid-line index it lies on.
struct Obs {
    x: f64,
    y: f64,
    idx: f64,
}

/// Observations of a family (every gate-passed edge, clipped-from-the-linear-fit ones
/// included) with the fitted rising/falling edge bias removed.
fn observations(fam: &AxisFit, vertical: bool) -> Vec<Obs> {
    fam.samples
        .iter()
        .map(|q| {
            let s = q.s
                - if q.rising {
                    fam.edge_bias
                } else {
                    -fam.edge_bias
                };
            let (x, y) = if vertical { (s, q.c) } else { (q.c, s) };
            Obs {
                x,
                y,
                idx: q.index as f64,
            }
        })
        .collect()
}

fn fit_poly(
    obs: &[&Obs],
    terms: &[(i32, i32)],
    center: (f64, f64),
    half: (f64, f64),
) -> Option<Vec<f64>> {
    let mut b = [0.0; MAX_TERMS];
    let rows: Vec<Vec<f64>> = obs
        .iter()
        .map(|o| {
            let n = basis_into(
                terms,
                (o.x - center.0) / half.0,
                (o.y - center.1) / half.1,
                &mut b,
            );
            b[..n].to_vec()
        })
        .collect();
    let ys: Vec<f64> = obs.iter().map(|o| o.idx).collect();
    least_squares(&rows, &ys)
}

fn eval(c: &[f64], terms: &[(i32, i32)], nx: f64, ny: f64) -> f64 {
    let mut b = [0.0; MAX_TERMS];
    let n = basis_into(terms, nx, ny, &mut b);
    b[..n].iter().zip(c).map(|(b, k)| b * k).sum()
}

/// Fit with robust clipping against the polynomial itself (not a linear model, so lens
/// distortion at the corners is never mistaken for an outlier). Returns the coefficients and
/// the observations kept. Gate: 4 robust sigmas of the residual, never below 0.02 squares.
fn robust_fit<'a>(
    obs: &'a [Obs],
    terms: &[(i32, i32)],
    center: (f64, f64),
    half: (f64, f64),
) -> Option<(Vec<f64>, Vec<&'a Obs>)> {
    let mut active: Vec<&Obs> = obs.iter().collect();
    let mut c = fit_poly(&active, terms, center, half)?;
    for _ in 0..5 {
        let res: Vec<f64> = obs
            .iter()
            .map(|o| {
                (eval(
                    &c,
                    terms,
                    (o.x - center.0) / half.0,
                    (o.y - center.1) / half.1,
                ) - o.idx)
                    .abs()
            })
            .collect();
        let mut sorted = res.clone();
        sorted.sort_by(f64::total_cmp);
        let gate = (4.0 * 1.4826 * sorted[sorted.len() / 2]).max(0.02);
        let next: Vec<&Obs> = obs
            .iter()
            .zip(&res)
            .filter(|(_, r)| **r < gate)
            .map(|(o, _)| o)
            .collect();
        if next.len() == active.len() || next.len() < 4 * terms.len() {
            break;
        }
        active = next;
        c = fit_poly(&active, terms, center, half)?;
    }
    Some((c, active))
}

fn tile_of(x: f64, y: f64, size: f64) -> (i64, i64) {
    ((x / size).floor() as i64, (y / size).floor() as i64)
}

/// Singular values (max, min) of the 2x2 matrix with columns a1, a2.
fn singular_values(a1: (f64, f64), a2: (f64, f64)) -> (f64, f64) {
    let s = a1.0 * a1.0 + a1.1 * a1.1 + a2.0 * a2.0 + a2.1 * a2.1;
    let det = (a1.0 * a2.1 - a1.1 * a2.0).abs();
    let disc = (s * s - 4.0 * det * det).max(0.0).sqrt();
    (
        ((s + disc) / 2.0).sqrt(),
        ((s - disc) / 2.0).max(0.0).sqrt(),
    )
}

impl FieldMap {
    /// Fit the map to a lattice fit of a board with `square_mm` squares.
    pub fn fit(
        lattice: &LatticeFit,
        square_mm: f64,
        degree: usize,
        policy: TiltPolicy,
    ) -> Result<FieldMap, FieldError> {
        if degree == 0 || degree > MAX_DEGREE {
            return Err(FieldError::BadDegree);
        }
        let terms = terms_of(degree);
        let (ov, oh) = (
            observations(&lattice.vertical, true),
            observations(&lattice.horizontal, false),
        );
        let need = 4 * terms.len();
        for n in [ov.len(), oh.len()] {
            if n < need {
                return Err(FieldError::TooFewSamples { have: n, need });
            }
        }
        // normalise to [-1, 1] about the middle of everything the board covered
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for o in ov.iter().chain(&oh) {
            (x0, x1, y0, y1) = (x0.min(o.x), x1.max(o.x), y0.min(o.y), y1.max(o.y));
        }
        let center = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let half = (((x1 - x0) / 2.0).max(1.0), ((y1 - y0) / 2.0).max(1.0));
        let (ci, kept_v) = robust_fit(&ov, &terms, center, half).ok_or(FieldError::Singular)?;
        let (cj, kept_h) = robust_fit(&oh, &terms, center, half).ok_or(FieldError::Singular)?;
        let clipped = (ov.len() - kept_v.len(), oh.len() - kept_h.len());

        let rms = |obs: &[&Obs], c: &[f64], unit: f64| -> f64 {
            let s: f64 = obs
                .iter()
                .map(|o| {
                    (eval(
                        c,
                        &terms,
                        (o.x - center.0) / half.0,
                        (o.y - center.1) / half.1,
                    ) - o.idx)
                        .powi(2)
                })
                .sum();
            (s / obs.len() as f64).sqrt() * unit
        };
        // spatial 2-fold CV: blocks of one checkerboard parity train, the other tests
        let cv = |obs: &[&Obs], unit: f64| -> Result<f64, FieldError> {
            let parity = |o: &Obs| {
                let (bx, by) = tile_of(o.x - x0, o.y - y0, CV_BLOCK_PX);
                ((bx + by) & 1) as usize
            };
            let split = |p: usize| -> Vec<&Obs> {
                obs.iter().copied().filter(|o| parity(o) == p).collect()
            };
            let (even, odd) = (split(0), split(1));
            if even.len() < need || odd.len() < need {
                return Err(FieldError::TooFewSamples {
                    have: even.len().min(odd.len()),
                    need,
                });
            }
            let (ce, co) = (
                fit_poly(&even, &terms, center, half).ok_or(FieldError::Singular)?,
                fit_poly(&odd, &terms, center, half).ok_or(FieldError::Singular)?,
            );
            Ok(0.5 * (rms(&odd, &ce, unit) + rms(&even, &co, unit)))
        };
        let (pv, ph) = (lattice.vertical.pitch, lattice.horizontal.pitch);
        let (rms_px, cv_rms_px) = (
            (rms(&kept_v, &ci, pv), rms(&kept_h, &cj, ph)),
            (cv(&kept_v, pv)?, cv(&kept_h, ph)?),
        );

        // calibrated region: tiles holding enough kept edges of BOTH families, so a stray edge
        // pair or a tile clipped at a corner cannot create (or extend) trusted territory
        let tiles = |kept: &[&Obs]| -> HashSet<(i64, i64)> {
            let mut count: std::collections::HashMap<(i64, i64), usize> = Default::default();
            for o in kept {
                *count.entry(tile_of(o.x, o.y, TILE_PX)).or_default() += 1;
            }
            count
                .into_iter()
                .filter(|&(_, n)| n >= MIN_EDGES_PER_TILE)
                .map(|(t, _)| t)
                .collect()
        };
        let occupied: HashSet<(i64, i64)> = tiles(&kept_v)
            .intersection(&tiles(&kept_h))
            .copied()
            .collect();

        let mut map = FieldMap {
            degree,
            terms,
            center,
            half,
            ci,
            cj,
            policy,
            square_mm,
            a1: (1.0, 0.0),
            a2: (0.0, 1.0),
            sigma_max: 1.0,
            tilt_deg: 0.0,
            region: (x0, x1, y0, y1),
            occupied,
            rms_px,
            cv_rms_px,
            clipped,
        };
        // lattice basis from the map's own Jacobian at the centre (not the linear fit, whose
        // anisotropy is partly an artefact of distortion averaged over a non-square region)
        let j = map.jacobian(center.0, center.1);
        let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
        if det.abs() < 1e-12 {
            return Err(FieldError::Singular);
        }
        map.a1 = (j[1][1] / det, -j[1][0] / det); // d(x,y)/di
        map.a2 = (-j[0][1] / det, j[0][0] / det); // d(x,y)/dj
        let (smax, smin) = singular_values(map.a1, map.a2);
        map.sigma_max = smax;
        map.tilt_deg = (smin / smax).clamp(0.0, 1.0).acos().to_degrees();
        Ok(map)
    }

    pub fn degree(&self) -> usize {
        self.degree
    }

    pub fn policy(&self) -> TiltPolicy {
        self.policy
    }

    /// Apparent tilt (degrees) from the map's Jacobian at the region centre. Tilt, lens/pixel
    /// anisotropy and keystone are indistinguishable from one capture (see the module docs), and
    /// values below ~1.5 degrees (an anisotropy of ~2e-4) are within the noise of this estimate.
    pub fn apparent_tilt_deg(&self) -> f64 {
        self.tilt_deg
    }

    /// Bounding box of the kept edges, as (x_min, x_max, y_min, y_max) in pixels. Informational:
    /// `contains` is stricter.
    pub fn region(&self) -> (f64, f64, f64, f64) {
        self.region
    }

    /// True where the reticle gave edges of both families (within a 40 px tile).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let (x0, x1, y0, y1) = self.region;
        x >= x0 && x <= x1 && y >= y0 && y <= y1 && self.occupied.contains(&tile_of(x, y, TILE_PX))
    }

    /// Distortion-aware grid coordinates (i, j) of an image point, in squares.
    pub fn grid_coords(&self, x: f64, y: f64) -> (f64, f64) {
        let (nx, ny) = (
            (x - self.center.0) / self.half.0,
            (y - self.center.1) / self.half.1,
        );
        (
            eval(&self.ci, &self.terms, nx, ny),
            eval(&self.cj, &self.terms, nx, ny),
        )
    }

    /// Jacobian of (i, j) with respect to (x, y) in squares per pixel: [[di/dx, di/dy],
    /// [dj/dx, dj/dy]].
    pub fn jacobian(&self, x: f64, y: f64) -> [[f64; 2]; 2] {
        let (nx, ny) = (
            (x - self.center.0) / self.half.0,
            (y - self.center.1) / self.half.1,
        );
        let (gx, gy) = basis_grad(&self.terms, nx, ny);
        let dot = |g: &[f64], c: &[f64]| -> f64 { g.iter().zip(c).map(|(g, k)| g * k).sum() };
        [
            [
                dot(&gx, &self.ci) / self.half.0,
                dot(&gy, &self.ci) / self.half.1,
            ],
            [
                dot(&gx, &self.cj) / self.half.0,
                dot(&gy, &self.cj) / self.half.1,
            ],
        ]
    }

    fn mm_of_grid(&self, i: f64, j: f64) -> (f64, f64) {
        match self.policy {
            TiltPolicy::BoardPlane => (i * self.square_mm, j * self.square_mm),
            TiltPolicy::Foreshortened => {
                let k = self.square_mm / self.sigma_max;
                (
                    (self.a1.0 * i + self.a2.0 * j) * k,
                    (self.a1.1 * i + self.a2.1 * j) * k,
                )
            }
        }
    }

    /// Millimetre position in the distortion-free plane (arbitrary origin; board axes for
    /// `BoardPlane`, image-like axes for `Foreshortened`). `None` outside the calibrated region.
    pub fn to_mm(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !self.contains(x, y) {
            return None;
        }
        let (i, j) = self.grid_coords(x, y);
        Some(self.mm_of_grid(i, j))
    }

    /// Convert many points. Points outside the calibrated region come back as `None` so the
    /// caller can flag exactly those edges instead of refusing the whole measurement.
    pub fn to_mm_all(&self, points: &[(f64, f64)]) -> Vec<Option<(f64, f64)>> {
        points.iter().map(|p| self.to_mm(p.0, p.1)).collect()
    }

    /// Distance in mm between two image points; `None` unless both are in the calibrated region.
    pub fn distance_mm(&self, p: (f64, f64), q: (f64, f64)) -> Option<f64> {
        let (a, b) = (self.to_mm(p.0, p.1)?, self.to_mm(q.0, q.1)?);
        Some((a.0 - b.0).hypot(a.1 - b.1))
    }

    /// Local scale in micrometres per pixel along the unit direction `dir` at (x, y), exact from
    /// the Jacobian. `None` outside the calibrated region or for a zero direction.
    pub fn local_um_per_px(&self, x: f64, y: f64, dir: (f64, f64)) -> Option<f64> {
        let n = dir.0.hypot(dir.1);
        if n == 0.0 || !n.is_finite() || !self.contains(x, y) {
            return None;
        }
        let j = self.jacobian(x, y);
        let (di, dj) = (
            j[0][0] * dir.0 / n + j[0][1] * dir.1 / n,
            j[1][0] * dir.0 / n + j[1][1] * dir.1 / n,
        );
        let (mx, my) = self.mm_of_grid(di, dj);
        Some(mx.hypot(my) * 1000.0)
    }
}

/// Version tag of the calibration text format.
const TEXT_VERSION: u32 = 1;

impl FieldMap {
    /// Serialise the calibration as plain text (one `key: values` line each, floats written so
    /// they parse back bit-for-bit). A calibration is only valid for the optics, exposure,
    /// focus and camera it was captured with; the caller must store that context alongside.
    pub fn to_text(&self) -> String {
        let nums = |v: &[f64]| {
            v.iter()
                .map(|x| format!("{x:?}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let mut tiles: Vec<_> = self.occupied.iter().copied().collect();
        tiles.sort_unstable();
        let tiles = tiles
            .iter()
            .map(|t| format!("{} {}", t.0, t.1))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "fieldmap_version: {TEXT_VERSION}\npolicy: {:?}\nsquare_mm: {:?}\ndegree: {}\ncenter: {}\nhalf: {}\nci: {}\ncj: {}\na1: {}\na2: {}\nsigma_max: {:?}\ntilt_deg: {:?}\nregion: {}\nrms_px: {}\ncv_rms_px: {}\nclipped: {} {}\ntiles: {tiles}\n",
            self.policy,
            self.square_mm,
            self.degree,
            nums(&[self.center.0, self.center.1]),
            nums(&[self.half.0, self.half.1]),
            nums(&self.ci),
            nums(&self.cj),
            nums(&[self.a1.0, self.a1.1]),
            nums(&[self.a2.0, self.a2.1]),
            self.sigma_max,
            self.tilt_deg,
            nums(&[self.region.0, self.region.1, self.region.2, self.region.3]),
            nums(&[self.rms_px.0, self.rms_px.1]),
            nums(&[self.cv_rms_px.0, self.cv_rms_px.1]),
            self.clipped.0,
            self.clipped.1,
        )
    }

    /// Parse text written by `to_text`. Any missing, malformed or inconsistent field is an error.
    pub fn from_text(text: &str) -> Result<FieldMap, String> {
        let map: std::collections::HashMap<&str, &str> =
            text.lines().filter_map(|l| l.split_once(": ")).collect();
        let get = |k: &str| {
            map.get(k)
                .copied()
                .ok_or_else(|| format!("missing field {k}"))
        };
        let floats = |k: &str| -> Result<Vec<f64>, String> {
            get(k)?
                .split_whitespace()
                .map(|x| x.parse::<f64>().map_err(|e| format!("{k}: {e}")))
                .collect()
        };
        let pair = |k: &str| -> Result<(f64, f64), String> {
            match floats(k)?[..] {
                [a, b] => Ok((a, b)),
                _ => Err(format!("{k}: expected 2 numbers")),
            }
        };
        let version: u32 = get("fieldmap_version")?
            .parse()
            .map_err(|e| format!("version: {e}"))?;
        if version != TEXT_VERSION {
            return Err(format!("unsupported calibration version {version}"));
        }
        let policy = match get("policy")? {
            "BoardPlane" => TiltPolicy::BoardPlane,
            "Foreshortened" => TiltPolicy::Foreshortened,
            other => return Err(format!("unknown policy {other}")),
        };
        let degree: usize = get("degree")?.parse().map_err(|e| format!("degree: {e}"))?;
        if degree == 0 || degree > MAX_DEGREE {
            return Err("degree out of range".into());
        }
        let terms = terms_of(degree);
        let (ci, cj) = (floats("ci")?, floats("cj")?);
        if ci.len() != terms.len() || cj.len() != terms.len() {
            return Err("coefficient count does not match the degree".into());
        }
        let region = match floats("region")?[..] {
            [a, b, c, d] => (a, b, c, d),
            _ => return Err("region: expected 4 numbers".into()),
        };
        let tiles: Vec<i64> = get("tiles")?
            .split_whitespace()
            .map(|x| x.parse::<i64>().map_err(|e| format!("tiles: {e}")))
            .collect::<Result<_, _>>()?;
        if !tiles.len().is_multiple_of(2) {
            return Err("tiles: odd number of values".into());
        }
        let clipped: Vec<usize> = get("clipped")?
            .split_whitespace()
            .map(|x| x.parse::<usize>().map_err(|e| format!("clipped: {e}")))
            .collect::<Result<_, _>>()?;
        if clipped.len() != 2 {
            return Err("clipped: expected 2 numbers".into());
        }
        let half = pair("half")?;
        if half.0 <= 0.0 || half.1 <= 0.0 || !floats("ci")?.iter().chain(&cj).all(|v| v.is_finite())
        {
            return Err("invalid normalisation or coefficients".into());
        }
        Ok(FieldMap {
            degree,
            terms,
            center: pair("center")?,
            half,
            ci,
            cj,
            policy,
            square_mm: get("square_mm")?
                .parse()
                .map_err(|e| format!("square_mm: {e}"))?,
            a1: pair("a1")?,
            a2: pair("a2")?,
            sigma_max: get("sigma_max")?
                .parse()
                .map_err(|e| format!("sigma_max: {e}"))?,
            tilt_deg: get("tilt_deg")?
                .parse()
                .map_err(|e| format!("tilt_deg: {e}"))?,
            region,
            occupied: tiles.chunks(2).map(|c| (c[0], c[1])).collect(),
            rms_px: pair("rms_px")?,
            cv_rms_px: pair("cv_rms_px")?,
            clipped: (clipped[0], clipped[1]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageView;
    use crate::lattice::{fit_lattice, LatticeParams};
    use crate::testutil::{basis as board_basis, render_g};

    const CENTRE: (f64, f64) = (350.0, 260.0);

    fn lattice_of(a1: (f64, f64), a2: (f64, f64), dist: f64, noise: f64) -> LatticeFit {
        let (w, h) = (700, 520);
        let px = render_g(w, h, a1, a2, CENTRE, (28.0, 20.0), 1.0, noise, 0.0, dist);
        let img = ImageView::new(w, h, &px).unwrap();
        fit_lattice(
            &img,
            &LatticeParams {
                nominal_pitch_px: 14.0,
                line_step: 8,
                ..Default::default()
            },
        )
        .unwrap()
    }

    /// Deterministic pairs of image points inside the board, 60..250 px apart.
    fn pairs() -> Vec<((f64, f64), (f64, f64))> {
        let mut seed = 7u64;
        let mut r = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut out = Vec::new();
        while out.len() < 60 {
            let p = (
                CENTRE.0 - 170.0 + 340.0 * r(),
                CENTRE.1 - 120.0 + 240.0 * r(),
            );
            let q = (
                CENTRE.0 - 170.0 + 340.0 * r(),
                CENTRE.1 - 120.0 + 240.0 * r(),
            );
            let d = (p.0 - q.0).hypot(p.1 - q.1);
            if (60.0..250.0).contains(&d) {
                out.push((p, q));
            }
        }
        out
    }

    /// Board coordinates (squares) of an image point under the renderer's radial distortion:
    /// B^-1 (p - c) * (1 + dist * r^2 / 250^2).
    fn true_board(a1: (f64, f64), a2: (f64, f64), dist: f64, pt: (f64, f64)) -> (f64, f64) {
        let det = a1.0 * a2.1 - a1.1 * a2.0;
        let (px, py) = (pt.0 - CENTRE.0, pt.1 - CENTRE.1);
        let f = 1.0 + dist * (px * px + py * py) / (250.0 * 250.0);
        (
            ((a2.1 * px - a2.0 * py) / det) * f,
            ((-a1.1 * px + a1.0 * py) / det) * f,
        )
    }

    fn true_squares(
        a1: (f64, f64),
        a2: (f64, f64),
        dist: f64,
        p: (f64, f64),
        q: (f64, f64),
    ) -> f64 {
        let (u, v) = (true_board(a1, a2, dist, p), true_board(a1, a2, dist, q));
        (u.0 - v.0).hypot(u.1 - v.1)
    }

    /// Worst chord error in pixel-equivalents (1 square = 14 px) against the true board geometry.
    fn px_err(m: &FieldMap, a1: (f64, f64), a2: (f64, f64), dist: f64) -> f64 {
        pairs()
            .iter()
            .map(|&(p, q)| {
                (m.distance_mm(p, q).unwrap() - true_squares(a1, a2, dist, p, q)).abs() * 14.0
            })
            .fold(0.0, f64::max)
    }

    /// 0.4 % radial distortion at 250 px (large enough to stand clear of the edge-noise floor
    /// of the synthetic board). The polynomial map recovers chord lengths to the noise floor;
    /// the linear (degree 1) map cannot, and the spatial cross-validation says so too.
    #[test]
    fn polynomial_map_removes_radial_distortion_that_a_linear_map_cannot() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.004, 0.5);
        let m3 = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        let m1 = FieldMap::fit(&lat, 1.0, 1, TiltPolicy::BoardPlane).unwrap();
        let (e3, e1) = (px_err(&m3, a1, a2, 0.004), px_err(&m1, a1, a2, 0.004));
        assert!(e3 < 0.04, "degree 3 worst chord error {e3:.4} px");
        assert!(
            e1 > 2.0 * e3,
            "linear {e1:.4} px should be clearly worse than cubic {e3:.4} px"
        );
        assert_eq!(
            m3.clipped,
            (0, 0),
            "no good edge may be clipped as distortion"
        );
        assert!(
            m3.cv_rms_px.0 < m1.cv_rms_px.0 && m3.cv_rms_px.1 < m1.cv_rms_px.1,
            "{:?} vs {:?}",
            m3.cv_rms_px,
            m1.cv_rms_px
        );
        assert!(
            m3.cv_rms_px.0 < 0.1 && m3.cv_rms_px.1 < 0.1,
            "{:?}",
            m3.cv_rms_px
        );
    }

    /// The Foreshortened basis now comes from the map's Jacobian at the centre, so on a
    /// distorted but untilted board it is as good as BoardPlane. (With the linear fit's basis it
    /// was off by up to 0.086 px: distortion averaged over a non-square region looked like tilt.)
    #[test]
    fn foreshortened_basis_is_not_fooled_by_distortion() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.004, 0.5);
        let plane = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        let fore = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::Foreshortened).unwrap();
        let (ep, ef) = (px_err(&plane, a1, a2, 0.004), px_err(&fore, a1, a2, 0.004));
        assert!(ef < 0.06, "foreshortened {ef:.4} px (board-plane {ep:.4})");
        // acos of the singular-value ratio is hypersensitive (1 degree = 1.5e-4 anisotropy), so an
        // untilted synthetic board still reads ~1.2 degrees from edge noise: below ~1.5 degrees
        // an apparent tilt is not distinguishable from zero.
        assert!(
            fore.apparent_tilt_deg() < 1.5,
            "no real tilt: {}",
            fore.apparent_tilt_deg()
        );
    }

    /// Orientation guard: stepping along a1 raises i and leaves j alone, and along a2 the
    /// reverse. Catches swapped i/j and sign errors that distances cannot.
    #[test]
    fn grid_coordinates_follow_the_board_axes() {
        let (a1, a2) = board_basis(14.0, 1.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.0, 0.5);
        let m = FieldMap::fit(&lat, 1.0, 2, TiltPolicy::BoardPlane).unwrap();
        let (i0, j0) = m.grid_coords(CENTRE.0, CENTRE.1);
        let (i1, j1) = m.grid_coords(CENTRE.0 + 3.0 * a1.0, CENTRE.1 + 3.0 * a1.1);
        assert!(
            (i1 - i0 - 3.0).abs() < 0.01 && (j1 - j0).abs() < 0.01,
            "a1 step: di {} dj {}",
            i1 - i0,
            j1 - j0
        );
        let (i2, j2) = m.grid_coords(CENTRE.0 + 2.0 * a2.0, CENTRE.1 + 2.0 * a2.1);
        assert!(
            (j2 - j0 - 2.0).abs() < 0.01 && (i2 - i0).abs() < 0.01,
            "a2 step: di {} dj {}",
            i2 - i0,
            j2 - j0
        );
    }

    /// Analytic Jacobian equals a numerical derivative of grid_coords.
    #[test]
    fn the_jacobian_matches_a_numerical_derivative() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.004, 0.5);
        let m = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        for (x, y) in [(300.0, 220.0), (420.0, 300.0), (250.0, 330.0)] {
            let j = m.jacobian(x, y);
            let h = 1e-3;
            let (ip, jp) = m.grid_coords(x + h, y);
            let (im, jm) = m.grid_coords(x - h, y);
            let (iq, jq) = m.grid_coords(x, y + h);
            let (ir, jr) = m.grid_coords(x, y - h);
            let num = [
                [(ip - im) / (2.0 * h), (iq - ir) / (2.0 * h)],
                [(jp - jm) / (2.0 * h), (jq - jr) / (2.0 * h)],
            ];
            for r in 0..2 {
                for c in 0..2 {
                    assert!(
                        (j[r][c] - num[r][c]).abs() < 1e-7,
                        "J[{r}][{c}] {} vs {}",
                        j[r][c],
                        num[r][c]
                    );
                }
            }
        }
    }

    /// Local scale at several points equals the analytic scale of the synthetic distortion
    /// (squares per pixel from a fine difference of the true board coordinates).
    #[test]
    fn local_scale_follows_the_analytic_gradient_of_the_distortion() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.004, 0.5);
        let m = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for (x, y) in [
            (230.0, 190.0),
            (350.0, 260.0),
            (470.0, 330.0),
            (240.0, 330.0),
            (460.0, 200.0),
        ] {
            for dir in [(1.0, 0.0), (0.0, 1.0)] {
                let h = 0.01;
                let (p, q) = (
                    (x - dir.0 * h, y - dir.1 * h),
                    (x + dir.0 * h, y + dir.1 * h),
                );
                let want = true_squares(a1, a2, 0.004, p, q) / (2.0 * h) * 1000.0;
                let got = m.local_um_per_px(x, y, dir).unwrap();
                assert!(
                    (got - want).abs() / want < 1e-3,
                    "({x},{y}) {dir:?}: got {got} want {want}"
                );
                lo = lo.min(want);
                hi = hi.max(want);
            }
        }
        assert!(
            (hi - lo) / lo > 2.5e-3,
            "the test must actually span a scale gradient: {lo}..{hi}"
        );
    }

    /// Distortion-free, untilted: chord lengths equal true size, and the local scale is
    /// 1000/14 um per px in every direction.
    #[test]
    fn undistorted_board_gives_true_lengths_and_uniform_local_scale() {
        let (a1, a2) = board_basis(14.0, 1.0, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.0, 0.5);
        let m = FieldMap::fit(&lat, 1.0, 2, TiltPolicy::BoardPlane).unwrap();
        assert!(px_err(&m, a1, a2, 0.0) < 0.05);
        for dir in [(1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let s = m.local_um_per_px(CENTRE.0, CENTRE.1, dir).unwrap();
            assert!(
                (s - 1000.0 / 14.0).abs() / (1000.0 / 14.0) < 3e-4,
                "{dir:?}: {s}"
            );
        }
    }

    /// A board tilted 4 degrees is foreshortened by cos(4 deg) along one axis; a part in the
    /// plane perpendicular to the optical axis is not. mm = image distance / sigma_max.
    #[test]
    fn a_tilted_reticle_does_not_foreshorten_the_measured_lengths() {
        let (a1, a2) = board_basis(14.0, 1.0, 4.0, 25.0);
        let lat = lattice_of(a1, a2, 0.0, 0.5);
        let m = FieldMap::fit(&lat, 1.0, 2, TiltPolicy::Foreshortened).unwrap();
        assert!(
            (m.apparent_tilt_deg() - 4.0).abs() < 0.5,
            "{}",
            m.apparent_tilt_deg()
        );
        for (p, q) in pairs().into_iter().take(20) {
            let want = (p.0 - q.0).hypot(p.1 - q.1) / 14.0; // image distance over true magnification
            let got = m.distance_mm(p, q).unwrap();
            assert!((got - want).abs() / want < 4e-4, "got {got} want {want}");
        }
    }

    /// On a board tilted 4 degrees the policy is a real decision: BoardPlane treats the
    /// foreshortening as lens anisotropy and "corrects" it (wrong if the tilt is real, by up to
    /// 1/cos(4 deg) - 1 = 2.4e-3 along the tilt direction); Foreshortened is right.
    #[test]
    fn the_tilt_policy_changes_the_answer_on_a_tilted_board() {
        let (a1, a2) = board_basis(14.0, 1.0, 4.0, 25.0);
        let lat = lattice_of(a1, a2, 0.0, 0.5);
        let fore = FieldMap::fit(&lat, 1.0, 2, TiltPolicy::Foreshortened).unwrap();
        let plane = FieldMap::fit(&lat, 1.0, 2, TiltPolicy::BoardPlane).unwrap();
        let (mut worst_fore, mut worst_plane) = (0.0f64, 0.0f64);
        for (p, q) in pairs().into_iter().take(40) {
            let want = (p.0 - q.0).hypot(p.1 - q.1) / 14.0;
            worst_fore = worst_fore.max((fore.distance_mm(p, q).unwrap() - want).abs() / want);
            worst_plane = worst_plane.max((plane.distance_mm(p, q).unwrap() - want).abs() / want);
        }
        assert!(worst_fore < 4e-4, "foreshortened {worst_fore:.2e}");
        assert!(
            worst_plane > 1.5e-3,
            "board-plane should be visibly different, got {worst_plane:.2e}"
        );
    }

    /// `contains` needs data in the tile, not just a place inside the bounding box: a tile whose
    /// edges are removed must be refused while its neighbours still answer.
    #[test]
    fn a_tile_without_edges_is_outside_the_calibrated_region() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let mut lat = lattice_of(a1, a2, 0.0, 0.5);
        let probe = (CENTRE.0 + 10.0, CENTRE.1 + 10.0);
        let full = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        assert!(full.contains(probe.0, probe.1));
        let hole = tile_of(probe.0, probe.1, TILE_PX);
        let in_hole = |x: f64, y: f64| tile_of(x, y, TILE_PX) == hole;
        lat.vertical.samples.retain(|q| !in_hole(q.s, q.c));
        lat.horizontal.samples.retain(|q| !in_hole(q.c, q.s));
        let holed = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        assert!(
            !holed.contains(probe.0, probe.1),
            "tile {hole:?} has no edges"
        );
        let (x0, x1, y0, y1) = holed.region();
        assert!(
            probe.0 > x0 && probe.0 < x1 && probe.1 > y0 && probe.1 < y1,
            "still inside the bounding box"
        );
        assert!(
            holed.contains(probe.0 + 3.0 * TILE_PX, probe.1),
            "a neighbouring tile still answers"
        );
    }

    #[test]
    fn outside_the_calibrated_region_there_is_no_answer() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.0, 0.5);
        let m = FieldMap::fit(&lat, 1.0, 3, TiltPolicy::BoardPlane).unwrap();
        let (x0, x1, y0, y1) = m.region();
        assert!(x1 - x0 > 300.0 && y1 - y0 > 200.0, "{:?}", m.region());
        assert!(m.to_mm(CENTRE.0, CENTRE.1).is_some());
        assert!(m.to_mm(x0 - 5.0, CENTRE.1).is_none());
        assert!(m.to_mm(CENTRE.0, y1 + 5.0).is_none());
        assert!(m.to_mm(f64::NAN, 1.0).is_none());
        assert!(m.distance_mm((CENTRE.0, CENTRE.1), (5.0, 5.0)).is_none());
        assert!(m.local_um_per_px(CENTRE.0, CENTRE.1, (0.0, 0.0)).is_none());
        assert!(m.local_um_per_px(5.0, 5.0, (1.0, 0.0)).is_none());
    }

    #[test]
    fn batch_conversion_flags_each_point_outside_the_region() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let m = FieldMap::fit(
            &lattice_of(a1, a2, 0.0, 0.5),
            1.0,
            3,
            TiltPolicy::BoardPlane,
        )
        .unwrap();
        let pts = [
            (CENTRE.0, CENTRE.1),
            (CENTRE.0 + 20.0, CENTRE.1),
            (5.0, 5.0),
            (CENTRE.0, CENTRE.1 + 30.0),
        ];
        let all = m.to_mm_all(&pts);
        assert_eq!(
            all.iter().map(Option::is_some).collect::<Vec<_>>(),
            vec![true, true, false, true],
            "only the point at (5, 5) is outside"
        );
        let (a, b) = (all[0].unwrap(), all[1].unwrap());
        assert!(((b.0 - a.0) - 20.0 / 14.0).abs() < 2e-3, "{all:?}");
    }

    #[test]
    fn bad_degrees_and_too_little_data_are_errors() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let lat = lattice_of(a1, a2, 0.0, 0.5);
        assert_eq!(
            FieldMap::fit(&lat, 1.0, 0, TiltPolicy::BoardPlane).unwrap_err(),
            FieldError::BadDegree
        );
        assert_eq!(
            FieldMap::fit(&lat, 1.0, MAX_DEGREE + 1, TiltPolicy::BoardPlane).unwrap_err(),
            FieldError::BadDegree
        );
        let mut small = lat.clone();
        small.vertical.samples.truncate(10);
        assert!(matches!(
            FieldMap::fit(&small, 1.0, 3, TiltPolicy::BoardPlane),
            Err(FieldError::TooFewSamples { have: 10, .. })
        ));
    }

    /// A saved and reloaded calibration must answer exactly like the original, everywhere,
    /// including where it refuses.
    #[test]
    fn calibration_text_round_trips_exactly() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let m = FieldMap::fit(
            &lattice_of(a1, a2, 0.004, 0.5),
            0.6,
            3,
            TiltPolicy::BoardPlane,
        )
        .unwrap();
        let text = m.to_text();
        let back = FieldMap::from_text(&text).unwrap();
        assert_eq!(back.to_text(), text, "text is a fixed point");
        let mut checked = (0, 0);
        for ix in 0..40 {
            for iy in 0..30 {
                let (x, y) = (60.0 + ix as f64 * 14.5, 40.0 + iy as f64 * 15.3);
                assert_eq!(m.contains(x, y), back.contains(x, y), "contains({x},{y})");
                assert_eq!(m.to_mm(x, y), back.to_mm(x, y), "to_mm({x},{y})");
                assert_eq!(m.grid_coords(x, y), back.grid_coords(x, y));
                assert_eq!(
                    m.local_um_per_px(x, y, (1.0, 1.0)),
                    back.local_um_per_px(x, y, (1.0, 1.0))
                );
                if m.contains(x, y) {
                    checked.0 += 1
                } else {
                    checked.1 += 1
                }
            }
        }
        assert!(
            checked.0 > 100 && checked.1 > 100,
            "must exercise both inside and outside: {checked:?}"
        );
        assert_eq!(
            (back.rms_px, back.cv_rms_px, back.clipped),
            (m.rms_px, m.cv_rms_px, m.clipped)
        );
    }

    #[test]
    fn corrupt_calibration_text_is_rejected_not_half_loaded() {
        let (a1, a2) = board_basis(14.0, 0.5, 0.0, 0.0);
        let m = FieldMap::fit(
            &lattice_of(a1, a2, 0.0, 0.5),
            0.6,
            2,
            TiltPolicy::BoardPlane,
        )
        .unwrap();
        let text = m.to_text();
        assert!(FieldMap::from_text("").is_err());
        assert!(
            FieldMap::from_text(&text.replace("fieldmap_version: 1", "fieldmap_version: 2"))
                .unwrap_err()
                .contains("version")
        );
        assert!(
            FieldMap::from_text(&text.replace("policy: BoardPlane", "policy: Sideways")).is_err()
        );
        let no_ci: String = text
            .lines()
            .filter(|l| !l.starts_with("ci: "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(FieldMap::from_text(&no_ci).unwrap_err().contains("ci"));
        let short: String = text
            .lines()
            .map(|l| {
                if l.starts_with("cj: ") {
                    "cj: 1.0 2.0"
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(FieldMap::from_text(&short)
            .unwrap_err()
            .contains("coefficient"));
        let nan = text.replace("degree: 2", "degree: 9");
        assert!(FieldMap::from_text(&nan).is_err());
        let odd: String = text
            .lines()
            .map(|l| {
                if l.starts_with("tiles: ") {
                    "tiles: 1 2 3"
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(FieldMap::from_text(&odd).unwrap_err().contains("tiles"));
    }
}
