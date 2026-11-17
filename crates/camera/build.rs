//! Links the Basler pylon C runtime. Uses PYLON_DEV_DIR (set by the pylon installer)
//! and falls back to the default install path.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=PYLON_DEV_DIR");
    let dev = std::env::var("PYLON_DEV_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files\Basler\pylon 7\Development"));
    let lib = dev.join("lib").join("x64");
    if !lib.join("PylonC_v8_0.lib").exists() {
        println!(
            "cargo:warning=PylonC_v8_0.lib not found in {}; install the Basler pylon SDK or set PYLON_DEV_DIR",
            lib.display()
        );
    }
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=PylonC_v8_0");
}
