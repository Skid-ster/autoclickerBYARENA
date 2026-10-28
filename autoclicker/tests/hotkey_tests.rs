#![allow(dead_code)]

#[path = "../src/hotkey.rs"]
mod hotkey;

use hotkey::{
    capturable_vks, is_reserved_panic_vk, is_supported_toggle_vk, sanitize_ui_scale,
    vk_display_name, AppPrefs, AppTheme, PANIC_VK, UI_SCALE_OPTIONS, VK_ESCAPE, VK_F6,
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
fn ui_scale_sanitizer_keeps_every_offered_option() {
    for percent in UI_SCALE_OPTIONS {
        assert_eq!(sanitize_ui_scale(percent), percent);
    }
    assert_eq!(sanitize_ui_scale(0), 75);
    assert_eq!(sanitize_ui_scale(74), 75);
    assert_eq!(sanitize_ui_scale(201), 200);
    assert_eq!(sanitize_ui_scale(u32::MAX), 200);
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
