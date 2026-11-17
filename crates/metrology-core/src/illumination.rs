//! Image-based backlight check (DESIGN_GUIDELINES 6.1). The DCS-100E only reports its
//! own setpoint, so illumination is judged from the captured image itself.
//! Default limits are placeholders to be tuned on real captures.

use crate::image::{ImageView, Roi};

const BINS: usize = 1024; // over 0..256 => 0.25 gray-level resolution
const BINS_PER_LEVEL: f32 = BINS as f32 / 256.0;
const BIN_WIDTH: f32 = 256.0 / BINS as f32;
// Note: an 8-frame mean is a multiple of 0.125, so values sit on bin edges and the in-bin
// interpolation reads up to ~0.125 gray high (a flat 200.0 reports 200.125). Irrelevant
// against the 5-10 gray-level scale of the limits, but do not use these as exact gray values.

/// Fallback saturation test when no raw per-pixel peak is available: the mean of an
/// all-255 burst is exactly 255. A mean cannot reveal a pixel that clipped in only some
/// frames, so prefer passing the burst's per-pixel peak to `roi_stats`.
pub const MEAN_SATURATION_LEVEL: f32 = 254.99;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IllumStats {
    pub p05: f32,
    pub median: f32,
    pub p95: f32,
    /// Fraction of ROI pixels that clipped (raw peak 255 if given, else mean at 255).
    pub saturated_fraction: f64,
    /// p05 / p95: 1.0 is perfectly flat, lower means uneven illumination in the ROI.
    pub uniformity: f32,
    pub pixels: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IllumLimits {
    pub median_min: f32,
    pub median_max: f32,
    /// Saturated fraction must be strictly below this ("< 0.5 %").
    pub max_saturated_fraction: f64,
    pub min_uniformity: f32,
}

impl Default for IllumLimits {
    fn default() -> Self {
        IllumLimits {
            median_min: 150.0,
            median_max: 230.0,
            max_saturated_fraction: 0.005,
            min_uniformity: 0.85,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IllumIssue {
    TooDark,
    TooBright,
    Saturated,
    NonUniform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoiError {
    /// Empty, or extends past the image.
    OutOfBounds,
    /// The ROI contains NaN or infinite pixels, so no statistic can be trusted.
    NonFinite(usize),
    /// The peak plane does not match the image size.
    PeakSizeMismatch,
}

/// Percentiles, saturation and uniformity over `roi`. The ROI must contain background
/// only (no part silhouette) for the uniformity figure to mean anything.
/// `peak` is the burst's per-pixel raw maximum (same size as the image), if known.
pub fn roi_stats(img: &ImageView, roi: Roi, peak: Option<&[u8]>) -> Result<IllumStats, RoiError> {
    if !roi.fits(img) {
        return Err(RoiError::OutOfBounds);
    }
    if peak.is_some_and(|p| p.len() != img.pixels().len()) {
        return Err(RoiError::PeakSizeMismatch);
    }
    // Four interleaved histograms avoid store-to-load stalls when most pixels share a bin.
    let mut hist = [[0u32; BINS]; 4];
    let (mut saturated, mut non_finite) = (0usize, 0usize);
    for y in roi.y0..roi.y1 {
        let row = &img.row(y)[roi.x0..roi.x1];
        for (i, &v) in row.iter().enumerate() {
            non_finite += usize::from(!v.is_finite());
            // `as usize` maps NaN and negatives to 0 and saturates large values.
            let b = ((v * BINS_PER_LEVEL) as usize).min(BINS - 1);
            hist[i & 3][b] += 1;
        }
        saturated += match peak {
            Some(p) => p[y * img.width() + roi.x0..y * img.width() + roi.x1]
                .iter()
                .filter(|&&m| m == 255)
                .count(),
            None => row.iter().filter(|&&v| v >= MEAN_SATURATION_LEVEL).count(),
        };
    }
    if non_finite > 0 {
        return Err(RoiError::NonFinite(non_finite));
    }
    let mut merged = [0u32; BINS];
    for (b, m) in merged.iter_mut().enumerate() {
        *m = hist[0][b] + hist[1][b] + hist[2][b] + hist[3][b];
    }
    let n = roi.area();
    let (p05, median, p95) = (
        percentile(&merged, n, 0.05),
        percentile(&merged, n, 0.50),
        percentile(&merged, n, 0.95),
    );
    Ok(IllumStats {
        p05,
        median,
        p95,
        saturated_fraction: saturated as f64 / n as f64,
        uniformity: if p95 > 0.0 { p05 / p95 } else { 0.0 },
        pixels: n,
    })
}

/// Value below which fraction `q` of the pixels lie, interpolated inside its bin.
fn percentile(hist: &[u32; BINS], n: usize, q: f64) -> f32 {
    let target = q * n as f64;
    let mut cum = 0.0f64;
    for (b, &c) in hist.iter().enumerate() {
        let next = cum + f64::from(c);
        if next >= target && c > 0 {
            let frac = ((target - cum) / f64::from(c)).clamp(0.0, 1.0) as f32;
            return (b as f32 + frac) * BIN_WIDTH;
        }
        cum = next;
    }
    255.0
}

/// Every limit that is violated; empty means the illumination is acceptable.
pub fn check(stats: &IllumStats, limits: &IllumLimits) -> Vec<IllumIssue> {
    let mut out = Vec::new();
    if stats.median < limits.median_min {
        out.push(IllumIssue::TooDark);
    }
    if stats.median > limits.median_max {
        out.push(IllumIssue::TooBright);
    }
    if stats.saturated_fraction >= limits.max_saturated_fraction {
        out.push(IllumIssue::Saturated);
    }
    if stats.uniformity < limits.min_uniformity {
        out.push(IllumIssue::NonUniform);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_of(w: usize, h: usize, px: &[f32], roi: Roi) -> IllumStats {
        roi_stats(&ImageView::new(w, h, px).unwrap(), roi, None).unwrap()
    }

    #[test]
    fn flat_field_has_unit_uniformity() {
        let px = vec![200.0f32; 100 * 100];
        let s = stats_of(100, 100, &px, Roi::new(0, 0, 100, 100));
        assert!((s.median - 200.0).abs() <= 0.25, "median {}", s.median);
        assert!(
            (s.uniformity - 1.0).abs() < 0.002,
            "uniformity {}",
            s.uniformity
        );
        assert_eq!(s.saturated_fraction, 0.0);
        assert_eq!(s.pixels, 10_000);
    }

    /// Uniform ramp 0..255.75 in 0.25 steps: percentile q must land on 256*q.
    #[test]
    fn ramp_percentiles_land_on_their_true_values() {
        let px: Vec<f32> = (0..1024).map(|i| i as f32 * 0.25).collect();
        let s = stats_of(1024, 1, &px, Roi::new(0, 0, 1024, 1));
        assert!((s.p05 - 12.8).abs() < 0.3, "p05 {}", s.p05);
        assert!((s.median - 128.0).abs() < 0.3, "median {}", s.median);
        assert!((s.p95 - 243.2).abs() < 0.3, "p95 {}", s.p95);
    }

    #[test]
    fn two_level_field_percentiles() {
        let mut px = vec![100.0f32; 100 * 10];
        for row in px.chunks_mut(100) {
            row[50..].fill(200.0);
        }
        let s = stats_of(100, 10, &px, Roi::new(0, 0, 100, 10));
        assert!((s.p05 - 100.0).abs() <= 0.25, "p05 {}", s.p05);
        assert!((s.p95 - 200.0).abs() <= 0.25, "p95 {}", s.p95);
        assert!((s.uniformity - 0.5).abs() < 0.01, "u {}", s.uniformity);
    }

    #[test]
    fn roi_uses_row_stride_correctly() {
        let mut px = vec![50.0f32; 40 * 20];
        for row in px.chunks_mut(40) {
            row[20..].fill(200.0);
        }
        let s = stats_of(40, 20, &px, Roi::new(25, 5, 38, 15));
        assert!((s.median - 200.0).abs() <= 0.25);
        assert!((s.p05 - 200.0).abs() <= 0.25);
        assert_eq!(s.pixels, 13 * 10);
    }

    #[test]
    fn mean_fallback_counts_only_full_scale_pixels() {
        let mut px = vec![180.0f32; 1000];
        px[..10].fill(255.0); // exactly 1 %
        px[10..20].fill(254.0); // near but not saturated
        assert_eq!(
            stats_of(100, 10, &px, Roi::new(0, 0, 100, 10)).saturated_fraction,
            0.01
        );
    }

    /// A pixel clipped in some frames has a low mean; only the raw peak reveals it.
    #[test]
    fn peak_plane_finds_clipping_the_mean_hides() {
        let px = vec![200.0f32; 100]; // means all look healthy
        let mut peak = vec![220u8; 100];
        peak[..5].fill(255); // 5 % clipped in at least one frame
        let img = ImageView::new(10, 10, &px).unwrap();
        let s = roi_stats(&img, Roi::new(0, 0, 10, 10), Some(&peak)).unwrap();
        assert_eq!(s.saturated_fraction, 0.05);
        let no_peak = roi_stats(&img, Roi::new(0, 0, 10, 10), None).unwrap();
        assert_eq!(no_peak.saturated_fraction, 0.0, "the mean alone misses it");
    }

    #[test]
    fn peak_plane_respects_roi_offsets() {
        let px = vec![200.0f32; 100];
        let mut peak = vec![0u8; 100];
        peak[0] = 255; // top-left pixel only
        let img = ImageView::new(10, 10, &px).unwrap();
        let outside = roi_stats(&img, Roi::new(1, 0, 10, 10), Some(&peak)).unwrap();
        assert_eq!(outside.saturated_fraction, 0.0);
        let inside = roi_stats(&img, Roi::new(0, 0, 2, 1), Some(&peak)).unwrap();
        assert_eq!(inside.saturated_fraction, 0.5);
    }

    #[test]
    fn wrong_peak_size_is_rejected() {
        let px = vec![200.0f32; 100];
        let img = ImageView::new(10, 10, &px).unwrap();
        assert_eq!(
            roi_stats(&img, Roi::new(0, 0, 10, 10), Some(&[0u8; 99])),
            Err(RoiError::PeakSizeMismatch)
        );
    }

    #[test]
    fn nan_and_infinity_are_errors_not_black_pixels() {
        let mut px = vec![200.0f32; 100];
        px[3] = f32::NAN;
        px[4] = f32::INFINITY;
        let img = ImageView::new(10, 10, &px).unwrap();
        assert_eq!(
            roi_stats(&img, Roi::new(0, 0, 10, 10), None),
            Err(RoiError::NonFinite(2))
        );
        // outside the ROI they are ignored
        assert!(roi_stats(&img, Roi::new(0, 1, 10, 10), None).is_ok());
    }

    #[test]
    fn out_of_range_values_clamp_into_end_bins() {
        let mut px = vec![200.0f32; 100];
        px[0] = -5.0;
        px[1] = 400.0;
        let s = stats_of(10, 10, &px, Roi::new(0, 0, 10, 10));
        assert!((s.median - 200.0).abs() <= 0.25);
        assert!(s.p95 <= 256.0 && s.p05 >= 0.0);
    }

    #[test]
    fn bad_rois_are_rejected() {
        let px = vec![0.0f32; 100];
        let img = ImageView::new(10, 10, &px).unwrap();
        assert_eq!(
            roi_stats(&img, Roi::new(0, 0, 11, 10), None),
            Err(RoiError::OutOfBounds)
        );
        assert_eq!(
            roi_stats(&img, Roi::new(5, 5, 5, 9), None),
            Err(RoiError::OutOfBounds)
        );
    }

    fn st(median: f32, sat: f64, uniformity: f32) -> IllumStats {
        IllumStats {
            p05: 0.0,
            median,
            p95: 0.0,
            saturated_fraction: sat,
            uniformity,
            pixels: 1,
        }
    }

    #[test]
    fn check_reports_each_violation_with_exact_boundaries() {
        let l = IllumLimits::default();
        assert!(check(&st(190.0, 0.0, 0.95), &l).is_empty());
        // inclusive limits
        assert!(check(&st(150.0, 0.0049, 0.85), &l).is_empty());
        assert!(check(&st(230.0, 0.0, 0.95), &l).is_empty());
        // just outside each
        assert_eq!(check(&st(149.9, 0.0, 0.95), &l), vec![IllumIssue::TooDark]);
        assert_eq!(
            check(&st(230.1, 0.0, 0.95), &l),
            vec![IllumIssue::TooBright]
        );
        assert_eq!(
            check(&st(190.0, 0.0, 0.849), &l),
            vec![IllumIssue::NonUniform]
        );
        // saturated must be strictly below 0.5 %
        assert_eq!(
            check(&st(190.0, 0.005, 0.95), &l),
            vec![IllumIssue::Saturated]
        );
        assert_eq!(
            check(&st(240.0, 0.02, 0.5), &l),
            vec![
                IllumIssue::TooBright,
                IllumIssue::Saturated,
                IllumIssue::NonUniform
            ]
        );
    }
}
