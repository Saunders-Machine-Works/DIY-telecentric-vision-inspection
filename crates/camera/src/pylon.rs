//! Basler pylon C API binding (hand-written, only what is used) and `PylonCamera`.
//! All `unsafe` in the camera path lives in this file.
//!
//! Every grab uses one persistent stream grabber with pre-registered buffers.
//! Each grab/burst starts a fresh acquisition (AcquisitionStart ... AcquisitionStop,
//! then flush), so no frame can be older than the call that asked for it.

use crate::{Camera, CameraError, Frame, FrameAccumulator, MeanImage, RetryPolicy, MAX_BURST};
use std::ffi::{c_char, c_void, CString};
use std::sync::OnceLock;

type GenApiResult = i32;
type Handle = *mut c_void;

const PAYLOAD_TYPE_IMAGE: i32 = 0;
const PIXEL_TYPE_MONO8: i32 = 0x0108_0001;
const GRAB_STATUS_GRABBED: i32 = 2;
const ACCESS_MODE_CONTROL_STREAM: i32 = 1 | 2;
const DEVICE_INFO_STR_LEN: usize = 64;
const FILENAME_MAX: usize = 260; // MSVC value; this binding is Windows-only
const NUM_BUFFERS: usize = 8;
const DEFAULT_TIMEOUT_MS: u32 = 1000;
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// The camera is held by another process (or by our own just-exited previous run).
fn is_busy(e: &CameraError) -> bool {
    matches!(e, CameraError::Driver(m) if m.contains("controlled by another application"))
}

#[repr(C)]
struct PylonDeviceInfo {
    full_name: [c_char; FILENAME_MAX],
    friendly_name: [c_char; DEVICE_INFO_STR_LEN],
    vendor_name: [c_char; DEVICE_INFO_STR_LEN],
    model_name: [c_char; DEVICE_INFO_STR_LEN],
    serial_number: [c_char; DEVICE_INFO_STR_LEN],
    device_class: [c_char; DEVICE_INFO_STR_LEN],
    device_version: [c_char; DEVICE_INFO_STR_LEN],
    user_defined_name: [c_char; DEVICE_INFO_STR_LEN],
}

#[repr(C)]
struct PylonGrabResult {
    context: *const c_void,
    h_buffer: Handle,
    p_buffer: *const c_void,
    status: i32,
    payload_type: i32,
    pixel_type: i32,
    time_stamp: u64,
    size_x: i32,
    size_y: i32,
    offset_x: i32,
    offset_y: i32,
    padding_x: i32,
    padding_y: i32,
    payload_size: u64,
    error_code: u32,
    block_id: u64,
}

// Layout guards against PylonC.h (pylon 7, x64).
const _: () = assert!(std::mem::size_of::<PylonGrabResult>() == 96);
const _: () =
    assert!(std::mem::size_of::<PylonDeviceInfo>() == FILENAME_MAX + 7 * DEVICE_INFO_STR_LEN);

#[link(name = "PylonC_v8_0")]
extern "system" {
    fn PylonInitialize() -> GenApiResult;
    fn PylonEnumerateDevices(num: *mut usize) -> GenApiResult;
    fn PylonGetDeviceInfo(index: usize, di: *mut PylonDeviceInfo) -> GenApiResult;
    fn PylonCreateDeviceByIndex(index: usize, dev: *mut Handle) -> GenApiResult;
    fn PylonDestroyDevice(dev: Handle) -> GenApiResult;
    fn PylonDeviceOpen(dev: Handle, mode: i32) -> GenApiResult;
    fn PylonDeviceClose(dev: Handle) -> GenApiResult;
    fn PylonDeviceGetIntegerFeature(dev: Handle, name: *const c_char, v: *mut i64) -> GenApiResult;
    fn PylonDeviceGetFloatFeature(dev: Handle, name: *const c_char, v: *mut f64) -> GenApiResult;
    fn PylonDeviceSetFloatFeature(dev: Handle, name: *const c_char, v: f64) -> GenApiResult;
    fn PylonDeviceFeatureFromString(
        dev: Handle,
        name: *const c_char,
        v: *const c_char,
    ) -> GenApiResult;
    fn PylonDeviceFeatureToString(
        dev: Handle,
        name: *const c_char,
        buf: *mut c_char,
        len: *mut usize,
    ) -> GenApiResult;
    fn PylonDeviceExecuteCommandFeature(dev: Handle, name: *const c_char) -> GenApiResult;
    fn PylonDeviceGetStreamGrabber(dev: Handle, index: usize, stg: *mut Handle) -> GenApiResult;
    fn PylonStreamGrabberOpen(stg: Handle) -> GenApiResult;
    fn PylonStreamGrabberClose(stg: Handle) -> GenApiResult;
    fn PylonStreamGrabberGetWaitObject(stg: Handle, w: *mut Handle) -> GenApiResult;
    fn PylonStreamGrabberGetPayloadSize(dev: Handle, stg: Handle, size: *mut usize)
        -> GenApiResult;
    fn PylonStreamGrabberSetMaxNumBuffer(stg: Handle, n: usize) -> GenApiResult;
    fn PylonStreamGrabberSetMaxBufferSize(stg: Handle, n: usize) -> GenApiResult;
    fn PylonStreamGrabberPrepareGrab(stg: Handle) -> GenApiResult;
    fn PylonStreamGrabberFinishGrab(stg: Handle) -> GenApiResult;
    fn PylonStreamGrabberRegisterBuffer(
        stg: Handle,
        buf: *mut c_void,
        len: usize,
        h: *mut Handle,
    ) -> GenApiResult;
    fn PylonStreamGrabberDeregisterBuffer(stg: Handle, h: Handle) -> GenApiResult;
    fn PylonStreamGrabberQueueBuffer(stg: Handle, h: Handle, ctx: *const c_void) -> GenApiResult;
    fn PylonStreamGrabberRetrieveResult(
        stg: Handle,
        r: *mut PylonGrabResult,
        ready: *mut bool,
    ) -> GenApiResult;
    fn PylonStreamGrabberStartStreamingIfMandatory(stg: Handle) -> GenApiResult;
    fn PylonStreamGrabberStopStreamingIfMandatory(stg: Handle) -> GenApiResult;
    fn PylonStreamGrabberFlushBuffersToOutput(stg: Handle) -> GenApiResult;
    fn PylonWaitObjectWait(w: Handle, timeout_ms: u32, ready: *mut bool) -> GenApiResult;
    fn GenApiGetLastErrorMessage(buf: *mut c_char, len: *mut usize) -> GenApiResult;
    fn GenApiGetLastErrorDetail(buf: *mut c_char, len: *mut usize) -> GenApiResult;
}

fn last_error(code: GenApiResult) -> CameraError {
    let mut msg = String::new();
    for f in [GenApiGetLastErrorMessage, GenApiGetLastErrorDetail] {
        let mut buf = vec![0 as c_char; 1024];
        let mut len = buf.len();
        // SAFETY: buf is valid for `len` bytes; the driver NUL-terminates and updates len.
        if unsafe { f(buf.as_mut_ptr(), &mut len) } == 0 {
            let s = cstr(&buf);
            if !s.is_empty() {
                if !msg.is_empty() {
                    msg.push_str(" | ");
                }
                msg.push_str(&s);
            }
        }
    }
    CameraError::Driver(format!("0x{:08X}: {msg}", code as u32))
}

fn check(code: GenApiResult) -> Result<(), CameraError> {
    if code == 0 {
        Ok(())
    } else {
        Err(last_error(code))
    }
}

/// Text up to the first NUL (or the whole slice if there is none).
fn cstr(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn init() -> Result<(), CameraError> {
    static INIT: OnceLock<GenApiResult> = OnceLock::new();
    // SAFETY: PylonInitialize runs once; the runtime lives until process exit.
    let code = *INIT.get_or_init(|| unsafe { PylonInitialize() });
    check(code)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub friendly_name: String,
    pub model: String,
    pub serial: String,
    pub device_class: String,
}

pub fn enumerate() -> Result<Vec<DeviceInfo>, CameraError> {
    init()?;
    let mut n = 0usize;
    // SAFETY: n is a valid out-pointer.
    check(unsafe { PylonEnumerateDevices(&mut n) })?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        // SAFETY: PylonDeviceInfo is plain old data; zeroed is a valid initial value.
        let mut di: PylonDeviceInfo = unsafe { std::mem::zeroed() };
        // SAFETY: di is a valid out-pointer and i < n.
        check(unsafe { PylonGetDeviceInfo(i, &mut di) })?;
        out.push(DeviceInfo {
            friendly_name: cstr(&di.friendly_name),
            model: cstr(&di.model_name),
            serial: cstr(&di.serial_number),
            device_class: cstr(&di.device_class),
        });
    }
    Ok(out)
}

/// The region of interest configured on the camera. Frames must match it exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Roi {
    width: usize,
    height: usize,
    offset_x: i32,
    offset_y: i32,
}

/// Validate one grab result against the configured geometry. Pure, so it is unit-tested.
/// Transient driver-reported failures are `BadFrame`; anything that means the frame
/// is not the configured full Mono8 image is `Format` and is never retried.
fn check_grab(r: &PylonGrabResult, roi: Roi) -> Result<(), CameraError> {
    let Roi {
        width,
        height,
        offset_x,
        offset_y,
    } = roi;
    if r.status != GRAB_STATUS_GRABBED {
        return Err(CameraError::BadFrame(format!(
            "status {} error 0x{:X}",
            r.status, r.error_code
        )));
    }
    let fmt = |what: &str| Err(CameraError::Format(what.to_string()));
    if r.payload_type != PAYLOAD_TYPE_IMAGE {
        return fmt(&format!("payload type {}", r.payload_type));
    }
    if r.pixel_type != PIXEL_TYPE_MONO8 {
        return fmt(&format!("pixel type {:#x}", r.pixel_type));
    }
    if r.size_x as usize != width || r.size_y as usize != height {
        return fmt(&format!(
            "size {}x{} (want {width}x{height})",
            r.size_x, r.size_y
        ));
    }
    if r.offset_x != offset_x || r.offset_y != offset_y {
        return fmt(&format!(
            "ROI offset {},{} (want {offset_x},{offset_y})",
            r.offset_x, r.offset_y
        ));
    }
    if r.padding_x != 0 || r.padding_y != 0 {
        return fmt(&format!("padding {},{}", r.padding_x, r.padding_y));
    }
    if r.payload_size != (width * height) as u64 {
        return fmt(&format!("payload {} bytes", r.payload_size));
    }
    if r.p_buffer.is_null() {
        return fmt("null buffer");
    }
    Ok(())
}

struct Stream {
    stg: Handle,
    wait: Handle,
    buffers: Vec<Box<[u8]>>,
    handles: Vec<Handle>,
    prepared: bool,
}

pub struct PylonCamera {
    dev: Handle,
    stream: Option<Stream>,
    width: usize,
    height: usize,
    offset_x: i32,
    offset_y: i32,
    timeout_ms: u32,
}

// SAFETY: a pylon device and its stream grabber may be used from any one thread at a
// time; PylonCamera needs &mut self for every operation and owns its handles uniquely.
unsafe impl Send for PylonCamera {}

impl PylonCamera {
    /// Open the device at `index`, lock acquisition settings so every frame in a burst
    /// is comparable (no auto exposure/gain, no trigger, Mono8, continuous), and open
    /// a persistent stream with pre-registered buffers.
    pub fn open(index: usize) -> Result<Self, CameraError> {
        // PylonCreateDeviceByIndex indexes the list built by the latest enumeration.
        if enumerate()?.len() <= index {
            return Err(CameraError::NotFound);
        }
        let mut dev: Handle = std::ptr::null_mut();
        // SAFETY: dev is a valid out-pointer.
        check(unsafe { PylonCreateDeviceByIndex(index, &mut dev) })?;
        // A GigE camera stays "controlled by another application" for a few seconds after the
        // previous process exits (heartbeat timeout), so a quick restart retries for ~10 s.
        let mut opened = Ok(());
        for attempt in 0..OPEN_ATTEMPTS {
            // SAFETY: dev was just created and is not yet open.
            opened = check(unsafe { PylonDeviceOpen(dev, ACCESS_MODE_CONTROL_STREAM) });
            match &opened {
                Err(e) if attempt + 1 < OPEN_ATTEMPTS && is_busy(e) => {
                    std::thread::sleep(OPEN_RETRY_DELAY)
                }
                _ => break,
            }
        }
        if let Err(e) = opened {
            // SAFETY: dev is valid and not opened.
            unsafe { PylonDestroyDevice(dev) };
            return Err(e);
        }
        let mut cam = PylonCamera {
            dev,
            stream: None,
            width: 0,
            height: 0,
            offset_x: 0,
            offset_y: 0,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        };
        cam.configure()?; // on error `cam` drops and releases the device
        cam.open_stream()?;
        Ok(cam)
    }

    fn configure(&mut self) -> Result<(), CameraError> {
        self.set_string("PixelFormat", "Mono8")?;
        self.set_string("TriggerSelector", "FrameStart")?;
        self.set_string("TriggerMode", "Off")?;
        self.set_string("ExposureAuto", "Off")?;
        self.set_string("GainAuto", "Off")?;
        self.set_string("AcquisitionMode", "Continuous")?;
        self.width = self.get_int("Width")? as usize;
        self.height = self.get_int("Height")? as usize;
        self.offset_x = self.get_int("OffsetX")? as i32;
        self.offset_y = self.get_int("OffsetY")? as i32;
        Ok(())
    }

    fn open_stream(&mut self) -> Result<(), CameraError> {
        let mut stg: Handle = std::ptr::null_mut();
        // SAFETY: self.dev is open; stg is a valid out-pointer.
        check(unsafe { PylonDeviceGetStreamGrabber(self.dev, 0, &mut stg) })?;
        // SAFETY: stg was just obtained.
        check(unsafe { PylonStreamGrabberOpen(stg) })?;
        // Store immediately so Drop cleans up whatever happens below.
        self.stream = Some(Stream {
            stg,
            wait: std::ptr::null_mut(),
            buffers: Vec::new(),
            handles: Vec::new(),
            prepared: false,
        });
        let mut wait: Handle = std::ptr::null_mut();
        // SAFETY: stream grabber is open; wait is a valid out-pointer.
        check(unsafe { PylonStreamGrabberGetWaitObject(stg, &mut wait) })?;
        let mut payload = 0usize;
        // SAFETY: device and grabber are open; payload is a valid out-pointer.
        check(unsafe { PylonStreamGrabberGetPayloadSize(self.dev, stg, &mut payload) })?;
        if payload < self.width * self.height {
            return Err(CameraError::Format(format!(
                "payload {payload} < frame size"
            )));
        }
        // SAFETY: grabber is open and not yet prepared.
        check(unsafe { PylonStreamGrabberSetMaxNumBuffer(stg, NUM_BUFFERS) })?;
        check(unsafe { PylonStreamGrabberSetMaxBufferSize(stg, payload) })?;
        check(unsafe { PylonStreamGrabberPrepareGrab(stg) })?;
        let s = self.stream.as_mut().expect("stream was just stored");
        s.wait = wait;
        s.prepared = true;
        for _ in 0..NUM_BUFFERS {
            let mut buf = vec![0u8; payload].into_boxed_slice();
            let mut h: Handle = std::ptr::null_mut();
            // SAFETY: the boxed buffer's heap storage never moves or is freed until Drop
            // deregisters it.
            check(unsafe {
                PylonStreamGrabberRegisterBuffer(stg, buf.as_mut_ptr().cast(), payload, &mut h)
            })?;
            s.buffers.push(buf);
            s.handles.push(h);
        }
        Ok(())
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Per-grab wait for the next frame. A grab at the default 3 ms exposure takes ~52 ms.
    pub fn set_timeout_ms(&mut self, ms: u32) {
        self.timeout_ms = ms;
    }

    /// Exposure in microseconds (legacy GigE name first, then SFNC).
    pub fn exposure_us(&self) -> Result<f64, CameraError> {
        self.get_float("ExposureTimeAbs")
            .or_else(|_| self.get_float("ExposureTime"))
    }

    pub fn set_exposure_us(&mut self, us: f64) -> Result<(), CameraError> {
        if self.set_float("ExposureTimeAbs", us).is_ok() {
            return Ok(());
        }
        self.set_float("ExposureTime", us)
    }

    pub fn gain_raw(&self) -> Result<i64, CameraError> {
        self.get_int("GainRaw")
    }

    fn name(feature: &str) -> Result<CString, CameraError> {
        CString::new(feature)
            .map_err(|_| CameraError::Driver(format!("bad feature name {feature}")))
    }

    pub fn get_int(&self, feature: &str) -> Result<i64, CameraError> {
        let n = Self::name(feature)?;
        let mut v = 0i64;
        // SAFETY: self.dev is open; n is NUL-terminated; v is a valid out-pointer.
        check(unsafe { PylonDeviceGetIntegerFeature(self.dev, n.as_ptr(), &mut v) })?;
        Ok(v)
    }

    pub fn get_float(&self, feature: &str) -> Result<f64, CameraError> {
        let n = Self::name(feature)?;
        let mut v = 0f64;
        // SAFETY: as in get_int.
        check(unsafe { PylonDeviceGetFloatFeature(self.dev, n.as_ptr(), &mut v) })?;
        Ok(v)
    }

    pub(crate) fn set_float(&mut self, feature: &str, value: f64) -> Result<(), CameraError> {
        let n = Self::name(feature)?;
        // SAFETY: as in get_int.
        check(unsafe { PylonDeviceSetFloatFeature(self.dev, n.as_ptr(), value) })
    }

    pub fn get_string(&self, feature: &str) -> Result<String, CameraError> {
        let n = Self::name(feature)?;
        let mut buf = vec![0 as c_char; 256];
        let mut len = buf.len();
        // SAFETY: buf is valid for len bytes.
        check(unsafe {
            PylonDeviceFeatureToString(self.dev, n.as_ptr(), buf.as_mut_ptr(), &mut len)
        })?;
        Ok(cstr(&buf))
    }

    pub(crate) fn set_string(&mut self, feature: &str, value: &str) -> Result<(), CameraError> {
        let n = Self::name(feature)?;
        let v = CString::new(value).map_err(|_| CameraError::Driver("bad value".into()))?;
        // SAFETY: both strings are NUL-terminated.
        check(unsafe { PylonDeviceFeatureFromString(self.dev, n.as_ptr(), v.as_ptr()) })
    }

    fn command(&mut self, name: &str) -> Result<(), CameraError> {
        let n = Self::name(name)?;
        // SAFETY: self.dev is open; n is NUL-terminated.
        check(unsafe { PylonDeviceExecuteCommandFeature(self.dev, n.as_ptr()) })
    }

    /// Acquire `n` fresh frames, handing each frame's pixels to `sink` in order.
    /// Always stops the acquisition and returns every buffer to idle, success or not.
    fn run_burst(
        &mut self,
        n: usize,
        mut sink: impl FnMut(&[u8]) -> Result<(), CameraError>,
    ) -> Result<(), CameraError> {
        let result = self.burst_inner(n, &mut sink);
        self.end_acquisition();
        result
    }

    fn burst_inner(
        &mut self,
        n: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), CameraError>,
    ) -> Result<(), CameraError> {
        let (w, h, timeout) = (self.width, self.height, self.timeout_ms);
        let roi = Roi {
            width: w,
            height: h,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
        };
        let (stg, wait, nbuf) = {
            let s = self
                .stream
                .as_ref()
                .ok_or_else(|| CameraError::Driver("stream not open".into()))?;
            (s.stg, s.wait, s.handles.len())
        };
        let mut queued = n.min(nbuf);
        for i in 0..queued {
            let s = self.stream.as_ref().expect("checked above");
            // SAFETY: handle i was registered on this grabber and is idle.
            check(unsafe { PylonStreamGrabberQueueBuffer(stg, s.handles[i], i as *const c_void) })?;
        }
        // SAFETY: grabber is prepared with registered, queued buffers.
        check(unsafe { PylonStreamGrabberStartStreamingIfMandatory(stg) })?;
        self.command("AcquisitionStart")?;

        let mut got = 0usize;
        let mut last_block: Option<u64> = None;
        while got < n {
            let mut ready = false;
            // SAFETY: wait is the grabber's wait object; ready is a valid out-pointer.
            check(unsafe { PylonWaitObjectWait(wait, timeout, &mut ready) })?;
            if !ready {
                return Err(CameraError::Timeout);
            }
            while got < n {
                // SAFETY: PylonGrabResult is plain old data; zeroed is valid.
                let mut res: PylonGrabResult = unsafe { std::mem::zeroed() };
                let mut have = false;
                // SAFETY: res and have are valid out-pointers.
                check(unsafe { PylonStreamGrabberRetrieveResult(stg, &mut res, &mut have) })?;
                if !have {
                    break;
                }
                check_grab(&res, roi)?;
                if res.block_id != 0 && res.block_id != u64::MAX {
                    if last_block == Some(res.block_id) {
                        return Err(CameraError::BadFrame(format!(
                            "duplicate block id {}",
                            res.block_id
                        )));
                    }
                    last_block = Some(res.block_id);
                }
                let idx = res.context as usize;
                let s = self.stream.as_ref().expect("checked above");
                let buf = s
                    .buffers
                    .get(idx)
                    .ok_or_else(|| CameraError::Driver("bad buffer context".into()))?;
                sink(&buf[..w * h])?;
                got += 1;
                if queued < n {
                    // SAFETY: res.h_buffer is the buffer just retrieved, now idle.
                    check(unsafe {
                        PylonStreamGrabberQueueBuffer(stg, res.h_buffer, res.context)
                    })?;
                    queued += 1;
                }
            }
        }
        Ok(())
    }

    /// Best effort: stop acquisition, flush queued buffers to the output queue and drain it.
    fn end_acquisition(&mut self) {
        let _ = self.command("AcquisitionStop");
        let Some(s) = self.stream.as_ref() else {
            return;
        };
        // SAFETY: stg is a prepared stream grabber; res/ready are valid out-pointers.
        unsafe {
            PylonStreamGrabberStopStreamingIfMandatory(s.stg);
            PylonStreamGrabberFlushBuffersToOutput(s.stg);
            loop {
                let mut res: PylonGrabResult = std::mem::zeroed();
                let mut have = false;
                if PylonStreamGrabberRetrieveResult(s.stg, &mut res, &mut have) != 0 || !have {
                    break;
                }
            }
        }
    }
}

impl Camera for PylonCamera {
    fn grab(&mut self) -> Result<Frame, CameraError> {
        let (w, h) = (self.width, self.height);
        let mut out: Option<Vec<u8>> = None;
        self.run_burst(1, |px| {
            out = Some(px.to_vec());
            Ok(())
        })?;
        Frame::new(w, h, out.ok_or(CameraError::Timeout)?)
    }

    /// Streaming burst: one acquisition start, `n` back-to-back frames summed into a
    /// u16 accumulator as they arrive. A transient failure restarts the whole burst so
    /// frames from before and after a hiccup are never mixed.
    fn grab_mean(&mut self, n: usize, policy: RetryPolicy) -> Result<MeanImage, CameraError> {
        if n == 0 || n > MAX_BURST {
            return Err(CameraError::Format(format!("burst of {n} frames")));
        }
        let (w, h) = (self.width, self.height);
        let mut last = CameraError::Timeout;
        for attempt in 0..policy.attempts.max(1) {
            let mut acc = FrameAccumulator::new(w, h);
            match self.run_burst(n, |px| acc.add_pixels(px)) {
                Ok(()) => return acc.finish(),
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

impl Drop for PylonCamera {
    fn drop(&mut self) {
        self.end_acquisition();
        if let Some(s) = self.stream.take() {
            // SAFETY: all handles belong to this camera; order follows the pylon sample:
            // deregister buffers, FinishGrab, close the grabber. Buffers outlive deregistration.
            unsafe {
                for h in &s.handles {
                    PylonStreamGrabberDeregisterBuffer(s.stg, *h);
                }
                if s.prepared {
                    PylonStreamGrabberFinishGrab(s.stg);
                }
                PylonStreamGrabberClose(s.stg);
            }
            drop(s.buffers);
        }
        // SAFETY: dev is a valid open device owned by self.
        unsafe {
            PylonDeviceClose(self.dev);
            PylonDestroyDevice(self.dev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROI: Roi = Roi {
        width: 4,
        height: 3,
        offset_x: 8,
        offset_y: 4,
    };

    fn good() -> PylonGrabResult {
        // SAFETY: plain old data.
        let mut r: PylonGrabResult = unsafe { std::mem::zeroed() };
        r.status = GRAB_STATUS_GRABBED;
        r.payload_type = PAYLOAD_TYPE_IMAGE;
        r.pixel_type = PIXEL_TYPE_MONO8;
        r.size_x = 4;
        r.size_y = 3;
        r.offset_x = 8;
        r.offset_y = 4;
        r.payload_size = 12;
        r.p_buffer = std::ptr::dangling::<c_void>();
        r
    }

    #[test]
    fn only_the_busy_message_is_retried_on_open() {
        let busy = CameraError::Driver(
            "0xC2000001: Failed to open. The device is controlled by another application. Err: x"
                .into(),
        );
        assert!(is_busy(&busy));
        assert!(!is_busy(&CameraError::Driver(
            "0xC2000005: node not found".into()
        )));
        assert!(!is_busy(&CameraError::NotFound));
        assert!(!is_busy(&CameraError::Timeout));
    }

    #[test]
    fn full_mono8_frame_passes() {
        assert_eq!(check_grab(&good(), ROI), Ok(()));
    }

    #[test]
    fn driver_failed_status_is_transient() {
        let mut r = good();
        r.status = 4;
        r.error_code = 0x5;
        assert!(
            matches!(check_grab(&r, ROI), Err(CameraError::BadFrame(m)) if m.contains("error 0x5"))
        );
    }

    #[test]
    fn shifted_roi_padding_and_short_payload_are_format_errors() {
        let cases: [fn(&mut PylonGrabResult); 6] = [
            |r| r.offset_x = 0,
            |r| r.offset_y = 0,
            |r| r.padding_x = 4,
            |r| r.padding_y = 1,
            |r| r.payload_size = 11,
            |r| r.payload_type = 3,
        ];
        for (i, mutate) in cases.iter().enumerate() {
            let mut r = good();
            mutate(&mut r);
            assert!(
                matches!(check_grab(&r, ROI), Err(CameraError::Format(_))),
                "case {i}"
            );
        }
    }

    #[test]
    fn wrong_size_or_pixel_type_is_a_format_error() {
        let mut r = good();
        r.size_x = 5;
        assert!(matches!(check_grab(&r, ROI), Err(CameraError::Format(_))));
        let mut r = good();
        r.pixel_type = 0x0110_0005; // Mono12
        assert!(matches!(check_grab(&r, ROI), Err(CameraError::Format(_))));
    }

    #[test]
    fn cstr_stops_at_nul_and_tolerates_missing_terminator() {
        let a: Vec<c_char> = [b'a', b'b', 0, b'z'].iter().map(|&b| b as c_char).collect();
        assert_eq!(cstr(&a), "ab");
        let b: Vec<c_char> = b"xy".iter().map(|&b| b as c_char).collect();
        assert_eq!(cstr(&b), "xy");
    }
}
