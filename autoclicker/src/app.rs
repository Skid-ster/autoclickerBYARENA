//! MacroClicker — an OP Auto Clicker 4.1-style dashboard (classic Win32 look).
//!
//! The main window is a naturally resizable, non-scrolling replica of the
//! classic OP Auto Clicker 4.x screen: four bordered group boxes (Click
//! interval, Click options, Click repeat, Cursor position) above the
//! authentic 2x2 grid of standard push buttons. The classic 560x456 size
//! is the enforced minimum; a larger window only widens the group boxes
//! and the button grid and adds breathing room between the groups, so
//! every control stays the same size and visible at every reachable
//! window size. There is no diagnostics surface: every
//! control on screen configures clicking, and the engine's telemetry stays
//! an internal, test-verified concern.
//!
//! All worker interaction goes through atomics. The update thread never
//! blocks on the worker.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Frame, Grid, Layout, Margin, Rounding, Sense, Stroke, Vec2};

use crate::config::{
    interval_parts_to_ms, ms_to_interval_parts, ClickType, CoordinateMode, EngineConfig, InputMode,
    MouseButton,
};
use crate::engine::{ClickEngine, STATE_PANIC};
use crate::hotkey::{
    self, is_reserved_panic_vk, is_supported_toggle_vk, vk_display_name, AppPrefs, AppTheme,
    UI_SCALE_OPTIONS, VK_ESCAPE,
};
use crate::macro_rec::{self, MacroRecorder};
use crate::settings;
use crate::win32;

/// Complete color scheme for one theme. Both themes share every layout
/// metric — only colors differ — so switching can never move a control.
struct Palette {
    dark: bool,
    face: Color32,
    button: Color32,
    button_disabled: Color32,
    button_border: Color32,
    hover_fill: Color32,
    pressed_fill: Color32,
    input_bg: Color32,
    input_disabled: Color32,
    /// Group-box surface. One step above `face` in the dark themes so
    /// each group reads as an elevated card; identical to `face` in the
    /// Classic theme, where a native group box sits flat on the dialog.
    group_fill: Color32,
    /// Sunken status-strip surface in the dark themes (the strip reads
    /// as a recessed readout); identical to `face` in Classic.
    well_fill: Color32,
    group_border: Color32,
    input_border: Color32,
    text: Color32,
    /// Color of the group-box caption riding the top border. Kept as its
    /// own channel so a theme can give captions a different weight than
    /// body text without touching layout.
    group_title: Color32,
    text_dim: Color32,
    text_disabled: Color32,
    accent: Color32,
    success: Color32,
    danger: Color32,
    warning: Color32,
    chart_bg: Color32,
    chart_grid: Color32,
    chart_bar_alt: Color32,
    selection_bg: Color32,
}

// Classic Windows light palette, matched to the native 4.x screenshots.
static LIGHT_PALETTE: Palette = Palette {
    dark: false,
    face: Color32::from_rgb(240, 240, 240),
    button: Color32::from_rgb(236, 236, 236),
    button_disabled: Color32::from_rgb(245, 245, 245),
    button_border: Color32::from_rgb(173, 173, 173),
    hover_fill: Color32::from_rgb(229, 241, 251),
    pressed_fill: Color32::from_rgb(204, 228, 247),
    input_bg: Color32::WHITE,
    input_disabled: Color32::from_rgb(250, 250, 250),
    group_fill: Color32::from_rgb(240, 240, 240),
    well_fill: Color32::from_rgb(240, 240, 240),
    group_border: Color32::from_rgb(166, 166, 166),
    input_border: Color32::from_rgb(122, 122, 122),
    text: Color32::from_rgb(28, 28, 28),
    group_title: Color32::from_rgb(28, 28, 28),
    text_dim: Color32::from_rgb(102, 102, 102),
    text_disabled: Color32::from_rgb(166, 166, 166),
    accent: Color32::from_rgb(0, 103, 192),
    success: Color32::from_rgb(16, 124, 16),
    danger: Color32::from_rgb(196, 43, 28),
    warning: Color32::from_rgb(141, 98, 0),
    chart_bg: Color32::WHITE,
    chart_grid: Color32::from_rgb(226, 229, 233),
    chart_bar_alt: Color32::from_rgba_premultiplied(0, 103, 192, 110),
    selection_bg: Color32::from_rgba_premultiplied(0, 103, 192, 60),
};

// Dark counterpart in the Windows 11 dark-dialog spirit: near-black face,
// slightly lighter sunken fields, light text, brighter accent for contrast.
static DARK_PALETTE: Palette = Palette {
    dark: true,
    face: Color32::from_rgb(32, 32, 32),
    button: Color32::from_rgb(51, 51, 51),
    button_disabled: Color32::from_rgb(40, 40, 40),
    button_border: Color32::from_rgb(116, 116, 116),
    hover_fill: Color32::from_rgb(58, 62, 66),
    pressed_fill: Color32::from_rgb(70, 76, 82),
    input_bg: Color32::from_rgb(30, 30, 30),
    input_disabled: Color32::from_rgb(35, 35, 35),
    group_fill: Color32::from_rgb(40, 40, 40),
    well_fill: Color32::from_rgb(24, 24, 24),
    group_border: Color32::from_rgb(108, 108, 108),
    input_border: Color32::from_rgb(130, 130, 130),
    text: Color32::from_rgb(235, 235, 235),
    group_title: Color32::from_rgb(96, 165, 250),
    text_dim: Color32::from_rgb(158, 158, 158),
    text_disabled: Color32::from_rgb(118, 118, 118),
    accent: Color32::from_rgb(96, 165, 250),
    success: Color32::from_rgb(106, 200, 112),
    danger: Color32::from_rgb(248, 106, 106),
    warning: Color32::from_rgb(222, 180, 66),
    chart_bg: Color32::from_rgb(26, 26, 26),
    chart_grid: Color32::from_rgb(58, 58, 58),
    chart_bar_alt: Color32::from_rgba_premultiplied(38, 66, 100, 110),
    selection_bg: Color32::from_rgba_premultiplied(24, 48, 78, 150),
};

// Pure-black dark variant for OLED screens.
static OLED_PALETTE: Palette = Palette {
    dark: true,
    face: Color32::BLACK,
    button: Color32::from_rgb(24, 24, 24),
    button_disabled: Color32::from_rgb(14, 14, 14),
    button_border: Color32::from_rgb(100, 100, 100),
    hover_fill: Color32::from_rgb(32, 36, 40),
    pressed_fill: Color32::from_rgb(44, 50, 56),
    input_bg: Color32::from_rgb(8, 8, 8),
    input_disabled: Color32::from_rgb(10, 10, 10),
    group_fill: Color32::from_rgb(12, 12, 12),
    well_fill: Color32::BLACK,
    group_border: Color32::from_rgb(96, 96, 96),
    input_border: Color32::from_rgb(110, 110, 110),
    text: Color32::from_rgb(230, 230, 230),
    group_title: Color32::from_rgb(76, 194, 255),
    text_dim: Color32::from_rgb(150, 150, 150),
    text_disabled: Color32::from_rgb(104, 104, 104),
    accent: Color32::from_rgb(76, 194, 255),
    success: Color32::from_rgb(98, 212, 120),
    danger: Color32::from_rgb(255, 99, 99),
    warning: Color32::from_rgb(226, 186, 60),
    chart_bg: Color32::from_rgb(8, 8, 8),
    chart_grid: Color32::from_rgb(44, 44, 44),
    chart_bar_alt: Color32::from_rgba_premultiplied(28, 70, 92, 110),
    selection_bg: Color32::from_rgba_premultiplied(18, 48, 64, 150),
};

// Green-on-black terminal look.
static MATRIX_PALETTE: Palette = Palette {
    dark: true,
    face: Color32::from_rgb(8, 14, 8),
    button: Color32::from_rgb(16, 30, 16),
    button_disabled: Color32::from_rgb(10, 20, 10),
    button_border: Color32::from_rgb(52, 122, 52),
    hover_fill: Color32::from_rgb(22, 44, 22),
    pressed_fill: Color32::from_rgb(30, 60, 30),
    input_bg: Color32::from_rgb(7, 15, 7),
    input_disabled: Color32::from_rgb(8, 16, 8),
    group_fill: Color32::from_rgb(13, 23, 13),
    well_fill: Color32::from_rgb(4, 8, 4),
    group_border: Color32::from_rgb(50, 118, 50),
    input_border: Color32::from_rgb(58, 136, 58),
    text: Color32::from_rgb(130, 235, 130),
    group_title: Color32::from_rgb(0, 230, 118),
    text_dim: Color32::from_rgb(74, 150, 74),
    text_disabled: Color32::from_rgb(54, 104, 54),
    accent: Color32::from_rgb(0, 230, 118),
    success: Color32::from_rgb(0, 230, 118),
    danger: Color32::from_rgb(255, 90, 90),
    warning: Color32::from_rgb(210, 200, 70),
    chart_bg: Color32::from_rgb(5, 10, 5),
    chart_grid: Color32::from_rgb(22, 48, 22),
    chart_bar_alt: Color32::from_rgba_premultiplied(0, 92, 48, 110),
    selection_bg: Color32::from_rgba_premultiplied(0, 70, 36, 150),
};

// Modern deep-indigo look: layered blue-slate surfaces, violet accent.
// Every value proofed by contrast script (see docs/VERIFICATION.md):
// worst enabled text pair 6.11:1, every border >= 3.0:1 on both
// surfaces it touches.
static AURORA_PALETTE: Palette = Palette {
    dark: true,
    face: Color32::from_rgb(18, 19, 27),
    button: Color32::from_rgb(37, 40, 56),
    button_disabled: Color32::from_rgb(25, 27, 37),
    button_border: Color32::from_rgb(98, 106, 140),
    hover_fill: Color32::from_rgb(45, 49, 68),
    pressed_fill: Color32::from_rgb(55, 60, 84),
    input_bg: Color32::from_rgb(14, 15, 22),
    input_disabled: Color32::from_rgb(21, 23, 32),
    group_fill: Color32::from_rgb(27, 29, 41),
    well_fill: Color32::from_rgb(12, 13, 19),
    group_border: Color32::from_rgb(96, 104, 136),
    input_border: Color32::from_rgb(124, 134, 168),
    text: Color32::from_rgb(233, 235, 246),
    group_title: Color32::from_rgb(159, 140, 255),
    text_dim: Color32::from_rgb(160, 166, 190),
    text_disabled: Color32::from_rgb(105, 111, 134),
    accent: Color32::from_rgb(159, 140, 255),
    success: Color32::from_rgb(92, 224, 150),
    danger: Color32::from_rgb(251, 113, 133),
    warning: Color32::from_rgb(252, 211, 77),
    chart_bg: Color32::from_rgb(11, 12, 18),
    chart_grid: Color32::from_rgb(42, 46, 64),
    chart_bar_alt: Color32::from_rgba_premultiplied(64, 52, 120, 110),
    selection_bg: Color32::from_rgba_premultiplied(52, 42, 104, 150),
};

/// Logical (point) floor size of the main window: the exact classic
/// footprint, declared to the OS as the minimum inner size so the window
/// can never shrink to a size where a control would not fit. Larger
/// sizes are restored from the persisted preferences.
pub const MAIN_WINDOW_SIZE: [f32; 2] = [
    hotkey::WINDOW_MIN_WIDTH as f32,
    hotkey::WINDOW_MIN_HEIGHT as f32,
];

/// Widest the centered content column ever grows, in logical points.
/// The floor content is 536 points wide (560 minus the side margins);
/// the cap lets group boxes and the button grid share 200 points of
/// extra width before extra space becomes side margin instead, so a
/// maximized window never produces comically wide buttons.
const MAIN_CONTENT_MAX_WIDTH: f32 = 736.0_f32;

/// Most extra height each breathing gap absorbs before the remaining
/// height centers the whole content column instead. Past the gaps'
/// bounded share the rest splits above and below the column, so a tall
/// window composes a centered dialog instead of ever-deepening voids
/// between control clusters that belong together.
const MAIN_GAP_EXTRA_MAX: f32 = 14.0_f32;

/// Widest content column in the wide layout. Three group boxes across
/// need more elbow room than the stacked classic column before the
/// side margin takes over.
const MAIN_CONTENT_MAX_WIDTH_WIDE: f32 = 880.0_f32;

/// Window height at which the wide layout's column exactly fills the
/// window (its three-across row and single button row are shorter than
/// the stacked classic column, whose fill height is the 456-point
/// floor). The vertical rhythm measures spare height from here.
const MAIN_WIDE_FILL_HEIGHT: f32 = 372.0_f32;

/// Window-driven UI scale steps: (engage at or above, disengage below,
/// zoom factor). Widths are *neutral points* — live points times the
/// total applied zoom, i.e. the width the window has at 100 % zoom —
/// so a step describes the window's physical size and engages at the
/// same physical size whatever the user's chosen percentage is.
/// Growing the window past a step enlarges the whole UI inside the
/// same physical window; shrinking back below the lower edge of the
/// band restores the previous step. The gap between the two edges is
/// the hysteresis that keeps a window parked on a boundary from
/// flip-flopping scales.
/// Both window axes feed the same table: the height is converted to
/// width-equivalent neutral points (scaled by the floor's aspect
/// ratio), so a tall narrow window earns the same steps a wide one
/// does. The height's vote is fit-capped (see `auto_zoom_for`) so it
/// can never force the OS to grow the window past the size the user
/// chose.
const AUTO_ZOOM_TIERS: [(f32, f32, f32); 3] = [
    (800.0_f32, 744.0_f32, 1.10_f32),
    (1024.0_f32, 968.0_f32, 1.25_f32),
    (1440.0_f32, 1384.0_f32, 1.40_f32),
];

/// Wide-layout band, tracked to the second scale step: at or above the
/// engage width the middle row becomes Click options | Click repeat |
/// Cursor position and the action buttons form one row of four; below
/// the disengage width the classic stack returns.
const WIDE_LAYOUT_ENGAGE: f32 = 1024.0_f32;
const WIDE_LAYOUT_DISENGAGE: f32 = 968.0_f32;

/// Smallest side margin around the content column, kept at the floor
/// size so the classic layout is pixel-identical there.
const MAIN_SIDE_MARGIN: f32 = 12.0_f32;

const LOG_LIMIT: usize = 200;
const SAVE_THROTTLE_MS: u64 = 1_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Ready,
    Running,
    Stopping,
    Panic,
    Warning,
    Error,
}

impl StatusKind {
    fn color(self, pal: &Palette) -> Color32 {
        match self {
            StatusKind::Ready => pal.text_dim,
            StatusKind::Running => pal.success,
            StatusKind::Stopping => pal.warning,
            StatusKind::Panic => pal.danger,
            StatusKind::Warning => pal.warning,
            StatusKind::Error => pal.danger,
        }
    }
}

pub struct OpAutoClickerApp {
    engine: ClickEngine,
    config: EngineConfig,
    prefs: AppPrefs,

    // Click Interval editors (4.1 uses separate h/m/s/ms fields).
    hours: u64,
    minutes: u64,
    seconds: u64,
    millis: u64,
    offset_ms: f64,
    repeat_limited: bool,

    // Screen-point capture for "Pick location".
    picking_location: bool,
    pick_was_down: bool,

    // Record & Playback.
    show_macro_window: bool,
    macro_recorder: MacroRecorder,
    macro_recording: bool,
    macro_record_started: Option<Instant>,
    macro_lmb_was_down: bool,
    macro_rmb_was_down: bool,
    macro_mmb_was_down: bool,
    macro_until_stopped: bool,
    macro_loops: u64,
    macro_playing: bool,

    // Bounded in-memory breadcrumb log (never rendered; kept for parity
    // with the status strip and capped by LOG_LIMIT).
    logs: Vec<String>,
    last_seen_error: Option<String>,

    // Hotkey edge detection.
    toggle_was_down: bool,
    f8_was_down: bool,
    capture_key_was_down: bool,

    // Dialog state.
    show_hotkey_dialog: bool,
    capturing: bool,
    show_settings: bool,

    // Window-chrome bookkeeping: once the zoom factor for the chosen
    // scale has been applied, re-declare the minimum size and apply the
    // floor-aware size correction for that scale.
    pending_resize: bool,
    // Apply the persisted window size once (at startup and after a
    // factory reset) as soon as the zoom factor has settled.
    restore_window_size: bool,
    // Live-size observation stays paused until this instant so the
    // transient point sizes seen while a zoom or restore resize is in
    // flight are never mistaken for a size the user chose. The deadline
    // always resolves: observation resumes whether or not the OS granted
    // the exact requested size.
    window_settle_until: Instant,
    // Tallest content height among the middle row's group boxes (two in
    // the classic stack, three in the wide layout), rewritten every
    // frame so the whole row meets at one bottom edge. Seeded with the
    // classic pair height so the first frame already aligns.
    row_group_height: f32,
    // Natural widths of the four interval field+label groups, rewritten
    // every frame; the row's spare width is split between the groups
    // so the units spread tastefully without ever overflowing a cell.
    interval_group_widths: [f32; 4],
    // Currently applied window-driven scale factor (1.0 when the
    // feature is off or the window is below the first step). The total
    // egui zoom is the user's percentage times this factor.
    auto_zoom: f32,
    // Wide three-across layout currently in effect.
    wide_layout: bool,
    // Scale-step debounce: the candidate factor and how many
    // consecutive frames the neutral-point window size has held
    // still while asking for it. A step is applied only once the size
    // settles, so scaling never jumps mid-drag or chases OS resize
    // transients.
    tier_candidate: f32,
    stable_frames: u32,
    // Last neutral-point window size (live points x total zoom).
    last_base_size: Vec2,

    // Persistence. The "last saved" fingerprints make the dirty check exact
    // so the disk is never written while nothing changed.
    settings_path: Option<PathBuf>,
    settings_dirty: bool,
    last_save: Instant,
    last_saved_config: EngineConfig,
    last_saved_prefs: AppPrefs,
    was_running: bool,

    status_kind: StatusKind,
    /// Paint-clock time of the last status-KIND change; the status dot
    /// plays one short acknowledgement pulse from it. `NEG_INFINITY`
    /// means no pulse — the age test then fails on its own.
    status_flash_at: f64,
    /// The frame's paint-clock seconds, re-recorded at the top of every
    /// update pass so status changes (which happen outside the painter)
    /// can timestamp themselves against the same clock the dot reads.
    ui_clock_secs: f64,
    status_text: String,
}

impl Default for OpAutoClickerApp {
    fn default() -> Self {
        Self::new()
    }
}

impl OpAutoClickerApp {
    pub fn new() -> Self {
        let config = EngineConfig::default();
        let prefs = AppPrefs::default();
        let (hours, minutes, seconds, millis) =
            ms_to_interval_parts(config.base_interval_ms as u64);
        let mut app = Self {
            engine: ClickEngine::new(),
            config,
            prefs,
            hours,
            minutes,
            seconds,
            millis,
            offset_ms: 0.0,
            repeat_limited: false,
            picking_location: false,
            pick_was_down: false,
            show_macro_window: false,
            macro_recorder: MacroRecorder::new(),
            macro_recording: false,
            macro_record_started: None,
            macro_lmb_was_down: false,
            macro_rmb_was_down: false,
            macro_mmb_was_down: false,
            macro_until_stopped: true,
            macro_loops: 1,
            macro_playing: false,
            logs: vec![
                "MacroClicker initialized. F6 starts/stops, F8 is the panic stop key.".to_string(),
            ],
            last_seen_error: None,
            toggle_was_down: false,
            f8_was_down: false,
            capture_key_was_down: false,
            show_hotkey_dialog: false,
            capturing: false,
            show_settings: false,
            pending_resize: false,
            restore_window_size: true,
            window_settle_until: Instant::now(),
            row_group_height: 60.0_f32,
            interval_group_widths: [0.0_f32; 4],
            auto_zoom: 1.0_f32,
            wide_layout: false,
            tier_candidate: 1.0_f32,
            stable_frames: 0,
            last_base_size: Vec2::ZERO,
            settings_path: Self::settings_file_path(),
            settings_dirty: false,
            last_save: Instant::now(),
            last_saved_config: EngineConfig::default(),
            last_saved_prefs: AppPrefs::default(),
            was_running: false,
            status_kind: StatusKind::Ready,
            status_flash_at: f64::NEG_INFINITY,
            ui_clock_secs: 0.0_f64,
            status_text: String::new(),
        };
        app.load_persisted();
        app.sanitize_loaded_config();
        app.last_saved_config = app.config.clone();
        app.last_saved_prefs = app.prefs;
        app
    }

    /// Modes that no longer have an on-screen control are neutralized at
    /// load so a settings file written by an older build can never strand
    /// the app in a state the user cannot see or leave (for example a
    /// persisted background-window delivery whose target handle is long
    /// dead, or Gaussian humanization silently jittering every interval).
    /// Runs before the saved-fingerprint snapshot, so nothing is rewritten
    /// to disk until the user actually changes something.
    fn sanitize_loaded_config(&mut self) {
        self.config.input_mode = InputMode::GlobalSendInput;
        self.config.target_hwnd = 0;
        self.config.background_synchronous = false;
        self.config.humanize_enabled = false;
    }

    // ------------------------------------------------------------ persistence

    fn settings_file_path() -> Option<PathBuf> {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return Some(
                    PathBuf::from(appdata)
                        .join("op-auto-clicker")
                        .join("settings.conf"),
                );
            }
        }
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join("settings.conf"))
    }

    /// The persisted inner size for the not-yet-created main window, read
    /// through the same tolerant parser the app itself loads with. Any
    /// problem (no file, unreadable file) falls back to the classic
    /// floor, and parsed values are already sanitized into the supported
    /// range, so the OS window can open directly at the right size. The
    /// running app re-applies the same size once its zoom factor settles,
    /// which also resolves the invalid-file case back to the defaults.
    /// Persisted sizes are step-independent points; the builder wants
    /// live points, so the opening scale step is divided back out.
    pub fn persisted_window_size() -> [f32; 2] {
        let Some(path) = Self::settings_file_path() else {
            return MAIN_WINDOW_SIZE;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return MAIN_WINDOW_SIZE;
        };
        let (_, prefs) = settings::parse_settings(&text);
        // Persisted sizes are step-independent points; the step
        // thresholds are neutral points, one user-factor further.
        let user_factor = prefs.ui_scale_percent as f32 / 100.0_f32;
        let neutral = Vec2::new(
            prefs.window_width as f32 * user_factor,
            prefs.window_height as f32 * user_factor,
        );
        let auto = Self::initial_auto_zoom(neutral, user_factor, prefs.auto_scale);
        [
            (prefs.window_width as f32 / auto).max(MAIN_WINDOW_SIZE[0]),
            (prefs.window_height as f32 / auto).max(MAIN_WINDOW_SIZE[1]),
        ]
    }

    fn load_persisted(&mut self) {
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let (mut config, mut prefs) = settings::parse_settings(&text);
        // A window handle from a previous process can never be reused: the OS
        // may have recycled it for an unrelated window.
        config.target_hwnd = 0;
        if config.validate().is_err() {
            self.log("Saved settings failed validation; defaults kept.");
            self.set_status(
                StatusKind::Warning,
                "Saved settings were invalid; factory defaults used.",
            );
            return;
        }
        // Backward compatibility: files written before the checkbox existed
        // encode the intent through a non-zero offset magnitude.
        if config.random_offset_ms > 0.0 {
            prefs.random_offset_enabled = true;
        }
        self.config = config;
        self.prefs = prefs;
        if !hotkey::is_supported_toggle_vk(self.prefs.toggle_hotkey_vk) {
            self.log("Saved hotkey code was unsupported or reserved; restored F6.");
            self.prefs.toggle_hotkey_vk = hotkey::DEFAULT_TOGGLE_VK;
        }
        self.repeat_limited = self.config.max_clicks != 0;
        self.offset_ms = self.config.random_offset_ms;
        // Open at the scale step (and layout) the persisted size calls
        // for, so the first frames don't start small and then step up.
        // Persisted sizes are step-independent points; the thresholds
        // are neutral points, one user-factor further.
        let user_factor = self.prefs.ui_scale_percent as f32 / 100.0_f32;
        let neutral = Vec2::new(
            self.prefs.window_width as f32 * user_factor,
            self.prefs.window_height as f32 * user_factor,
        );
        self.auto_zoom = Self::initial_auto_zoom(neutral, user_factor, self.prefs.auto_scale);
        self.tier_candidate = self.auto_zoom;
        self.wide_layout = self.prefs.auto_scale && Self::wide_layout_for(neutral.x, false);
        self.sync_interval_editors();
        self.log(format!("Settings loaded from {}.", path.display()));
    }

    fn mark_dirty(&mut self) {
        self.settings_dirty = true;
    }

    fn flush_settings(&mut self) {
        self.settings_dirty = false;
        self.last_save = Instant::now();
        self.last_saved_config = self.config.clone();
        self.last_saved_prefs = self.prefs;
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let payload = settings::serialize_settings(&self.config, &self.prefs);
        if let Err(error) = std::fs::create_dir_all(parent).and_then(|_| {
            let tmp = parent.join("settings.conf.tmp");
            std::fs::write(&tmp, payload.as_bytes())?;
            std::fs::rename(&tmp, &path)
        }) {
            self.log(format!("Settings not saved: {error}"));
        }
    }

    fn maybe_autosave(&mut self) {
        if self.settings_dirty
            && self.last_save.elapsed() >= Duration::from_millis(SAVE_THROTTLE_MS)
        {
            self.flush_settings();
        }
    }

    // ------------------------------------------------------------- status/log

    fn log(&mut self, line: impl Into<String>) {
        self.logs.push(line.into());
        if self.logs.len() > LOG_LIMIT {
            let drop_count = self.logs.len() - LOG_LIMIT;
            self.logs.drain(0..drop_count);
        }
    }

    fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        // Only a kind CHANGE earns the dot's acknowledgement pulse; a new
        // message of the same kind just replaces the text, so chatter
        // inside one state never re-triggers the motion.
        if kind != self.status_kind {
            self.status_flash_at = self.ui_clock_secs;
        }
        self.status_kind = kind;
        self.status_text = text.into();
    }

    /// Progress (0..1) of the status-dot acknowledgement pulse at paint
    /// time `now`, given the recorded kind-change time `at`. The pulse
    /// lives for 280 ms (inside the 150-300 ms motion budget every other
    /// affordance uses), is paint-only, and ends exactly at the dot's
    /// resting radius — nothing about it can stick, accumulate or move a
    /// pixel of layout.
    fn status_pulse_t(now: f64, at: f64) -> Option<f32> {
        let age = now - at;
        if (0.0_f64..0.28_f64).contains(&age) {
            Some((age / 0.28_f64) as f32)
        } else {
            None
        }
    }

    // -------------------------------------------------------------- styling

    /// The palette for the persisted theme choice. `'static` on purpose so
    /// closures can hold it without borrowing `self`.
    fn palette(&self) -> &'static Palette {
        match self.prefs.theme {
            AppTheme::Classic => &LIGHT_PALETTE,
            AppTheme::Dark => &DARK_PALETTE,
            AppTheme::Oled => &OLED_PALETTE,
            AppTheme::Matrix => &MATRIX_PALETTE,
            AppTheme::Aurora => &AURORA_PALETTE,
        }
    }

    fn apply_style(&self, ctx: &egui::Context) {
        let pal = self.palette();
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = pal.dark;
        style.visuals.panel_fill = pal.face;
        style.visuals.window_fill = pal.face;
        // DragValue's keyboard-edit mode is a TextEdit, which paints
        // `extreme_bg_color`; it must match the button-mode field fill.
        style.visuals.extreme_bg_color = pal.input_bg;
        style.visuals.window_stroke = Stroke::new(1.0_f32, pal.group_border);
        // Fill-less buttons (DragValue's button mode) and ComboBoxes paint
        // `weak_bg_fill`, not `bg_fill`, so every widget state needs both set
        // explicitly or egui's built-in defaults bleed through with the
        // wrong contrast for the active theme.
        style.visuals.widgets.noninteractive.bg_fill = pal.face;
        style.visuals.widgets.noninteractive.weak_bg_fill = pal.input_disabled;
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.button_border);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text);
        style.visuals.widgets.inactive.bg_fill = pal.input_bg;
        style.visuals.widgets.inactive.weak_bg_fill = pal.input_bg;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal.text);
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.input_border);
        style.visuals.widgets.hovered.bg_fill = pal.hover_fill;
        style.visuals.widgets.hovered.weak_bg_fill = pal.hover_fill;
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, pal.text);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.accent);
        style.visuals.widgets.active.bg_fill = pal.pressed_fill;
        style.visuals.widgets.active.weak_bg_fill = pal.pressed_fill;
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, pal.text);
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, pal.accent);
        // `open` drives an expanded ComboBox button; its popup list inherits
        // the theme's window fill, so the whole dropdown matches.
        style.visuals.widgets.open.bg_fill = pal.input_bg;
        style.visuals.widgets.open.weak_bg_fill = pal.input_bg;
        style.visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, pal.text);
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, pal.input_border);
        style.visuals.selection.bg_fill = pal.selection_bg;
        style.visuals.selection.stroke = Stroke::new(1.5_f32, pal.accent);
        style.visuals.hyperlink_color = pal.accent;
        let rounding = Rounding::same(2.5_f32);
        style.visuals.widgets.noninteractive.rounding = rounding;
        style.visuals.widgets.inactive.rounding = rounding;
        style.visuals.widgets.hovered.rounding = rounding;
        style.visuals.widgets.active.rounding = rounding;
        style.visuals.widgets.open.rounding = rounding;
        style.spacing.item_spacing = Vec2::new(7.0_f32, 6.0_f32);
        style.spacing.button_padding = Vec2::new(10.0_f32, 5.0_f32);
        // Motion that communicates, never performs: state fades finish in
        // 180 ms — above the ~100 ms "did anything happen?" threshold,
        // comfortably under the 300 ms sluggishness ceiling.
        style.animation_time = 0.18_f32;
        // Disciplined depth: floating surfaces (Settings, Hotkey dialog,
        // dropdowns) carry a soft shadow so layering reads as hierarchy.
        let shadow_alpha: u8 = if pal.dark { 120 } else { 48 };
        style.visuals.window_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0_f32, 6.0_f32),
            blur: 18.0_f32,
            spread: 0.0_f32,
            color: Color32::from_black_alpha(shadow_alpha),
        };
        style.visuals.popup_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0_f32, 3.0_f32),
            blur: 10.0_f32,
            spread: 0.0_f32,
            color: Color32::from_black_alpha(shadow_alpha),
        };
        style.visuals.window_rounding = Rounding::same(6.0_f32);
        ctx.set_style(style);
    }

    // ----------------------------------------------------------- hotkey input

    fn poll_hotkeys(&mut self) {
        let f8_down = win32::is_f8_down();
        if f8_down && !self.f8_was_down && self.macro_recording {
            self.stop_macro_recording("F8");
        }
        if f8_down && !self.f8_was_down && self.engine.is_running() {
            self.engine.panic_stop();
            self.set_status(StatusKind::Panic, "Panic stop triggered by F8.");
            self.log("PANIC STOP triggered by F8.");
            self.mark_dirty();
            self.flush_settings();
        }
        self.f8_was_down = f8_down;

        // While the assignment dialog is capturing, the toggle key must not
        // start or stop sessions: every key press belongs to the dialog.
        // Pretend the toggle is held so a key still physically down when
        // capture ends cannot produce an immediate start/stop edge.
        if self.capturing {
            self.toggle_was_down = true;
            return;
        }
        let toggle_down = self.prefs.hotkeys_enabled
            && is_supported_toggle_vk(self.prefs.toggle_hotkey_vk)
            && win32::is_vk_down(self.prefs.toggle_hotkey_vk);
        if toggle_down && !self.toggle_was_down {
            self.toggle_session();
        }
        self.toggle_was_down = toggle_down;
    }

    fn poll_hotkey_capture(&mut self) {
        let escape_down = win32::is_vk_down(VK_ESCAPE);
        let hit = hotkey::capturable_vks()
            .into_iter()
            .find(|vk| win32::is_vk_down(*vk));
        let any_down = escape_down || hit.is_some();
        // Act only on a fresh press; keys already held when capture starts
        // (or held over from the rejection of F8) are ignored until released.
        let fresh_press = any_down && !self.capture_key_was_down;
        self.capture_key_was_down = any_down;
        if !fresh_press {
            return;
        }

        if escape_down {
            self.capturing = false;
            self.set_status(StatusKind::Ready, "Hotkey assignment cancelled.");
            return;
        }
        let Some(vk) = hit else { return };
        if is_reserved_panic_vk(vk) {
            self.set_status(
                StatusKind::Warning,
                "F8 is reserved for Panic Stop and cannot be reassigned.",
            );
            self.log("Hotkey assignment rejected: F8 is reserved for panic stop.");
            return;
        }
        if is_supported_toggle_vk(vk) {
            self.prefs.toggle_hotkey_vk = vk;
            self.capturing = false;
            let name = vk_display_name(vk);
            self.set_status(
                StatusKind::Ready,
                format!("Start/Stop hotkey set to {name}."),
            );
            self.log(format!("Start/Stop hotkey changed to {name}."));
            self.mark_dirty();
            self.flush_settings();
        }
    }

    // ------------------------------------------------------ location picking

    fn arm_location_pick(&mut self) {
        self.config.coordinate_mode = CoordinateMode::FixedScreen;
        self.picking_location = true;
        // The click that pressed the button must not capture itself.
        self.pick_was_down = true;
        self.set_status(
            StatusKind::Warning,
            "Click anywhere on screen to pick the point — Esc cancels.",
        );
    }

    fn poll_pick_capture(&mut self) {
        if win32::is_vk_down(VK_ESCAPE) {
            if self.picking_location {
                self.picking_location = false;
                self.set_status(StatusKind::Ready, "Location pick cancelled.");
            }
            self.pick_was_down = true;
            return;
        }
        let mouse_down = win32::is_vk_down(win32::VK_LBUTTON);
        if self.picking_location && mouse_down && !self.pick_was_down {
            if let Some((x, y)) = win32::get_cursor_pos() {
                self.config.fixed_x = x as i64;
                self.config.fixed_y = y as i64;
                self.picking_location = false;
                self.set_status(
                    StatusKind::Ready,
                    format!("Picked screen position ({x}, {y})."),
                );
                self.log(format!("Fixed position picked: ({x}, {y})."));
                self.mark_dirty();
                self.flush_settings();
            }
        }
        self.pick_was_down = mouse_down;
    }

    // -------------------------------------------------------- record & play

    fn start_macro_recording(&mut self) {
        if self.engine.is_running() || self.macro_recording {
            return;
        }
        self.macro_recorder.clear();
        self.macro_recording = true;
        self.macro_record_started = Some(Instant::now());
        // The click that pressed "Record" must not capture itself.
        self.macro_lmb_was_down = true;
        self.macro_rmb_was_down = win32::is_vk_down(win32::VK_RBUTTON);
        self.macro_mmb_was_down = win32::is_vk_down(win32::VK_MBUTTON);
        self.set_status(
            StatusKind::Warning,
            "Recording clicks — click anywhere; clicks on this window are ignored. \
             Stop with the button, the hotkey or F8.",
        );
        self.log("Macro recording started.");
    }

    fn stop_macro_recording(&mut self, via: &str) {
        if !self.macro_recording {
            return;
        }
        self.macro_recording = false;
        self.macro_record_started = None;
        let count = self.macro_recorder.len();
        self.set_status(
            StatusKind::Ready,
            format!("Recording stopped: {count} steps captured."),
        );
        self.log(format!("Macro recording stopped via {via}: {count} steps."));
    }

    /// Polls the three physical mouse buttons while a recording is armed.
    /// Runs on the same GetAsyncKeyState heartbeat as the global hotkeys,
    /// so clicks are captured even while another application is focused.
    fn poll_macro_record(&mut self) {
        let Some(started) = self.macro_record_started else {
            return;
        };
        let lmb = win32::is_vk_down(win32::VK_LBUTTON);
        let rmb = win32::is_vk_down(win32::VK_RBUTTON);
        let mmb = win32::is_vk_down(win32::VK_MBUTTON);
        let edges = [
            (lmb && !self.macro_lmb_was_down, MouseButton::Left),
            (rmb && !self.macro_rmb_was_down, MouseButton::Right),
            (mmb && !self.macro_mmb_was_down, MouseButton::Middle),
        ];
        self.macro_lmb_was_down = lmb;
        self.macro_rmb_was_down = rmb;
        self.macro_mmb_was_down = mmb;

        for (pressed, button) in edges {
            if !pressed {
                continue;
            }
            // Clicks aimed at MacroClicker itself (stopping the recording,
            // moving the window) never belong in the macro.
            if win32::foreground_is_own_process() {
                continue;
            }
            let Some((x, y)) = win32::get_cursor_pos() else {
                continue;
            };
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            if !self
                .macro_recorder
                .record_click(elapsed_ms, x as i64, y as i64, button)
            {
                self.stop_macro_recording("step limit");
                self.set_status(
                    StatusKind::Warning,
                    format!(
                        "Recording stopped: the {} step limit was reached.",
                        macro_rec::MAX_STEPS
                    ),
                );
                return;
            }
        }
    }

    fn start_macro_playback(&mut self) {
        if self.engine.is_running() || self.macro_recording || self.macro_recorder.is_empty() {
            return;
        }
        let steps: std::sync::Arc<[macro_rec::MacroStep]> =
            std::sync::Arc::from(self.macro_recorder.steps().to_vec());
        let loops = if self.macro_until_stopped {
            0
        } else {
            self.macro_loops.max(1)
        };
        let mut config = self.config.clone();
        config.input_mode = InputMode::GlobalSendInput;
        config.coordinate_mode = CoordinateMode::FixedScreen;
        config.target_hwnd = 0;
        config.max_clicks = macro_rec::clicks_for_loops(steps.len(), loops);
        match self.engine.start_with_macro(&config, steps) {
            Ok(()) => {
                self.macro_playing = true;
                self.last_seen_error = None;
                let count = self.macro_recorder.len();
                let loops_text = if loops == 0 {
                    "until stopped".to_string()
                } else {
                    format!("{loops}x")
                };
                self.set_status(
                    StatusKind::Running,
                    format!("Playing macro — {count} steps, {loops_text}."),
                );
                self.log(format!(
                    "Macro playback started: {count} steps, {loops_text}."
                ));
            }
            Err(error) => {
                self.set_status(StatusKind::Error, format!("Error: {error}"));
                self.log(format!("Macro playback rejected: {error}"));
            }
        }
    }

    // ------------------------------------------------------------- engine I/O

    fn surface_engine_errors(&mut self) {
        if let Some(error) = self.engine.last_error() {
            if Some(&error) != self.last_seen_error.as_ref() {
                self.log(format!("Engine error: {error}"));
                self.set_status(StatusKind::Error, format!("Error: {error}"));
                self.last_seen_error = Some(error);
            }
        }
    }

    fn toggle_session(&mut self) {
        if self.macro_recording {
            self.stop_macro_recording("hotkey");
            return;
        }
        if self.engine.is_running() {
            self.engine.request_stop();
            self.set_status(StatusKind::Stopping, "Stopping…");
            self.log("Stop requested via hotkey.");
            self.mark_dirty();
            self.flush_settings();
        } else {
            self.handle_start();
        }
    }

    fn handle_start(&mut self) {
        // Only global SendInput delivery is exposed by the UI; the target
        // handle is pinned clear so a stale persisted value can never leak
        // into a session.
        self.config.input_mode = InputMode::GlobalSendInput;
        self.config.target_hwnd = 0;
        match self.engine.start(&self.config) {
            Ok(()) => {
                self.last_seen_error = None;
                self.set_status(
                    StatusKind::Running,
                    format!(
                        "Running at {:.2} CPS — {} button, {} click.",
                        self.config.target_cps(),
                        self.config.mouse_button.label().to_lowercase(),
                        self.config.click_type.label().to_lowercase()
                    ),
                );
                self.log(format!(
                    "Click session started at {:.2} CPS.",
                    self.config.target_cps()
                ));
            }
            Err(error) => {
                self.set_status(StatusKind::Error, format!("Start rejected: {error}"));
                self.log(format!("Start rejected: {error}"));
            }
        }
    }

    fn reset_to_defaults(&mut self) {
        self.engine.request_stop();
        self.config = EngineConfig::default();
        self.prefs = AppPrefs::default();
        // Factory defaults include the classic window size: re-apply the
        // (floor) persisted size once the 100 % zoom has settled.
        self.auto_zoom = 1.0_f32;
        self.tier_candidate = 1.0_f32;
        self.wide_layout = false;
        self.stable_frames = 0;
        self.restore_window_size = true;
        self.repeat_limited = false;
        self.offset_ms = 0.0;
        self.picking_location = false;
        self.sync_interval_editors();
        self.set_status(StatusKind::Ready, "Factory defaults restored.");
        self.log("Configuration reset to factory defaults.");
        self.mark_dirty();
        self.flush_settings();
    }

    // ---------------------------------------------------------- interval math

    fn sync_interval_editors(&mut self) {
        let total = self.config.base_interval_ms.round().max(0.0) as u64;
        let (hours, minutes, seconds, millis) = ms_to_interval_parts(total);
        self.hours = hours;
        self.minutes = minutes;
        self.seconds = seconds;
        self.millis = millis;
    }

    fn commit_interval_editors(&mut self) {
        let total = interval_parts_to_ms(self.hours, self.minutes, self.seconds, self.millis);
        if total > 0 {
            self.config.base_interval_ms = total as f64;
        }
        // The 4.1 checkbox gates the offset; zero means exact timing.
        self.config.random_offset_ms = if self.prefs.random_offset_enabled {
            self.offset_ms
        } else {
            0.0
        };
    }

    fn interval_total_ms(&self) -> u64 {
        interval_parts_to_ms(self.hours, self.minutes, self.seconds, self.millis)
    }

    /// Compares the live configuration against the last flushed fingerprint.
    fn detect_settings_changes(&mut self) {
        if self.config != self.last_saved_config || self.prefs != self.last_saved_prefs {
            self.mark_dirty();
        }
    }

    fn validation_message(&self) -> Option<String> {
        if self.interval_total_ms() == 0 {
            return Some("Interval must be at least 1 millisecond.".to_string());
        }
        self.config.validate().err()
    }

    // ----------------------------------------------------------- window sizing

    /// Side margin that centers the content column once the window grows
    /// past the content cap. At (and below) the classic width the result
    /// is exactly the original 12-point margin in every theme.
    fn centered_side_margin(window_width: f32, content_cap: f32) -> f32 {
        if !window_width.is_finite() {
            return MAIN_SIDE_MARGIN;
        }
        let overflow = window_width - 2.0_f32 * MAIN_SIDE_MARGIN - content_cap;
        MAIN_SIDE_MARGIN + overflow.max(0.0_f32) / 2.0_f32
    }

    /// How spare window height is spent: each breathing gap between the
    /// control clusters first loosens by up to `MAIN_GAP_EXTRA_MAX`, and
    /// whatever remains centers the whole column (half lands in the
    /// returned top pad; the other half is simply the leftover panel
    /// space below the column). `fill_height` is the window height at
    /// which the active layout's column exactly fills the window, so
    /// both values are zero there and the classic composition is
    /// reproduced exactly.
    fn vertical_rhythm(window_height: f32, fill_height: f32, gaps: f32) -> (f32, f32) {
        if !window_height.is_finite() || !fill_height.is_finite() || gaps <= 0.0_f32 {
            return (0.0_f32, 0.0_f32);
        }
        let extra = (window_height - fill_height).max(0.0_f32);
        let gap_extra = (extra / gaps).min(MAIN_GAP_EXTRA_MAX);
        let top_pad = (extra - gaps * gap_extra).max(0.0_f32) / 2.0_f32;
        (gap_extra, top_pad)
    }

    /// One axis of the scale-step decision: maps a neutral-point
    /// extent (already expressed in width-equivalent points) through
    /// the tier table, with per-step hysteresis against the currently
    /// applied factor — a step engages at its upper edge and only
    /// releases below its lower edge, so an extent parked inside the
    /// band holds whatever step it arrived with.
    fn tier_for_extent(extent: f32, current: f32) -> f32 {
        if !extent.is_finite() || extent <= 0.0_f32 {
            return current;
        }
        let mut target = 1.0_f32;
        for &(engage, disengage, factor) in &AUTO_ZOOM_TIERS {
            let threshold = if current >= factor { disengage } else { engage };
            if extent >= threshold {
                target = target.max(factor);
            }
        }
        target
    }

    /// The width-equivalent neutral extent of the window height: the
    /// height scaled by the floor dialog's aspect ratio, so both axes
    /// speak the same tier-table language — a window grown to twice
    /// the floor's height votes exactly like one grown to twice its
    /// width.
    fn height_equivalent(base_height: f32) -> f32 {
        base_height * (MAIN_WINDOW_SIZE[0] / MAIN_WINDOW_SIZE[1])
    }

    /// The largest tier that keeps the live point size at or above the
    /// floor on BOTH axes under the user's own zoom percentage. A
    /// larger step would push live points under the declared OS
    /// minimum on one axis and the window would be grown to compensate
    /// — acceptable when the user's own percentage dictates it (that
    /// grow-only correction predates the step system), never as a
    /// surprise side effect of an automatic height-driven step.
    fn largest_fittable_step(base: Vec2, user_factor: f32) -> f32 {
        if !user_factor.is_finite()
            || user_factor <= 0.0_f32
            || !base.x.is_finite()
            || !base.y.is_finite()
        {
            return 1.0_f32;
        }
        let cap = (base.x / MAIN_WINDOW_SIZE[0]).min(base.y / MAIN_WINDOW_SIZE[1]) / user_factor;
        AUTO_ZOOM_TIERS
            .iter()
            .map(|&(_, _, factor)| factor)
            .filter(|&factor| factor <= cap)
            .fold(1.0_f32, f32::max)
    }

    /// The window-driven scale factor for a neutral-point window size.
    /// Width votes exactly as it always has; height votes through the
    /// same table in width-equivalent points, fit-capped so a tall
    /// window earns real steps (the previous width-only rule left a
    /// tall narrow window at the smallest scale, floating in empty
    /// space) without ever outvoting the physical size the user
    /// chose. Every input is a neutral-point extent or a persisted
    /// preference, so the result cannot feed back into itself.
    fn auto_zoom_for(base: Vec2, user_factor: f32, current: f32) -> f32 {
        if !base.x.is_finite() || !base.y.is_finite() || base.x <= 0.0_f32 || base.y <= 0.0_f32 {
            return current;
        }
        let width_step = Self::tier_for_extent(base.x, current);
        let height_step = Self::tier_for_extent(Self::height_equivalent(base.y), current);
        width_step.max(height_step.min(Self::largest_fittable_step(base, user_factor)))
    }

    /// The scale step to open with for a neutral-point window size.
    /// Rising edges only on both axes: on a cold start there is no
    /// previous step for hysteresis to protect. The height vote takes
    /// the same fit cap as the running decision, so the opening frame
    /// and the settled frame agree about which step a size deserves.
    fn initial_auto_zoom(base: Vec2, user_factor: f32, enabled: bool) -> f32 {
        if !enabled || !base.x.is_finite() || !base.y.is_finite() {
            return 1.0_f32;
        }
        let rising = |extent: f32| {
            AUTO_ZOOM_TIERS
                .iter()
                .filter(|&&(engage, _, _)| extent >= engage)
                .map(|&(_, _, factor)| factor)
                .fold(1.0_f32, f32::max)
        };
        let width_step = rising(base.x);
        let height_step = rising(Self::height_equivalent(base.y));
        width_step.max(height_step.min(Self::largest_fittable_step(base, user_factor)))
    }

    /// Wide layout for a neutral-point width, with the same
    /// arrive-and-hold hysteresis as the scale steps.
    fn wide_layout_for(base_width: f32, current: bool) -> bool {
        if !base_width.is_finite() {
            return current;
        }
        if current {
            base_width >= WIDE_LAYOUT_DISENGAGE
        } else {
            base_width >= WIDE_LAYOUT_ENGAGE
        }
    }

    /// Records the live inner size as the size to restore next launch,
    /// in step-independent points (live points times the applied
    /// window-driven factor), so the physical size round-trips no
    /// matter which scale step is active when it is written or read.
    /// Maximized, minimized and fullscreen sizes are never recorded (the
    /// last normal size is what a restore should return to), sub-floor
    /// transients are ignored, and recording stays paused while a zoom
    /// or restore resize is in flight. Persistence itself flows through
    /// the existing dirty fingerprint and 1-second autosave throttle, so
    /// a drag-resize writes at most once per second plus once on exit.
    fn observe_live_window_size(&mut self, ctx: &egui::Context) {
        if self.restore_window_size
            || self.pending_resize
            || Instant::now() < self.window_settle_until
        {
            return;
        }
        let (size, maximized, minimized, fullscreen) = ctx.input(|i| {
            let viewport = i.viewport();
            (
                i.screen_rect.size(),
                viewport.maximized,
                viewport.minimized,
                viewport.fullscreen,
            )
        });
        if maximized == Some(true) || minimized == Some(true) || fullscreen == Some(true) {
            return;
        }
        // The OS minimum is enforced in physical pixels, so a live point
        // size can land a fraction below the floor after rounding; allow
        // that much slack, and treat anything further below (or not
        // finite) as a transient that must not be recorded.
        if !size.x.is_finite()
            || !size.y.is_finite()
            || size.x < MAIN_WINDOW_SIZE[0] - 2.0_f32
            || size.y < MAIN_WINDOW_SIZE[1] - 2.0_f32
        {
            return;
        }
        let base = size * self.auto_zoom;
        let (width, height) = hotkey::sanitize_window_size(f64::from(base.x), f64::from(base.y));
        if width != self.prefs.window_width || height != self.prefs.window_height {
            self.prefs.window_width = width;
            self.prefs.window_height = height;
            self.mark_dirty();
        }
    }

    /// Window-driven presentation: derives the layout mode and the
    /// target scale step from the neutral-point window size (live
    /// points times the total applied zoom — invariant under the very
    /// zoom changes it drives, which is what makes feedback impossible,
    /// and under user-scale changes too, so re-picking a percentage
    /// never by itself engages or releases a step).
    /// The layout mode follows immediately (hysteresis only); a scale
    /// step applies only once the size has held still for a few
    /// consecutive frames, so it lands just after a drag ends rather
    /// than jumping mid-drag, and never chases OS resize transients.
    /// Runs only when the zoom machinery is idle (see `update`), so a
    /// step never compounds with an in-flight zoom or restore.
    fn adapt_to_window_size(&mut self, ctx: &egui::Context) {
        const SETTLE_FRAMES: u32 = 6;
        let (size, minimized) = ctx.input(|i| (i.screen_rect.size(), i.viewport().minimized));
        if minimized == Some(true)
            || !size.x.is_finite()
            || !size.y.is_finite()
            || size.x <= 0.0_f32
            || size.y <= 0.0_f32
        {
            self.stable_frames = 0;
            return;
        }
        // Neutral points via the applied zoom (not the desired one):
        // this only runs once the applied zoom equals user x step, so
        // the measure is the physical window size at 100 % zoom.
        let base = size * ctx.zoom_factor();
        self.wide_layout = self.prefs.auto_scale && Self::wide_layout_for(base.x, self.wide_layout);
        let target = if self.prefs.auto_scale {
            Self::auto_zoom_for(
                base,
                self.prefs.ui_scale_percent as f32 / 100.0_f32,
                self.auto_zoom,
            )
        } else {
            1.0_f32
        };
        let size_held = (base.x - self.last_base_size.x).abs() <= 1.5_f32
            && (base.y - self.last_base_size.y).abs() <= 1.5_f32;
        if size_held && (target - self.tier_candidate).abs() <= 0.001_f32 {
            self.stable_frames = self.stable_frames.saturating_add(1);
        } else {
            self.stable_frames = 0;
            self.tier_candidate = target;
        }
        self.last_base_size = base;
        if self.stable_frames >= SETTLE_FRAMES && (target - self.auto_zoom).abs() > 0.001_f32 {
            self.auto_zoom = target;
            self.stable_frames = 0;
            // The live point size this frame still belongs to the old
            // factor (the new zoom applies next frame), so size
            // observation must pause from this instant — otherwise the
            // observer in this very frame would multiply pre-step
            // points by the new factor and persist a phantom size.
            // The zoom bookkeeping refreshes the same deadline when it
            // runs.
            self.window_settle_until = Instant::now() + Duration::from_millis(750);
            ctx.request_repaint();
        }
    }

    // --------------------------------------------------------- classic widgets

    /// Win32 group box: a bordered frame whose title text breaks the top
    /// border, exactly like the native BS_GROUPBOX control. The box
    /// always spans the full width handed to it: `Frame` paints exactly
    /// the rect its contents claim, so the contents claim the whole
    /// available width first — without this the border would hug the
    /// widest inner row and stop mid-window. `min_height` lets the
    /// paired Click options / Click repeat boxes stretch to the same
    /// bottom edge; the call returns the contents' natural height
    /// (without `min_height` applied) so the pair can converge on the
    /// taller of the two.
    ///
    /// In the dark themes the box is drawn as an elevated card: a fill
    /// one step above the panel, a reserved drop shadow behind it and a
    /// hairline of light on its inner top edge, so the dialog reads as
    /// layered plates rather than flat outlines. The shadow slot is
    /// reserved before the frame paints and filled once the frame's real
    /// rect exists — same layer, strictly behind the card. Classic keeps
    /// `group_fill == face` and no shadow, i.e. byte-identical native
    /// paint. On the pure-black OLED face a black shadow could never be
    /// seen, so it is skipped there too.
    fn group_box<R>(
        ui: &mut egui::Ui,
        pal: &Palette,
        title: &str,
        min_height: f32,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (R, f32) {
        let shadow_slot = if pal.dark && pal.face != Color32::BLACK {
            Some(ui.painter().add(egui::Shape::Noop))
        } else {
            None
        };
        let response = Frame::none()
            .fill(pal.group_fill)
            .stroke(Stroke::new(1.0_f32, pal.group_border))
            .rounding(Rounding::same(2.0_f32))
            .inner_margin(Margin {
                left: 11.0_f32,
                right: 11.0_f32,
                top: 17.0_f32,
                bottom: 11.0_f32,
            })
            .show(ui, |ui| {
                let claim_width = ui.available_width();
                ui.set_min_width(claim_width);
                let value = add(ui);
                // Natural content height, measured before the minimum
                // claim below grows the box: the frame paints through
                // the (post-claim) min rect, while the caller learns
                // how tall the contents actually are on their own.
                let content = ui.min_rect();
                let natural_height = content.height().max(0.0_f32);
                if min_height > natural_height {
                    // Grow the box by pinning its bottom at an absolute
                    // y. `set_min_height` cannot do this: it spans the
                    // requested height forward from the live cursor, so
                    // the total overshoots by the box's scroll position
                    // in the window (verified against egui 0.27.2's
                    // placer: the span starts at `cursor.min`, which
                    // sits below the content in absolute coordinates).
                    let target_bottom = content.min.y + min_height.max(natural_height).ceil();
                    let pin = egui::Rect::from_min_max(
                        egui::pos2(content.min.x, target_bottom),
                        egui::pos2(content.min.x, target_bottom),
                    );
                    ui.allocate_rect(pin, Sense::hover());
                }
                (value, natural_height)
            });
        let rect = response.response.rect;
        if let Some(slot) = shadow_slot {
            // The shadow peeks out below and beside the card only; the
            // top stays tucked under the card fill.
            let shadow = rect
                .expand2(Vec2::new(1.0_f32, 0.0_f32))
                .translate(Vec2::new(0.0_f32, 2.0_f32));
            ui.painter().set(
                slot,
                egui::Shape::rect_filled(
                    shadow,
                    Rounding::same(3.0_f32),
                    Color32::from_black_alpha(72),
                ),
            );
        }
        // The title rides the top border, half of it above rect.top().
        // `painter_at` clips to the given rect, so painting through a clip
        // region expanded upward is what keeps the glyphs whole; the frame's
        // own stroke was already painted by `Frame::show` above.
        let painter = ui.painter_at(rect.expand2(Vec2::new(12.0_f32, 16.0_f32)));
        // Card rim light: one pale line along the inner top edge, pairing
        // with the shadow below to complete the raised-plate read. The
        // title backdrop painted next covers the segment behind the text,
        // so an uninterrupted line is correct here.
        if pal.dark {
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 2.0_f32, rect.top() + 1.5_f32),
                    egui::pos2(rect.right() - 2.0_f32, rect.top() + 1.5_f32),
                ],
                Stroke::new(1.0_f32, Color32::from_white_alpha(16)),
            );
        }
        let font = FontId::proportional(13.0_f32);
        let galley = painter.layout_no_wrap(title.to_string(), font.clone(), pal.group_title);
        let text_pos = rect.left_top() + Vec2::new(9.0_f32, -galley.size().y / 2.0_f32);
        // The backdrop must cover the 1 px border line entirely so the
        // title cleanly breaks it like a native BS_GROUPBOX caption. It
        // straddles two surfaces now — panel `face` above the border,
        // card `group_fill` below it — so it is painted in two halves
        // split at the border's midline; a single color would leave a
        // visible patch on one side of the line (in Classic both halves
        // are the same color, so the native paint is unchanged).
        let backdrop = egui::Rect::from_min_size(
            text_pos - Vec2::new(4.0_f32, 1.0_f32),
            galley.size() + Vec2::new(8.0_f32, 2.0_f32),
        );
        let seam = rect.top() + 0.5_f32;
        let upper = egui::Rect::from_min_max(backdrop.min, egui::pos2(backdrop.max.x, seam));
        let lower = egui::Rect::from_min_max(egui::pos2(backdrop.min.x, seam), backdrop.max);
        painter.rect_filled(upper, 0.0_f32, pal.face);
        painter.rect_filled(lower, 0.0_f32, pal.group_fill);
        // Paint the token the galley was laid out with: the palette's
        // accent-toned group title (identical to `text` in the Classic
        // theme, so the native look stays faithful there).
        painter.text(text_pos, Align2::LEFT_TOP, title, font, pal.group_title);
        response.inner
    }

    /// Hand-painted push button with full interaction feedback: the fill,
    /// border and elevation all track the pointer state, and the hover lift
    /// is eased over ~180 ms (inside the 150–250 ms window where motion
    /// reads as responsive rather than sluggish). The pressed state drops
    /// the lift so the button visibly sits down under the pointer.
    fn classic_button(
        ui: &mut egui::Ui,
        pal: &Palette,
        text: &str,
        enabled: bool,
        width: f32,
    ) -> egui::Response {
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 31.0_f32), sense);
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered() && !pressed;
        let (fill, border, text_color) = if !enabled {
            (pal.button_disabled, pal.button_border, pal.text_disabled)
        } else if pressed {
            (pal.pressed_fill, pal.accent, pal.text)
        } else if hovered {
            (pal.hover_fill, pal.accent, pal.text)
        } else {
            (pal.button, pal.button_border, pal.text)
        };
        // Eased hover lift: 0 at rest and while pressed, 1 px when hovered.
        let lift = ui.ctx().animate_value_with_time(
            response.id.with("hover_lift"),
            if hovered { 1.0_f32 } else { 0.0_f32 },
            0.18_f32,
        );
        let face_rect = rect.translate(Vec2::new(0.0_f32, -lift));
        let rounding = Rounding::same(2.5_f32);
        let painter = ui.painter();
        // Soft elevation shadow under enabled buttons; it deepens with the
        // lift so "what floats acts" — depth encodes interactivity.
        if enabled && !pressed {
            let shadow_alpha = if pal.dark { 110 } else { 26 };
            painter.rect_filled(
                face_rect.translate(Vec2::new(0.0_f32, 1.5_f32 + lift)),
                rounding,
                Color32::from_black_alpha(shadow_alpha),
            );
        }
        painter.rect(face_rect, rounding, fill, Stroke::new(1.0_f32, border));
        // Classic 3D face cue: a one-pixel inner highlight along the top
        // edge, the same bevel the native BS_PUSHBUTTON control carries.
        // While pressed the cue inverts to a dark line — a pushed button
        // catches shadow on its top edge, not light.
        if enabled {
            let bevel = if pressed {
                Color32::from_black_alpha(if pal.dark { 90 } else { 60 })
            } else {
                Color32::from_white_alpha(if pal.dark { 18 } else { 110 })
            };
            painter.line_segment(
                [
                    egui::pos2(face_rect.left() + 3.0_f32, face_rect.top() + 1.5_f32),
                    egui::pos2(face_rect.right() - 3.0_f32, face_rect.top() + 1.5_f32),
                ],
                Stroke::new(1.0_f32, bevel),
            );
        }
        painter.text(
            face_rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(13.5_f32),
            text_color,
        );
        response
    }

    fn numeric_field(ui: &mut egui::Ui, value: &mut u64, max: u64, width: f32) {
        ui.add_sized(
            [width, 22.0_f32],
            egui::DragValue::new(value).clamp_range(0..=max),
        );
    }

    // ------------------------------------------------------------- the screen

    fn draw_interval_group(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let _ = Self::group_box(ui, pal, "Click interval", 0.0_f32, |ui| {
            // The row's spare width is split between the four
            // field+label groups, so the units keep their reading
            // rhythm at every window size and widening the window
            // opens the gaps between groups instead of stranding dead
            // space at the right end. The split is computed from last
            // frame's measured group widths — a group is never forced
            // wider than it naturally is, so no font or window size
            // can make a label overflow and stretch the frame.
            let natural_total: f32 = self.interval_group_widths.iter().sum();
            let mut measured = [0.0_f32; 4];
            ui.horizontal(|ui| {
                let available = ui.available_width();
                let spacing = ui.spacing().item_spacing.x;
                // The final group's trailing item spacing still counts
                // toward the claimed row width, so reserve it up front.
                let share = if natural_total > 1.0_f32 {
                    ((available - spacing - natural_total) / 3.0_f32).max(spacing + 2.0_f32)
                } else {
                    spacing + 2.0_f32
                };
                let fields: [(&str, &mut u64, u64); 4] = [
                    ("hours", &mut self.hours, 99),
                    ("mins", &mut self.minutes, 59),
                    ("secs", &mut self.seconds, 59),
                    ("milliseconds", &mut self.millis, 999),
                ];
                for (index, (label, value, max)) in fields.into_iter().enumerate() {
                    let before = ui.cursor().left();
                    Self::numeric_field(ui, value, max, 56.0_f32);
                    ui.label(egui::RichText::new(label).color(pal.text));
                    measured[index] = (ui.cursor().left() - before - spacing).max(0.0_f32);
                    if index < 3 {
                        ui.add_space(share - spacing);
                    }
                }
            });
            self.interval_group_widths = measured;
            ui.add_space(7.0_f32);
            ui.horizontal(|ui| {
                let mut enabled = self.prefs.random_offset_enabled;
                if ui.checkbox(&mut enabled, "Random offset ±").changed() {
                    self.prefs.random_offset_enabled = enabled;
                    if enabled && self.offset_ms <= 0.0 {
                        // The reference 4.1 build shows 40 ms by default.
                        self.offset_ms = 40.0;
                    }
                    self.mark_dirty();
                }
                ui.add_enabled_ui(enabled, |ui| {
                    ui.add_sized(
                        [72.0_f32, 22.0_f32],
                        egui::DragValue::new(&mut self.offset_ms)
                            .speed(1.0)
                            .clamp_range(0.0..=10_000.0)
                            .fixed_decimals(1),
                    );
                });
                ui.label(egui::RichText::new("milliseconds").color(pal.text));
            });
        });
    }

    fn options_group_contents(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        Grid::new("click_options_grid")
            .num_columns(2)
            .spacing(Vec2::new(8.0_f32, 9.0_f32))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Mouse button:").color(pal.text));
                egui::ComboBox::from_id_source("mouse_button_combo")
                    .width(ui.available_width())
                    .selected_text(self.config.mouse_button.label())
                    .show_ui(ui, |ui| {
                        for button in MouseButton::all() {
                            ui.selectable_value(
                                &mut self.config.mouse_button,
                                button,
                                button.label(),
                            );
                        }
                    });
                ui.end_row();

                ui.label(egui::RichText::new("Click type:").color(pal.text));
                egui::ComboBox::from_id_source("click_type_combo")
                    .width(ui.available_width())
                    .selected_text(self.config.click_type.label())
                    .show_ui(ui, |ui| {
                        for kind in ClickType::all() {
                            ui.selectable_value(&mut self.config.click_type, kind, kind.label());
                        }
                    });
                ui.end_row();
            });
    }

    fn repeat_group_contents(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.repeat_limited, true, "Repeat");
            ui.add_enabled_ui(self.repeat_limited, |ui| {
                if self.repeat_limited && self.config.max_clicks == 0 {
                    self.config.max_clicks = 1;
                }
                ui.add_sized(
                    [76.0_f32, 22.0_f32],
                    egui::DragValue::new(&mut self.config.max_clicks)
                        .clamp_range(1..=1_000_000_000),
                );
            });
            ui.label(egui::RichText::new("times").color(pal.text));
        });
        ui.add_space(4.0_f32);
        ui.radio_value(&mut self.repeat_limited, false, "Repeat until stopped");
        if !self.repeat_limited {
            self.config.max_clicks = 0;
        }
    }

    fn pick_location_button(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let pick_label = if self.picking_location {
            "Cancel pick"
        } else {
            "Pick location"
        };
        let pick_response = Self::classic_button(ui, pal, pick_label, true, 104.0_f32)
            .on_hover_text("Capture the fixed click point by clicking it on screen");
        if pick_response.clicked() {
            if self.picking_location {
                self.picking_location = false;
                self.set_status(StatusKind::Ready, "Location pick cancelled.");
            } else {
                self.arm_location_pick();
            }
        }
    }

    fn fixed_xy_fields(&mut self, ui: &mut egui::Ui) {
        let fixed = self.config.coordinate_mode == CoordinateMode::FixedScreen;
        ui.add_enabled_ui(fixed, |ui| {
            ui.add_sized(
                [78.0_f32, 22.0_f32],
                egui::DragValue::new(&mut self.config.fixed_x)
                    .clamp_range(-100_000..=100_000)
                    .prefix("X: "),
            );
            ui.add_sized(
                [78.0_f32, 22.0_f32],
                egui::DragValue::new(&mut self.config.fixed_y)
                    .clamp_range(-100_000..=100_000)
                    .prefix("Y: "),
            );
        });
    }

    /// `stacked` is the narrow-column arrangement for the wide
    /// layout's three-across row: the pick row folds into separate
    /// radio, button and coordinate rows so nothing is squeezed
    /// sideways. Unstacked, the classic full-width box keeps them
    /// all on one line.
    fn position_group_contents(&mut self, ui: &mut egui::Ui, pal: &Palette, stacked: bool) {
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut self.config.coordinate_mode,
                CoordinateMode::CurrentCursor,
                "Current location",
            );
        });
        ui.add_space(4.0_f32);
        if stacked {
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.config.coordinate_mode,
                    CoordinateMode::FixedScreen,
                    "Pick location",
                );
            });
            ui.add_space(4.0_f32);
            ui.horizontal(|ui| self.pick_location_button(ui, pal));
            ui.add_space(4.0_f32);
            ui.horizontal(|ui| self.fixed_xy_fields(ui));
        } else {
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.config.coordinate_mode,
                    CoordinateMode::FixedScreen,
                    "Pick location",
                );
                self.pick_location_button(ui, pal);
                self.fixed_xy_fields(ui);
            });
        }
    }

    /// Feeds a row of group boxes' tallest natural height back as the
    /// shared minimum so they all meet at one bottom edge. Rounded up
    /// to a whole point: the painted frame of the tallest box rounds
    /// outward to whole pixels, so the raw fractional height would
    /// leave the stretched boxes a pixel short of their sibling.
    fn store_row_group_height(&mut self, natural: f32) {
        if natural > 0.0_f32 && natural.is_finite() {
            self.row_group_height = natural.ceil();
        }
    }

    // The row's shared height is last frame's tallest content height
    // (measured as the columns are laid out): every box meets at one
    // bottom edge and the column stays exactly as tall as its tallest
    // member needs. The classic-height seed keeps the first frame
    // already aligned, and the constant repaint heartbeat lands any
    // later correction within tens of milliseconds, so no visible
    // jump occurs.
    fn draw_options_and_repeat(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let row_height = self.row_group_height.max(0.0_f32);
        let mut taller_natural = 0.0_f32;
        ui.columns(2, |columns| {
            let (_, left_natural) =
                Self::group_box(&mut columns[0], pal, "Click options", row_height, |ui| {
                    self.options_group_contents(ui, pal)
                });
            let (_, right_natural) =
                Self::group_box(&mut columns[1], pal, "Click repeat", row_height, |ui| {
                    self.repeat_group_contents(ui, pal)
                });
            taller_natural = left_natural.max(right_natural);
        });
        self.store_row_group_height(taller_natural);
    }

    /// Wide layout's middle row: Click options, Click repeat and
    /// Cursor position three-abreast, still meeting at one bottom
    /// edge through the same shared-height mechanism.
    fn draw_wide_group_row(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let row_height = self.row_group_height.max(0.0_f32);
        let mut tallest_natural = 0.0_f32;
        ui.columns(3, |columns| {
            let (_, options_natural) =
                Self::group_box(&mut columns[0], pal, "Click options", row_height, |ui| {
                    self.options_group_contents(ui, pal)
                });
            let (_, repeat_natural) =
                Self::group_box(&mut columns[1], pal, "Click repeat", row_height, |ui| {
                    self.repeat_group_contents(ui, pal)
                });
            let (_, position_natural) =
                Self::group_box(&mut columns[2], pal, "Cursor position", row_height, |ui| {
                    self.position_group_contents(ui, pal, true)
                });
            tallest_natural = options_natural.max(repeat_natural).max(position_natural);
        });
        self.store_row_group_height(tallest_natural);
    }

    fn draw_position_group(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let _ = Self::group_box(ui, pal, "Cursor position", 0.0_f32, |ui| {
            self.position_group_contents(ui, pal, false)
        });
    }

    fn start_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let running = self.engine.is_running();
        let config_ok = self.validation_message().is_none();
        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
        if Self::classic_button(
            ui,
            pal,
            &format!("Start ({hotkey_name})"),
            !running && config_ok,
            width,
        )
        .clicked()
        {
            self.handle_start();
        }
    }

    fn stop_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let running = self.engine.is_running();
        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
        if Self::classic_button(ui, pal, &format!("Stop ({hotkey_name})"), running, width).clicked()
        {
            self.engine.request_stop();
            self.set_status(StatusKind::Stopping, "Stopping…");
            self.log("Graceful stop requested.");
            self.mark_dirty();
            self.flush_settings();
        }
    }

    fn hotkey_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let response = Self::classic_button(ui, pal, "Hotkey setting", true, width)
            .on_hover_text("Choose the key that starts and stops the clicker");
        if response.clicked() {
            self.show_hotkey_dialog = true;
            self.capturing = false;
        }
    }

    fn record_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let response = Self::classic_button(ui, pal, "Record & Playback", true, width)
            .on_hover_text("Record your own clicks and play them back as a macro");
        if response.clicked() {
            self.show_macro_window = true;
        }
    }

    fn draw_action_buttons(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let gap = ui.spacing().item_spacing.x;
        if self.wide_layout {
            // One row of four: the wide canvas earns the flatter,
            // single-glance action strip.
            let button_width = (ui.available_width() - 3.0_f32 * gap) / 4.0_f32;
            Grid::new("action_button_grid_wide")
                .num_columns(4)
                .spacing(Vec2::new(gap, 6.0_f32))
                .show(ui, |ui| {
                    self.start_button(ui, pal, button_width);
                    self.stop_button(ui, pal, button_width);
                    self.hotkey_button(ui, pal, button_width);
                    self.record_button(ui, pal, button_width);
                    ui.end_row();
                });
        } else {
            let button_width = (ui.available_width() - gap) / 2.0_f32;
            Grid::new("action_button_grid")
                .num_columns(2)
                .spacing(Vec2::new(gap, 6.0_f32))
                .show(ui, |ui| {
                    self.start_button(ui, pal, button_width);
                    self.stop_button(ui, pal, button_width);
                    ui.end_row();

                    self.hotkey_button(ui, pal, button_width);
                    self.record_button(ui, pal, button_width);
                    ui.end_row();
                });
        }
    }

    // ---------------------------------------------------------- hotkey dialog

    fn draw_hotkey_dialog(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        let mut open = self.show_hotkey_dialog;
        let mut should_close = false;
        egui::Window::new("Hotkey setting")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(340.0_f32)
            .show(ctx, |ui| {
                ui.checkbox(&mut self.prefs.hotkeys_enabled, "Enable global hotkey");
                ui.add_space(6.0_f32);
                ui.label(
                    egui::RichText::new(format!(
                        "Start/Stop hotkey: {}",
                        vk_display_name(self.prefs.toggle_hotkey_vk)
                    ))
                    .color(pal.text)
                    .strong(),
                );
                ui.add_space(6.0_f32);
                if self.capturing {
                    ui.label(
                        egui::RichText::new("Press any key to assign it…  (Esc cancels)")
                            .color(pal.warning),
                    );
                } else if ui.button("Assign new key…").clicked() {
                    self.capturing = true;
                    // Baseline: ignore anything currently held; require a
                    // distinct press after the button click.
                    self.capture_key_was_down = true;
                }
                ui.add_space(6.0_f32);
                ui.label(
                    egui::RichText::new(
                        "Supported: F1–F24, A–Z, 0–9 and Space. F8 is permanently reserved for Panic Stop.",
                    )
                    .color(pal.text_dim)
                    .small(),
                );
                ui.add_space(8.0_f32);
                if ui.button("Close").clicked() {
                    should_close = true;
                }
            });
        if self.capturing {
            self.poll_hotkey_capture();
        }
        if !open || should_close {
            self.show_hotkey_dialog = false;
            self.capturing = false;
            self.capture_key_was_down = false;
            self.mark_dirty();
            self.flush_settings();
        }
    }

    // --------------------------------------------------- record & playback UI

    fn draw_macro_window(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        let mut open = self.show_macro_window;
        egui::Window::new("Record & Playback")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(360.0_f32)
            .show(ctx, |ui| {
                let steps = self.macro_recorder.len();
                let duration_s = self.macro_recorder.total_duration_ms() / 1000.0;
                ui.label(
                    egui::RichText::new(format!(
                        "Recorded steps: {steps}   ·   one loop: {duration_s:.1} s"
                    ))
                    .color(pal.text)
                    .strong(),
                );
                if self.macro_recording {
                    ui.label(
                        egui::RichText::new(
                            "Recording… click anywhere on screen. Clicks on this \
                             window are ignored; the hotkey or F8 also stops recording.",
                        )
                        .color(pal.warning),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Record real clicks (position, button and timing), then \
                             replay them exactly through the precision engine.",
                        )
                        .color(pal.text_dim)
                        .small(),
                    );
                }
                ui.add_space(6.0_f32);

                let engine_running = self.engine.is_running();
                ui.horizontal(|ui| {
                    if self.macro_recording {
                        if ui.button("Stop recording").clicked() {
                            self.stop_macro_recording("button");
                        }
                    } else {
                        ui.add_enabled_ui(!engine_running, |ui| {
                            if ui.button("Record").clicked() {
                                self.start_macro_recording();
                            }
                        });
                    }
                    ui.add_enabled_ui(
                        steps > 0 && !self.macro_recording && !engine_running,
                        |ui| {
                            if ui.button("Clear").clicked() {
                                self.macro_recorder.clear();
                                self.set_status(StatusKind::Ready, "Macro cleared.");
                                self.log("Macro cleared.");
                            }
                        },
                    );
                });
                ui.add_space(6.0_f32);

                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.macro_until_stopped, true, "Play until stopped");
                    ui.radio_value(&mut self.macro_until_stopped, false, "Play");
                    let mut loops = self.macro_loops.max(1);
                    ui.add_enabled_ui(!self.macro_until_stopped, |ui| {
                        ui.add_sized(
                            [64.0_f32, 20.0_f32],
                            egui::DragValue::new(&mut loops).clamp_range(1..=1_000_000),
                        );
                    });
                    self.macro_loops = loops;
                    ui.label("times");
                });
                ui.add_space(8.0_f32);

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(
                        steps > 0 && !self.macro_recording && !engine_running,
                        |ui| {
                            if ui.button("Play").clicked() {
                                self.start_macro_playback();
                            }
                        },
                    );
                    ui.add_enabled_ui(engine_running && self.macro_playing, |ui| {
                        if ui.button("Stop playback").clicked() {
                            self.engine.request_stop();
                            self.set_status(StatusKind::Stopping, "Stopping playback…");
                            self.log("Playback stop requested.");
                        }
                    });
                });
                ui.add_space(6.0_f32);
                ui.label(
                    egui::RichText::new(
                        "Playback uses global SendInput at the recorded screen \
                         positions; gaps are replayed on the drift-free QPC \
                         schedule and F8 still panic-stops instantly.",
                    )
                    .color(pal.text_dim)
                    .small(),
                );
            });
        self.show_macro_window = open;
    }

    // ------------------------------------------------------------ advanced UI

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        let mut open = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(280.0_f32)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Theme").color(pal.text).strong());
                egui::ComboBox::from_id_source("settings_theme")
                    .width(220.0_f32)
                    .selected_text(self.prefs.theme.label())
                    .show_ui(ui, |ui| {
                        for theme in AppTheme::all() {
                            if ui
                                .selectable_value(&mut self.prefs.theme, theme, theme.label())
                                .changed()
                            {
                                self.mark_dirty();
                            }
                        }
                    });
                ui.label(
                    egui::RichText::new(
                        "All four themes share the exact same layout — switching never moves a control.",
                    )
                    .color(pal.text_dim)
                    .small(),
                );
                ui.add_space(10.0_f32);
                ui.label(egui::RichText::new("Window scale").color(pal.text).strong());
                egui::ComboBox::from_id_source("settings_scale")
                    .width(220.0_f32)
                    .selected_text(format!("{} %", self.prefs.ui_scale_percent))
                    .show_ui(ui, |ui| {
                        for percent in UI_SCALE_OPTIONS {
                            if ui
                                .selectable_value(
                                    &mut self.prefs.ui_scale_percent,
                                    percent,
                                    format!("{percent} %"),
                                )
                                .changed()
                            {
                                self.mark_dirty();
                            }
                        }
                    });
                ui.label(
                    egui::RichText::new(
                        "You can also drag any window edge or corner: the chosen size \
                         is remembered, and a scale change only enlarges the window \
                         if the controls would otherwise no longer fit.",
                    )
                    .color(pal.text_dim)
                    .small(),
                );
                ui.add_space(6.0_f32);
                if ui
                    .checkbox(
                        &mut self.prefs.auto_scale,
                        "Scale and rearrange with window size",
                    )
                    .changed()
                {
                    if !self.prefs.auto_scale {
                        // Back to the fixed classic composition at once;
                        // the evaluator would retire the step on its own,
                        // just a few frames later.
                        self.auto_zoom = 1.0_f32;
                        self.tier_candidate = 1.0_f32;
                        self.wide_layout = false;
                        self.stable_frames = 0;
                    }
                    self.mark_dirty();
                }
                ui.label(
                    egui::RichText::new(
                        "As the window grows past set widths the whole interface \
                         steps up in scale, and the widest step rearranges the \
                         middle groups three-abreast with one row of buttons. \
                         The steps multiply with the percentage above.",
                    )
                    .color(pal.text_dim)
                    .small(),
                );
                ui.add_space(10.0_f32);
                ui.separator();
                ui.add_space(6.0_f32);
                if ui
                    .button("Reset all settings to factory defaults")
                    .clicked()
                {
                    self.reset_to_defaults();
                }
            });
        if !open {
            self.show_settings = false;
        }
    }
}

impl eframe::App for OpAutoClickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_clock_secs = ctx.input(|i| i.time);
        self.apply_style(ctx);
        let pal = self.palette();

        // UI scale and window size: the zoom factor follows the persisted
        // percentage times the window-driven step (`adapt_to_window_size`
        // owns the step; it only moves on settled sizes, and because it
        // changes no physical size the point size simply rescales inside
        // the same window). One frame after a zoom change (once egui
        // reports the new factor as applied) the window bookkeeping runs
        // exactly once. The minimum size is re-declared in points —
        // egui-winit multiplies viewport sizes by the current
        // pixels-per-point (zoom x native scale) when it talks to the
        // OS, so the physical floor rescales with the chosen percentage
        // without any multiplication here (doing it in both places would
        // double-scale the floor). A pending restore (startup, factory
        // reset) applies the persisted size converted to live points; a
        // plain scale change only ever grows the window, and only if the
        // new zoom left it below the floor, so a window the user enlarged
        // is never forced back down.
        let desired_zoom = self.prefs.ui_scale_percent as f32 / 100.0_f32 * self.auto_zoom;
        if (ctx.zoom_factor() - desired_zoom).abs() > 0.001_f32 {
            ctx.set_zoom_factor(desired_zoom);
            self.pending_resize = true;
            ctx.request_repaint();
        } else if self.pending_resize || self.restore_window_size {
            self.pending_resize = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(
                MAIN_WINDOW_SIZE[0],
                MAIN_WINDOW_SIZE[1],
            )));
            if self.restore_window_size {
                self.restore_window_size = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                    self.prefs.window_width as f32 / self.auto_zoom,
                    self.prefs.window_height as f32 / self.auto_zoom,
                )));
            } else {
                let live = ctx.input(|i| i.screen_rect.size());
                if live.x.is_finite() && live.y.is_finite() && live.x > 0.0_f32 && live.y > 0.0_f32
                {
                    let grown = Vec2::new(
                        live.x.max(MAIN_WINDOW_SIZE[0]),
                        live.y.max(MAIN_WINDOW_SIZE[1]),
                    );
                    if (grown.x - live.x).abs() > 0.5_f32 || (grown.y - live.y).abs() > 0.5_f32 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(grown));
                    }
                }
            }
            // Point sizes are meaningless for a few frames while the OS
            // applies a resize under a new zoom; pause live observation
            // briefly. The deadline resolves on its own even if the OS
            // refuses the exact size (for example a smaller monitor).
            self.window_settle_until = Instant::now() + Duration::from_millis(750);
            ctx.request_repaint();
        } else {
            // Zoom is at its target and no resize bookkeeping is in
            // flight: safe to evaluate the window-driven presentation.
            self.adapt_to_window_size(ctx);
        }
        self.observe_live_window_size(ctx);

        self.poll_hotkeys();
        self.poll_pick_capture();
        self.poll_macro_record();
        self.surface_engine_errors();
        // During macro playback the engine runs a cloned configuration
        // (fixed-step budget, per-step positions); pushing the live UI
        // config into it would overwrite that budget mid-run.
        if self.engine.is_running() && !self.macro_playing {
            if let Err(error) = self.engine.update_config(&self.config) {
                if self.last_seen_error.as_deref() != Some(error.as_str()) {
                    self.log(format!("Engine error: {error}"));
                    self.set_status(StatusKind::Error, format!("Error: {error}"));
                    self.last_seen_error = Some(error);
                }
            }
        }
        // Watchdog: if the dedicated worker thread ever died while a
        // session is marked running, force the engine back to idle and say
        // so — the UI must never report a running session that no thread
        // is actually driving, and Start/Stop must recover.
        if self.engine.is_running() && !self.engine.worker_alive() {
            self.engine.force_idle();
            self.macro_playing = false;
            self.set_status(
                StatusKind::Error,
                "Click worker lost — session aborted. Restart the app before clicking again.",
            );
            self.log("Worker thread found dead; engine forced idle.");
        }
        // Detect natural session completion (repeat count reached) so the
        // status line and persistence react even without a button press.
        // Manual stops and worker errors keep their own status instead of
        // being misreported as a clean completion.
        let was_macro_playback = self.macro_playing;
        if self.was_running && !self.engine.is_running() && was_macro_playback {
            self.macro_playing = false;
            if self.engine.state() != STATE_PANIC {
                // A manual stop ended the playback early; say so instead of
                // claiming a clean completion.
                let verdict = if self.status_kind == StatusKind::Stopping {
                    "Playback stopped"
                } else {
                    "Playback complete"
                };
                self.set_status(
                    StatusKind::Ready,
                    format!(
                        "{verdict} — {} clicks delivered.",
                        self.engine.click_count()
                    ),
                );
                self.log(format!(
                    "Macro playback finished after {} clicks.",
                    self.engine.click_count()
                ));
            }
        }
        if self.was_running && !self.engine.is_running() {
            let count_reached = !was_macro_playback
                && self.repeat_limited
                && self.config.max_clicks > 0
                && self.engine.click_count() >= self.config.max_clicks;
            if count_reached && self.engine.state() != STATE_PANIC {
                self.set_status(
                    StatusKind::Ready,
                    format!(
                        "Session complete — {} clicks delivered.",
                        self.engine.click_count()
                    ),
                );
                self.log(format!(
                    "Session finished after {} clicks.",
                    self.engine.click_count()
                ));
            } else if self.status_kind == StatusKind::Stopping {
                // The worker has fully drained after a manual stop. Without
                // this confirmation the transient "Stopping…" status would
                // stay on screen forever — the feedback loop must visibly
                // close once the engine actually stops.
                self.set_status(
                    StatusKind::Ready,
                    format!("Stopped — {} clicks delivered.", self.engine.click_count()),
                );
                self.log(format!(
                    "Session stopped after {} clicks.",
                    self.engine.click_count()
                ));
            }
            self.mark_dirty();
            self.flush_settings();
        }
        self.was_running = self.engine.is_running();

        self.commit_interval_editors();
        self.detect_settings_changes();
        self.maybe_autosave();
        // Heartbeat repaint — this is what keeps the GLOBAL hotkeys alive.
        // F6/F8 are polled via GetAsyncKeyState once per UI frame, but egui
        // is event-driven: without a standing repaint request the app stops
        // producing frames the moment it is idle or unfocused, and the poll
        // silently stops running (F6 appeared completely dead unless the
        // window was being interacted with). Interactive states keep the
        // fast 16 ms tick; idle keeps a light 50 ms tick so an F6 press
        // from inside a game is noticed within one poll interval.
        let heartbeat = if self.engine.is_running()
            || self.capturing
            || self.picking_location
            || self.macro_recording
            || Self::status_pulse_t(self.ui_clock_secs, self.status_flash_at).is_some()
        {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(50)
        };
        ctx.request_repaint_after(heartbeat);

        // Single non-scrolling screen: no ScrollArea, everything fits at
        // the OS-enforced floor and still fits above it. Growing the
        // window widens the group boxes and the button grid (all field
        // widths and control heights stay fixed) up to the content cap
        // and centers the column beyond it; past the wide threshold
        // (`adapt_to_window_size` owns the switch) the middle groups go
        // three-abreast and the buttons form one row. Extra height
        // first loosens the gaps between control clusters by a bounded
        // amount, then centers the whole column so related controls
        // stay visually grouped instead of drifting into separate
        // islands. At the floor size every metric below reduces to the
        // original classic layout. The top margin leaves room for the
        // first group title, which rides half above its frame's top
        // border.
        let window_size = ctx.screen_rect().size();
        let wide = self.wide_layout;
        let content_cap = if wide {
            MAIN_CONTENT_MAX_WIDTH_WIDE
        } else {
            MAIN_CONTENT_MAX_WIDTH
        };
        let side_margin = Self::centered_side_margin(window_size.x, content_cap);
        let (fill_height, gap_count) = if wide {
            (MAIN_WIDE_FILL_HEIGHT, 2.0_f32)
        } else {
            (MAIN_WINDOW_SIZE[1], 3.0_f32)
        };
        let (gap_extra, top_pad) = Self::vertical_rhythm(window_size.y, fill_height, gap_count);
        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal.face).inner_margin(Margin {
                left: side_margin,
                right: side_margin,
                top: 14.0_f32,
                bottom: 10.0_f32,
            }))
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.add_space(top_pad);
                    self.draw_interval_group(ui);
                    if wide {
                        ui.add_space(7.0_f32 + gap_extra);
                        self.draw_wide_group_row(ui);
                        ui.add_space(6.0_f32 + gap_extra);
                    } else {
                        ui.add_space(7.0_f32 + gap_extra);
                        self.draw_options_and_repeat(ui);
                        ui.add_space(7.0_f32 + gap_extra);
                        self.draw_position_group(ui);
                        ui.add_space(6.0_f32 + gap_extra);
                    }

                    // Reserved one-line status strip; its height never
                    // changes, so the rows below it stay anchored.
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 16.0_f32),
                        Sense::hover(),
                    );
                    // Ready-kind messages (session complete, defaults
                    // restored, …) are painted too — they used to be
                    // skipped entirely, which silently hid them. An empty
                    // status (fresh start) shows the theme's idle line so
                    // the strip is never a dead blank.
                    let shown: &str = if self.status_text.is_empty() {
                        "Ready."
                    } else {
                        &self.status_text
                    };
                    // Semantic state dot: shape + position carry the state,
                    // not color alone, and while a session runs the dot
                    // breathes on the already-running repaint heartbeat so
                    // "working" is visible even from the corner of the eye.
                    let state_color = self.status_kind.color(pal);
                    let now = ui.input(|i| i.time);
                    let dot_color = if self.status_kind == StatusKind::Running {
                        let pulse = 0.5_f32 + 0.5_f32 * ((now * 3.6_f64).sin() as f32);
                        Color32::from_rgba_unmultiplied(
                            state_color.r(),
                            state_color.g(),
                            state_color.b(),
                            140 + (pulse * 115.0_f32) as u8,
                        )
                    } else {
                        state_color
                    };
                    let painter = ui.painter_at(rect);
                    // Sunken well (dark themes only): a recessed readout
                    // plate one step below the panel, matching the input
                    // fields' depth level. It lives entirely inside the
                    // reserved 16 px strip, so the dot, the text and the
                    // rows below never move; Classic keeps the flat
                    // native status line.
                    if pal.dark {
                        painter.rect(
                            rect.shrink2(Vec2::new(0.0_f32, 1.0_f32)),
                            Rounding::same(3.0_f32),
                            pal.well_fill,
                            Stroke::new(1.0_f32, pal.group_border),
                        );
                    }
                    let dot_center = egui::pos2(rect.left() + 4.0_f32, rect.center().y);
                    // Kind-change acknowledgement: the dot swells once and
                    // a ring expands off it, easing out over 280 ms, so a
                    // state transition is noticed even without reading
                    // the text. It composes with the Running breathing
                    // (alpha above) instead of fighting it, and finishes
                    // exactly at the resting 3.5 px radius.
                    let pulse_t = Self::status_pulse_t(now, self.status_flash_at);
                    if let Some(t) = pulse_t {
                        let ease = 1.0_f32 - (1.0_f32 - t) * (1.0_f32 - t);
                        painter.circle_stroke(
                            dot_center,
                            3.5_f32 + 3.0_f32 * ease,
                            Stroke::new(
                                1.5_f32,
                                Color32::from_rgba_unmultiplied(
                                    state_color.r(),
                                    state_color.g(),
                                    state_color.b(),
                                    (200.0_f32 * (1.0_f32 - t)) as u8,
                                ),
                            ),
                        );
                    }
                    let dot_radius = pulse_t.map_or(3.5_f32, |t| 3.5_f32 + 1.3_f32 * (1.0_f32 - t));
                    painter.circle_filled(dot_center, dot_radius, dot_color);
                    painter.text(
                        egui::pos2(rect.left() + 12.0_f32, rect.center().y),
                        Align2::LEFT_CENTER,
                        shown,
                        FontId::proportional(12.5_f32),
                        state_color,
                    );

                    ui.add_space(2.0_f32);
                    self.draw_action_buttons(ui);
                    ui.add_space(5.0_f32);
                    // Status-bar style footer: key reminder on the left
                    // (tracking the real assigned hotkey, in the active
                    // theme's wording), Settings entry point on the right.
                    ui.horizontal(|ui| {
                        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
                        ui.label(
                            egui::RichText::new(format!(
                                "{hotkey_name} start/stop  ·  F8 emergency stop"
                            ))
                            .color(pal.text_dim)
                            .small(),
                        );
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().button_padding = Vec2::new(8.0_f32, 2.0_f32);
                            let button = egui::Button::new(
                                egui::RichText::new("Settings…")
                                    .size(12.0_f32)
                                    .color(pal.text),
                            )
                            .fill(pal.button)
                            .stroke(Stroke::new(1.0_f32, pal.button_border))
                            .rounding(Rounding::same(3.0_f32));
                            let settings_response = ui.add(button).on_hover_text(
                                "Theme, interface scale, clicking behaviour and the session log",
                            );
                            if settings_response.clicked() {
                                self.show_settings = true;
                            }
                        });
                    });
                });
            });

        if self.show_settings {
            self.draw_settings_window(ctx);
        }
        if self.show_hotkey_dialog {
            self.draw_hotkey_dialog(ctx);
        }
        if self.show_macro_window {
            self.draw_macro_window(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings_dirty = true;
        self.flush_settings();
    }
}
