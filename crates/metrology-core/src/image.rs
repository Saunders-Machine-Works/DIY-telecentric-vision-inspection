//! Borrowed grayscale image view used by all measurement code.

/// Row-major f32 grayscale pixels (0..=255 scale) borrowed from the caller.
/// Fields are private so the `width * height == pixels.len()` invariant always holds.
#[derive(Debug, Clone, Copy)]
pub struct ImageView<'a> {
    width: usize,
    height: usize,
    pixels: &'a [f32],
}

impl<'a> ImageView<'a> {
    /// `None` unless both dimensions are non-zero and `pixels.len() == width * height`.
    pub fn new(width: usize, height: usize, pixels: &'a [f32]) -> Option<Self> {
        let n = width.checked_mul(height)?;
        (n > 0 && pixels.len() == n).then_some(ImageView {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &'a [f32] {
        self.pixels
    }

    pub fn row(&self, y: usize) -> &'a [f32] {
        &self.pixels[y * self.width..(y + 1) * self.width]
    }
}

/// Half-open pixel rectangle: x in [x0, x1), y in [y0, y1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roi {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Roi {
    pub fn new(x0: usize, y0: usize, x1: usize, y1: usize) -> Self {
        Roi { x0, y0, x1, y1 }
    }

    /// Centered box covering `fraction` (clamped to 0..=1) of each dimension.
    pub fn centered(width: usize, height: usize, fraction: f64) -> Self {
        let f = fraction.clamp(0.0, 1.0);
        let (w, h) = (
            ((width as f64 * f) as usize).min(width),
            ((height as f64 * f) as usize).min(height),
        );
        let (x0, y0) = ((width - w) / 2, (height - h) / 2);
        Roi::new(x0, y0, x0 + w, y0 + h)
    }

    /// Centered box covering `fraction` of the box (x0,y0)-(x1,y1) in each dimension.
    pub fn centered_in(x0: usize, y0: usize, x1: usize, y1: usize, fraction: f64) -> Self {
        let inner = Roi::centered(x1 - x0, y1 - y0, fraction);
        Roi::new(x0 + inner.x0, y0 + inner.y0, x0 + inner.x1, y0 + inner.y1)
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub fn fits(&self, img: &ImageView) -> bool {
        !self.is_empty() && self.x1 <= img.width && self.y1 <= img.height
    }

    /// Pixel count; 0 for an empty or inverted ROI (never underflows).
    pub fn area(&self) -> usize {
        self.x1.saturating_sub(self.x0) * self.y1.saturating_sub(self.y0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_requires_matching_length_and_nonzero_dims() {
        let px = [0.0f32; 6];
        assert!(ImageView::new(3, 2, &px).is_some());
        assert!(ImageView::new(2, 2, &px).is_none());
        assert!(ImageView::new(0, 6, &px).is_none());
        assert!(
            ImageView::new(usize::MAX, 2, &px).is_none(),
            "overflowing size"
        );
    }

    #[test]
    fn view_accessors_and_rows() {
        let px = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let v = ImageView::new(3, 2, &px).unwrap();
        assert_eq!((v.width(), v.height()), (3, 2));
        assert_eq!(v.row(1), &[4.0, 5.0, 6.0]);
    }

    #[test]
    fn centered_roi_is_centered() {
        assert_eq!(Roi::centered(100, 80, 0.5), Roi::new(25, 20, 75, 60));
        assert_eq!(
            Roi::centered(10, 10, 5.0),
            Roi::new(0, 0, 10, 10),
            "clamped"
        );
    }

    #[test]
    fn roi_fit_and_area() {
        let px = [0.0f32; 12];
        let img = ImageView::new(4, 3, &px).unwrap();
        assert!(Roi::new(1, 1, 4, 3).fits(&img));
        assert!(!Roi::new(1, 1, 5, 3).fits(&img));
        assert!(!Roi::new(2, 2, 2, 3).fits(&img));
        assert_eq!(Roi::new(1, 1, 4, 3).area(), 6);
    }

    #[test]
    fn inverted_roi_has_zero_area_and_does_not_panic() {
        assert_eq!(Roi::new(5, 5, 2, 9).area(), 0);
        assert_eq!(Roi::new(2, 9, 5, 5).area(), 0);
    }
}
