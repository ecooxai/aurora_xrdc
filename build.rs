use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=tools/apple_capture.swift");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source = manifest.join("tools/apple_capture.swift");
    let output = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("apple_xrdc_capture");
    let status = Command::new("xcrun")
        .arg("swiftc")
        .arg("-O")
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .args([
            "-framework","ScreenCaptureKit",
            "-framework","VideoToolbox",
            "-framework","CoreMedia",
            "-framework","CoreVideo",
            "-framework","Foundation",
        ])
        .status()
        .expect("failed to invoke xcrun swiftc for Apple capture helper");
    if !status.success() {
        panic!("failed to build Apple ScreenCaptureKit helper");
    }
    println!("cargo:rustc-env=APPLE_XRDC_NATIVE_CAPTURE_BIN={}", output.display());
}
