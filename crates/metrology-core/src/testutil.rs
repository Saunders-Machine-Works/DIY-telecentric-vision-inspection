//! Synthetic checkerboard generator shared by the calibration tests.

use crate::profile::gaussian_smooth;

/// Render a checkerboard whose squares are mapped by M = [a1 a2] (pixels per square),
/// 4x4 supersampled, Gaussian-blurred, with deterministic noise. Light outside the board.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_g(
    w: usize,
    h: usize,
    a1: (f64, f64),
    a2: (f64, f64),
    centre: (f64, f64),
    squares: (f64, f64),
    blur: f64,
    noise: f64,
    grow: f64,
    dist: f64,
) -> Vec<f32> {
    let det = a1.0 * a2.1 - a1.1 * a2.0;
    let inv = [[a2.1 / det, -a2.0 / det], [-a1.1 / det, a1.0 / det]];
    let mut img = vec![0.0f64; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let (px, py) = (
                        x as f64 + (sx as f64 + 0.5) / 4.0 - 0.5 - centre.0,
                        y as f64 + (sy as f64 + 0.5) / 4.0 - 0.5 - centre.1,
                    );
                    let (u, v) = (
                        inv[0][0] * px + inv[0][1] * py,
                        inv[1][0] * px + inv[1][1] * py,
                    );
                    let f = 1.0 + dist * (px * px + py * py) / (250.0 * 250.0);
                    let (u, v) = (u * f + squares.0 / 2.0, v * f + squares.1 / 2.0);
                    acc += if (0.0..squares.0).contains(&u) && (0.0..squares.1).contains(&v) {
                        // dark squares extend `grow` (in squares) beyond their edges along u,
                        // shifting dark->light and light->dark edges in opposite directions
                        let kv = v.floor() as i64;
                        let ku0 = u.floor() as i64;
                        let dark = (ku0 - 1..=ku0 + 1).any(|ku| {
                            (ku + kv) & 1 == 0
                                && u >= ku as f64 - grow
                                && u < ku as f64 + 1.0 + grow
                        });
                        if dark {
                            25.0
                        } else {
                            195.0
                        }
                    } else {
                        210.0
                    };
                }
            }
            img[y * w + x] = acc / 16.0;
        }
    }
    // separable Gaussian blur
    for row in img.chunks_mut(w) {
        let s = gaussian_smooth(row, blur);
        row.copy_from_slice(&s);
    }
    for x in 0..w {
        let col: Vec<f64> = (0..h).map(|y| img[y * w + x]).collect();
        for (y, v) in gaussian_smooth(&col, blur).into_iter().enumerate() {
            img[y * w + x] = v;
        }
    }
    let mut seed = 99u64;
    img.iter()
        .map(|v| {
            let mut n = 0.0;
            for _ in 0..12 {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                n += (seed >> 11) as f64 / (1u64 << 53) as f64;
            }
            (v + noise * (n - 6.0)).clamp(0.0, 255.0) as f32
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    w: usize,
    h: usize,
    a1: (f64, f64),
    a2: (f64, f64),
    centre: (f64, f64),
    squares: (f64, f64),
    blur: f64,
    noise: f64,
) -> Vec<f32> {
    render_g(w, h, a1, a2, centre, squares, blur, noise, 0.0, 0.0)
}

/// Basis for squares of `l` px, rotated by `rot` deg, foreshortened by `tilt` deg about
/// an axis at `axis` deg.
pub(crate) fn basis(l: f64, rot: f64, tilt: f64, axis: f64) -> ((f64, f64), (f64, f64)) {
    let (r, al) = (rot.to_radians(), axis.to_radians());
    let c = tilt.to_radians().cos();
    // F = R(al) diag(1, c) R(-al)
    let f = |x: f64, y: f64| {
        let (xr, yr) = (al.cos() * x + al.sin() * y, -al.sin() * x + al.cos() * y);
        let (xr, yr) = (xr, yr * c);
        (al.cos() * xr - al.sin() * yr, al.sin() * xr + al.cos() * yr)
    };
    (f(l * r.cos(), l * r.sin()), f(-l * r.sin(), l * r.cos()))
}
