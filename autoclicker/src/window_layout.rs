//! Window-shape selection and zoom arithmetic for the main dashboard.
//!
//! Dimensions are neutral points: the live egui size multiplied by the
//! zoom currently applied to it. This keeps resizing independent of the
//! zoom decision it drives and lets the same rules be tested without a
//! native window or a renderer.

use crate::hotkey::{WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH};

pub const CLASSIC_CANVAS: [f32; 2] = [WINDOW_MIN_WIDTH as f32, WINDOW_MIN_HEIGHT as f32];
pub const WIDE_CANVAS: [f32; 2] = [904.0, WINDOW_MIN_HEIGHT as f32];
/// Two interval rows and vertically stacked cards make a tall, narrow
/// window useful instead of leaving the original small dialog in a void.
pub const PORTRAIT_CANVAS: [f32; 2] = [400.0, 600.0];

const ZOOM_QUANTUM: f32 = 0.01;
const MAX_TOTAL_ZOOM: f32 = 2.80;
const WIDE_MIN_LIVE_WIDTH: f32 = 700.0;
const WIDE_ENGAGE_LIVE_WIDTH: f32 = 760.0;
const WIDE_ENGAGE_ASPECT: f32 = 1.58;
const WIDE_HOLD_ASPECT: f32 = 1.46;
const PORTRAIT_ENGAGE_ASPECT: f32 = 0.82;
const PORTRAIT_HOLD_ASPECT: f32 = 0.92;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainLayout {
    #[default]
    Classic,
    Portrait,
    Wide,
}

impl MainLayout {
    pub fn canvas(self) -> [f32; 2] {
        match self {
            Self::Classic => CLASSIC_CANVAS,
            Self::Portrait => PORTRAIT_CANVAS,
            Self::Wide => WIDE_CANVAS,
        }
    }
}

fn valid(size: (f32, f32), user_factor: f32) -> bool {
    size.0.is_finite()
        && size.1.is_finite()
        && size.0 > 0.0
        && size.1 > 0.0
        && user_factor.is_finite()
        && user_factor > 0.0
}

/// The additional window-driven factor, excluding the user's own scale.
/// Round down so the chosen canvas never exceeds the available window.
pub fn fit_auto_zoom(size: (f32, f32), user_factor: f32, layout: MainLayout) -> f32 {
    if !valid(size, user_factor) {
        return 1.0;
    }
    let canvas = layout.canvas();
    let fit_total = (size.0 / canvas[0]).min(size.1 / canvas[1]);
    let max_auto = (MAX_TOTAL_ZOOM / user_factor).max(1.0);
    let raw = (fit_total / user_factor).min(max_auto);
    if !raw.is_finite() || raw <= 1.0 {
        1.0
    } else {
        ((raw / ZOOM_QUANTUM).floor() * ZOOM_QUANTUM).max(1.0)
    }
}

/// Pick a shape-aware composition with separate arrival and hold bands.
/// A portrait layout is accepted only when its entire canvas fits at
/// the user's scale, including while a window is being pulled smaller.
pub fn choose_layout(
    size: (f32, f32),
    user_factor: f32,
    current: MainLayout,
    enabled: bool,
) -> MainLayout {
    if !enabled {
        return MainLayout::Classic;
    }
    if !valid(size, user_factor) {
        return current;
    }
    let aspect = size.0 / size.1;
    let portrait_zoom = user_factor * fit_auto_zoom(size, user_factor, MainLayout::Portrait);
    let portrait_fits = size.0 / portrait_zoom >= PORTRAIT_CANVAS[0]
        && size.1 / portrait_zoom >= PORTRAIT_CANVAS[1];
    let portrait_limit = if current == MainLayout::Portrait {
        PORTRAIT_HOLD_ASPECT
    } else {
        PORTRAIT_ENGAGE_ASPECT
    };
    if aspect <= portrait_limit && portrait_fits {
        return MainLayout::Portrait;
    }

    let wide_zoom = user_factor * fit_auto_zoom(size, user_factor, MainLayout::Wide);
    let live_width = size.0 / wide_zoom;
    let wide = if current == MainLayout::Wide {
        aspect >= WIDE_HOLD_ASPECT && live_width >= WIDE_MIN_LIVE_WIDTH
    } else {
        aspect >= WIDE_ENGAGE_ASPECT && live_width >= WIDE_ENGAGE_LIVE_WIDTH
    };
    if wide {
        MainLayout::Wide
    } else {
        MainLayout::Classic
    }
}

pub fn initial_auto_zoom(size: (f32, f32), user_factor: f32, enabled: bool) -> f32 {
    if enabled {
        let layout = choose_layout(size, user_factor, MainLayout::Classic, true);
        fit_auto_zoom(size, user_factor, layout)
    } else {
        1.0
    }
}
