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
    let mut config = EngineConfig {
        base_interval_ms: 0.0,
        ..EngineConfig::default()
    };
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
    let mut config = EngineConfig {
        sigma_ms: -0.1,
        ..EngineConfig::default()
    };
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
    let mut config = EngineConfig {
        min_interval_ms: 200.0,
        max_interval_ms: 50.0,
        ..EngineConfig::default()
    };
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
    let mut config = EngineConfig {
        hold_min_ms: 10.0,
        hold_max_ms: 20.0,
        hold_mean_ms: 50.0,
        ..EngineConfig::default()
    };
    assert!(config.validate().is_err());
    config.hold_mean_ms = 15.0;
    assert!(config.validate().is_ok());
    config.hold_min_ms = -1.0;
    assert!(config.validate().is_err());
}

#[test]
fn background_requires_hwnd() {
    let mut config = EngineConfig {
        input_mode: InputMode::BackgroundHwnd,
        target_hwnd: 0,
        ..EngineConfig::default()
    };
    assert!(config.validate().is_err());
    config.target_hwnd = 0x1234;
    assert!(config.validate().is_ok());
}

#[test]
fn negative_client_coordinates_rejected() {
    let mut config = EngineConfig {
        client_x: -5,
        ..EngineConfig::default()
    };
    assert!(config.validate().is_err());
    config.client_x = 10;
    config.client_y = -1;
    assert!(config.validate().is_err());
}

#[test]
fn sub_millisecond_interval_accepted() {
    let config = EngineConfig {
        base_interval_ms: 0.25,
        min_interval_ms: 0.05,
        max_interval_ms: 5.0,
        sigma_ms: 0.02,
        ..EngineConfig::default()
    };
    assert!(config.validate().is_ok());
    assert!((config.target_cps() - 4000.0).abs() < 1e-9);
}

#[test]
fn atomic_snapshot_round_trip() {
    let config = EngineConfig {
        base_interval_ms: 12.5,
        sigma_ms: 1.25,
        coordinate_mode: CoordinateMode::WindowClient,
        fixed_x: -640,
        client_y: 250,
        max_clicks: 42,
        humanize_enabled: false,
        ..EngineConfig::default()
    };

    let snapshot = AtomicConfigSnapshot::default();
    snapshot.store(&config);
    let restored = snapshot.try_load().expect("no concurrent writer");
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
    let mut config = EngineConfig {
        random_offset_ms: 5.0,
        ..EngineConfig::default()
    };
    assert!(config.validate().is_ok());

    config.random_offset_ms = -1.0;
    assert!(config.validate().is_err());
    config.random_offset_ms = f64::NAN;
    assert!(config.validate().is_err());

    let mut config = EngineConfig {
        base_interval_ms: 100.0,
        min_interval_ms: 10.0,
        random_offset_ms: 95.0, // base - offset = 5 ms, below minimum
        ..EngineConfig::default()
    };
    assert!(config.validate().is_err());
    config.random_offset_ms = 90.0; // exactly the minimum is allowed
    assert!(config.validate().is_ok());
}

#[test]
fn click_type_and_mouse_button_survive_snapshot() {
    let config = EngineConfig {
        mouse_button: MouseButton::Middle,
        click_type: ClickType::Triple,
        random_offset_ms: 7.5,
        ..EngineConfig::default()
    };

    let snapshot = AtomicConfigSnapshot::default();
    snapshot.store(&config);
    let restored = snapshot.try_load().expect("no concurrent writer");
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

#[test]
fn dashboard_supports_long_intervals_and_the_full_offset_range() {
    let mut config = EngineConfig::default();
    let one_hour = interval_parts_to_ms(1, 0, 0, 0);
    config::commit_ui_interval(&mut config, one_hour, 10_000.0);
    assert_eq!(config.base_interval_ms, 3_600_000.0);
    assert_eq!(config.max_interval_ms, 3_610_000.0);
    assert!(config.validate().is_ok());

    let max = interval_parts_to_ms(99, 59, 59, 999);
    config::commit_ui_interval(&mut config, max, 10_000.0);
    assert_eq!(config.max_interval_ms, max as f64 + 10_000.0);
    assert!(config.validate().is_ok());

    config::commit_ui_interval(&mut config, 0, 0.0);
    assert_eq!(
        config.base_interval_ms, max as f64,
        "a zero editor must not overwrite a live session"
    );
    assert_eq!(config.random_offset_ms, 0.0);
}

#[test]
fn dashboard_still_rejects_offsets_that_would_go_below_the_minimum() {
    let mut config = EngineConfig::default();
    config::commit_ui_interval(&mut config, 20, 40.0);
    assert!(config.validate().is_err());
    config::commit_ui_interval(&mut config, 100, 40.0);
    assert!(config.validate().is_ok());
}

#[test]
fn concurrent_config_snapshots_are_complete_even_with_two_writers() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};
    let first = EngineConfig {
        base_interval_ms: 100.0,
        random_offset_ms: 40.0,
        fixed_x: 11,
        fixed_y: 11,
        ..EngineConfig::default()
    };
    let second = EngineConfig {
        base_interval_ms: 1000.0,
        random_offset_ms: 400.0,
        fixed_x: 222,
        fixed_y: 222,
        ..EngineConfig::default()
    };
    let snapshot = Arc::new(AtomicConfigSnapshot::default());
    snapshot.store(&first);
    let barrier = Arc::new(Barrier::new(3));
    let initial_writes = Arc::new(AtomicUsize::new(0));
    let proceed = Arc::new(AtomicBool::new(false));
    let writers_done = Arc::new(AtomicUsize::new(0));
    let mut writers = Vec::new();
    for config in [first.clone(), second.clone()] {
        let (snapshot, barrier) = (Arc::clone(&snapshot), Arc::clone(&barrier));
        let (initial_writes, proceed, writers_done) = (
            Arc::clone(&initial_writes),
            Arc::clone(&proceed),
            Arc::clone(&writers_done),
        );
        writers.push(std::thread::spawn(move || {
            barrier.wait();
            snapshot.store(&config);
            initial_writes.fetch_add(1, Ordering::Release);
            while !proceed.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            for _ in 0..20_000 {
                snapshot.store(&config);
            }
            writers_done.fetch_add(1, Ordering::Release);
        }));
    }
    barrier.wait();
    let deadline = Instant::now() + Duration::from_secs(10);
    while initial_writes.load(Ordering::Acquire) != 2 {
        if Instant::now() >= deadline {
            proceed.store(true, Ordering::Release);
            panic!("writers did not reach the publication checkpoint");
        }
        std::thread::yield_now();
    }
    let stable = snapshot.try_load();
    proceed.store(true, Ordering::Release);
    let stable = stable.expect("both writers paused");
    assert!(stable == first || stable == second);
    while writers_done.load(Ordering::Acquire) != 2 {
        assert!(Instant::now() < deadline, "writers failed to finish");
        if let Some(value) = snapshot.try_load() {
            assert!(
                value == first || value == second,
                "torn configuration: {value:?}"
            );
        }
        std::hint::spin_loop();
    }
    for writer in writers {
        writer.join().expect("writer thread");
    }
    let last = snapshot.try_load().expect("writers finished");
    assert!(last == first || last == second);
}
