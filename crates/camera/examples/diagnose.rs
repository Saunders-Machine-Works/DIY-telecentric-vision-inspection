//! Capture diagnostics: grab an 8-frame mean, save it as a lossless PNG, and report
//! backlight statistics (whole frame, central box, 4x4 block grid, optional ROI).
//! Usage: diagnose [out.png] [x0 y0 x1 y1]
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::illumination::{check, roi_stats, IllumLimits};
use metrology_core::image::{ImageView, Roi};
use metrology_core::png::encode_gray8;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .cloned()
        .unwrap_or_else(|| "diag_mean.png".into());
    let mut cam = PylonCamera::open(0).expect("open camera");
    let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
    let img = ImageView::new(m.width, m.height, &m.pixels).expect("image");
    println!(
        "exposure {:?} us, gain {:?}, {}x{}",
        cam.exposure_us(),
        cam.gain_raw(),
        m.width,
        m.height
    );

    let bytes: Vec<u8> = m
        .pixels
        .iter()
        .map(|&v| v.round().clamp(0.0, 255.0) as u8)
        .collect();
    std::fs::write(&out, encode_gray8(m.width, m.height, &bytes).unwrap()).unwrap();
    println!("wrote {out}");

    let limits = IllumLimits::default();
    let report = |name: &str, roi: Roi| {
        match roi_stats(&img, roi, Some(&m.peak)) {
        Ok(s) => println!(
            "{name:<14} p05 {:6.1}  median {:6.1}  p95 {:6.1}  sat {:6.3}%  uniformity {:.3}  issues {:?}",
            s.p05, s.median, s.p95, s.saturated_fraction * 100.0, s.uniformity, check(&s, &limits)
        ),
        Err(e) => println!("{name}: {e:?}"),
    }
    };
    let t = std::time::Instant::now();
    let _ = roi_stats(&img, Roi::new(0, 0, m.width, m.height), Some(&m.peak));
    println!("roi_stats over the whole 5 Mpx frame: {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let _ = encode_gray8(m.width, m.height, &bytes);
    println!("PNG encode of 5 Mpx: {:?}", t.elapsed());
    report("whole frame", Roi::new(0, 0, m.width, m.height));
    report("central 40%", Roi::centered(m.width, m.height, 0.4));
    if let [x0, y0, x1, y1] = args[1.min(args.len())..]
        .iter()
        .filter_map(|a| a.parse().ok())
        .collect::<Vec<usize>>()[..]
    {
        report("ROI", Roi::new(x0, y0, x1, y1));
    }
    println!("4x4 block medians (rows top to bottom):");
    let (bw, bh) = (m.width / 4, m.height / 4);
    for r in 0..4 {
        let row: Vec<String> = (0..4)
            .map(|c| {
                let roi = Roi::new(c * bw, r * bh, (c + 1) * bw, (r + 1) * bh);
                format!(
                    "{:6.1}",
                    roi_stats(&img, roi, Some(&m.peak)).unwrap().median
                )
            })
            .collect();
        println!("  {}", row.join(" "));
    }
}
