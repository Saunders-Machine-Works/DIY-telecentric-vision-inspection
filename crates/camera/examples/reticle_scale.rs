//! Reticle scale from the live camera: grab an 8-frame mean, fit the checkerboard lattice
//! and report scale, tilt and residuals. Usage: reticle_scale [nominal_square_px] [square_mm]
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::image::ImageView;
use metrology_core::lattice::{fit_lattice, LatticeParams};
use std::time::Instant;

fn main() {
    let a: Vec<f64> = std::env::args()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    let nominal = a.first().copied().unwrap_or(20.0);
    let square_mm = a.get(1).copied().unwrap_or(0.6);
    let mut cam = PylonCamera::open(0).expect("open camera");
    for burst in 0..4 {
        let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
        let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
        let t = Instant::now();
        match fit_lattice(&img, &LatticeParams { nominal_pitch_px: nominal, ..Default::default() }) {
            Ok(f) => println!(
                "burst {burst}: {:?}  sigma_max {:.5} px/sq -> {:.4} um/px | tilt {:.3} deg  rot {:.3} deg | pitch x {:.5} y {:.5} | rms x {:.3} y {:.3} px | pts {} / {}",
                t.elapsed(), f.sigma_max, f.um_per_px(square_mm), f.tilt_deg, f.rotation_deg,
                f.vertical.pitch, f.horizontal.pitch, f.vertical.rms, f.horizontal.rms, f.vertical.points, f.horizontal.points
            ),
            Err(e) => println!("burst {burst}: {e:?}"),
        }
    }
}
