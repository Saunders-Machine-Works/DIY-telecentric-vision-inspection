//! Build the field-map calibration from a saved reticle frame (an 8-bit PNG written by this
//! project) and store it as text. No camera needed.
//! Usage: calibrate_from_png <reticle.png> <out.txt> [square_mm] [nominal_square_px] [degree]
use metrology_core::field::{FieldMap, TiltPolicy};
use metrology_core::image::ImageView;
use metrology_core::lattice::{fit_lattice, LatticeParams};
use metrology_core::png::decode_gray8;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (src, dst) = (
        a.first().expect("reticle png"),
        a.get(1).expect("output txt"),
    );
    let square_mm: f64 = a.get(2).map_or(0.6, |s| s.parse().expect("square_mm"));
    let nominal: f64 = a.get(3).map_or(20.0, |s| s.parse().expect("nominal px"));
    let degree: usize = a.get(4).map_or(3, |s| s.parse().expect("degree"));
    let (w, h, px) = decode_gray8(&std::fs::read(src).expect("read png")).expect("decode png");
    let f: Vec<f32> = px.iter().map(|&v| f32::from(v)).collect();
    let img = ImageView::new(w, h, &f).unwrap();
    let lat = fit_lattice(
        &img,
        &LatticeParams {
            nominal_pitch_px: nominal,
            ..Default::default()
        },
    )
    .expect("lattice fit");
    println!(
        "{w}x{h}: {:.5} px/sq = {:.4} um/px, apparent tilt {:.2} deg (noise ~1.5)",
        lat.sigma_max,
        lat.um_per_px(square_mm),
        lat.tilt_deg
    );
    let map = FieldMap::fit(&lat, square_mm, degree, TiltPolicy::BoardPlane).expect("field fit");
    println!(
        "degree {degree} BoardPlane: rms ({:.3}, {:.3}) px, cv ({:.3}, {:.3}) px, clipped {:?}",
        map.rms_px.0, map.rms_px.1, map.cv_rms_px.0, map.cv_rms_px.1, map.clipped
    );
    std::fs::write(dst, map.to_text()).expect("write calibration");
    println!("wrote {dst}");
}
