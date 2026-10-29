#![allow(dead_code)]

#[path = "../src/hotkey.rs"]
mod hotkey;
#[path = "../src/window_layout.rs"]
mod window_layout;

use window_layout::{
    choose_layout, dialog_safe_zoom, fit_auto_zoom, initial_auto_zoom, MainLayout,
};

#[test]
fn tall_window_uses_a_larger_portrait_composition() {
    let size = (560.0, 840.0);
    let layout = choose_layout(size, 1.0, MainLayout::Classic, true);
    assert_eq!(layout, MainLayout::Portrait);
    let zoom = fit_auto_zoom(size, 1.0, layout);
    assert!((zoom - 1.40).abs() < 0.001, "{zoom}");
    assert_eq!(fit_auto_zoom(size, 1.0, MainLayout::Classic), 1.0);
    assert!(size.0 / zoom >= 400.0);
    assert!(size.1 / zoom >= 600.0);
}

#[test]
fn screenshot_shape_uses_narrow_controls_and_taller_windows_reflow_again() {
    let screenshot = (556.0, 1360.0);
    let narrow = choose_layout(screenshot, 1.0, MainLayout::Classic, true);
    assert_eq!(narrow, MainLayout::Narrow);
    let zoom = fit_auto_zoom(screenshot, 1.0, narrow);
    let old_portrait_zoom = fit_auto_zoom(screenshot, 1.0, MainLayout::Portrait);
    assert!((zoom - 1.94).abs() < 0.001, "{zoom}");
    assert!(zoom > old_portrait_zoom * 1.35);

    let very_tall = (560.0, 1800.0);
    let slender = choose_layout(very_tall, 1.0, narrow, true);
    assert_eq!(slender, MainLayout::Slender);
    assert!(fit_auto_zoom(very_tall, 1.0, slender) > fit_auto_zoom(very_tall, 1.0, narrow));
    let extreme = (560.0, 2160.0);
    assert_eq!(
        choose_layout(extreme, 1.0, slender, true),
        MainLayout::Slender
    );
    assert!(fit_auto_zoom(extreme, 1.0, slender) > 2.5);
}

#[test]
fn classic_wide_and_disabled_modes_stay_distinct() {
    assert_eq!(
        choose_layout((560.0, 456.0), 1.0, MainLayout::Classic, true),
        MainLayout::Classic
    );
    assert_eq!(
        choose_layout((1_440.0, 720.0), 1.0, MainLayout::Classic, true),
        MainLayout::Wide
    );
    assert_eq!(
        choose_layout((560.0, 840.0), 1.0, MainLayout::Wide, false),
        MainLayout::Classic
    );
    assert_eq!(initial_auto_zoom((560.0, 840.0), 1.0, false), 1.0);
}

#[test]
fn portrait_and_wide_hysteresis_prevent_boundary_flicker() {
    let portrait_band = (1_000.0, 1_176.0); // aspect 0.85
    assert_eq!(
        choose_layout(portrait_band, 1.0, MainLayout::Classic, true),
        MainLayout::Classic
    );
    assert_eq!(
        choose_layout(portrait_band, 1.0, MainLayout::Portrait, true),
        MainLayout::Portrait
    );
    assert_eq!(
        choose_layout((1_000.0, 1_050.0), 1.0, MainLayout::Portrait, true),
        MainLayout::Classic
    );

    let wide_band = (1_050.0, 700.0); // aspect 1.50
    assert_eq!(
        choose_layout(wide_band, 1.0, MainLayout::Classic, true),
        MainLayout::Classic
    );
    assert_eq!(
        choose_layout(wide_band, 1.0, MainLayout::Wide, true),
        MainLayout::Wide
    );
    assert_eq!(
        choose_layout((980.0, 700.0), 1.0, MainLayout::Wide, true),
        MainLayout::Classic
    );
}

#[test]
fn compact_portrait_hysteresis_prevents_threshold_flicker() {
    let narrow_band = (565.0, 1000.0); // aspect 0.565
    assert_eq!(
        choose_layout(narrow_band, 1.0, MainLayout::Portrait, true),
        MainLayout::Portrait
    );
    assert_eq!(
        choose_layout(narrow_band, 1.0, MainLayout::Narrow, true),
        MainLayout::Narrow
    );
    let slender_band = (566.0, 1500.0); // aspect 0.3773
    assert_eq!(
        choose_layout(slender_band, 1.0, MainLayout::Narrow, true),
        MainLayout::Narrow
    );
    assert_eq!(
        choose_layout(slender_band, 1.0, MainLayout::Slender, true),
        MainLayout::Slender
    );
    assert_eq!(
        choose_layout((560.0, 1400.0), 1.0, MainLayout::Slender, true),
        MainLayout::Narrow
    );
    assert_eq!(
        choose_layout((560.0, 900.0), 1.0, MainLayout::Narrow, true),
        MainLayout::Portrait
    );
}

#[test]
fn growing_and_shrinking_through_every_tall_layout_keeps_controls_fitting() {
    let mut current = MainLayout::Classic;
    let mut previous_zoom = 1.0;
    for height in 456..=2160 {
        let size = (560.0, height as f32);
        let chosen = choose_layout(size, 1.0, current, true);
        let zoom = fit_auto_zoom(size, 1.0, chosen);
        let canvas = chosen.canvas();
        assert!(size.0 / zoom + 0.001 >= canvas[0], "{height} {chosen:?}");
        assert!(size.1 / zoom + 0.001 >= canvas[1], "{height} {chosen:?}");
        assert!(
            zoom + 0.011 >= previous_zoom,
            "growth shrank at {height}: {previous_zoom} to {zoom}"
        );
        previous_zoom = zoom;
        current = chosen;
    }
    for height in (456..=2160).rev() {
        let size = (560.0, height as f32);
        let chosen = choose_layout(size, 1.0, current, true);
        let zoom = fit_auto_zoom(size, 1.0, chosen);
        let canvas = chosen.canvas();
        assert!(size.0 / zoom + 0.001 >= canvas[0], "{height} {chosen:?}");
        assert!(size.1 / zoom + 0.001 >= canvas[1], "{height} {chosen:?}");
        assert!(
            zoom <= previous_zoom + 0.021,
            "shrink grew at {height}: {previous_zoom} to {zoom}"
        );
        previous_zoom = zoom;
        current = chosen;
    }
    assert_eq!(current, MainLayout::Classic);
}

#[test]
fn dialog_zoom_never_expands_the_dashboard_and_restores_the_space_needed() {
    for size in [(556.0, 1360.0), (560.0, 2160.0), (1200.0, 2720.0)] {
        let layout = choose_layout(size, 1.0, MainLayout::Classic, true);
        let dashboard = fit_auto_zoom(size, 1.0, layout);
        let dialog = dialog_safe_zoom(size, dashboard);
        assert!(dialog <= dashboard);
        assert!(size.0 / dialog >= 420.0 - 0.001);
        assert!(size.1 / dialog >= 420.0 - 0.001);
    }
    assert_eq!(dialog_safe_zoom((560.0, 456.0), 1.0), 1.0);
    assert_eq!(dialog_safe_zoom((f32::NAN, 456.0), 1.5), 1.5);
}

#[test]
fn shrinking_a_portrait_immediately_returns_to_a_fitting_layout() {
    let size = (560.0, 600.0);
    let next = choose_layout(size, 1.0, MainLayout::Portrait, true);
    assert_eq!(next, MainLayout::Classic);
    assert_eq!(fit_auto_zoom(size, 1.0, next), 1.0);
    let floor = hotkey::window_floor_for_auto_zoom(1.29);
    let neutral_floor = hotkey::fit_independent_window_size(floor, 1.29);
    assert!((neutral_floor.0 - 560.0).abs() < 0.01);
    assert!((neutral_floor.1 - 456.0).abs() < 0.01);
}

#[test]
fn selected_canvas_fits_across_scales_and_window_shapes() {
    for user in [0.75, 1.0, 1.25, 1.5, 2.0] {
        for width in (560..=2240).step_by(47) {
            for height in (456..=1824).step_by(53) {
                let size = (width as f32 * user, height as f32 * user);
                for current in [
                    MainLayout::Classic,
                    MainLayout::Portrait,
                    MainLayout::Narrow,
                    MainLayout::Slender,
                    MainLayout::Wide,
                ] {
                    let chosen = choose_layout(size, user, current, true);
                    let total = user * fit_auto_zoom(size, user, chosen);
                    let canvas = chosen.canvas();
                    // The wide design scales against a 904-point canvas,
                    // but its compact controls are measured to fit down
                    // to 700 points (covered by the rendered UI test).
                    let minimum_width = if chosen == MainLayout::Wide {
                        700.0
                    } else {
                        canvas[0]
                    };
                    assert!(
                        size.0 / total + 0.001 >= minimum_width,
                        "{size:?} {user} {current:?} {chosen:?}"
                    );
                    assert!(
                        size.1 / total + 0.001 >= canvas[1],
                        "{size:?} {user} {current:?} {chosen:?}"
                    );
                    assert_eq!(
                        choose_layout(size, user, current, false),
                        MainLayout::Classic
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_input_and_zoom_cap_are_safe() {
    assert_eq!(
        fit_auto_zoom((f32::NAN, 840.0), 1.0, MainLayout::Portrait),
        1.0
    );
    assert_eq!(
        fit_auto_zoom((560.0, 840.0), 0.0, MainLayout::Portrait),
        1.0
    );
    assert_eq!(
        choose_layout((f32::INFINITY, 1.0), 1.0, MainLayout::Portrait, true),
        MainLayout::Portrait
    );
    assert!(fit_auto_zoom((10_000.0, 20_000.0), 1.0, MainLayout::Portrait) <= 2.80);
}
