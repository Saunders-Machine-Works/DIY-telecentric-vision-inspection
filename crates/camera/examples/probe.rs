//! Bring-up probe: list cameras, open the first, print locked settings, time single
//! grabs and 8-frame bursts, and report burst-to-burst noise.
use camera::pylon::{self, PylonCamera};
use camera::{Camera, RetryPolicy};
use std::time::Instant;

fn main() {
    let devices = match pylon::enumerate() {
        Ok(d) => d,
        Err(e) => return eprintln!("enumerate failed: {e}"),
    };
    println!("{} device(s)", devices.len());
    for (i, d) in devices.iter().enumerate() {
        println!(
            "  [{i}] {} | {} | sn {} | {}",
            d.friendly_name, d.model, d.serial, d.device_class
        );
    }
    if devices.is_empty() {
        return;
    }
    let t = Instant::now();
    let mut cam = match PylonCamera::open(0) {
        Ok(c) => c,
        Err(e) => return eprintln!("open failed: {e}"),
    };
    println!("opened in {:?}, size {:?}", t.elapsed(), cam.size());
    for f in [
        "PixelFormat",
        "ExposureAuto",
        "GainAuto",
        "TriggerMode",
        "AcquisitionMode",
    ] {
        println!("  {f} = {:?}", cam.get_string(f));
    }
    println!(
        "  exposure_us = {:?}, gain_raw = {:?}",
        cam.exposure_us(),
        cam.gain_raw()
    );
    for f in [
        "Width",
        "Height",
        "OffsetX",
        "OffsetY",
        "WidthMax",
        "HeightMax",
        "SensorWidth",
        "SensorHeight",
        "BinningHorizontal",
        "BinningVertical",
        "DecimationHorizontal",
        "DecimationVertical",
    ] {
        println!("  {f} = {:?}", cam.get_int(f));
    }
    for f in ["ReverseX", "ReverseY", "CenterX", "CenterY"] {
        println!("  {f} = {:?}", cam.get_string(f));
    }
    for f in ["GevSCPSPacketSize", "GevSCPD"] {
        println!("  {f} = {:?}", cam.get_int(f));
    }

    for i in 0..3 {
        let t = Instant::now();
        match cam.grab() {
            Ok(f) => {
                let px = f.pixels();
                let mean = px.iter().map(|&p| u64::from(p)).sum::<u64>() as f64 / px.len() as f64;
                println!(
                    "single grab {i}: {:?}  min {} max {} mean {mean:.1}",
                    t.elapsed(),
                    px.iter().min().unwrap(),
                    px.iter().max().unwrap()
                );
            }
            Err(e) => println!("single grab {i} failed: {e}"),
        }
    }

    let policy = RetryPolicy::default();
    let mut bursts = Vec::new();
    for i in 0..3 {
        let t = Instant::now();
        match cam.grab_mean(8, policy) {
            Ok(m) => {
                println!("burst of {} frames {i}: {:?}", m.frames, t.elapsed());
                bursts.push(m);
            }
            Err(e) => println!("burst {i} failed: {e}"),
        }
    }
    if bursts.len() >= 2 {
        let (a, b) = (&bursts[0], &bursts[1]);
        let n = a.pixels.len() as f64;
        let rms = (a
            .pixels
            .iter()
            .zip(&b.pixels)
            .map(|(x, y)| f64::from(x - y).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        println!("RMS difference between two untouched 8-frame means: {rms:.3} gray levels (single-frame noise ~ {:.3} if independent)", rms / 2f64.sqrt() * 8f64.sqrt());
    }
}
