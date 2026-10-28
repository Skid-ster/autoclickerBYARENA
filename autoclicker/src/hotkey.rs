//! Virtual-key constants and validation for the configurable toggle hotkey.
//!
//! The module is platform-independent: it only describes Windows virtual-key
//! codes as plain integers so the rules and name mapping stay unit testable
//! on any host. The Win32 `GetAsyncKeyState` polling itself lives in
//! [`crate::win32`]. F8 is permanently reserved for the panic stop and can
//! never be assigned to the toggle action.

/// F6: the OP Auto Clicker 4.1 default start/stop key.
pub const VK_F6: i32 = 0x75;
/// F7: a common alternative start/stop binding.
pub const VK_F7: i32 = 0x76;
/// F8: permanently reserved for the panic stop action.
pub const VK_F8: i32 = 0x77;
/// Escape: cancels hotkey capture in the assignment dialog.
pub const VK_ESCAPE: i32 = 0x1B;

/// Default start/stop hotkey.
pub const DEFAULT_TOGGLE_VK: i32 = VK_F6;
/// Immutable panic-stop hotkey.
pub const PANIC_VK: i32 = VK_F8;

const VK_F1: i32 = 0x70;
const VK_F24: i32 = 0x87;
const VK_0: i32 = 0x30;
const VK_9: i32 = 0x39;
const VK_A: i32 = 0x41;
const VK_Z: i32 = 0x5A;
const VK_SPACE: i32 = 0x20;

/// Visual theme applied to every window. Themes change colors only —
/// never layout metrics or label strings — so switching can never move
/// or hide a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTheme {
    /// The classic light OP Auto Clicker 4.x dialog (default).
    #[default]
    Classic,
    /// Windows 11 dark-dialog colors with surface-elevation tiers.
    Dark,
    /// Pure-black dark theme for OLED screens.
    Oled,
    /// Green-on-black terminal look.
    Matrix,
    /// Deep indigo modern look with a violet accent.
    Aurora,
}

impl AppTheme {
    pub fn all() -> [AppTheme; 5] {
        [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::Oled,
            AppTheme::Matrix,
            AppTheme::Aurora,
        ]
    }

    /// Stable key used in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            AppTheme::Classic => "classic",
            AppTheme::Dark => "dark",
            AppTheme::Oled => "oled",
            AppTheme::Matrix => "matrix",
            AppTheme::Aurora => "aurora",
        }
    }

    /// Parses a persisted theme key. Keys from retired experimental themes
    /// ("mog", "vaporwave") are unknown on purpose: the tolerant settings
    /// parser falls back to the default theme instead of failing.
    pub fn from_key(value: &str) -> Option<AppTheme> {
        match value.trim() {
            "classic" => Some(AppTheme::Classic),
            "dark" => Some(AppTheme::Dark),
            "oled" => Some(AppTheme::Oled),
            "matrix" => Some(AppTheme::Matrix),
            "aurora" => Some(AppTheme::Aurora),
            _ => None,
        }
    }

    /// Name shown in the Settings window.
    pub fn label(self) -> &'static str {
        match self {
            AppTheme::Classic => "Classic light",
            AppTheme::Dark => "Dark",
            AppTheme::Oled => "Midnight OLED",
            AppTheme::Matrix => "Terminal green",
            AppTheme::Aurora => "Aurora modern",
        }
    }
}

/// Window-scale percentages offered in the Settings window.
pub const UI_SCALE_OPTIONS: [u32; 6] = [75, 100, 125, 150, 175, 200];
/// Default window scale.
pub const UI_SCALE_DEFAULT: u32 = 100;
const UI_SCALE_MIN: u32 = 75;
const UI_SCALE_MAX: u32 = 200;

/// Clamps a persisted scale percentage into the supported range so a
/// hand-edited file can never produce an unusable window.
pub fn sanitize_ui_scale(percent: u32) -> u32 {
    percent.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

/// Minimum inner width of the main window, in logical (point) units: the
/// classic dialog footprint below which a control would have to shrink.
pub const WINDOW_MIN_WIDTH: u32 = 560;
/// Minimum inner height of the main window, in logical (point) units.
pub const WINDOW_MIN_HEIGHT: u32 = 456;
const WINDOW_SIZE_MAX_FACTOR: u32 = 4;

/// Sanitizes one persisted window dimension: non-finite values fall back
/// to the floor, finite values clamp into [floor, 4x floor] and round to
/// a whole point. A hand-edited or corrupt file can therefore never
/// produce an unusable (tiny, giant or non-finite) restored window, which
/// mirrors the `ui_scale` guarantee above.
pub fn sanitize_window_dimension(points: f64, floor: u32) -> u32 {
    if !points.is_finite() {
        return floor;
    }
    let ceiling = f64::from(floor * WINDOW_SIZE_MAX_FACTOR);
    points.clamp(f64::from(floor), ceiling).round() as u32
}

/// Sanitizes a persisted window size against the main-window floor.
pub fn sanitize_window_size(width: f64, height: f64) -> (u32, u32) {
    (
        sanitize_window_dimension(width, WINDOW_MIN_WIDTH),
        sanitize_window_dimension(height, WINDOW_MIN_HEIGHT),
    )
}

/// Application-level hotkey and UI preferences persisted between sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppPrefs {
    pub toggle_hotkey_vk: i32,
    pub hotkeys_enabled: bool,
    /// State of the 4.1 "Random offset" checkbox; the offset magnitude itself
    /// lives in [`crate::config::EngineConfig::random_offset_ms`].
    pub random_offset_enabled: bool,
    /// Color/label theme for every window. Classic light by default so a
    /// fresh install shows the dialog the 4.x look is modeled on.
    pub theme: AppTheme,
    /// Window scale percentage (75–200). The whole UI zooms and the
    /// window only grows if the content would otherwise no longer fit,
    /// so every control stays visible at every scale.
    pub ui_scale_percent: u32,
    /// Let the window size drive the presentation: as the window grows
    /// past fixed thresholds the whole UI steps up in scale, and past
    /// the wide threshold the group boxes rearrange into a three-across
    /// row with a single row of action buttons. Off restores the fixed
    /// classic composition at every size.
    pub auto_scale: bool,
    /// Last user-chosen inner window width, sanitized to
    /// [WINDOW_MIN_WIDTH, 4x WINDOW_MIN_WIDTH]. Measured in
    /// step-independent points: the point width the window has with
    /// the window-driven scale factor neutralized, so the same physical
    /// size round-trips no matter which scale step is active. The floor
    /// defaults mean a fresh install opens at the exact classic size.
    pub window_width: u32,
    /// Last user-chosen inner window height in the same
    /// step-independent points, sanitized to
    /// [WINDOW_MIN_HEIGHT, 4x WINDOW_MIN_HEIGHT].
    pub window_height: u32,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            toggle_hotkey_vk: DEFAULT_TOGGLE_VK,
            hotkeys_enabled: true,
            random_offset_enabled: false,
            theme: AppTheme::Classic,
            ui_scale_percent: UI_SCALE_DEFAULT,
            auto_scale: true,
            window_width: WINDOW_MIN_WIDTH,
            window_height: WINDOW_MIN_HEIGHT,
        }
    }
}

/// True when the code may be assigned as the start/stop hotkey.
pub fn is_supported_toggle_vk(vk: i32) -> bool {
    if vk == PANIC_VK || vk == VK_ESCAPE {
        return false;
    }
    (VK_F1..=VK_F24).contains(&vk)
        || (VK_0..=VK_9).contains(&vk)
        || (VK_A..=VK_Z).contains(&vk)
        || vk == VK_SPACE
}

/// True only for the reserved panic-stop key.
pub fn is_reserved_panic_vk(vk: i32) -> bool {
    vk == PANIC_VK
}

/// Human-readable name shown in the hotkey dialog and the activity log.
pub fn vk_display_name(vk: i32) -> String {
    match vk {
        PANIC_VK => "F8".to_string(),
        VK_ESCAPE => "Esc".to_string(),
        VK_SPACE => "Space".to_string(),
        code if (VK_F1..=VK_F24).contains(&code) => {
            format!("F{}", code - VK_F1 + 1)
        }
        code if (VK_0..=VK_9).contains(&code) => char::from_u32((code - VK_0) as u32 + '0' as u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("VK {code:#04X}")),
        code if (VK_A..=VK_Z).contains(&code) => char::from_u32((code - VK_A) as u32 + 'A' as u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("VK {code:#04X}")),
        code => format!("VK {code:#04X}"),
    }
}

/// Every code the assignment dialog polls while listening for a key press.
pub fn capturable_vks() -> Vec<i32> {
    let mut codes: Vec<i32> = Vec::new();
    codes.extend(VK_F1..=VK_F24);
    codes.extend(VK_0..=VK_9);
    codes.extend(VK_A..=VK_Z);
    codes.push(VK_SPACE);
    codes
}
