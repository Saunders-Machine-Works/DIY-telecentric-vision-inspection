//! Local scale across the reticle: fit the whole board, then the left/right/top/bottom
//! halves separately to expose a scale gradient (lens distortion) across the field.
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::image::{ImageView, Roi};
use metrology_core::lattice::{fit_lattice, LatticeParams};

fn main() {
    let mut cam = PylonCamera::open(0).expect("open camera");
    let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
    let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
    let whole = fit_lattice(&img, &LatticeParams::default()).expect("whole-image fit");
    // board extent from the edges the whole-image fit actually used
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for q in &whole.vertical.samples {
        (x0, x1, y0, y1) = (x0.min(q.s), x1.max(q.s), y0.min(q.c), y1.max(q.c));
    }
    let (bx0, bx1, by0, by1) = (
        x0.ceil() as usize,
        x1.floor() as usize,
        y0.ceil() as usize,
        y1.floor() as usize,
    );
    println!("board extent from the fit: x {bx0}..{bx1}, y {by0}..{by1}");
    let (mx, my) = ((bx0 + bx1) / 2, (by0 + by1) / 2);
    let regions = [
        ("whole board", Roi::new(bx0, by0, bx1, by1)),
        ("left half", Roi::new(bx0, by0, mx, by1)),
        ("right half", Roi::new(mx, by0, bx1, by1)),
        ("top half", Roi::new(bx0, by0, bx1, my)),
        ("bottom half", Roi::new(bx0, my, bx1, by1)),
        ("centre 40%", Roi::centered_in(bx0, by0, bx1, by1, 0.4)),
    ];
    for (name, roi) in regions {
        match fit_lattice(&img, &LatticeParams { roi: Some(roi), ..Default::default() }) {
            Ok(f) => println!(
                "{name:<12} sigma_max {:.5} px/sq ({:+.0} ppm vs whole-image)  pitch x {:.5} y {:.5}  tilt {:.2} deg  edge bias x {:+.3} y {:+.3} px  rms x {:.3} y {:.3}  pts {}/{}",
                f.sigma_max, (f.sigma_max / whole.sigma_max - 1.0) * 1e6, f.vertical.pitch, f.horizontal.pitch, f.tilt_deg,
                f.vertical.edge_bias, f.horizontal.edge_bias, f.vertical.rms, f.horizontal.rms, f.vertical.points, f.horizontal.points
            ),
            Err(e) => println!("{name:<12} {e:?}"),
        }
    }
}
