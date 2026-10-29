#![allow(dead_code)]

#[path = "../src/hotkey.rs"]
mod hotkey;
#[path = "../src/window_layout.rs"]
mod window_layout;

use window_layout::{choose_layout, fit_auto_zoom, initial_auto_zoom, MainLayout};

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
fn portrait_canvas_fits_across_scales_and_window_shapes() {
    for user in [0.75, 1.0, 1.25, 1.5, 2.0] {
        for width in (560..=2240).step_by(47) {
            for height in (456..=1824).step_by(53) {
                let size = (width as f32 * user, height as f32 * user);
                for current in [MainLayout::Classic, MainLayout::Portrait, MainLayout::Wide] {
                    let chosen = choose_layout(size, user, current, true);
                    if chosen == MainLayout::Portrait {
                        let total = user * fit_auto_zoom(size, user, chosen);
                        let canvas = chosen.canvas();
                        assert!(
                            size.0 / total + 0.001 >= canvas[0],
                            "{size:?} {user} {current:?}"
                        );
                        assert!(
                            size.1 / total + 0.001 >= canvas[1],
                            "{size:?} {user} {current:?}"
                        );
                    }
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
