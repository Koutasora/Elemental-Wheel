//! Embeds the application icon (resource 1, which `tray.rs` and Explorer use) and the version info.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/elemental-wheel.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/elemental-wheel.ico")
        .set("ProductName", "Elemental Wheel")
        .set("FileDescription", "Elemental Wheel")
        .set("ProductVersion", &version)
        .set("FileVersion", &version)
        .set("OriginalFilename", "wheel.exe")
        .set(
            "LegalCopyright",
            "Copyright (c) 2026 Elemental Wheel contributors. MIT License.",
        );
    res.compile().expect("failed to embed Windows resources");
}
