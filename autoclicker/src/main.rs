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
mod schedule;
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
            // Fixed-size classic window: every control fits without
            // scrolling. 560x448 logical points hugs the content with ~10 px
            // to spare below the footer row (content bottom measured
            // headlessly through the real egui 0.27.2 engine across all six
            // themes). The in-app Settings window can zoom the UI; the app
            // then re-applies this same logical size so the physical window
            // scales with the chosen percentage.
            .with_inner_size(app::MAIN_WINDOW_SIZE)
            .with_resizable(false)
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
