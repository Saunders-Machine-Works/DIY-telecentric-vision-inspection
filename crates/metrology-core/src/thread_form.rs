//! 60-degree V-thread form relations (ASME B1.1 / ISO 68-1 basic profile).

/// 0.75 * sqrt(3) / 2: pitch-diameter offset below the major diameter, in units of pitch.
pub const PD_OFFSET_PER_PITCH: f64 = 0.649_519_052_838_329;

/// Pitch diameter *calculated* from the major diameter and pitch of a basic
/// 60-degree thread. This is a form calculation, not a flank measurement.
pub fn pitch_diameter_from_major(major: f64, pitch: f64) -> f64 {
    major - PD_OFFSET_PER_PITCH * pitch
}

/// Pitch in inches for a threads-per-inch count.
pub fn pitch_from_tpi(tpi: f64) -> f64 {
    1.0 / tpi
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1/2-13 UNC basic pitch diameter is 0.4500 in (the GO gage in view).
    #[test]
    fn half_inch_13_basic_pitch_diameter() {
        let pd = pitch_diameter_from_major(0.5, pitch_from_tpi(13.0));
        assert!((pd - 0.450_037).abs() < 1e-6, "pd = {pd}");
    }

    /// M10x1.5 basic pitch diameter is 9.026 mm.
    #[test]
    fn m10_coarse_basic_pitch_diameter() {
        let pd = pitch_diameter_from_major(10.0, 1.5);
        assert!((pd - 9.025_721).abs() < 1e-6, "pd = {pd}");
    }
}
