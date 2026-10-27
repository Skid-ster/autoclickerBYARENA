#![allow(dead_code)]

#[path = "../src/hotkey.rs"]
mod hotkey;

use hotkey::{
    capturable_vks, is_reserved_panic_vk, is_supported_toggle_vk, vk_display_name, AppPrefs,
    PANIC_VK, VK_ESCAPE, VK_F6,
};

#[test]
fn default_prefs_use_f6() {
    let prefs = AppPrefs::default();
    assert_eq!(prefs.toggle_hotkey_vk, VK_F6);
    assert!(prefs.hotkeys_enabled);
    assert!(
        !prefs.dark_mode,
        "fresh installs start with the light theme"
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
