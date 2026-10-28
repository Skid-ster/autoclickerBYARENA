#![allow(dead_code)]

#[path = "../src/config.rs"]
mod config;
#[path = "../src/hotkey.rs"]
mod hotkey;
#[path = "../src/settings.rs"]
mod settings;

use config::{ClickType, CoordinateMode, EngineConfig, InputMode, MouseButton};
use hotkey::{AppPrefs, AppTheme, VK_F7};
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
        theme: AppTheme::Matrix,
        ui_scale_percent: 150,
        auto_scale: false,
        check_updates: false,
        window_width: 720,
        window_height: 560,
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
fn every_theme_round_trips_through_the_settings_file() {
    for theme in AppTheme::all() {
        let mut wanted = AppPrefs::default();
        wanted.theme = theme;
        let document = serialize_settings(&EngineConfig::default(), &wanted);
        assert!(
            document.contains(&format!("theme={}", theme.key())),
            "serialized document must name the {theme:?} theme"
        );
        let (_, restored) = parse_settings(&document);
        assert_eq!(restored.theme, theme);
    }

    // Unknown names keep the classic default instead of poisoning the load.
    let (_, prefs) = parse_settings(
        "theme=comic_sans
",
    );
    assert_eq!(prefs.theme, AppTheme::Classic);
}

#[test]
fn legacy_dark_mode_files_map_onto_the_theme() {
    // Files written by the dark-mode-only build carry a boolean; upgrades
    // must keep the user's look, and files with neither key stay classic.
    let (_, prefs) = parse_settings(
        "dark_mode=true
",
    );
    assert_eq!(prefs.theme, AppTheme::Dark);
    let (_, prefs) = parse_settings(
        "dark_mode=false
",
    );
    assert_eq!(prefs.theme, AppTheme::Classic);
    let (_, prefs) = parse_settings(
        "toggle_hotkey_vk=117
",
    );
    assert_eq!(prefs.theme, AppTheme::Classic);
    let (_, prefs) = parse_settings(
        "dark_mode=maybe
",
    );
    assert_eq!(prefs.theme, AppTheme::Classic);

    // A newer theme key wins over the legacy boolean regardless of order.
    let (_, prefs) = parse_settings(
        "theme=oled
dark_mode=false
",
    );
    assert_eq!(prefs.theme, AppTheme::Oled);

    // Keys from retired experimental themes are tolerated but ignored, so
    // an old file can never resurrect a theme that no longer ships.
    let (_, prefs) = parse_settings(
        "theme=mog
",
    );
    assert_eq!(prefs.theme, AppTheme::Classic);
}

#[test]
fn ui_scale_is_clamped_and_defaulted() {
    let (_, prefs) = parse_settings("");
    assert_eq!(prefs.ui_scale_percent, 100);
    let (_, prefs) = parse_settings(
        "ui_scale=150
",
    );
    assert_eq!(prefs.ui_scale_percent, 150);
    // Out-of-range values clamp to the supported window instead of
    // producing an unusable (giant or microscopic) window.
    let (_, prefs) = parse_settings(
        "ui_scale=10
",
    );
    assert_eq!(prefs.ui_scale_percent, 75);
    let (_, prefs) = parse_settings(
        "ui_scale=5000
",
    );
    assert_eq!(prefs.ui_scale_percent, 200);
    let (_, prefs) = parse_settings(
        "ui_scale=99999999999999999999
",
    );
    assert_eq!(prefs.ui_scale_percent, 100);
    // Malformed values keep the default.
    let (_, prefs) = parse_settings(
        "ui_scale=big
",
    );
    assert_eq!(prefs.ui_scale_percent, 100);
    let (_, prefs) = parse_settings(
        "ui_scale=-50
",
    );
    assert_eq!(prefs.ui_scale_percent, 100);
}

#[test]
fn window_size_is_clamped_and_defaulted() {
    // Missing keys keep the classic floor size.
    let (_, prefs) = parse_settings("");
    assert_eq!((prefs.window_width, prefs.window_height), (560, 456));

    // A normal user-chosen size round-trips exactly.
    let (_, prefs) = parse_settings("window_width=720\nwindow_height=560\n");
    assert_eq!((prefs.window_width, prefs.window_height), (720, 560));

    // Fractional points from a live resize round to the nearest point.
    let (_, prefs) = parse_settings("window_width=720.6\nwindow_height=559.4\n");
    assert_eq!((prefs.window_width, prefs.window_height), (721, 559));

    // Below-floor values clamp up: a restored window can never hide a
    // control. Above-ceiling values clamp to 4x the floor.
    let (_, prefs) = parse_settings("window_width=10\nwindow_height=0\n");
    assert_eq!((prefs.window_width, prefs.window_height), (560, 456));
    let (_, prefs) = parse_settings("window_width=99999\nwindow_height=1e30\n");
    assert_eq!((prefs.window_width, prefs.window_height), (2240, 1824));

    // Garbage, empty and non-finite values keep the floor for that
    // dimension only; the other dimension still applies.
    let (_, prefs) = parse_settings("window_width=wide\nwindow_height=600\n");
    assert_eq!((prefs.window_width, prefs.window_height), (560, 600));
    let (_, prefs) = parse_settings("window_width=\nwindow_height=\n");
    assert_eq!((prefs.window_width, prefs.window_height), (560, 456));
    let (_, prefs) = parse_settings("window_width=NaN\nwindow_height=inf\n");
    assert_eq!((prefs.window_width, prefs.window_height), (560, 456));
    let (_, prefs) = parse_settings("window_width=-800\nwindow_height=-1\n");
    assert_eq!((prefs.window_width, prefs.window_height), (560, 456));
}
