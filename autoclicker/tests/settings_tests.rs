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
fn legacy_scale_values_and_auto_resize_preferences_survive_round_trip() {
    for percent in hotkey::UI_SCALE_OPTIONS {
        for enabled in [true, false] {
            let old_file = format!(
                "[ui]\ntheme=dark\nui_scale={percent}\nauto_scale={enabled}\n\
                 window_width=560\nwindow_height=1360\nunknown_future_key=ignored\n"
            );
            let (config, prefs) = parse_settings(&old_file);
            assert_eq!(prefs.ui_scale_percent, percent);
            assert_eq!(prefs.auto_scale, enabled);
            assert_eq!(prefs.theme, AppTheme::Dark);
            assert_eq!((prefs.window_width, prefs.window_height), (560, 1360));
            let new_file = serialize_settings(&config, &prefs);
            let (restored_config, restored_prefs) = parse_settings(&new_file);
            assert_eq!(restored_config, config);
            assert_eq!(restored_prefs, prefs);
        }
    }
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

#[test]
fn atomic_settings_writes_replace_complete_files_and_clean_the_temp() {
    let dir = std::env::temp_dir().join(format!(
        "macroclicker-settings-write-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("nested").join("settings.conf");
    settings::write_settings_atomic(&path, "first=complete\n").expect("first save");
    settings::write_settings_atomic(&path, "second=complete\n").expect("replacement");
    assert_eq!(
        std::fs::read_to_string(&path).expect("saved file"),
        "second=complete\n"
    );
    assert_eq!(
        std::fs::read_dir(path.parent().expect("parent"))
            .expect("dir")
            .count(),
        1
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failed_atomic_settings_write_does_not_destroy_the_old_file() {
    let dir =
        std::env::temp_dir().join(format!("macroclicker-settings-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.conf");
    // A directory at the destination makes the final rename fail on all
    // supported platforms, without relying on permissions or administrator status.
    std::fs::create_dir(&path).expect("destination directory");
    std::fs::write(path.join("keep"), b"known good").expect("marker");
    assert!(settings::write_settings_atomic(&path, "new settings").is_err());
    assert_eq!(
        std::fs::read(path.join("keep")).expect("marker retained"),
        b"known good"
    );
    assert_eq!(
        std::fs::read_dir(&dir).expect("dir").count(),
        1,
        "failed temp removed"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn concurrent_saves_never_expose_a_mixed_or_partial_document() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};
    let dir = std::env::temp_dir().join(format!(
        "macroclicker-settings-concurrent-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("directory");
    let path = Arc::new(dir.join("settings.conf"));
    let documents = Arc::new(
        (0..4)
            .map(|n| format!("writer={n}\n{}", (b'A' + n) as char).repeat(4096))
            .collect::<Vec<_>>(),
    );
    settings::write_settings_atomic(&path, &documents[0]).expect("baseline");
    let barrier = Arc::new(Barrier::new(5));
    let finished = Arc::new(AtomicUsize::new(0));
    let succeeded = Arc::new(AtomicUsize::new(0));
    let mut writers = Vec::new();
    for index in 0..4 {
        let (path, documents, barrier) = (
            Arc::clone(&path),
            Arc::clone(&documents),
            Arc::clone(&barrier),
        );
        let (finished, succeeded) = (Arc::clone(&finished), Arc::clone(&succeeded));
        writers.push(std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..12 {
                // An external sharing violation can fail a Windows rename;
                // that is reported/retried by the app, never a partial save.
                if settings::write_settings_atomic(&path, &documents[index]).is_ok() {
                    succeeded.fetch_add(1, Ordering::Relaxed);
                }
            }
            finished.fetch_add(1, Ordering::Release);
        }));
    }
    barrier.wait();
    let deadline = Instant::now() + Duration::from_secs(30);
    while finished.load(Ordering::Acquire) != 4 {
        assert!(Instant::now() < deadline, "settings writers did not finish");
        match std::fs::read_to_string(path.as_ref()) {
            Ok(document) => assert!(
                documents.contains(&document),
                "reader saw a partial or mixed settings file"
            ),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => panic!("settings disappeared or became unreadable: {error}"),
        }
        std::thread::yield_now();
    }
    for writer in writers {
        writer.join().expect("writer");
    }
    assert!(
        succeeded.load(Ordering::Relaxed) > 0,
        "all concurrent saves failed"
    );
    let final_document = std::fs::read_to_string(path.as_ref()).expect("final settings");
    assert!(documents.contains(&final_document));
    assert_eq!(
        std::fs::read_dir(&dir).expect("directory").count(),
        1,
        "temp files cleaned"
    );
    let _ = std::fs::remove_dir_all(dir);
}
