use metrology_core::png::decode_gray8;
use metrology_core::profile::silhouette_edges;
fn main() {
    let (w, _h, px) = decode_gray8(&std::fs::read("captures/gage_vertical.png").unwrap()).unwrap();
    println!("row  left_x  right_x  width");
    for row in (1100..1236).step_by(4) {
        let v: Vec<f64> = px[row * w + 760..row * w + 1640]
            .iter()
            .map(|&p| f64::from(p))
            .collect();
        match silhouette_edges(&v) {
            Some((l, r)) => println!("{row}  {:7.2}  {:7.2}  {:7.2}", 760.0 + l, 760.0 + r, r - l),
            None => println!("{row}  none"),
        }
    }
}
