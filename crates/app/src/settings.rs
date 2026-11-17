//! The few settings worth keeping between runs, as plain `key=value` lines. Reading is lenient:
//! an unknown key, a malformed line or a number that is not finite is ignored and the current
//! value kept, so a damaged file can never put the app into a broken state.

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub cal_path: String,
    pub rows: (usize, usize),
    pub cols: (usize, usize),
    pub nominal_in: f64,
    pub plus_in: f64,
    pub minus_in: f64,
    pub guard_in: f64,
    pub thread_size: String,
    pub thread_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            cal_path: "captures/calibration.txt".into(),
            rows: (940, 1460),
            cols: (760, 1640),
            nominal_in: 0.5,
            plus_in: 0.0005,
            minus_in: 0.0005,
            guard_in: 0.0,
            thread_size: "1/2-13".into(),
            thread_mode: false,
        }
    }
}

impl Settings {
    pub fn to_text(&self) -> String {
        format!(
            "cal={}\nrows={} {}\ncols={} {}\nnominal={:?}\nplus={:?}\nminus={:?}\nguard={:?}\nsize={}\nthread_mode={}\n",
            self.cal_path,
            self.rows.0,
            self.rows.1,
            self.cols.0,
            self.cols.1,
            self.nominal_in,
            self.plus_in,
            self.minus_in,
            self.guard_in,
            self.thread_size,
            self.thread_mode
        )
    }

    /// The settings in `text` laid over the defaults.
    pub fn from_text(text: &str) -> Settings {
        let mut s = Settings::default();
        let pair = |v: &str| -> Option<(usize, usize)> {
            let mut it = v.split_whitespace().map(|x| x.parse::<usize>().ok());
            match (it.next().flatten(), it.next().flatten(), it.next()) {
                (Some(a), Some(b), None) if a < b => Some((a, b)),
                _ => None,
            }
        };
        let num = |v: &str| v.trim().parse::<f64>().ok().filter(|x| x.is_finite());
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "cal" if !v.trim().is_empty() => s.cal_path = v.trim().to_string(),
                "rows" => s.rows = pair(v).unwrap_or(s.rows),
                "cols" => s.cols = pair(v).unwrap_or(s.cols),
                "nominal" => s.nominal_in = num(v).unwrap_or(s.nominal_in),
                "plus" => s.plus_in = num(v).filter(|x| *x >= 0.0).unwrap_or(s.plus_in),
                "minus" => s.minus_in = num(v).filter(|x| *x >= 0.0).unwrap_or(s.minus_in),
                "guard" => s.guard_in = num(v).filter(|x| *x >= 0.0).unwrap_or(s.guard_in),
                "size" if !v.trim().is_empty() => s.thread_size = v.trim().to_string(),
                "thread_mode" => s.thread_mode = v.trim() == "true",
                _ => {}
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_field() {
        let s = Settings {
            cal_path: "C:\\cal files\\a b.txt".into(),
            rows: (1060, 1300),
            cols: (700, 1700),
            nominal_in: 0.4991,
            plus_in: 0.0003,
            minus_in: 0.0002,
            guard_in: 0.0001,
            thread_size: "#10-24".into(),
            thread_mode: true,
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
    }

    #[test]
    fn damaged_lines_keep_the_defaults() {
        let d = Settings::default();
        let text =
            "rows=500 100\ncols=a b\nnominal=NaN\nplus=-1\nguard=inf\nbogus\nunknown=1\ncal=\n";
        assert_eq!(Settings::from_text(text), d);
        assert_eq!(Settings::from_text(""), d);
    }

    #[test]
    fn good_lines_survive_neighbouring_bad_ones() {
        let s = Settings::from_text("nominal=0.25\nrows=1 2 3\nplus=0.001\nthread_mode=true\n");
        assert_eq!(s.nominal_in, 0.25);
        assert_eq!(s.plus_in, 0.001);
        assert_eq!(s.rows, Settings::default().rows);
        assert!(s.thread_mode);
    }
}
