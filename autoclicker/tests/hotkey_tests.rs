#![allow(dead_code)]

#[path = "../src/hotkey.rs"]
mod hotkey;

use hotkey::{
    capturable_vks, fit_independent_window_size, is_reserved_panic_vk, is_supported_toggle_vk,
    sanitize_ui_scale, sanitize_window_size, vk_display_name, window_floor_for_auto_zoom, AppPrefs,
    AppTheme, AppearanceMode, AppearanceStyle, PANIC_VK, UI_SCALE_OPTIONS, VK_ESCAPE, VK_F6,
};

#[test]
fn default_prefs_use_f6() {
    let prefs = AppPrefs::default();
    assert_eq!(prefs.toggle_hotkey_vk, VK_F6);
    assert!(prefs.hotkeys_enabled);
    assert_eq!(
        prefs.theme,
        AppTheme::Classic,
        "fresh installs start with the classic light theme"
    );
    assert_eq!(prefs.ui_scale_percent, 100);
    assert_eq!(
        (prefs.window_width, prefs.window_height),
        (560, 456),
        "fresh installs open at the exact classic window size"
    );
}

#[test]
fn theme_keys_and_labels_are_stable_and_unique() {
    for theme in AppTheme::all() {
        assert_eq!(
            AppTheme::from_key(theme.key()),
            Some(theme),
            "{theme:?} must survive a save/load round trip"
        );
        assert_eq!(
            AppTheme::from_key(&format!(" {} ", theme.key())),
            Some(theme)
        );
        assert!(!theme.label().is_empty());
    }
    let keys: std::collections::HashSet<_> = AppTheme::all().iter().map(|t| t.key()).collect();
    assert_eq!(keys.len(), AppTheme::all().len(), "keys must be unique");
    assert_eq!(AppTheme::Oled.label(), "Midnight OLED");
    assert_eq!(AppTheme::from_key("comic_sans"), None);
    // Retired experimental themes must no longer parse: a persisted file
    // that still names them falls back to the default theme instead.
    assert_eq!(AppTheme::from_key("mog"), None);
    assert_eq!(AppTheme::from_key("vaporwave"), None);
}

#[test]
fn style_and_mode_form_four_distinct_appearances() {
    for (style, mode, expected, key) in [
        (
            AppearanceStyle::Classic,
            AppearanceMode::Light,
            AppTheme::Classic,
            "classic",
        ),
        (
            AppearanceStyle::Classic,
            AppearanceMode::Dark,
            AppTheme::Dark,
            "dark",
        ),
        (
            AppearanceStyle::Modern,
            AppearanceMode::Light,
            AppTheme::ModernLight,
            "modern_light",
        ),
        (
            AppearanceStyle::Modern,
            AppearanceMode::Dark,
            AppTheme::ModernDark,
            "modern_dark",
        ),
    ] {
        assert_eq!(AppTheme::from_appearance(style, mode), expected);
        assert_eq!(expected.style(), Some(style));
        assert_eq!(expected.mode(), mode);
        assert_eq!(expected.key(), key);
        assert_eq!(AppTheme::from_key(key), Some(expected));
    }
    for theme in AppTheme::previous() {
        assert_eq!(
            theme.style(),
            None,
            "a previous theme is not silently remapped"
        );
        assert_eq!(theme.mode(), AppearanceMode::Dark);
        assert_eq!(AppTheme::from_key(theme.key()), Some(theme));
    }
}

#[test]
fn ui_scale_sanitizer_keeps_every_offered_option() {
    for percent in UI_SCALE_OPTIONS {
        assert_eq!(sanitize_ui_scale(percent), percent);
    }
    assert_eq!(sanitize_ui_scale(0), 75);
    assert_eq!(sanitize_ui_scale(74), 75);
    assert_eq!(sanitize_ui_scale(201), 200);
    assert_eq!(sanitize_ui_scale(u32::MAX), 200);

    // The window-size sanitizer shares the same contract: the floor is
    // kept verbatim, in-range sizes round to whole points, out-of-range
    // sizes clamp to [floor, 4x floor] and non-finite values fall back.
    assert_eq!(sanitize_window_size(560.0, 456.0), (560, 456));
    assert_eq!(sanitize_window_size(720.6, 559.4), (721, 559));
    assert_eq!(sanitize_window_size(0.0, -10.0), (560, 456));
    assert_eq!(sanitize_window_size(99_999.0, 99_999.0), (2240, 1824));
    assert_eq!(sanitize_window_size(f64::NAN, f64::INFINITY), (560, 456));
}

#[test]
fn auto_zoom_does_not_inflate_the_physical_window_floor() {
    assert_eq!(window_floor_for_auto_zoom(1.0), (560.0, 456.0));
    assert_eq!(window_floor_for_auto_zoom(2.0), (280.0, 228.0));

    let (width, height) = window_floor_for_auto_zoom(2.8);
    assert!((width - 200.0).abs() < 0.001);
    assert!((height - (456.0 / 2.8)).abs() < 0.001);

    // Invalid or sub-unity factors can never make the declared floor
    // larger than the classic footprint.
    for factor in [0.0, -2.0, f32::NAN, f32::INFINITY] {
        assert_eq!(window_floor_for_auto_zoom(factor), (560.0, 456.0));
    }

    // The same physical window converts to the same fit-independent
    // size regardless of the transient auto-zoom used to present it:
    // egui-winit will multiply the live floor by user% x auto-zoom,
    // which reduces to classic floor x user% after this division.
    for auto_zoom in [1.0, 1.33, 2.0, 2.8] {
        let live_floor = window_floor_for_auto_zoom(auto_zoom);
        let fit_independent = fit_independent_window_size(live_floor, auto_zoom);
        assert!((fit_independent.0 - 560.0).abs() < 0.01);
        assert!((fit_independent.1 - 456.0).abs() < 0.01);
    }
    assert_eq!(
        fit_independent_window_size((280.0, 228.0), 2.0),
        (560.0, 456.0)
    );
}

#[test]
fn panic_and_escape_cannot_be_toggle_keys() {
    assert!(is_reserved_panic_vk(PANIC_VK));
    assert!(!is_supported_toggle_vk(PANIC_VK));
    assert!(!is_supported_toggle_vk(VK_ESCAPE));
}

#[test]
fn supported_ranges_cover_function_letter_digit_and_space_keys() {
    for vk in 0x70..=0x87 {
        // F1..F24, with F8 removed for panic.
        assert_eq!(is_supported_toggle_vk(vk), vk != PANIC_VK);
    }
    for vk in 0x30..=0x39 {
        assert!(is_supported_toggle_vk(vk), "digit {vk:#x}");
    }
    for vk in 0x41..=0x5A {
        assert!(is_supported_toggle_vk(vk), "letter {vk:#x}");
    }
    assert!(is_supported_toggle_vk(0x20));
    assert!(!is_supported_toggle_vk(0x00));
    assert!(!is_supported_toggle_vk(0x01));
}

#[test]
fn display_names_are_human_readable() {
    assert_eq!(vk_display_name(VK_F6), "F6");
    assert_eq!(vk_display_name(0x70), "F1");
    assert_eq!(vk_display_name(0x87), "F24");
    assert_eq!(vk_display_name(PANIC_VK), "F8");
    assert_eq!(vk_display_name(0x41), "A");
    assert_eq!(vk_display_name(0x5A), "Z");
    assert_eq!(vk_display_name(0x30), "0");
    assert_eq!(vk_display_name(0x39), "9");
    assert_eq!(vk_display_name(0x20), "Space");
    assert_eq!(vk_display_name(VK_ESCAPE), "Esc");
    assert!(vk_display_name(0x07).starts_with("VK "));
}

#[test]
fn capture_set_contains_reserved_key_so_dialog_can_reject_it() {
    assert!(capturable_vks().contains(&PANIC_VK));
    assert!(capturable_vks().contains(&VK_F6));
    assert!(capturable_vks().contains(&0x41));
}
