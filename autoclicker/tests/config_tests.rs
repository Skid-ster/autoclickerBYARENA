#[path = "../src/config.rs"]
mod config;

use config::{
    interval_parts_to_ms, ms_to_interval_parts, AtomicConfigSnapshot, ClickType, CoordinateMode,
    EngineConfig, InputMode, MouseButton,
};

#[test]
fn default_config_validates() {
    let config = EngineConfig::default();
    assert!(config.validate().is_ok());
    assert!((config.target_cps() - 10.0).abs() < 1e-9);
    assert!((config.effective_interval_sigma_ms() - 5.0).abs() < 1e-9);
}

#[test]
fn invalid_intervals_rejected() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 0.0;
    assert!(config.validate().is_err());
    config.base_interval_ms = -1.0;
    assert!(config.validate().is_err());
    config.base_interval_ms = f64::NAN;
    assert!(config.validate().is_err());
    config.base_interval_ms = f64::INFINITY;
    assert!(config.validate().is_err());
}

#[test]
fn negative_sigma_rejected() {
    let mut config = EngineConfig::default();
    config.sigma_ms = -0.1;
    assert!(config.validate().is_err());
    config.sigma_ms = 5.0;
    config.sigma_percent = -2.0;
    assert!(config.validate().is_err());
    config.sigma_percent = 2.0;
    config.hold_sigma_ms = -1.0;
    assert!(config.validate().is_err());
}

#[test]
fn inverted_clamps_rejected() {
    let mut config = EngineConfig::default();
    config.min_interval_ms = 200.0;
    config.max_interval_ms = 50.0;
    assert!(config.validate().is_err());

    config.min_interval_ms = 1.0;
    config.max_interval_ms = 1000.0;
    config.base_interval_ms = 5000.0;
    assert!(config.validate().is_err());

    config.base_interval_ms = 100.0;
    config.hold_min_ms = 200.0;
    config.hold_max_ms = 100.0;
    assert!(config.validate().is_err());
}

#[test]
fn hold_range_validation() {
    let mut config = EngineConfig::default();
    config.hold_min_ms = 10.0;
    config.hold_max_ms = 20.0;
    config.hold_mean_ms = 50.0;
    assert!(config.validate().is_err());
    config.hold_mean_ms = 15.0;
    assert!(config.validate().is_ok());
    config.hold_min_ms = -1.0;
    assert!(config.validate().is_err());
}

#[test]
fn background_requires_hwnd() {
    let mut config = EngineConfig::default();
    config.input_mode = InputMode::BackgroundHwnd;
    config.target_hwnd = 0;
    assert!(config.validate().is_err());
    config.target_hwnd = 0x1234;
    assert!(config.validate().is_ok());
}

#[test]
fn negative_client_coordinates_rejected() {
    let mut config = EngineConfig::default();
    config.client_x = -5;
    assert!(config.validate().is_err());
    config.client_x = 10;
    config.client_y = -1;
    assert!(config.validate().is_err());
}

#[test]
fn sub_millisecond_interval_accepted() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 0.25;
    config.min_interval_ms = 0.05;
    config.max_interval_ms = 5.0;
    config.sigma_ms = 0.02;
    assert!(config.validate().is_ok());
    assert!((config.target_cps() - 4000.0).abs() < 1e-9);
}

#[test]
fn atomic_snapshot_round_trip() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 12.5;
    config.sigma_ms = 1.25;
    config.coordinate_mode = CoordinateMode::WindowClient;
    config.fixed_x = -640;
    config.client_y = 250;
    config.max_clicks = 42;
    config.humanize_enabled = false;

    let snapshot = AtomicConfigSnapshot::default();
    snapshot.store(&config);
    let restored = snapshot.load();
    assert!((restored.base_interval_ms - 12.5).abs() < 1e-12);
    assert!((restored.sigma_ms - 1.25).abs() < 1e-12);
    assert_eq!(restored.coordinate_mode, CoordinateMode::WindowClient);
    assert_eq!(restored.fixed_x, -640);
    assert_eq!(restored.client_y, 250);
    assert_eq!(restored.max_clicks, 42);
    assert!(!restored.humanize_enabled);
}

#[test]
fn interval_parts_round_trip() {
    assert_eq!(interval_parts_to_ms(0, 0, 0, 100), 100);
    assert_eq!(interval_parts_to_ms(1, 2, 3, 4), 3_723_004);
    assert_eq!(ms_to_interval_parts(3_723_004), (1, 2, 3, 4));
    assert_eq!(ms_to_interval_parts(999), (0, 0, 0, 999));
    assert_eq!(ms_to_interval_parts(60_000), (0, 1, 0, 0));
    for total in [0u64, 1, 999, 60_001, 3_600_000, 86_399_999] {
        let (h, m, s, ms) = ms_to_interval_parts(total);
        assert_eq!(interval_parts_to_ms(h, m, s, ms), total);
    }
}

#[test]
fn random_offset_validation_rules() {
    let mut config = EngineConfig::default();
    config.random_offset_ms = 5.0;
    assert!(config.validate().is_ok());

    config.random_offset_ms = -1.0;
    assert!(config.validate().is_err());
    config.random_offset_ms = f64::NAN;
    assert!(config.validate().is_err());

    let mut config = EngineConfig::default();
    config.base_interval_ms = 100.0;
    config.min_interval_ms = 10.0;
    config.random_offset_ms = 95.0; // base - offset = 5 ms, below minimum
    assert!(config.validate().is_err());
    config.random_offset_ms = 90.0; // exactly the minimum is allowed
    assert!(config.validate().is_ok());
}

#[test]
fn click_type_and_mouse_button_survive_snapshot() {
    let mut config = EngineConfig::default();
    config.mouse_button = MouseButton::Middle;
    config.click_type = ClickType::Triple;
    config.random_offset_ms = 7.5;

    let snapshot = AtomicConfigSnapshot::default();
    snapshot.store(&config);
    let restored = snapshot.load();
    assert_eq!(restored.mouse_button, MouseButton::Middle);
    assert_eq!(restored.click_type, ClickType::Triple);
    assert_eq!(restored.click_type.burst_count(), 3);
    assert!((restored.random_offset_ms - 7.5).abs() < 1e-12);

    // Unknown discriminants degrade to safe defaults rather than UB.
    assert_eq!(ClickType::Single.burst_count(), 1);
    assert_eq!(MouseButton::Left.label(), "Left");
}

#[test]
fn defaults_match_4p1_baseline() {
    let config = EngineConfig::default();
    assert_eq!(config.mouse_button, MouseButton::Left);
    assert_eq!(config.click_type, ClickType::Single);
    assert_eq!(config.random_offset_ms, 0.0);
    assert!(!config.humanize_enabled);
    assert_eq!(config.max_clicks, 0);
    assert_eq!(config.coordinate_mode, CoordinateMode::CurrentCursor);
    assert_eq!(config.input_mode, InputMode::GlobalSendInput);
}
