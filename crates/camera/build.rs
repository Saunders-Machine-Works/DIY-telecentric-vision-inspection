//! Links the Basler pylon runtime.
//! Windows: the PylonC library, from PYLON_DEV_DIR (set by the pylon installer) or the
//! default install path.
//! macOS: pylon ships only the C++ API, so shim/pylon_c_shim.cpp provides the PylonC
//! functions pylon.rs uses on top of it. It is compiled with the system clang++ against
//! pylon.framework (PYLON_FRAMEWORK_DIR, default /Library/Frameworks). Binaries find the
//! framework through the rpath set in .cargo/config.toml.
use std::path::PathBuf;
use std::process::Command;

fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => windows(),
        Ok("macos") => macos(),
        Ok(other) => println!("cargo:warning=no pylon binding for target OS {other}"),
        Err(_) => {}
    }
}

fn windows() {
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

fn macos() {
    const SHIM: &str = "shim/pylon_c_shim.cpp";
    println!("cargo:rerun-if-changed={SHIM}");
    println!("cargo:rerun-if-env-changed=PYLON_FRAMEWORK_DIR");
    let fw_dir = std::env::var("PYLON_FRAMEWORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Library/Frameworks"));
    let headers = fw_dir.join("pylon.framework").join("Headers");
    if !headers.exists() {
        panic!(
            "pylon.framework not found in {}: install pylon for macOS or set PYLON_FRAMEWORK_DIR",
            fw_dir.display()
        );
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => panic!("unsupported macOS arch {other:?}"),
    };
    let obj = out.join("pylon_c_shim.o");
    run(Command::new("xcrun")
        .args(["clang++", "-std=c++17", "-O2", "-fPIC", "-arch", arch])
        .arg(format!("-F{}", fw_dir.display()))
        .arg(format!("-I{}", headers.join("GenICam").display()))
        .args(["-Wno-deprecated-declarations", "-c", SHIM, "-o"])
        .arg(&obj));
    let lib = out.join("libpylon_c_shim.a");
    let _ = std::fs::remove_file(&lib);
    run(Command::new("xcrun")
        .args(["ar", "crs"])
        .arg(&lib)
        .arg(&obj));

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=pylon_c_shim");
    println!("cargo:rustc-link-search=framework={}", fw_dir.display());
    println!("cargo:rustc-link-lib=framework=pylon");
    println!("cargo:rustc-link-lib=dylib=c++");
}

fn run(cmd: &mut Command) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("could not run {cmd:?}: {e}"));
    assert!(status.success(), "{cmd:?} failed with {status}");
}
