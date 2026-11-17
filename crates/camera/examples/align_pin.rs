//! Live alignment meter for a vertical pin in a small lit aperture disc: move the backlight until
//! the light margin left and right of the pin is equal (offset ~ 0).
//! Usage: align_pin [seconds] [y_lo] [y_hi] [um_per_px]   (defaults: 120 s, rows 1050..1190, 30.14)
//! Each line: light margin left/right of the pin (mm), the pin width (mm) and the offset of the
//! pin centre from the disc centre (mm). Offset > 0: pin is right of the disc centre.
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize, d: f64| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let (secs, y0, y1, um) = (
        num(0, 120.0),
        num(1, 1050.0) as usize,
        num(2, 1190.0) as usize,
        num(3, 30.14),
    );
    let mut cam = PylonCamera::open(0).expect("open camera");
    let t0 = std::time::Instant::now();
    println!("margins are light left/right of the pin; offset -> 0 is centred. Ctrl+C to stop.");
    while t0.elapsed().as_secs_f64() < secs {
        let m = cam.grab_mean(2, RetryPolicy::default()).expect("grab");
        let (w, rows) = (m.width, y1 - y0);
        let mut avg = vec![0.0f64; w];
        for y in y0..y1 {
            for (x, v) in avg.iter_mut().enumerate() {
                *v += m.pixels[y * w + x] as f64 / rows as f64;
            }
        }
        // lit runs (> half of 190); the pin is the dark gap between the two widest runs
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut start = None;
        for (x, v) in avg.iter().enumerate() {
            match (*v > 95.0, start) {
                (true, None) => start = Some(x),
                (false, Some(s)) => {
                    runs.push((s, x - 1));
                    start = None;
                }
                _ => {}
            }
        }
        runs.retain(|r| r.1 - r.0 >= 3);
        if runs.len() < 2 {
            println!(
                "pin not found between two lit runs ({} runs): disc/pin not both visible",
                runs.len()
            );
            continue;
        }
        let (l, r) = (runs[0], runs[runs.len() - 1]);
        let mm = |px: f64| px * um / 1000.0;
        let (ml, mr) = ((l.1 - l.0) as f64, (r.1 - r.0) as f64);
        let (pin_l, pin_r) = (l.1 as f64, r.0 as f64);
        let pin_c = 0.5 * (pin_l + pin_r);
        let disc_c = 0.5 * (l.0 + r.1) as f64;
        println!(
            "light left {:5.2} mm  right {:5.2} mm   pin {:5.2} mm   offset {:+5.2} mm",
            mm(ml),
            mm(mr),
            mm(pin_r - pin_l),
            mm(pin_c - disc_c)
        );
    }
}
