//! Time the app's measurement path on the live camera: open, 8-frame burst, diameter scan and
//! thread inspection (the 1 s start and 1 s measure budgets).
//! Usage: time_measure <calibration.txt> [y_lo] [y_hi] [x_lo] [x_hi] [tpi]
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::diameter::{measure_diameter, scan_edges, DiameterWindow};
use metrology_core::field::FieldMap;
use metrology_core::image::ImageView;
use metrology_core::thread_measure::measure_thread;
use metrology_core::thread_teeth::{inspect, ThreadSpec};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = FieldMap::from_text(&std::fs::read_to_string(&a[0]).expect("read calibration"))
        .expect("parse calibration");
    let num = |i: usize, d: usize| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let window = DiameterWindow {
        rows: (num(1, 940), num(2, 1460)),
        cols: (num(3, 760), num(4, 1640)),
    };
    let tpi: f64 = a.get(5).map_or(13.0, |s| s.parse().expect("tpi"));
    let to_mm = |x: f64, y: f64| map.to_mm(x, y);
    let t = Instant::now();
    let mut cam = PylonCamera::open(0).expect("open camera");
    println!("open camera:        {:?}", t.elapsed());
    for run in 0..3 {
        let t = Instant::now();
        let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
        let grab = t.elapsed();
        let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
        let t = Instant::now();
        let d = measure_diameter(&img, &to_mm, window);
        let diameter = t.elapsed();
        let t = Instant::now();
        let scan = scan_edges(&img, &to_mm, window);
        let (mut fit_ok, mut teeth_ok) = (false, false);
        if let Ok(s) = &scan {
            fit_ok = measure_thread(&s.left_mm, &s.right_mm).is_ok();
            teeth_ok = inspect(&s.left_mm, &s.right_mm, &ThreadSpec::unified(tpi)).is_ok();
        }
        let thread = t.elapsed();
        println!(
            "run {run}: 8-frame grab {grab:?} | diameter {diameter:?} ({}) | thread scan+fit+teeth {thread:?} (fit ok {fit_ok}, teeth ok {teeth_ok})",
            d.map_or("no part".to_string(), |d| format!("{:.5} in", d.diameter_mm / 25.4))
        );
    }
}
