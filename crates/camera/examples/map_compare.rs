//! Compare two saved calibrations on the same pixel chords (where both are defined).
//! Usage: map_compare <old.txt> <new.txt>
//! Prints, for horizontal and vertical chords at several places, the length in mm under each map
//! and the ratio new/old (a uniform scale difference shows as a constant ratio).
use metrology_core::field::FieldMap;

fn load(p: &str) -> FieldMap {
    FieldMap::from_text(&std::fs::read_to_string(p).expect("read")).expect("parse")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (old, new) = (load(&a[0]), load(&a[1]));
    let len = |m: &FieldMap, p: (f64, f64), q: (f64, f64)| {
        let (u, v) = (m.to_mm(p.0, p.1)?, m.to_mm(q.0, q.1)?);
        Some((v.0 - u.0).hypot(v.1 - u.1))
    };
    println!(
        "{:>22} {:>10} {:>10} {:>9}",
        "chord (px)", "old mm", "new mm", "new/old"
    );
    type Px = (f64, f64);
    let mut chords: Vec<(Px, Px, &str)> = Vec::new();
    for y in [900.0, 1000.0, 1110.0, 1250.0, 1400.0] {
        chords.push(((957.0, y), (1379.0, y), "x 0.5009 pin"));
        chords.push(((1056.0, y), (1259.0, y), "x 6.12 pin"));
    }
    for x in [900.0, 1100.0, 1300.0, 1500.0] {
        chords.push(((x, 900.0), (x, 1400.0), "y 500 px"));
    }
    for (p, q, name) in chords {
        match (len(&old, p, q), len(&new, p, q)) {
            (Some(o), Some(n)) => println!(
                "{name:>12} {:>4.0},{:>4.0}->{:>4.0},{:>4.0} {o:10.4} {n:10.4} {:9.5}",
                p.0,
                p.1,
                q.0,
                q.1,
                n / o
            ),
            _ => println!(
                "{name:>12} {:>4.0},{:>4.0}: outside a calibrated region",
                p.0, p.1
            ),
        }
    }
}
