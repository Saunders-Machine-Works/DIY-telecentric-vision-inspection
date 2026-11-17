//! Edge repeatability on the real camera: N untouched 8-frame means, the same line each
//! time, edges located along it. Usage: edge_probe [x0 y0 x1 y1] [bursts]
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::image::ImageView;
use metrology_core::profile::{locate_edges, Polarity, Profile};
use std::time::Instant;

fn main() {
    let a: Vec<f64> = std::env::args()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    let (p0, p1) = if a.len() >= 4 {
        ((a[0], a[1]), (a[2], a[3]))
    } else {
        ((700.0, 700.0), (700.0, 1000.0))
    };
    let bursts = a.get(4).map_or(8, |&n| (n as usize).max(2));
    let mut cam = PylonCamera::open(0).expect("open camera");
    let mut positions: Vec<Vec<f64>> = Vec::new();
    for b in 0..bursts {
        let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
        let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
        let t = Instant::now();
        let prof = Profile::from_line(&img, p0, p1).expect("line");
        let t_sample = t.elapsed();
        let edges = locate_edges(&prof.values, 1.0, 10.0, Polarity::Either);
        let dt = t.elapsed();
        let _ = t_sample;
        let desc: Vec<String> = edges
            .iter()
            .map(|e| {
                format!(
                    "{:.3}{}({:.0})",
                    e.position,
                    if e.rising { "^" } else { "v" },
                    e.strength
                )
            })
            .collect();
        println!(
            "burst {b}: {} samples, {:?} to sample+locate, edges: {}",
            prof.values.len(),
            dt,
            desc.join(" ")
        );
        positions.push(edges.iter().map(|e| e.position).collect());
    }
    let n = positions[0].len();
    if n > 0 && positions.iter().all(|p| p.len() == n) {
        for k in 0..n {
            let v: Vec<f64> = positions.iter().map(|p| p[k]).collect();
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            let sd =
                (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt();
            let range = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - v.iter().cloned().fold(f64::INFINITY, f64::min);
            println!("edge {k}: mean {mean:.4} px  sd {sd:.4} px  range {range:.4} px  ({:.2} um sd at 30 um/px)", sd * 30.0);
        }
    } else {
        println!(
            "edge count differs between bursts: {:?}",
            positions.iter().map(Vec::len).collect::<Vec<_>>()
        );
    }
}
