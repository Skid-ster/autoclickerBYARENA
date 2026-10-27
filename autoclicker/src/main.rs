//! OP Auto Clicker binary entry point.
//!
//! The platform-independent modules (`config`, `hotkey`, `humanize`,
//! `settings`, `telemetry`) compile on every platform so their integration
//! tests run anywhere; the Win32 worker and egui dashboard are compiled only
//! on Windows, where the required APIs exist.

// Some spec-required Win32 wrappers (for example ClientToScreen and the event
// wait constants) are part of the FFI surface even when the current UI does
// not call them; keep those deliberately complete.
#![allow(dead_code)]

mod config;
mod hotkey;
mod humanize;
mod icon;
mod settings;
mod telemetry;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod timing;
#[cfg(windows)]
mod win32;

#[cfg(windows)]
fn main() -> eframe::Result<()> {
    let (rgba, icon_width, icon_height) = icon::app_icon_rgba();
    let icon = std::sync::Arc::new(egui::IconData {
        rgba,
        width: icon_width,
        height: icon_height,
    });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([470.0, 700.0])
            .with_min_inner_size([420.0, 600.0])
            .with_icon(icon)
            .with_title("OP Auto Clicker"),
        ..Default::default()
    };
    eframe::run_native(
        "OP Auto Clicker",
        options,
        Box::new(|_creation_context| Box::new(app::OpAutoClickerApp::new())),
    )
}

#[cfg(not(windows))]
fn main() {
    println!("OP Auto Clicker is a Windows-only GUI application.");
    println!("Build and run it on Windows 10 21H2+ or Windows 11 x64:");
    println!("    cargo run --release");
    println!("The platform-independent modules stay verifiable here via: cargo test");
}
