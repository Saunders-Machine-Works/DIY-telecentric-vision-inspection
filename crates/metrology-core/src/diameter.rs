//! Plain diameter of a vertical turned part (a pin, a shaft section) from one image.
//!
//! One horizontal scan per image row in a window: each scan gives the left and right silhouette
//! edges (`profile::silhouette_edges`, the 50 % crossings), both are converted to millimetres
//! through the caller's pixel-to-mm map, and the width is their distance. The diameter is the
//! mean of the row widths; the spread across rows, the taper (width change per axial length) and
//! the tilt of the part show a part that is not a clean cylinder or not standing straight.
//!
//! The brightness of the background on either side of the part is also reported, because the
//! two edges are only measured the same way if the light is even across the part: a gradient
//! across the field moves one edge's 50 % point differently from the other's (a 5 % ramp moved a
//! 0.5 in pin by 6 um on the bench).

use crate::image::ImageView;
use crate::profile::{silhouette_edges, Profile};

/// Fewest rows that make a measurement, and the least fraction of the window that must measure.
const MIN_ROWS: usize = 20;
const MIN_FRACTION: f64 = 0.5;
/// The background brightness is taken between this many samples from the edge and `MARGIN_FAR`.
const MARGIN_NEAR: usize = 20;
const MARGIN_FAR: usize = 60;

/// Pixel (x, y) to millimetres, `None` outside the calibrated region.
pub type PixelToMm<'a> = &'a dyn Fn(f64, f64) -> Option<(f64, f64)>;

/// Rows and columns to scan: half-open pixel ranges, the part must lie inside `cols` with
/// background on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiameterWindow {
    pub rows: (usize, usize),
    pub cols: (usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diameter {
    pub diameter_mm: f64,
    /// Standard deviation of the row widths, mm.
    pub row_sd_mm: f64,
    pub rows_used: usize,
    pub rows_skipped: usize,
    /// Width change per unit of axial length (dimensionless, positive when wider at larger y).
    pub taper: f64,
    /// Angle of the part's axis from the image y axis, degrees.
    pub tilt_deg: f64,
    /// Median over rows of (left - right) / (left + right) background brightness beside the
    /// part; 0 for even light, `None` if the margins were too narrow to judge.
    pub illumination_imbalance: Option<f64>,
    /// The measured edge points in pixels, one per used row, for overlays.
    pub left_px: Vec<(f64, f64)>,
    pub right_px: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiameterError {
    /// The window is empty or outside the image.
    BadWindow,
    /// Too few rows found a part: `used` of the requested rows measured.
    TooFewRows { used: usize, skipped: usize },
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

/// Least-squares slope of `v` against `x`.
fn slope(x: &[f64], v: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, mv) = (x.iter().sum::<f64>() / n, v.iter().sum::<f64>() / n);
    let num: f64 = x.iter().zip(v).map(|(a, b)| (a - mx) * (b - mv)).sum();
    let den: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    if den > 0.0 {
        num / den
    } else {
        0.0
    }
}

/// The silhouette edges of every row of a window that found a part.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeScan {
    /// Image rows that measured, ascending.
    pub rows: Vec<usize>,
    pub left_px: Vec<(f64, f64)>,
    pub right_px: Vec<(f64, f64)>,
    pub left_mm: Vec<(f64, f64)>,
    pub right_mm: Vec<(f64, f64)>,
    pub rows_requested: usize,
    /// Median over rows of (left - right) / (left + right) background brightness beside the
    /// part; 0 for even light, `None` if the margins were too narrow to judge.
    pub illumination_imbalance: Option<f64>,
}

/// Scan every row of `window` for the part's two silhouette edges. Rows where no part is found
/// or whose edges fall outside the calibrated region are left out; the scan is refused if fewer
/// than `MIN_ROWS` (or half of the window) measured.
pub fn scan_edges(
    img: &ImageView,
    to_mm: PixelToMm,
    window: DiameterWindow,
) -> Result<EdgeScan, DiameterError> {
    let (rows, cols) = (window.rows, window.cols);
    if rows.1 <= rows.0 || cols.1 <= cols.0 || rows.1 > img.height() || cols.1 > img.width() {
        return Err(DiameterError::BadWindow);
    }
    let mut scan = EdgeScan {
        rows: Vec::new(),
        left_px: Vec::new(),
        right_px: Vec::new(),
        left_mm: Vec::new(),
        right_mm: Vec::new(),
        rows_requested: rows.1 - rows.0,
        illumination_imbalance: None,
    };
    let mut balance = Vec::new();
    for row in rows.0..rows.1 {
        let Some(prof) = Profile::along_axis(img, true, row, cols.0, cols.1) else {
            continue;
        };
        let Some((l, r)) = silhouette_edges(&prof.values) else {
            continue;
        };
        let (pl, pr) = (
            (cols.0 as f64 + l, row as f64),
            (cols.0 as f64 + r, row as f64),
        );
        let (Some(ml), Some(mr)) = (to_mm(pl.0, pl.1), to_mm(pr.0, pr.1)) else {
            continue;
        };
        scan.rows.push(row);
        scan.left_px.push(pl);
        scan.right_px.push(pr);
        scan.left_mm.push(ml);
        scan.right_mm.push(mr);
        let side = |lo: f64, hi: f64| {
            let (lo, hi) = (
                lo.max(0.0) as usize,
                (hi.max(0.0) as usize).min(prof.values.len()),
            );
            (hi >= lo + 8)
                .then(|| median(prof.values[lo..hi].to_vec()))
                .flatten()
        };
        let (lo_l, hi_l) = (l - MARGIN_FAR as f64, l - MARGIN_NEAR as f64);
        let (lo_r, hi_r) = (r + MARGIN_NEAR as f64, r + MARGIN_FAR as f64);
        if let (Some(a), Some(b)) = (side(lo_l, hi_l), side(lo_r, hi_r)) {
            if a + b > 0.0 {
                balance.push((a - b) / (a + b));
            }
        }
    }
    let used = scan.rows.len();
    if used < MIN_ROWS || (used as f64) < MIN_FRACTION * scan.rows_requested as f64 {
        return Err(DiameterError::TooFewRows {
            used,
            skipped: scan.rows_requested - used,
        });
    }
    scan.illumination_imbalance = median(balance);
    Ok(scan)
}

/// Measure the diameter of the vertical part in `window`.
pub fn measure_diameter(
    img: &ImageView,
    to_mm: PixelToMm,
    window: DiameterWindow,
) -> Result<Diameter, DiameterError> {
    let scan = scan_edges(img, to_mm, window)?;
    let used = scan.rows.len();
    let ys: Vec<f64> = scan.rows.iter().map(|&r| r as f64).collect();
    let widths: Vec<f64> = scan
        .left_mm
        .iter()
        .zip(&scan.right_mm)
        .map(|(l, r)| (r.0 - l.0).hypot(r.1 - l.1))
        .collect();
    let n = used as f64;
    let mean = widths.iter().sum::<f64>() / n;
    let sd = (widths.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    // axial length of a row step, mm (the map is nearly uniform: use the first and last row)
    let (first, last) = (scan.left_mm[0], scan.left_mm[used - 1]);
    let step = (last.0 - first.0).hypot(last.1 - first.1) / (ys[used - 1] - ys[0]);
    let taper = if step > 0.0 {
        slope(&ys, &widths) / step
    } else {
        0.0
    };
    let centre: Vec<f64> = scan
        .left_px
        .iter()
        .zip(&scan.right_px)
        .map(|(l, r)| 0.5 * (l.0 + r.0))
        .collect();
    Ok(Diameter {
        diameter_mm: mean,
        row_sd_mm: sd,
        rows_used: used,
        rows_skipped: scan.rows_requested - used,
        taper,
        tilt_deg: slope(&ys, &centre).atan().to_degrees(),
        illumination_imbalance: scan.illumination_imbalance,
        left_px: scan.left_px,
        right_px: scan.right_px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 600;
    const H: usize = 300;
    const MM_PER_PX: f64 = 0.03;

    /// A dark bar with linear 3 px ramps at its edges (a 50 % crossing exactly at the ramp
    /// midpoints) on a background of `bright` that may vary across the field. `edge(y)` gives
    /// the (left, right) edge positions of row `y`.
    fn image(edge: &dyn Fn(usize) -> (f64, f64), bright: &dyn Fn(usize) -> f64) -> Vec<f32> {
        let mut px = vec![0.0f32; W * H];
        for y in 0..H {
            let (a, b) = edge(y);
            for x in 0..W {
                let xf = x as f64;
                let into = ((xf - (a - 1.5)) / 3.0).clamp(0.0, 1.0); // 0 outside .. 1 inside
                let out = ((b + 1.5 - xf) / 3.0).clamp(0.0, 1.0);
                let dark = into.min(out);
                px[y * W + x] = (bright(x) * (1.0 - dark) + 5.0 * dark) as f32;
            }
        }
        px
    }

    fn linear(x: f64, y: f64) -> Option<(f64, f64)> {
        Some((x * MM_PER_PX, y * MM_PER_PX))
    }

    fn window() -> DiameterWindow {
        DiameterWindow {
            rows: (20, 280),
            cols: (20, 580),
        }
    }

    #[test]
    fn straight_bar_diameter_with_real_values() {
        let px = image(&|_| (200.0, 400.4), &|_| 200.0);
        let img = ImageView::new(W, H, &px).unwrap();
        let d = measure_diameter(&img, &linear, window()).unwrap();
        assert!(
            (d.diameter_mm - 200.4 * MM_PER_PX).abs() < 0.0005,
            "{}",
            d.diameter_mm
        );
        assert_eq!((d.rows_used, d.rows_skipped), (260, 0));
        assert!(d.row_sd_mm < 1e-3 && d.taper.abs() < 1e-4 && d.tilt_deg.abs() < 0.01);
        assert!(d.illumination_imbalance.unwrap().abs() < 1e-3);
        assert_eq!((d.left_px.len(), d.right_px.len()), (260, 260));
        assert!((d.left_px[0].0 - 200.0).abs() < 0.05);
    }

    #[test]
    fn anisotropic_scale_uses_the_x_scale_for_a_vertical_part() {
        let px = image(&|_| (200.0, 300.0), &|_| 200.0);
        let img = ImageView::new(W, H, &px).unwrap();
        let skewed = |x: f64, y: f64| Some((x * 0.03, y * 0.05));
        let d = measure_diameter(&img, &skewed, window()).unwrap();
        assert!((d.diameter_mm - 3.0).abs() < 0.001, "{}", d.diameter_mm);
    }

    #[test]
    fn taper_and_tilt_are_reported() {
        // right edge moves 0.02 px per row (taper), left edge 0.01 px per row: the width grows
        // 0.01 px per row, the centre moves 0.015 px per row
        let px = image(
            &|y| (200.0 + 0.01 * y as f64, 400.0 + 0.02 * y as f64),
            &|_| 200.0,
        );
        let img = ImageView::new(W, H, &px).unwrap();
        let d = measure_diameter(&img, &linear, window()).unwrap();
        assert!((d.taper - 0.01).abs() < 5e-4, "taper {}", d.taper);
        let tilt = 0.015f64.atan().to_degrees();
        assert!((d.tilt_deg - tilt).abs() < 0.02, "tilt {}", d.tilt_deg);
    }

    #[test]
    fn uneven_light_is_reported_with_its_sign() {
        // background 220 at the left of the part falling to 180 at the right
        let px = image(&|_| (250.0, 350.0), &|x| 220.0 - 40.0 * x as f64 / W as f64);
        let img = ImageView::new(W, H, &px).unwrap();
        let d = measure_diameter(&img, &linear, window()).unwrap();
        let imb = d.illumination_imbalance.unwrap();
        // left margin about 220 - 40 * 205 / 600, right margin about 220 - 40 * 395 / 600
        let (l, r) = (220.0 - 40.0 * 205.0 / 600.0, 220.0 - 40.0 * 395.0 / 600.0);
        assert!((imb - (l - r) / (l + r)).abs() < 0.01, "{imb}");
        assert!(imb > 0.02);
    }

    #[test]
    fn empty_and_off_image_windows_and_no_part_are_errors() {
        let px = image(&|_| (200.0, 300.0), &|_| 200.0);
        let img = ImageView::new(W, H, &px).unwrap();
        for bad in [
            DiameterWindow {
                rows: (50, 50),
                cols: (20, 580),
            },
            DiameterWindow {
                rows: (20, 280),
                cols: (20, 700),
            },
            DiameterWindow {
                rows: (20, 400),
                cols: (20, 580),
            },
        ] {
            assert_eq!(
                measure_diameter(&img, &linear, bad),
                Err(DiameterError::BadWindow)
            );
        }
        // a window that sees only background: nothing measures
        let none = DiameterWindow {
            rows: (20, 280),
            cols: (400, 580),
        };
        assert_eq!(
            measure_diameter(&img, &linear, none),
            Err(DiameterError::TooFewRows {
                used: 0,
                skipped: 260
            })
        );
    }

    #[test]
    fn rows_outside_the_calibrated_region_are_skipped() {
        let px = image(&|_| (200.0, 300.0), &|_| 200.0);
        let img = ImageView::new(W, H, &px).unwrap();
        // the map only covers rows below 150
        let half = |x: f64, y: f64| (y >= 150.0).then_some((x * MM_PER_PX, y * MM_PER_PX));
        let d = measure_diameter(&img, &half, window()).unwrap();
        assert_eq!((d.rows_used, d.rows_skipped), (130, 130));
        // with under half the window covered the measurement is refused
        let sliver = |x: f64, y: f64| (y >= 250.0).then_some((x * MM_PER_PX, y * MM_PER_PX));
        assert!(matches!(
            measure_diameter(&img, &sliver, window()),
            Err(DiameterError::TooFewRows {
                used: 30,
                skipped: 230
            })
        ));
    }
}
