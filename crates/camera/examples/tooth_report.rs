//! Inspect every tooth of a vertical threaded part on the live camera and print a defect report.
//! Usage: tooth_report <calibration.txt> <tpi> [y_lo] [y_hi] [x_lo] [x_hi] [limit=inches ...]
//! `limit` overrides a class 2A limit from the Unified catalogue: major_max, major_min, minor_max,
//! minor_min (e.g. `major_max=0.4980`).
//! The row window must cover the threaded section only (a slot that sticks out is not judged).
//! Exit code 0 = every tooth good, 1 = at least one defect, 2 = could not inspect.
use camera::pylon::PylonCamera;
use camera::{Camera, RetryPolicy};
use metrology_core::field::FieldMap;
use metrology_core::image::ImageView;
use metrology_core::profile::{silhouette_edges, Profile};
use metrology_core::thread_standard::{fit, nearest, unified_catalog, Fit, MM_PER_IN};
use metrology_core::thread_teeth::{inspect, Side, ThreadSpec};

type Pts = Vec<(f64, f64)>;

fn edges(
    img: &ImageView,
    map: &FieldMap,
    y: (usize, usize),
    x: (usize, usize),
) -> Option<(Pts, Pts)> {
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for row in y.0..y.1 {
        let (a, b) = silhouette_edges(&Profile::along_axis(img, true, row, x.0, x.1)?.values)?;
        l.push(map.to_mm(x.0 as f64 + a, row as f64)?);
        r.push(map.to_mm(x.0 as f64 + b, row as f64)?);
    }
    Some((l, r))
}

fn main() {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let (overrides, a): (Vec<String>, Vec<String>) = all.into_iter().partition(|s| s.contains('='));
    if a.len() < 2 {
        eprintln!("usage: tooth_report <calibration.txt> <tpi> [y_lo y_hi x_lo x_hi]");
        std::process::exit(2);
    }
    let map = FieldMap::from_text(&std::fs::read_to_string(&a[0]).expect("read calibration"))
        .expect("parse calibration");
    let tpi: f64 = a[1].parse().expect("tpi");
    let num = |i: usize, d: usize| a.get(i).map_or(d, |s| s.parse().expect("number"));
    let (y_lo, y_hi, x_lo, x_hi) = (num(2, 1060), num(3, 1300), num(4, 760), num(5, 1640));
    let mut cam = PylonCamera::open(0).expect("open camera");
    let m = cam.grab_mean(8, RetryPolicy::default()).expect("grab");
    let img = ImageView::new(m.width, m.height, &m.pixels).unwrap();
    let Some((l, r)) = edges(&img, &map, (y_lo, y_hi), (x_lo, x_hi)) else {
        println!("could not find the silhouette on every row of {y_lo}..{y_hi}");
        std::process::exit(2);
    };
    let spec = ThreadSpec::unified(tpi);
    let rep = match inspect(&l, &r, &spec) {
        Ok(rep) => rep,
        Err(e) => {
            println!("could not inspect: {e:?}");
            std::process::exit(2);
        }
    };
    println!(
        "{tpi} TPI (pitch {:.4} mm)  reference crest radius {:.4} mm = dia {:.5} in, root radius {:.4} mm = dia {:.5} in",
        spec.pitch_mm,
        rep.reference_crest_r_mm,
        2.0 * rep.reference_crest_r_mm / 25.4,
        rep.reference_root_r_mm,
        2.0 * rep.reference_root_r_mm / 25.4
    );
    let f = |v: Option<f64>| v.map_or("   -".to_string(), |x| format!("{x:5.1}"));
    println!("side  #   crest r   root r  width  offset  rise deg  fall deg  verdict");
    for t in &rep.teeth {
        let side = if t.side == Side::Left { "L" } else { "R" };
        let verdict = if t.defects.is_empty() {
            "ok".to_string()
        } else {
            format!("{:?}", t.defects)
        };
        println!(
            "{side:>3} {:>2}  {:8.3}  {:7.3}  {}  {}   {}     {}    {verdict}",
            t.index,
            t.crest_r_mm,
            t.root_r_mm,
            t.width_mm
                .map_or("  -  ".to_string(), |w| format!("{w:5.3}")),
            t.centre_offset_mm
                .map_or("  -  ".to_string(), |o| format!("{o:+5.3}")),
            f(t.rise_half_angle_deg),
            f(t.fall_half_angle_deg),
        );
    }
    let (crest_d, root_d) = (
        2.0 * rep.reference_crest_r_mm,
        2.0 * rep.reference_root_r_mm,
    );
    let mut out_of_limits = false;
    let catalog = unified_catalog();
    if let Some(found) = nearest(&catalog, tpi, crest_d) {
        let mut lim = found.clone();
        for o in &overrides {
            let (k, v) = o.split_once('=').expect("key=value");
            let v = v.parse::<f64>().expect("inches") * MM_PER_IN;
            match k {
                "major_max" => lim.major_max_mm = v,
                "major_min" => lim.major_min_mm = v,
                "minor_max" => lim.minor_max_mm = v,
                "minor_min" => lim.minor_min_mm = v,
                _ => panic!("unknown limit {k}"),
            }
        }
        let inch = |v: f64| v / MM_PER_IN;
        println!(
            "
class 2A limits for {} (editable):",
            lim.name
        );
        for (what, d, lo, hi) in [
            ("major (crest)", crest_d, lim.major_min_mm, lim.major_max_mm),
            ("minor (root) ", root_d, lim.minor_min_mm, lim.minor_max_mm),
        ] {
            let f = fit(d, lo, hi);
            out_of_limits |= f != Fit::Within;
            let verdict = match f {
                Fit::Within => "within",
                Fit::Below => "BELOW MIN",
                Fit::Above => "ABOVE MAX",
                Fit::Invalid => "LIMITS NOT VALID",
            };
            println!(
                "  {what} {:.4} in  limits {:.4} to {:.4}  {verdict}",
                inch(d),
                inch(lo),
                inch(hi)
            );
        }
    } else {
        println!(
            "
no catalogue thread with {tpi} TPI near {:.4} in",
            crest_d / MM_PER_IN
        );
    }
    let bad = rep.flagged().count();
    let note = if out_of_limits {
        ", diameter out of limits"
    } else {
        ""
    };
    let verdict = if bad == 0 && !out_of_limits {
        "THREAD OK"
    } else {
        "DEFECTIVE THREAD"
    };
    println!(
        "
{bad} of {} teeth flagged{note}: {verdict}",
        rep.teeth.len()
    );
    std::process::exit(i32::from(bad > 0 || out_of_limits));
}
