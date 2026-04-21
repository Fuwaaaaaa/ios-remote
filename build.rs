fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        panic!(
            "ios-remote supports Windows only (detected target_os = \"{target_os}\"). \
             Apple Mobile Device Service / iTunes is required at runtime and is not \
             available on macOS or Linux as a drop-in replacement for this code path."
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
}
