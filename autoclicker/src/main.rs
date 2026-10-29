//! MacroClicker binary entry point.
//!
//! The platform-independent modules (`config`, `hotkey`, `humanize`,
//! `settings`, `telemetry`) compile on every platform so their integration
//! tests run anywhere; the Win32 worker and egui dashboard are compiled only
//! on Windows, where the required APIs exist.

// Some spec-required Win32 wrappers (for example ClientToScreen and the event
// wait constants) are part of the FFI surface even when the current UI does
// not call them; keep those deliberately complete.
#![allow(dead_code)]
// Release builds are GUI-subsystem binaries: launching the exe by any means
// (Explorer double-click, shortcut, `cargo run --release`) opens ONLY the
// application window, and Windows never allocates a console window next to
// it. Debug builds keep the console so development diagnostics stay visible
// in the terminal that launched them.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod coordinates;
mod hotkey;
mod humanize;
mod icon;
mod macro_rec;
mod schedule;
mod self_update;
mod settings;
mod telemetry;
mod update;
mod window_layout;

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
        // The application stores its own fit-independent window size in
        // settings.conf. Never let eframe persist a second, zoom-dependent
        // copy, even if its optional persistence feature is enabled later.
        persist_window: false,
        viewport: egui::ViewportBuilder::default()
            // Naturally resizable classic window: a resizable native frame
            // (WS_THICKFRAME | WS_MAXIMIZEBOX) is what gives edge/corner
            // drag, double-click maximize, Aero Snap, Win+arrow and Snap
            // Layouts, all with OS grips and cursors and no custom
            // hit-testing. The window opens at the size the user last
            // chose (sanitized through the tolerant settings parser).
            // This raw 560x456 value is only the pre-first-frame backstop
            // while egui zoom is still 1.0; the app immediately replaces
            // it with a fit-compensated live minimum whose physical size
            // is the classic footprint at the user's own percentage, so
            // the shell refuses undersized drags/snaps without letting a
            // transient auto-fit inflate that floor. Everything stays
            // visible without scrolling at every reachable size.
            .with_inner_size(app::OpAutoClickerApp::persisted_window_size())
            .with_min_inner_size(app::MAIN_WINDOW_SIZE)
            .with_resizable(true)
            .with_icon(icon)
            .with_title("MacroClicker"),
        ..Default::default()
    };
    eframe::run_native(
        "MacroClicker",
        options,
        Box::new(|_creation_context| Box::new(app::OpAutoClickerApp::new())),
    )
}

#[cfg(not(windows))]
fn main() {
    println!("MacroClicker is a Windows-only GUI application.");
    println!("Build and run it on Windows 10 21H2+ or Windows 11 x64:");
    println!("    cargo run --release");
    println!("The platform-independent modules stay verifiable here via: cargo test");
}
