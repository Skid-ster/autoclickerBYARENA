//! Dependency-free settings persistence.
//!
//! The configuration is stored as a hand-written, line-oriented `key=value`
//! document next to the user's application data. The parser is deliberately
//! tolerant: unknown keys and malformed values are ignored and the built-in
//! defaults kept, so an older or hand-edited file can never stop the program
//! from starting. Floating point values use Rust's shortest round-trip
//! `Display` formatting, which parses back to the identical bit pattern.
//!
//! The module contains no platform APIs, which keeps the full parse and
//! serialization logic unit testable on every host.

use crate::config::{ClickType, CoordinateMode, EngineConfig, InputMode, MouseButton};
use crate::hotkey::AppPrefs;

const SETTINGS_VERSION: &str = "1";

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_f64(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn parse_i64(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok()
}

fn parse_u64(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
}

fn parse_isize(value: &str) -> Option<isize> {
    value.trim().parse::<isize>().ok()
}

fn parse_i32(value: &str) -> Option<i32> {
    value.trim().parse::<i32>().ok()
}

fn input_mode_from_text(value: &str) -> Option<InputMode> {
    match value.trim() {
        "global" => Some(InputMode::GlobalSendInput),
        "background" => Some(InputMode::BackgroundHwnd),
        _ => None,
    }
}

fn input_mode_text(mode: InputMode) -> &'static str {
    match mode {
        InputMode::GlobalSendInput => "global",
        InputMode::BackgroundHwnd => "background",
    }
}

fn coordinate_mode_from_text(value: &str) -> Option<CoordinateMode> {
    match value.trim() {
        "cursor" => Some(CoordinateMode::CurrentCursor),
        "fixed" => Some(CoordinateMode::FixedScreen),
        "client" => Some(CoordinateMode::WindowClient),
        _ => None,
    }
}

fn coordinate_mode_text(mode: CoordinateMode) -> &'static str {
    match mode {
        CoordinateMode::CurrentCursor => "cursor",
        CoordinateMode::FixedScreen => "fixed",
        CoordinateMode::WindowClient => "client",
    }
}

fn mouse_button_from_text(value: &str) -> Option<MouseButton> {
    match value.trim() {
        "left" => Some(MouseButton::Left),
        "right" => Some(MouseButton::Right),
        "middle" => Some(MouseButton::Middle),
        _ => None,
    }
}

fn mouse_button_text(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// Serializes the full runtime configuration and hotkey preferences.
pub fn serialize_settings(config: &EngineConfig, prefs: &AppPrefs) -> String {
    let lines = vec![
        format!("# OP Auto Clicker settings (version {SETTINGS_VERSION})"),
        "# Hand-editable: unknown keys and invalid values are ignored.".to_string(),
        format!("version={SETTINGS_VERSION}"),
        String::new(),
        "[interval]".to_string(),
        format!("base_interval_ms={}", config.base_interval_ms),
        format!("random_offset_ms={}", config.random_offset_ms),
        format!("sigma_ms={}", config.sigma_ms),
        format!("sigma_percent={}", config.sigma_percent),
        format!("min_interval_ms={}", config.min_interval_ms),
        format!("max_interval_ms={}", config.max_interval_ms),
        String::new(),
        "[hold]".to_string(),
        format!("hold_mean_ms={}", config.hold_mean_ms),
        format!("hold_sigma_ms={}", config.hold_sigma_ms),
        format!("hold_min_ms={}", config.hold_min_ms),
        format!("hold_max_ms={}", config.hold_max_ms),
        String::new(),
        "[delivery]".to_string(),
        format!("input_mode={}", input_mode_text(config.input_mode)),
        format!(
            "coordinate_mode={}",
            coordinate_mode_text(config.coordinate_mode)
        ),
        format!("fixed_x={}", config.fixed_x),
        format!("fixed_y={}", config.fixed_y),
        format!("client_x={}", config.client_x),
        format!("client_y={}", config.client_y),
        format!("target_hwnd={}", config.target_hwnd),
        format!("background_synchronous={}", config.background_synchronous),
        format!("mouse_button={}", mouse_button_text(config.mouse_button)),
        format!("click_type={}", config.click_type.burst_count()),
        String::new(),
        "[session]".to_string(),
        format!("humanize_enabled={}", config.humanize_enabled),
        format!("max_clicks={}", config.max_clicks),
        String::new(),
        "[hotkey]".to_string(),
        format!("toggle_hotkey_vk={}", prefs.toggle_hotkey_vk),
        format!("hotkeys_enabled={}", prefs.hotkeys_enabled),
    ];
    lines.join("\n") + "\n"
}

/// Parses a settings document over the built-in defaults. Sections, blank
/// lines, comments and unrecognised keys are skipped; a single bad value only
/// drops that one field.
pub fn parse_settings(text: &str) -> (EngineConfig, AppPrefs) {
    let mut config = EngineConfig::default();
    let mut prefs = AppPrefs::default();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        match key {
            "base_interval_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.base_interval_ms = parsed;
                }
            }
            "random_offset_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.random_offset_ms = parsed;
                }
            }
            "sigma_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.sigma_ms = parsed;
                }
            }
            "sigma_percent" => {
                if let Some(parsed) = parse_f64(value) {
                    config.sigma_percent = parsed;
                }
            }
            "min_interval_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.min_interval_ms = parsed;
                }
            }
            "max_interval_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.max_interval_ms = parsed;
                }
            }
            "hold_mean_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.hold_mean_ms = parsed;
                }
            }
            "hold_sigma_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.hold_sigma_ms = parsed;
                }
            }
            "hold_min_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.hold_min_ms = parsed;
                }
            }
            "hold_max_ms" => {
                if let Some(parsed) = parse_f64(value) {
                    config.hold_max_ms = parsed;
                }
            }
            "input_mode" => {
                if let Some(parsed) = input_mode_from_text(value) {
                    config.input_mode = parsed;
                }
            }
            "coordinate_mode" => {
                if let Some(parsed) = coordinate_mode_from_text(value) {
                    config.coordinate_mode = parsed;
                }
            }
            "fixed_x" => {
                if let Some(parsed) = parse_i64(value) {
                    config.fixed_x = parsed;
                }
            }
            "fixed_y" => {
                if let Some(parsed) = parse_i64(value) {
                    config.fixed_y = parsed;
                }
            }
            "client_x" => {
                if let Some(parsed) = parse_i64(value) {
                    config.client_x = parsed;
                }
            }
            "client_y" => {
                if let Some(parsed) = parse_i64(value) {
                    config.client_y = parsed;
                }
            }
            "target_hwnd" => {
                if let Some(parsed) = parse_isize(value) {
                    config.target_hwnd = parsed;
                }
            }
            "background_synchronous" => {
                if let Some(parsed) = parse_bool(value) {
                    config.background_synchronous = parsed;
                }
            }
            "mouse_button" => {
                if let Some(parsed) = mouse_button_from_text(value) {
                    config.mouse_button = parsed;
                }
            }
            "click_type" => {
                if let Some(parsed) = parse_u64(value) {
                    config.click_type = match parsed {
                        2 => ClickType::Double,
                        3 => ClickType::Triple,
                        _ => ClickType::Single,
                    };
                }
            }
            "humanize_enabled" => {
                if let Some(parsed) = parse_bool(value) {
                    config.humanize_enabled = parsed;
                }
            }
            "max_clicks" => {
                if let Some(parsed) = parse_u64(value) {
                    config.max_clicks = parsed;
                }
            }
            "toggle_hotkey_vk" => {
                if let Some(parsed) = parse_i32(value) {
                    prefs.toggle_hotkey_vk = parsed;
                }
            }
            "hotkeys_enabled" => {
                if let Some(parsed) = parse_bool(value) {
                    prefs.hotkeys_enabled = parsed;
                }
            }
            _ => {}
        }
    }

    (config, prefs)
}
