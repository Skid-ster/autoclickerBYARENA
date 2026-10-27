#![allow(dead_code)]

#[path = "../src/config.rs"]
mod config;
#[path = "../src/hotkey.rs"]
mod hotkey;
#[path = "../src/settings.rs"]
mod settings;

use config::{ClickType, CoordinateMode, EngineConfig, InputMode, MouseButton};
use hotkey::{AppPrefs, VK_F7};
use settings::{parse_settings, serialize_settings};

#[test]
fn full_round_trip_preserves_every_field() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 250.5;
    config.random_offset_ms = 12.0;
    config.sigma_ms = 3.25;
    config.sigma_percent = 7.5;
    config.min_interval_ms = 2.0;
    config.max_interval_ms = 9_000.0;
    config.hold_mean_ms = 33.0;
    config.hold_sigma_ms = 4.0;
    config.hold_min_ms = 2.0;
    config.hold_max_ms = 400.0;
    config.input_mode = InputMode::BackgroundHwnd;
    config.coordinate_mode = CoordinateMode::WindowClient;
    config.fixed_x = -120;
    config.fixed_y = 340;
    config.client_x = 55;
    config.client_y = 77;
    config.target_hwnd = 0;
    config.background_synchronous = true;
    config.mouse_button = MouseButton::Right;
    config.click_type = ClickType::Triple;
    config.humanize_enabled = true;
    config.max_clicks = 1_234;

    let prefs = AppPrefs {
        toggle_hotkey_vk: VK_F7,
        hotkeys_enabled: false,
        random_offset_enabled: true,
        dark_mode: true,
    };

    let document = serialize_settings(&config, &prefs);
    let (restored_config, restored_prefs) = parse_settings(&document);
    assert_eq!(restored_config, config);
    assert_eq!(restored_prefs, prefs);
}

#[test]
fn unknown_keys_sections_and_comments_are_ignored() {
    let document = "\
# a comment
version=1

[future_section]
totally_new_key=whatever
another=123

[interval]
base_interval_ms=80
random_offset_ms=5
";
    let (config, prefs) = parse_settings(document);
    assert!((config.base_interval_ms - 80.0).abs() < 1e-12);
    assert!((config.random_offset_ms - 5.0).abs() < 1e-12);
    assert_eq!(prefs, AppPrefs::default());
}

#[test]
fn malformed_values_keep_field_defaults() {
    let defaults = EngineConfig::default();
    let document = "\
base_interval_ms=not_a_number
sigma_ms=
max_interval_ms=-9e999
click_type=7
mouse_button=sideways
humanize_enabled=maybe
toggle_hotkey_vk=abc
";
    let (config, prefs) = parse_settings(document);
    assert_eq!(config.base_interval_ms, defaults.base_interval_ms);
    assert_eq!(config.sigma_ms, defaults.sigma_ms);
    assert_eq!(config.max_interval_ms, defaults.max_interval_ms);
    assert_eq!(config.click_type, ClickType::Single);
    assert_eq!(config.mouse_button, MouseButton::Left);
    assert_eq!(config.humanize_enabled, defaults.humanize_enabled);
    assert_eq!(prefs, AppPrefs::default());
}

#[test]
fn click_type_and_boolean_text_forms_parse() {
    let document = "\
click_type=2
mouse_button=middle
input_mode=background
coordinate_mode=fixed
background_synchronous=on
humanize_enabled=off
hotkeys_enabled=no
toggle_hotkey_vk=116
random_offset_enabled=yes
";
    let (config, prefs) = parse_settings(document);
    assert_eq!(config.click_type, ClickType::Double);
    assert_eq!(config.mouse_button, MouseButton::Middle);
    assert_eq!(config.input_mode, InputMode::BackgroundHwnd);
    assert_eq!(config.coordinate_mode, CoordinateMode::FixedScreen);
    assert!(config.background_synchronous);
    assert!(!config.humanize_enabled);
    assert!(!prefs.hotkeys_enabled);
    assert_eq!(prefs.toggle_hotkey_vk, 116); // VK_F5
    assert!(prefs.random_offset_enabled);
}

#[test]
fn empty_document_yields_defaults() {
    let (config, prefs) = parse_settings("");
    assert_eq!(config, EngineConfig::default());
    assert_eq!(prefs, AppPrefs::default());
}

#[test]
fn dark_mode_round_trips_and_defaults_to_light() {
    // Files written before the theme existed have no dark_mode key: they
    // must keep loading with the classic light look.
    let (_, prefs) = parse_settings("toggle_hotkey_vk=117\n");
    assert!(!prefs.dark_mode);

    let mut wanted = AppPrefs::default();
    wanted.dark_mode = true;
    let document = serialize_settings(&EngineConfig::default(), &wanted);
    assert!(document.contains("dark_mode=true"));
    let (_, restored) = parse_settings(&document);
    assert!(restored.dark_mode);

    // Malformed values keep the default instead of poisoning the load.
    let (_, prefs) = parse_settings("dark_mode=maybe\n");
    assert!(!prefs.dark_mode);
}
