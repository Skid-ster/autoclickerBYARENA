//! OP Auto Clicker Engine B binary entry point.
//!
//! The platform-independent modules (`config`, `humanize`, `telemetry`) compile
//! on every platform so their integration tests run anywhere; the Win32 worker
//! and egui dashboard are compiled only on Windows, where the required APIs
//! exist.

// Some spec-required Win32 wrappers (for example ClientToScreen and the event
// wait constants) are part of the FFI surface even when the current UI does
// not call them; keep those deliberately complete.
#![allow(dead_code)]

mod config;
mod humanize;
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 940.0])
            .with_title("OP Auto Clicker - Engine B (Rust/egui)"),
        ..Default::default()
    };
    eframe::run_native(
        "OP Auto Clicker Engine B",
        options,
        Box::new(|_creation_context| Box::new(app::EngineBApp::new())),
    )
}

#[cfg(not(windows))]
fn main() {
    println!("autoclicker_engine_b is a Windows-only GUI application.");
    println!("Build and run it on Windows 10 21H2+ or Windows 11 x64:");
    println!("    cargo run --release");
    println!("The platform-independent modules stay verifiable here via: cargo test");
}
