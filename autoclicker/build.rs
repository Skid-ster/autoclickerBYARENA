//! Build script: embeds the application icon and version metadata into
//! the Windows executable's resources, so Explorer, the taskbar and the
//! file-properties dialog show the real crosshair icon and product info
//! instead of the generic placeholder.
//!
//! `assets/macro_clicker.ico` is generated from the exact signed-
//! distance spec in `src/icon.rs` (rendered natively at 16-256 px), so
//! the file icon always matches the runtime window icon pixel for
//! pixel. Resource embedding is cosmetic: if the Windows SDK resource
//! compiler is unavailable on a build machine, the build warns and
//! continues rather than failing.

fn main() {
    println!("cargo:rerun-if-changed=assets/macro_clicker.ico");
    println!("cargo:rerun-if-changed=build.rs");

    let target_is_windows = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows";
    if !target_is_windows {
        return;
    }

    let version = packed_version();
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/macro_clicker.ico")
        .set("ProductName", "MacroClicker")
        .set(
            "FileDescription",
            "MacroClicker - OP Auto Clicker 4.1-style auto-clicker",
        )
        .set("InternalName", "macro_clicker")
        .set("OriginalFilename", "macro_clicker.exe")
        .set_version_info(winresource::VersionInfo::FILEVERSION, version)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, version);
    if let Err(error) = resource.compile() {
        println!("cargo:warning=failed to embed Windows resources: {error}");
    }
}

/// Packs the crate's semantic version into the 64-bit
/// (major << 48) | (minor << 32) | (patch << 16) layout PE version
/// info uses. Unparseable components degrade to zero instead of
/// breaking the build over cosmetic metadata.
fn packed_version() -> u64 {
    let component = |name: &str| -> u64 {
        std::env::var(name)
            .unwrap_or_default()
            .parse::<u64>()
            .unwrap_or_default()
    };
    (component("CARGO_PKG_VERSION_MAJOR") << 48)
        | (component("CARGO_PKG_VERSION_MINOR") << 32)
        | (component("CARGO_PKG_VERSION_PATCH") << 16)
}
