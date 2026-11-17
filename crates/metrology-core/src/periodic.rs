//! Periodic edge runs: pick the regular train of alternating edges (a checkerboard row,
//! thread crests) out of everything a scan line found, and refuse harmonics.
//! PROJECT_BACKGROUND 5.2: a pitch can silently lock onto half or double the true one, so
//! the detected spacing is checked against the expected value and its 2x / 0.5x harmonics.

/// A regular run of edges: inclusive indices into the edge list and their mean spacing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Run {
    pub first: usize,
    pub last: usize,
    /// (position[last] - position[first]) / (last - first)
    pub pitch: f64,
}

impl Run {
    pub fn len(&self) -> usize {
        self.last - self.first + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PeriodicError {
    TooFewEdges,
    NoRegularRun,
    /// The typical edge spacing is about half or double the expected pitch.
    Harmonic {
        ratio: f64,
    },
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// Longest run of at least `min_edges` edges whose consecutive gaps are within
/// `tol` (fraction) of the reference pitch and whose polarity alternates.
///
/// `positions` must be ascending; `rising[i]` is the polarity of edge i. The reference is
/// `expected` if given, else the median gap. With `expected`, a median gap near 0.5x or 2x
/// of it is reported as a harmonic instead of being accepted.
pub fn longest_regular_run(
    positions: &[f64],
    rising: &[bool],
    expected: Option<f64>,
    tol: f64,
    min_edges: usize,
) -> Result<Run, PeriodicError> {
    let n = positions.len();
    if n < min_edges.max(2) || rising.len() != n {
        return Err(PeriodicError::TooFewEdges);
    }
    let mut gaps: Vec<f64> = positions.windows(2).map(|w| w[1] - w[0]).collect();
    let med = median(&mut gaps.clone());
    let reference = match expected {
        Some(e) if e > 0.0 => {
            let ratio = med / e;
            if (0.4..0.6).contains(&ratio) || (1.7..2.3).contains(&ratio) {
                return Err(PeriodicError::Harmonic { ratio });
            }
            e
        }
        _ => med,
    };
    gaps.clear();
    let ok = |i: usize| {
        let g = positions[i + 1] - positions[i];
        g >= (1.0 - tol) * reference && g <= (1.0 + tol) * reference && rising[i] != rising[i + 1]
    };
    let (mut best, mut start) = (None::<Run>, 0usize);
    for i in 0..n {
        let continues = i + 1 < n && ok(i);
        if !continues {
            let run = Run {
                first: start,
                last: i,
                pitch: if i > start {
                    (positions[i] - positions[start]) / (i - start) as f64
                } else {
                    0.0
                },
            };
            if run.len() >= min_edges && best.is_none_or(|b| run.len() > b.len()) {
                best = Some(run);
            }
            start = i + 1;
        }
    }
    best.ok_or(PeriodicError::NoRegularRun)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt(n: usize) -> Vec<bool> {
        (0..n).map(|i| i % 2 == 0).collect()
    }

    #[test]
    fn regular_train_is_one_run_with_exact_pitch() {
        let pos: Vec<f64> = (0..20).map(|i| 5.0 + 20.25 * i as f64).collect();
        let r = longest_regular_run(&pos, &alt(20), Some(20.0), 0.2, 8).unwrap();
        assert_eq!((r.first, r.last, r.len()), (0, 19, 20));
        assert!((r.pitch - 20.25).abs() < 1e-12);
    }

    #[test]
    fn picks_the_longest_run_and_excludes_irregular_edges_at_both_ends() {
        let mut pos = vec![1.0, 3.0]; // spurious edges before the train
        pos.extend((0..12).map(|i| 100.0 + 20.0 * i as f64));
        pos.push(500.0); // and one after
        let n = pos.len();
        let r = longest_regular_run(&pos, &alt(n), Some(20.0), 0.2, 8).unwrap();
        assert_eq!((r.first, r.last), (2, 13));
        assert!((r.pitch - 20.0).abs() < 1e-12);
    }

    #[test]
    fn half_pitch_and_double_pitch_are_harmonics_not_pitches() {
        let half: Vec<f64> = (0..30).map(|i| 10.0 * i as f64).collect();
        match longest_regular_run(&half, &alt(30), Some(20.0), 0.2, 8) {
            Err(PeriodicError::Harmonic { ratio }) => assert!((ratio - 0.5).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
        let double: Vec<f64> = (0..30).map(|i| 40.0 * i as f64).collect();
        assert!(matches!(
            longest_regular_run(&double, &alt(30), Some(20.0), 0.2, 8),
            Err(PeriodicError::Harmonic { .. })
        ));
    }

    #[test]
    fn without_an_expected_pitch_the_median_gap_is_the_reference() {
        let pos: Vec<f64> = (0..15).map(|i| 33.0 * i as f64).collect();
        let r = longest_regular_run(&pos, &alt(15), None, 0.1, 8).unwrap();
        assert!((r.pitch - 33.0).abs() < 1e-12);
    }

    #[test]
    fn repeated_polarity_breaks_a_run() {
        let pos: Vec<f64> = (0..12).map(|i| 20.0 * i as f64).collect();
        let mut pol = alt(12);
        pol[6] = pol[5]; // two rising edges in a row: not a checker/line train
        let r = longest_regular_run(&pos, &pol, Some(20.0), 0.2, 5).unwrap();
        assert_eq!(r.len(), 6);
        assert!(longest_regular_run(&pos, &pol, Some(20.0), 0.2, 8).is_err());
    }

    #[test]
    fn gaps_outside_tolerance_split_the_run() {
        let mut pos: Vec<f64> = (0..10).map(|i| 20.0 * i as f64).collect();
        pos.extend((0..6).map(|i| 330.0 + 20.0 * i as f64)); // gap of 150
        let r = longest_regular_run(&pos, &alt(16), Some(20.0), 0.2, 5).unwrap();
        assert_eq!((r.first, r.last), (0, 9));
    }

    #[test]
    fn too_few_edges_and_bad_input() {
        assert_eq!(
            longest_regular_run(&[1.0, 2.0], &alt(2), None, 0.2, 8),
            Err(PeriodicError::TooFewEdges)
        );
        assert_eq!(
            longest_regular_run(&[1.0, 2.0, 3.0], &alt(2), None, 0.2, 2),
            Err(PeriodicError::TooFewEdges)
        );
    }
}
