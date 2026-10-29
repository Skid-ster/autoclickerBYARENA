//! Window-shape selection and zoom arithmetic for the main dashboard.
//!
//! Dimensions are neutral points: the live egui size multiplied by the
//! zoom currently applied to it. This keeps resizing independent of the
//! zoom decision it drives and lets the same rules be tested without a
//! native window or a renderer.

use crate::hotkey::{WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH};

pub const CLASSIC_CANVAS: [f32; 2] = [WINDOW_MIN_WIDTH as f32, WINDOW_MIN_HEIGHT as f32];
pub const WIDE_CANVAS: [f32; 2] = [904.0, WINDOW_MIN_HEIGHT as f32];
/// Progressively narrower compositions give height-only resizing more
/// room to enlarge controls without making them overflow the window.
pub const PORTRAIT_CANVAS: [f32; 2] = [400.0, 600.0];
pub const NARROW_CANVAS: [f32; 2] = [280.0, 700.0];
pub const SLENDER_CANVAS: [f32; 2] = [220.0, 740.0];

const ZOOM_QUANTUM: f32 = 0.01;
const MAX_TOTAL_ZOOM: f32 = 2.80;
const WIDE_MIN_LIVE_WIDTH: f32 = 700.0;
const WIDE_ENGAGE_LIVE_WIDTH: f32 = 760.0;
const WIDE_ENGAGE_ASPECT: f32 = 1.58;
const WIDE_HOLD_ASPECT: f32 = 1.46;
const PORTRAIT_ENGAGE_ASPECT: f32 = 0.82;
const PORTRAIT_HOLD_ASPECT: f32 = 0.92;
const NARROW_ENGAGE_ASPECT: f32 = 0.56;
const NARROW_HOLD_ASPECT: f32 = 0.57;
const SLENDER_ENGAGE_ASPECT: f32 = 0.375;
const SLENDER_HOLD_ASPECT: f32 = 0.38;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainLayout {
    #[default]
    Classic,
    Portrait,
    Narrow,
    Slender,
    Wide,
}

impl MainLayout {
    pub fn canvas(self) -> [f32; 2] {
        match self {
            Self::Classic => CLASSIC_CANVAS,
            Self::Portrait => PORTRAIT_CANVAS,
            Self::Narrow => NARROW_CANVAS,
            Self::Slender => SLENDER_CANVAS,
            Self::Wide => WIDE_CANVAS,
        }
    }

    pub fn is_portrait(self) -> bool {
        matches!(self, Self::Portrait | Self::Narrow | Self::Slender)
    }

    pub fn is_compact_portrait(self) -> bool {
        matches!(self, Self::Narrow | Self::Slender)
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
    let fits = |layout: MainLayout| {
        let zoom = user_factor * fit_auto_zoom(size, user_factor, layout);
        let canvas = layout.canvas();
        size.0 / zoom >= canvas[0] && size.1 / zoom >= canvas[1]
    };
    let slender_limit = if current == MainLayout::Slender {
        SLENDER_HOLD_ASPECT
    } else {
        SLENDER_ENGAGE_ASPECT
    };
    if aspect <= slender_limit && fits(MainLayout::Slender) {
        return MainLayout::Slender;
    }
    let narrow_limit = if current == MainLayout::Narrow {
        NARROW_HOLD_ASPECT
    } else {
        NARROW_ENGAGE_ASPECT
    };
    if aspect <= narrow_limit && fits(MainLayout::Narrow) {
        return MainLayout::Narrow;
    }
    let portrait_limit = if current == MainLayout::Portrait {
        PORTRAIT_HOLD_ASPECT
    } else {
        PORTRAIT_ENGAGE_ASPECT
    };
    if aspect <= portrait_limit && fits(MainLayout::Portrait) {
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

/// Temporarily fit the existing secondary windows within a narrow main
/// window. `size` is the zoom-independent viewport, and `desired_zoom`
/// is the dashboard's total zoom. The dashboard returns to that zoom as
/// soon as the dialog closes; no window dimensions or preferences change.
pub fn dialog_safe_zoom(size: (f32, f32), desired_zoom: f32) -> f32 {
    const DIALOG_VIEWPORT_WIDTH: f32 = 420.0;
    const DIALOG_VIEWPORT_HEIGHT: f32 = 420.0;
    if !valid(size, desired_zoom) {
        return desired_zoom;
    }
    desired_zoom
        .min(size.0 / DIALOG_VIEWPORT_WIDTH)
        .min(size.1 / DIALOG_VIEWPORT_HEIGHT)
}
