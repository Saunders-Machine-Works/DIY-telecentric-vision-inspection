use crate::{Camera, CameraError, Frame};
use std::time::Duration;

/// GigE links can fail the first grabs after open ("buffer incompletely grabbed")
/// while the link settles. Retry a few times with a short sleep.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            attempts: 5,
            delay: Duration::from_millis(100),
        }
    }
}

/// Retries only transient frame problems. Driver errors are returned immediately.
pub fn grab_with_retry<C: Camera + ?Sized>(
    cam: &mut C,
    policy: RetryPolicy,
) -> Result<Frame, CameraError> {
    let mut last = CameraError::Timeout;
    for attempt in 0..policy.attempts.max(1) {
        match cam.grab() {
            Ok(f) => return Ok(f),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockCamera;

    fn px(v: u8) -> Frame {
        Frame::new(1, 1, vec![v]).unwrap()
    }
    fn fast() -> RetryPolicy {
        RetryPolicy {
            attempts: 3,
            delay: Duration::ZERO,
        }
    }

    #[test]
    fn succeeds_on_third_try_after_two_bad_frames() {
        let mut cam = MockCamera::new(vec![
            Err(CameraError::BadFrame("incomplete".into())),
            Err(CameraError::Timeout),
            Ok(px(7)),
        ]);
        assert_eq!(grab_with_retry(&mut cam, fast()).unwrap(), px(7));
        assert_eq!(cam.grabs, 3);
    }

    #[test]
    fn gives_up_after_attempts_and_reports_last_error() {
        let mut cam = MockCamera::new(vec![
            Err(CameraError::Timeout),
            Err(CameraError::Timeout),
            Err(CameraError::BadFrame("still bad".into())),
            Ok(px(1)),
        ]);
        let err = grab_with_retry(&mut cam, fast()).unwrap_err();
        assert_eq!(err, CameraError::BadFrame("still bad".into()));
        assert_eq!(cam.grabs, 3);
    }

    #[test]
    fn driver_error_is_not_retried() {
        let mut cam = MockCamera::new(vec![Err(CameraError::Driver("gone".into())), Ok(px(1))]);
        assert!(grab_with_retry(&mut cam, fast()).is_err());
        assert_eq!(cam.grabs, 1);
    }

    #[test]
    fn format_error_is_not_retried() {
        let mut cam = MockCamera::new(vec![Err(CameraError::Format("padding".into())), Ok(px(1))]);
        assert_eq!(
            grab_with_retry(&mut cam, fast()).unwrap_err(),
            CameraError::Format("padding".into())
        );
        assert_eq!(cam.grabs, 1);
    }

    #[test]
    fn mean_of_burst_of_eight() {
        let script = (0..8).map(|i| Ok(px(10 + i))).collect();
        let mut cam = MockCamera::new(script);
        let m = cam.grab_mean(8, fast()).unwrap();
        assert_eq!((m.frames, m.pixels), (8, vec![13.5]));
    }

    /// A failure mid-burst restarts the whole burst: the frame grabbed before the
    /// failure (10) must NOT contribute to the mean.
    #[test]
    fn failure_mid_burst_restarts_the_whole_burst() {
        let mut cam = MockCamera::new(vec![
            Ok(px(10)),
            Err(CameraError::BadFrame("x".into())),
            Err(CameraError::Timeout),
            Ok(px(30)),
            Ok(px(40)),
        ]);
        let m = cam.grab_mean(2, fast()).unwrap();
        assert_eq!((m.frames, m.pixels), (2, vec![35.0]));
        assert_eq!(cam.grabs, 5);
    }

    #[test]
    fn burst_gives_up_after_attempts() {
        let mut cam = MockCamera::new(vec![
            Ok(px(1)),
            Err(CameraError::Timeout),
            Ok(px(2)),
            Err(CameraError::Timeout),
            Ok(px(3)),
            Err(CameraError::BadFrame("last".into())),
        ]);
        assert_eq!(
            cam.grab_mean(2, fast()).unwrap_err(),
            CameraError::BadFrame("last".into())
        );
    }

    #[test]
    fn zero_frames_requested_is_a_setup_error_not_retried() {
        let mut cam = MockCamera::new(vec![]);
        assert!(matches!(
            cam.grab_mean(0, fast()),
            Err(CameraError::Format(_))
        ));
        assert_eq!(cam.grabs, 0);
    }
}
