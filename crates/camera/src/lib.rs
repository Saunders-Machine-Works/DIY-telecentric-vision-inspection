//! Camera abstraction. The measurement core never sees a driver type.

mod frame;
mod mock;
mod retry;

#[allow(unsafe_code)]
pub mod pylon;

pub use frame::{Frame, FrameAccumulator, MeanImage, MAX_BURST};
pub use mock::MockCamera;
pub use retry::{grab_with_retry, RetryPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraError {
    NotFound,
    Driver(String),
    /// Transient: the grab completed but the frame was incomplete or failed
    /// (GigE link settling). Retried.
    BadFrame(String),
    /// The frame is well-formed but not what was configured (size, pixel type,
    /// padding, ROI offset). A setup error, never retried.
    Format(String),
    Timeout,
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CameraError::NotFound => write!(f, "no camera found"),
            CameraError::Driver(s) => write!(f, "driver error: {s}"),
            CameraError::BadFrame(s) => write!(f, "bad frame: {s}"),
            CameraError::Format(s) => write!(f, "unexpected frame format: {s}"),
            CameraError::Timeout => write!(f, "grab timed out"),
        }
    }
}
impl std::error::Error for CameraError {}

/// One camera that delivers 8-bit monochrome frames. `Send` so it can live on a
/// worker thread away from the GUI.
pub trait Camera: Send {
    fn grab(&mut self) -> Result<Frame, CameraError>;

    /// Mean of `n` consecutive fresh frames (the measurement snapshot).
    /// Default: sequential single grabs. A transient failure restarts the WHOLE
    /// burst (up to `policy.attempts`), so frames from before and after a hiccup are
    /// never mixed. Drivers may override with a faster streaming burst.
    fn grab_mean(&mut self, n: usize, policy: RetryPolicy) -> Result<MeanImage, CameraError> {
        if n == 0 || n > MAX_BURST {
            return Err(CameraError::Format(format!("burst of {n} frames")));
        }
        let mut last = CameraError::Timeout;
        for attempt in 0..policy.attempts.max(1) {
            let mut acc: Option<FrameAccumulator> = None;
            let mut outcome = Ok(());
            for _ in 0..n {
                match self.grab() {
                    Ok(f) => {
                        let a =
                            acc.get_or_insert_with(|| FrameAccumulator::new(f.width(), f.height()));
                        a.add(&f)?;
                    }
                    Err(e) => {
                        outcome = Err(e);
                        break;
                    }
                }
            }
            match outcome {
                Ok(()) => return acc.expect("n >= 1").finish(),
                Err(e @ (CameraError::BadFrame(_) | CameraError::Timeout)) => {
                    last = e;
                    if attempt + 1 < policy.attempts {
                        std::thread::sleep(policy.delay);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }
}
