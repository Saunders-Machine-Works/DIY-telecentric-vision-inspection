use crate::{Camera, CameraError, Frame};
use std::collections::VecDeque;

/// Scripted camera for tests: returns queued results in order.
pub struct MockCamera {
    script: VecDeque<Result<Frame, CameraError>>,
    pub grabs: usize,
}

impl MockCamera {
    pub fn new(script: Vec<Result<Frame, CameraError>>) -> Self {
        MockCamera {
            script: script.into(),
            grabs: 0,
        }
    }
}

impl Camera for MockCamera {
    fn grab(&mut self) -> Result<Frame, CameraError> {
        self.grabs += 1;
        self.script.pop_front().unwrap_or(Err(CameraError::Timeout))
    }
}
