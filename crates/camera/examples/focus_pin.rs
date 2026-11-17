//! Live edge-sharpness meter for a vertical pin/shaft: adjust focus (move the part or the lens
//! along the optical axis) until the 10%-90% edge width is as small as it will go.
//! Usage: focus_pin [seconds] [y_lo] [y_hi] [x_lo] [x_hi]   (defaults: 120 s, rows 900..1500)
//! Each line: left/right 10-90% edge width in px (smaller = sharper) and the 50%-level width in px.
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};

fn crossing(seg: &[f64], lev: f64, falling: bool) -> Option<f64> {
    seg.windows(2).enumerate().find_map(|(i, w)| {
        let hit = if falling {
            w[0] >= lev && lev > w[1]
        } else {
            w[0] < lev && lev <= w[1]
        };
        hit.then(|| i as f64 + (w[0] - lev) / (w[0] - w[1]))
    })
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize, d: f64| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let secs = num(0, 120.0);
    let (y0, y1, x0, x1) = (
        num(1, 900.0) as usize,
        num(2, 1500.0) as usize,
        num(3, 800.0) as usize,
        num(4, 1600.0) as usize,
    );
    let mut cam = PylonCamera::open(0).expect("open camera");
    let t0 = std::time::Instant::now();
    println!("sharpest = smallest 10-90 widths. Ctrl+C to stop.");
    while t0.elapsed().as_secs_f64() < secs {
        let m = cam
            .grab_mean(2, camera::RetryPolicy::default())
            .expect("grab");
        let _ = RetryPolicy::default();
        let w = m.width;
        let mut avg = vec![0.0f64; x1 - x0];
        for y in y0..y1 {
            for (i, v) in avg.iter_mut().enumerate() {
                *v += m.pixels[y * w + x0 + i] as f64;
            }
        }
        avg.iter_mut().for_each(|v| *v /= (y1 - y0) as f64);
        let mut sorted = avg.clone();
        sorted.sort_by(|p, q| p.partial_cmp(q).unwrap());
        let bg = sorted[sorted.len() * 9 / 10];
        let mid = avg.len() / 2;
        let (left, right) = avg.split_at(mid);
        let edge = |seg: &[f64], falling: bool| {
            let lv = |f: f64| crossing(seg, bg * f, falling);
            Some((lv(0.9)? - lv(0.1)?).abs()).zip(lv(0.5))
        };
        match (edge(left, true), edge(right, false)) {
            (Some((wl, cl)), Some((wr, cr))) => {
                let width50 = (mid as f64 + cr) - cl;
                println!(
                    "edge 10-90 width: left {wl:5.1} px  right {wr:5.1} px   pin width at 50%: {width50:7.2} px",
                );
            }
            _ => println!("no pin edges found in the window"),
        }
    }
}
