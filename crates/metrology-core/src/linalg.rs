//! Small dense linear algebra for the calibration fits (normal equations, partial pivoting).

/// Solve the n x n system `a x = b` (normal equations) by Gaussian elimination with
/// partial pivoting. `None` if singular.
pub(crate) fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let pivot_row = a[col].clone();
        for r in col + 1..n {
            let f = a[r][col] / pivot_row[col];
            for (v, pv) in a[r].iter_mut().zip(&pivot_row).skip(col) {
                *v -= f * pv;
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

/// Linear least squares: minimise sum (y - row . x)^2 over the given rows.
pub(crate) fn least_squares(rows: &[Vec<f64>], y: &[f64]) -> Option<Vec<f64>> {
    let n = rows.first()?.len();
    let (mut a, mut b) = (vec![vec![0.0; n]; n], vec![0.0; n]);
    for (row, &yv) in rows.iter().zip(y) {
        for r in 0..n {
            for k in 0..n {
                a[r][k] += row[r] * row[k];
            }
            b[r] += row[r] * yv;
        }
    }
    solve(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_a_known_system_and_reports_singular_ones() {
        // 2x + y = 5, x + 3y = 10  =>  x = 1, y = 3
        let x = solve(vec![vec![2.0, 1.0], vec![1.0, 3.0]], vec![5.0, 10.0]).unwrap();
        assert!(
            (x[0] - 1.0).abs() < 1e-12 && (x[1] - 3.0).abs() < 1e-12,
            "{x:?}"
        );
        assert!(solve(vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![1.0, 2.0]).is_none());
    }

    /// Needs a row swap: the first pivot is zero.
    #[test]
    fn pivoting_handles_a_zero_leading_element() {
        let x = solve(vec![vec![0.0, 1.0], vec![1.0, 0.0]], vec![3.0, 4.0]).unwrap();
        assert!((x[0] - 4.0).abs() < 1e-12 && (x[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn least_squares_fits_a_noisy_plane_through_the_mean() {
        // y = 1 + 2a - 3b with +-0.01 alternating noise: coefficients within 1e-2
        let mut rows = Vec::new();
        let mut ys = Vec::new();
        for a in 0..10 {
            for b in 0..10 {
                let n = if (a + b) % 2 == 0 { 0.01 } else { -0.01 };
                rows.push(vec![1.0, a as f64, b as f64]);
                ys.push(1.0 + 2.0 * a as f64 - 3.0 * b as f64 + n);
            }
        }
        let c = least_squares(&rows, &ys).unwrap();
        assert!(
            (c[0] - 1.0).abs() < 1e-2 && (c[1] - 2.0).abs() < 1e-3 && (c[2] + 3.0).abs() < 1e-3,
            "{c:?}"
        );
        assert!(least_squares(&[], &[]).is_none());
    }
}
