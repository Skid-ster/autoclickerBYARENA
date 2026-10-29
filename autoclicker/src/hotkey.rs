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

/// Each current visual style can be used in light or dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceStyle {
    Classic,
    Modern,
    Custom,
}

/// Color mode is independent of the selected visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceMode {
    Light,
    Dark,
}

/// Persisted appearance applied to every window. The first two keys retain
/// their historic meaning; older extra themes remain available and loadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTheme {
    /// The familiar light dialog (also the default for existing settings).
    #[default]
    Classic,
    /// The familiar dark dialog, persisted as `dark` by older versions.
    Dark,
    /// Modern light cards and controls.
    ModernLight,
    /// Modern dark cards and controls.
    ModernDark,
    /// A user-defined gradient with light surfaces.
    CustomLight,
    /// A user-defined gradient with dark surfaces.
    CustomDark,
    /// Previous pure-black theme.
    Oled,
    /// Previous green-on-black theme.
    Matrix,
    /// Previous indigo theme.
    Aurora,
}

impl AppTheme {
    pub fn all() -> [AppTheme; 9] {
        [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
            AppTheme::CustomLight,
            AppTheme::CustomDark,
            AppTheme::Oled,
            AppTheme::Matrix,
            AppTheme::Aurora,
        ]
    }

    pub fn previous() -> [AppTheme; 3] {
        [AppTheme::Oled, AppTheme::Matrix, AppTheme::Aurora]
    }

    /// Previous themes have no current style, so selecting a new style is
    /// an explicit choice rather than an implicit migration on load.
    pub fn style(self) -> Option<AppearanceStyle> {
        match self {
            AppTheme::Classic | AppTheme::Dark => Some(AppearanceStyle::Classic),
            AppTheme::ModernLight | AppTheme::ModernDark => Some(AppearanceStyle::Modern),
            AppTheme::CustomLight | AppTheme::CustomDark => Some(AppearanceStyle::Custom),
            AppTheme::Oled | AppTheme::Matrix | AppTheme::Aurora => None,
        }
    }

    pub fn mode(self) -> AppearanceMode {
        match self {
            AppTheme::Classic | AppTheme::ModernLight | AppTheme::CustomLight => {
                AppearanceMode::Light
            }
            AppTheme::Dark
            | AppTheme::ModernDark
            | AppTheme::CustomDark
            | AppTheme::Oled
            | AppTheme::Matrix
            | AppTheme::Aurora => AppearanceMode::Dark,
        }
    }

    pub fn from_appearance(style: AppearanceStyle, mode: AppearanceMode) -> AppTheme {
        match (style, mode) {
            (AppearanceStyle::Classic, AppearanceMode::Light) => AppTheme::Classic,
            (AppearanceStyle::Classic, AppearanceMode::Dark) => AppTheme::Dark,
            (AppearanceStyle::Modern, AppearanceMode::Light) => AppTheme::ModernLight,
            (AppearanceStyle::Modern, AppearanceMode::Dark) => AppTheme::ModernDark,
            (AppearanceStyle::Custom, AppearanceMode::Light) => AppTheme::CustomLight,
            (AppearanceStyle::Custom, AppearanceMode::Dark) => AppTheme::CustomDark,
        }
    }

    /// Stable key used in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            AppTheme::Classic => "classic",
            AppTheme::Dark => "dark",
            AppTheme::ModernLight => "modern_light",
            AppTheme::ModernDark => "modern_dark",
            AppTheme::CustomLight => "custom_light",
            AppTheme::CustomDark => "custom_dark",
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
            "modern_light" => Some(AppTheme::ModernLight),
            "modern_dark" => Some(AppTheme::ModernDark),
            "custom_light" => Some(AppTheme::CustomLight),
            "custom_dark" => Some(AppTheme::CustomDark),
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
            AppTheme::Dark => "Classic dark",
            AppTheme::ModernLight => "Modern light",
            AppTheme::ModernDark => "Modern dark",
            AppTheme::CustomLight => "Custom light",
            AppTheme::CustomDark => "Custom dark",
            AppTheme::Oled => "Midnight OLED",
            AppTheme::Matrix => "Terminal green",
            AppTheme::Aurora => "Aurora modern",
        }
    }
}

/// Scale values offered by older releases, retained for settings compatibility.
pub const UI_SCALE_OPTIONS: [u32; 6] = [75, 100, 125, 150, 175, 200];
/// Default scale for new and reset settings.
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

fn sanitized_auto_zoom(auto_zoom: f32) -> f32 {
    if auto_zoom.is_finite() && auto_zoom > 1.0 {
        auto_zoom
    } else {
        1.0
    }
}

/// The main-window floor expressed in the *live* egui points that exist
/// while `auto_zoom` is applied.
///
/// The persisted/physical floor is fit-independent: at a given user
/// percentage, Windows must always be allowed to shrink the window back
/// to the physical size that shows `WINDOW_MIN_WIDTH` x
/// `WINDOW_MIN_HEIGHT` with the window-driven fit at 1.0. egui-winit
/// converts a declared minimum from live points to physical pixels using
/// the total zoom (user percentage x auto-zoom), so declaring the raw
/// 560x456 floor at a high auto-zoom would multiply the physical minimum
/// by that transient fit factor and trap a large window at a huge size.
/// Dividing by the auto-zoom keeps the resulting physical floor constant
/// while leaving the user's own percentage fully effective.
pub fn window_floor_for_auto_zoom(auto_zoom: f32) -> (f32, f32) {
    let factor = sanitized_auto_zoom(auto_zoom);
    (
        WINDOW_MIN_WIDTH as f32 / factor,
        WINDOW_MIN_HEIGHT as f32 / factor,
    )
}

/// Converts a live point size into fit-independent points by removing
/// the window-driven fit factor. This is the coordinate system used for
/// the persisted window size and for deciding whether a live size is
/// below the classic floor.
pub fn fit_independent_window_size(live_size: (f32, f32), auto_zoom: f32) -> (f32, f32) {
    let factor = sanitized_auto_zoom(auto_zoom);
    (live_size.0 * factor, live_size.1 * factor)
}

/// User-selected gradient colors and its presentation. Values are bounded
/// when loaded, so even a hand-edited settings file remains safe to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomGradient {
    pub start: [u8; 3],
    pub end: [u8; 3],
    pub angle_degrees: u16,
    pub strength: u8,
    /// Amount of the subtle, procedurally generated surface grain.
    pub texture_strength: u8,
}

impl Default for CustomGradient {
    fn default() -> Self {
        Self {
            start: [27, 166, 154],
            end: [107, 112, 223],
            angle_degrees: 35,
            strength: 70,
            texture_strength: 60,
        }
    }
}

pub const MIN_STABILITY_SECONDS: u32 = 10;
pub const MAX_STABILITY_SECONDS: u32 = 60;

pub fn sanitize_stability_seconds(seconds: u32) -> u32 {
    seconds.clamp(MIN_STABILITY_SECONDS, MAX_STABILITY_SECONDS)
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
    /// Retained even when another appearance is selected.
    pub custom_gradient: CustomGradient,
    /// Preferred real-click stability-test duration in seconds.
    pub stability_test_seconds: u32,
    /// Legacy saved window scale (75–200), retained so upgrades preserve
    /// the user's existing appearance and fit-independent window size.
    pub ui_scale_percent: u32,
    /// Let the window size drive the presentation: the whole UI
    /// scales uniformly to fit the window as it grows, and in
    /// panoramic windows the group boxes rearrange into a
    /// three-across row with a single row of action buttons. Off
    /// restores the fixed classic composition at every size.
    pub auto_scale: bool,
    /// Ask GitHub once per launch whether a newer release exists (a
    /// single HTTPS GET of the public latest-release metadata; nothing
    /// about the user or their settings is sent). The Updates window
    /// can always check manually, and checking never downloads or
    /// installs anything on its own.
    pub check_updates: bool,
    /// Last user-chosen inner window width, sanitized to
    /// [WINDOW_MIN_WIDTH, 4x WINDOW_MIN_WIDTH]. Measured in
    /// fit-independent points: the point width the window has with
    /// the window-driven scale factor neutralized, so the same
    /// physical size round-trips no matter which fit factor is
    /// active. The floor defaults are upgraded once, on a fresh
    /// install only, to the display-aware opening size.
    pub window_width: u32,
    /// Last user-chosen inner window height in the same
    /// fit-independent points, sanitized to
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
            custom_gradient: CustomGradient::default(),
            stability_test_seconds: 20,
            ui_scale_percent: UI_SCALE_DEFAULT,
            auto_scale: true,
            check_updates: true,
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
