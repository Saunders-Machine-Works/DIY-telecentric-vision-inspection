//! Does 8-bit rounding of the averaged frame bias the reticle scale? Uses a saved reticle PNG
//! (already rounded): fit as is, then with +-0.5 uniform dither added (undoes the rounding's
//! structure on average), over several seeds, and with extra coarsening for a dose-response.
use metrology_core::image::ImageView;
use metrology_core::lattice::{fit_lattice, LatticeParams};
use metrology_core::png::decode_gray8;

fn fit(name: &str, px: &[f32], w: usize, h: usize) {
    let img = ImageView::new(w, h, px).unwrap();
    match fit_lattice(&img, &LatticeParams::default()) {
        Ok(f) => println!("{name:<34} {:.5} px/sq = {:.4} um/px  pitch x {:.5} y {:.5}  tilt {:.2}  rms {:.3}/{:.3}", f.sigma_max, f.um_per_px(0.6), f.vertical.pitch, f.horizontal.pitch, f.tilt_deg, f.vertical.rms, f.horizontal.rms),
        Err(e) => println!("{name}: {e:?}"),
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("reticle png");
    let (w, h, px) = decode_gray8(&std::fs::read(path).unwrap()).unwrap();
    let base: Vec<f32> = px.iter().map(|&v| f32::from(v)).collect();
    fit("saved PNG as is", &base, w, h);
    for seed0 in 1..=4u64 {
        let mut seed = seed0 * 7919;
        let d: Vec<f32> = base
            .iter()
            .map(|v| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                v + ((seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5)
            })
            .collect();
        fit(&format!("PNG + uniform dither (seed {seed0})"), &d, w, h);
    }
    for q in [2.0f32, 4.0] {
        let c: Vec<f32> = base.iter().map(|v| (v / q).round() * q).collect();
        fit(&format!("PNG re-quantised to steps of {q}"), &c, w, h);
    }
}
