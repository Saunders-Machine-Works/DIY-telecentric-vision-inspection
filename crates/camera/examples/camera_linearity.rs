//! Read-only check that the camera path is linear: gamma, LUT, black level and digital shift must
//! be off or known, because any of them moves a 50 % edge crossing. Prints every feature it can
//! read and "n/a" for those the camera does not have. Changes nothing on the camera.
use camera::pylon::PylonCamera;

fn main() {
    let cam = PylonCamera::open(0).expect("open camera");
    for f in [
        "PixelFormat",
        "GammaEnable",
        "GammaSelector",
        "Gamma",
        "LUTEnable",
        "LUTSelector",
        "BlackLevel",
        "BlackLevelRaw",
        "BlackLevelSelector",
        "DigitalShift",
        "ExposureAuto",
        "GainAuto",
        "Gain",
        "GainRaw",
        "ExposureTime",
        "ExposureTimeAbs",
        "BslBrightness",
        "BslContrast",
        "BslSharpnessEnhancementEnable",
        "BslNoiseReduction",
        "SharpnessEnhancement",
        "NoiseReduction",
        "ShadingCorrectionEnable",
        "PgiMode",
        "DefectPixelCorrectionMode",
        "ReverseX",
        "ReverseY",
        "BinningHorizontal",
        "DecimationHorizontal",
    ] {
        let value = cam
            .get_string(f)
            .or_else(|_| cam.get_int(f).map(|v| v.to_string()))
            .or_else(|_| cam.get_float(f).map(|v| format!("{v}")))
            .unwrap_or_else(|_| "n/a".to_string());
        println!("{f:<32} {value}");
    }
}
