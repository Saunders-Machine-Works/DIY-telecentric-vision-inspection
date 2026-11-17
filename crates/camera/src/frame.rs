use crate::CameraError;

/// 8-bit monochrome image, row-major, no padding. The width*height invariant is
/// enforced at construction; fields are read through accessors.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Frame {
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self, CameraError> {
        if width.checked_mul(height) != Some(pixels.len()) {
            return Err(CameraError::Format(format!(
                "{} pixels for {width}x{height}",
                pixels.len()
            )));
        }
        Ok(Frame {
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
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Result of averaging a burst: float pixels so averaging does not re-quantize.
#[derive(Debug, Clone, PartialEq)]
pub struct MeanImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<f32>,
    /// Per-pixel maximum raw value across the burst. 255 means that pixel clipped in at
    /// least one frame, which the mean alone cannot show.
    pub peak: Vec<u8>,
    pub frames: usize,
}

/// 8-bit frames summed into u16 as they arrive, so a burst never holds all of its
/// frames at once. 257 frames of 255 is the most u16 can hold.
pub struct FrameAccumulator {
    width: usize,
    height: usize,
    sum: Vec<u16>,
    peak: Vec<u8>,
    frames: usize,
}

pub const MAX_BURST: usize = 257;

impl FrameAccumulator {
    pub fn new(width: usize, height: usize) -> Self {
        FrameAccumulator {
            width,
            height,
            sum: vec![0; width * height],
            peak: vec![0; width * height],
            frames: 0,
        }
    }

    /// Add raw row-major pixels (exactly width*height bytes).
    pub fn add_pixels(&mut self, pixels: &[u8]) -> Result<(), CameraError> {
        if pixels.len() != self.sum.len() {
            return Err(CameraError::Format(
                "frame size differs from accumulator".into(),
            ));
        }
        if self.frames >= MAX_BURST {
            return Err(CameraError::Format(format!(
                "burst larger than {MAX_BURST}"
            )));
        }
        for ((a, m), &p) in self.sum.iter_mut().zip(self.peak.iter_mut()).zip(pixels) {
            *a += u16::from(p);
            *m = (*m).max(p);
        }
        self.frames += 1;
        Ok(())
    }

    pub fn add(&mut self, f: &Frame) -> Result<(), CameraError> {
        if f.width != self.width || f.height != self.height {
            return Err(CameraError::Format(
                "frame size differs from accumulator".into(),
            ));
        }
        self.add_pixels(&f.pixels)
    }

    pub fn finish(self) -> Result<MeanImage, CameraError> {
        if self.frames == 0 {
            return Err(CameraError::BadFrame("no frames to average".into()));
        }
        let n = self.frames as f32;
        Ok(MeanImage {
            width: self.width,
            height: self.height,
            pixels: self.sum.into_iter().map(|s| f32::from(s) / n).collect(),
            peak: self.peak,
            frames: self.frames,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(w: usize, h: usize, px: Vec<u8>) -> Frame {
        Frame::new(w, h, px).unwrap()
    }

    #[test]
    fn average_of_known_pixels() {
        let mut acc = FrameAccumulator::new(2, 1);
        acc.add(&f(2, 1, vec![10, 200])).unwrap();
        acc.add(&f(2, 1, vec![11, 201])).unwrap();
        let m = acc.finish().unwrap();
        assert_eq!((m.width, m.height, m.frames), (2, 1, 2));
        assert_eq!(m.pixels, vec![10.5, 200.5]);
    }

    /// A pixel clipped in one frame of three has a mean far below 255, but its peak is 255.
    #[test]
    fn peak_exposes_clipping_the_mean_hides() {
        let mut acc = FrameAccumulator::new(2, 1);
        acc.add_pixels(&[100, 255]).unwrap();
        acc.add_pixels(&[100, 100]).unwrap();
        acc.add_pixels(&[90, 100]).unwrap();
        let m = acc.finish().unwrap();
        assert_eq!(m.peak, vec![100, 255]);
        assert!((m.pixels[1] - 151.666_67).abs() < 1e-4, "{}", m.pixels[1]);
    }

    #[test]
    fn full_scale_burst_does_not_overflow() {
        let mut acc = FrameAccumulator::new(1, 1);
        for _ in 0..MAX_BURST {
            acc.add_pixels(&[255]).unwrap();
        }
        assert_eq!(acc.finish().unwrap().pixels, vec![255.0]);
    }

    #[test]
    fn burst_beyond_u16_capacity_is_rejected() {
        let mut acc = FrameAccumulator::new(1, 1);
        for _ in 0..MAX_BURST {
            acc.add_pixels(&[255]).unwrap();
        }
        assert!(matches!(
            acc.add_pixels(&[255]),
            Err(CameraError::Format(_))
        ));
    }

    #[test]
    fn mismatched_sizes_rejected() {
        let mut acc = FrameAccumulator::new(2, 1);
        assert!(matches!(
            acc.add(&f(1, 2, vec![0, 0])),
            Err(CameraError::Format(_))
        ));
    }

    #[test]
    fn empty_burst_rejected_as_bad_frame() {
        assert!(matches!(
            FrameAccumulator::new(1, 1).finish(),
            Err(CameraError::BadFrame(_))
        ));
    }

    #[test]
    fn frame_new_rejects_wrong_pixel_count() {
        assert!(matches!(
            Frame::new(2, 2, vec![0; 3]),
            Err(CameraError::Format(_))
        ));
    }
}
