//! One CSV line per measurement, appended to `measurements.csv`, so results survive the next
//! measurement and can be opened in a spreadsheet.

use std::io::Write;

pub const HEADER: &str = "unix_seconds,mode,value,verdict,notes";

/// A CSV field, quoted if it holds a comma, a quote or a line break.
pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn row(unix_seconds: u64, mode: &str, value: &str, verdict: &str, notes: &str) -> String {
    [
        unix_seconds.to_string(),
        csv_field(mode),
        csv_field(value),
        csv_field(verdict),
        csv_field(notes),
    ]
    .join(",")
}

/// Append `line` to `path`, writing the header first if the file is new or empty.
pub fn append(path: &str, line: &str) -> std::io::Result<()> {
    let new = std::fs::metadata(path).map_or(true, |m| m.len() == 0);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if new {
        writeln!(f, "{HEADER}")?;
    }
    writeln!(f, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_with_commas_quotes_and_newlines_are_quoted() {
        assert_eq!(csv_field("plain 0.50095 in"), "plain 0.50095 in");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn a_row_has_the_five_columns_in_order() {
        let r = row(
            1_763_000_000,
            "diameter",
            "0.50095 in",
            "PASS",
            "light 0.4 %, 520 rows",
        );
        assert_eq!(
            r,
            "1763000000,diameter,0.50095 in,PASS,\"light 0.4 %, 520 rows\""
        );
        assert_eq!(HEADER.split(',').count(), 5);
    }

    #[test]
    fn append_writes_the_header_once_then_one_line_per_call() {
        let path = std::env::temp_dir().join(format!("tv_log_test_{}.csv", std::process::id()));
        let p = path.to_str().unwrap();
        let _ = std::fs::remove_file(p);
        append(p, "1,diameter,0.5 in,PASS,").unwrap();
        append(p, "2,thread,0.45 in,FAIL,\"3 of 6 teeth\"").unwrap();
        let text = std::fs::read_to_string(p).unwrap();
        std::fs::remove_file(p).unwrap();
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            vec![
                HEADER,
                "1,diameter,0.5 in,PASS,",
                "2,thread,0.45 in,FAIL,\"3 of 6 teeth\""
            ]
        );
    }
}
