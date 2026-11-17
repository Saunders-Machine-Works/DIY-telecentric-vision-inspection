//! The camera thread: owns the camera, streams live frames, and runs measurements, so the window
//! never waits on the camera or on a measurement. The window talks to it through two channels,
//! plus one shared slot that holds only the newest live frame (a slow window drops frames
//! instead of queueing 5 MB each).

use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::diameter::{measure_diameter, scan_edges, Diameter, DiameterWindow, EdgeScan};
use metrology_core::field::FieldMap;
use metrology_core::image::ImageView;
use metrology_core::thread_measure::{measure_thread, ThreadFit};
use metrology_core::thread_teeth::{inspect, TeethReport, ThreadSpec};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Diameter,
    Thread,
}

/// What one measurement was asked to do; kept with its result so a result can be recognised as
/// belonging to settings that have since changed.
#[derive(Clone)]
pub struct Job {
    pub mode: Mode,
    pub window: DiameterWindow,
    pub tpi: f64,
    pub calibration: Arc<FieldMap>,
}

pub enum Cmd {
    Connect,
    Disconnect,
    Live(bool),
    Measure(Job),
    Quit,
}

/// An 8-bit gray frame for display; the full image is `scale` times larger in each direction.
pub struct Frame8 {
    pub width: usize,
    pub height: usize,
    pub scale: usize,
    pub pixels: Vec<u8>,
}

pub type LiveSlot = Arc<Mutex<Option<Frame8>>>;

pub struct ThreadOutcome {
    pub scan: EdgeScan,
    pub fit: Result<ThreadFit, String>,
    pub teeth: Result<TeethReport, String>,
}

pub enum Outcome {
    Diameter(Result<Diameter, String>),
    Thread(Result<Box<ThreadOutcome>, String>),
}

pub struct Measured {
    pub job: Job,
    pub outcome: Outcome,
    pub grab: Duration,
    pub compute: Duration,
}

pub enum Event {
    Status(String),
    Connected(bool),
    /// A new live frame is waiting in the slot.
    FrameReady,
    LiveStopped,
    Measured(Box<Measured>, Frame8),
    MeasureFailed(String),
}

const BURST: usize = 8;

fn to_u8(pixels: &[f32]) -> Vec<u8> {
    pixels
        .iter()
        .map(|p| p.round().clamp(0.0, 255.0) as u8)
        .collect()
}

/// Half-size copy (2 x 2 mean) of an 8-bit frame, for the live view.
pub fn halve(pixels: &[u8], width: usize, height: usize) -> Frame8 {
    let (w, h) = (width / 2, height / 2);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let (r0, r1) = (&pixels[2 * y * width..], &pixels[(2 * y + 1) * width..]);
        for x in 0..w {
            let sum = u32::from(r0[2 * x])
                + u32::from(r0[2 * x + 1])
                + u32::from(r1[2 * x])
                + u32::from(r1[2 * x + 1]);
            out.push(((sum + 2) / 4) as u8);
        }
    }
    Frame8 {
        width: w,
        height: h,
        scale: 2,
        pixels: out,
    }
}

fn run_job(job: &Job, img: &ImageView) -> Outcome {
    let map = &job.calibration;
    let to_mm = |x: f64, y: f64| map.to_mm(x, y);
    match job.mode {
        Mode::Diameter => Outcome::Diameter(
            measure_diameter(img, &to_mm, job.window).map_err(|e| format!("{e:?}")),
        ),
        Mode::Thread => Outcome::Thread(
            scan_edges(img, &to_mm, job.window)
                .map_err(|e| format!("{e:?}"))
                .map(|scan| {
                    let fit =
                        measure_thread(&scan.left_mm, &scan.right_mm).map_err(|e| format!("{e:?}"));
                    let teeth =
                        inspect(&scan.left_mm, &scan.right_mm, &ThreadSpec::unified(job.tpi))
                            .map_err(|e| format!("{e:?}"));
                    Box::new(ThreadOutcome { scan, fit, teeth })
                }),
        ),
    }
}

/// Start the worker; `repaint` wakes the window whenever an event is sent.
pub fn spawn(
    cmds: Receiver<Cmd>,
    events: Sender<Event>,
    slot: LiveSlot,
    repaint: impl Fn() + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let send = |e: Event| {
            let _ = events.send(e);
            repaint();
        };
        let mut cam: Option<PylonCamera> = None;
        let mut live = false;
        loop {
            // while live, poll for commands without waiting; otherwise sleep until one arrives
            let cmd = if live && cam.is_some() {
                match cmds.try_recv() {
                    Ok(c) => Some(c),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(_) => return,
                }
            } else {
                match cmds.recv_timeout(Duration::from_millis(200)) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            match cmd {
                Some(Cmd::Quit) => return,
                Some(Cmd::Connect) => match PylonCamera::open(0) {
                    Ok(c) => {
                        let (w, h) = c.size();
                        cam = Some(c);
                        send(Event::Connected(true));
                        send(Event::Status(format!("camera connected, {w} x {h}")));
                    }
                    Err(e) => send(Event::Status(format!("could not open the camera: {e}"))),
                },
                Some(Cmd::Disconnect) => {
                    live = false;
                    cam = None;
                    send(Event::Connected(false));
                    send(Event::Status("camera disconnected".into()));
                }
                Some(Cmd::Live(on)) => live = on,
                Some(Cmd::Measure(job)) => match cam.as_mut() {
                    None => send(Event::MeasureFailed("connect the camera first".into())),
                    Some(c) => {
                        let t0 = Instant::now();
                        match c.grab_mean(BURST, RetryPolicy::default()) {
                            Ok(m) => {
                                let grab = t0.elapsed();
                                let t1 = Instant::now();
                                match ImageView::new(m.width, m.height, &m.pixels) {
                                    Some(img) => {
                                        let outcome = run_job(&job, &img);
                                        let frame = Frame8 {
                                            width: m.width,
                                            height: m.height,
                                            scale: 1,
                                            pixels: to_u8(&m.pixels),
                                        };
                                        let measured = Measured {
                                            job,
                                            outcome,
                                            grab,
                                            compute: t1.elapsed(),
                                        };
                                        send(Event::Measured(Box::new(measured), frame));
                                    }
                                    None => send(Event::MeasureFailed(
                                        "the camera returned an image of the wrong size".into(),
                                    )),
                                }
                            }
                            Err(e) => send(Event::MeasureFailed(format!("grab failed: {e}"))),
                        }
                    }
                },
                None => {}
            }
            if live {
                if let Some(c) = cam.as_mut() {
                    let (w, h) = c.size();
                    match c.grab() {
                        Ok(f) => {
                            let frame = halve(f.pixels(), w, h);
                            // only wake the window if it has consumed the previous frame
                            let fresh = slot.lock().map(|mut s| s.replace(frame).is_none());
                            if fresh.unwrap_or(false) {
                                send(Event::FrameReady);
                            }
                        }
                        Err(e) => {
                            live = false;
                            send(Event::LiveStopped);
                            send(Event::Status(format!("live view stopped: {e}")));
                        }
                    }
                    // about 8 frames a second is plenty for aiming and costs the link little
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halve_averages_two_by_two_blocks_with_rounding() {
        // 4 x 2: blocks (10, 20, 30, 41) -> 25.25 -> 25 and (0, 0, 0, 1) -> 0.25 -> 0
        let px = [10, 20, 0, 0, 30, 41, 0, 1];
        let f = halve(&px, 4, 2);
        assert_eq!((f.width, f.height, f.scale), (2, 1, 2));
        assert_eq!(f.pixels, vec![25, 0]);
        // 255 stays 255 (no overflow in the sum)
        assert_eq!(halve(&[255; 16], 4, 4).pixels, vec![255; 4]);
    }

    #[test]
    fn to_u8_rounds_and_clamps() {
        assert_eq!(
            to_u8(&[0.4, 0.5, 254.6, 300.0, -3.0]),
            vec![0, 1, 255, 255, 0]
        );
    }
}
