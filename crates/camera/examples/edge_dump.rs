//! Dump the calibrated left/right silhouette of a vertical part, row by row, as CSV.
//! Usage: edge_dump <calibration.txt> <out.csv> [y_lo] [y_hi] [x_lo] [x_hi]
//! Columns: row, left_x_mm, left_y_mm, right_x_mm, right_y_mm (one untouched 8-frame mean).
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::field::FieldMap;
use metrology_core::image::ImageView;
use metrology_core::profile::{silhouette_edges, Profile};
use std::fmt::Write as _;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = FieldMap::from_text(&std::fs::read_to_string(&a[0]).expect("read calibration"))
        .expect("parse calibration");
    let num = |i: usize, d: usize| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let (y_lo, y_hi, x_lo, x_hi) = (num(2, 940), num(3, 1500), num(4, 760), num(5, 1640));
    let mut cam = PylonCamera::open(0).expect("open camera");
    let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
    let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
    let mut csv = String::from("row,lx,ly,rx,ry\n");
    let mut skipped = 0;
    for row in y_lo..y_hi {
        let pts = Profile::along_axis(&img, true, row, x_lo, x_hi)
            .and_then(|p| silhouette_edges(&p.values))
            .and_then(|(l, r)| {
                let f = row as f64;
                Some((
                    map.to_mm(x_lo as f64 + l, f)?,
                    map.to_mm(x_lo as f64 + r, f)?,
                ))
            });
        match pts {
            Some((l, r)) => {
                writeln!(csv, "{row},{:.4},{:.4},{:.4},{:.4}", l.0, l.1, r.0, r.1).unwrap()
            }
            None => skipped += 1,
        }
    }
    std::fs::write(&a[1], csv).expect("write csv");
    println!("wrote {} (skipped {skipped} rows)", a[1]);
}
