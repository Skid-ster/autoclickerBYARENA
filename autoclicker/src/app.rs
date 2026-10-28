//! MacroClicker — an OP Auto Clicker 4.1-style dashboard (classic Win32 look).
//!
//! The main window is a fixed-size, non-scrolling replica of the classic
//! OP Auto Clicker 4.x screen: four bordered group boxes (Click interval,
//! Click options, Click repeat, Cursor position) above the authentic 2x2
//! grid of standard push buttons. There is no diagnostics surface: every
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
use crate::icon;
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
    group_border: Color32,
    input_border: Color32,
    text: Color32,
    /// Color of the group-box caption riding the top border. Equal to
    /// `text` in most themes; MogClicker uses its hot-pink accent as a
    /// visual flourish.
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
    group_border: Color32::from_rgb(166, 166, 166),
    input_border: Color32::from_rgb(122, 122, 122),
    text: Color32::from_rgb(28, 28, 28),
    group_title: Color32::from_rgb(28, 28, 28),
    text_dim: Color32::from_rgb(102, 102, 102),
    text_disabled: Color32::from_rgb(166, 166, 166),
    accent: Color32::from_rgb(0, 103, 192),
    success: Color32::from_rgb(16, 124, 16),
    danger: Color32::from_rgb(196, 43, 28),
    warning: Color32::from_rgb(154, 107, 0),
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
    button_border: Color32::from_rgb(96, 96, 96),
    hover_fill: Color32::from_rgb(58, 62, 66),
    pressed_fill: Color32::from_rgb(70, 76, 82),
    input_bg: Color32::from_rgb(44, 44, 44),
    input_disabled: Color32::from_rgb(37, 37, 37),
    group_border: Color32::from_rgb(96, 96, 96),
    input_border: Color32::from_rgb(130, 130, 130),
    text: Color32::from_rgb(235, 235, 235),
    group_title: Color32::from_rgb(235, 235, 235),
    text_dim: Color32::from_rgb(158, 158, 158),
    text_disabled: Color32::from_rgb(109, 109, 109),
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
    button_border: Color32::from_rgb(70, 70, 70),
    hover_fill: Color32::from_rgb(32, 36, 40),
    pressed_fill: Color32::from_rgb(44, 50, 56),
    input_bg: Color32::from_rgb(18, 18, 18),
    input_disabled: Color32::from_rgb(10, 10, 10),
    group_border: Color32::from_rgb(74, 74, 74),
    input_border: Color32::from_rgb(110, 110, 110),
    text: Color32::from_rgb(230, 230, 230),
    group_title: Color32::from_rgb(230, 230, 230),
    text_dim: Color32::from_rgb(150, 150, 150),
    text_disabled: Color32::from_rgb(96, 96, 96),
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
    button_border: Color32::from_rgb(38, 92, 38),
    hover_fill: Color32::from_rgb(22, 44, 22),
    pressed_fill: Color32::from_rgb(30, 60, 30),
    input_bg: Color32::from_rgb(12, 24, 12),
    input_disabled: Color32::from_rgb(8, 16, 8),
    group_border: Color32::from_rgb(42, 100, 42),
    input_border: Color32::from_rgb(58, 136, 58),
    text: Color32::from_rgb(130, 235, 130),
    group_title: Color32::from_rgb(130, 235, 130),
    text_dim: Color32::from_rgb(74, 150, 74),
    text_disabled: Color32::from_rgb(46, 92, 46),
    accent: Color32::from_rgb(0, 230, 118),
    success: Color32::from_rgb(0, 230, 118),
    danger: Color32::from_rgb(255, 90, 90),
    warning: Color32::from_rgb(210, 200, 70),
    chart_bg: Color32::from_rgb(5, 10, 5),
    chart_grid: Color32::from_rgb(22, 48, 22),
    chart_bar_alt: Color32::from_rgba_premultiplied(0, 92, 48, 110),
    selection_bg: Color32::from_rgba_premultiplied(0, 70, 36, 150),
};

// Pastel pink/purple light look.
static VAPORWAVE_PALETTE: Palette = Palette {
    dark: false,
    face: Color32::from_rgb(244, 230, 250),
    button: Color32::from_rgb(238, 218, 246),
    button_disabled: Color32::from_rgb(246, 238, 250),
    button_border: Color32::from_rgb(196, 156, 220),
    hover_fill: Color32::from_rgb(250, 224, 244),
    pressed_fill: Color32::from_rgb(252, 204, 232),
    input_bg: Color32::from_rgb(253, 248, 255),
    input_disabled: Color32::from_rgb(246, 240, 250),
    group_border: Color32::from_rgb(182, 142, 208),
    input_border: Color32::from_rgb(150, 100, 180),
    text: Color32::from_rgb(58, 30, 78),
    group_title: Color32::from_rgb(58, 30, 78),
    text_dim: Color32::from_rgb(142, 102, 164),
    text_disabled: Color32::from_rgb(192, 170, 206),
    accent: Color32::from_rgb(232, 62, 176),
    success: Color32::from_rgb(0, 168, 150),
    danger: Color32::from_rgb(226, 64, 84),
    warning: Color32::from_rgb(196, 118, 0),
    chart_bg: Color32::from_rgb(252, 245, 255),
    chart_grid: Color32::from_rgb(230, 210, 240),
    chart_bar_alt: Color32::from_rgba_premultiplied(160, 44, 122, 110),
    selection_bg: Color32::from_rgba_premultiplied(120, 32, 92, 60),
};

// The MogClicker meme theme: deep purple face, hot-pink accent. Pairs with
// the re-skinned labels in MOG_TEXT below.
static MOG_PALETTE: Palette = Palette {
    dark: true,
    face: Color32::from_rgb(24, 16, 32),
    button: Color32::from_rgb(44, 30, 60),
    button_disabled: Color32::from_rgb(30, 20, 40),
    button_border: Color32::from_rgb(104, 70, 138),
    hover_fill: Color32::from_rgb(54, 36, 72),
    pressed_fill: Color32::from_rgb(70, 46, 94),
    input_bg: Color32::from_rgb(36, 24, 48),
    input_disabled: Color32::from_rgb(28, 19, 38),
    group_border: Color32::from_rgb(110, 70, 150),
    input_border: Color32::from_rgb(150, 100, 190),
    text: Color32::from_rgb(240, 230, 250),
    group_title: Color32::from_rgb(255, 46, 136),
    text_dim: Color32::from_rgb(174, 150, 196),
    text_disabled: Color32::from_rgb(112, 90, 134),
    accent: Color32::from_rgb(255, 46, 136),
    success: Color32::from_rgb(90, 240, 140),
    danger: Color32::from_rgb(255, 92, 92),
    warning: Color32::from_rgb(236, 196, 70),
    chart_bg: Color32::from_rgb(18, 12, 26),
    chart_grid: Color32::from_rgb(58, 40, 78),
    chart_bar_alt: Color32::from_rgba_premultiplied(124, 28, 70, 110),
    selection_bg: Color32::from_rgba_premultiplied(96, 20, 54, 150),
};

/// The handful of strings a theme is allowed to re-skin. Every variant
/// keeps similar lengths so the fixed layout always fits (proven by the
/// headless layout probe across all themes).
struct ThemeText {
    window_title: &'static str,
    group_interval: &'static str,
    group_options: &'static str,
    group_repeat: &'static str,
    group_position: &'static str,
    /// Start-button verb, completed as "{start_verb} ({hotkey})".
    start_verb: &'static str,
    /// Stop-button verb, completed as "{stop_verb} ({hotkey})".
    stop_verb: &'static str,
    /// Footer tail after the hotkey name, e.g. "{hotkey} start/stop".
    footer_toggle: &'static str,
    /// Footer right half, describing the F8 panic key.
    footer_panic: &'static str,
    /// Verb opening the Running status line.
    running_verb: &'static str,
    /// Flavor suffix appended to the Running status line.
    running_suffix: &'static str,
    /// Status shown while a stop request is draining.
    stopping_line: &'static str,
    /// Prefix of the natural-completion status line.
    complete_prefix: &'static str,
    /// Prefix of the session-started log entry.
    log_started_prefix: &'static str,
    /// Idle line shown in the status strip before anything has happened.
    status_idle: &'static str,
}

static CLASSIC_TEXT: ThemeText = ThemeText {
    window_title: "MacroClicker",
    group_interval: "Click interval",
    group_options: "Click options",
    group_repeat: "Click repeat",
    group_position: "Cursor position",
    start_verb: "Start",
    stop_verb: "Stop",
    footer_toggle: "start/stop",
    footer_panic: "F8 emergency stop",
    running_verb: "Running",
    running_suffix: "",
    stopping_line: "Stopping…",
    complete_prefix: "Session complete",
    log_started_prefix: "Click session started",
    status_idle: "Ready.",
};

static MOG_TEXT: ThemeText = ThemeText {
    window_title: "MogClicker",
    group_interval: "Mogging interval",
    group_options: "Mog options",
    group_repeat: "Mog repeat",
    group_position: "Aura position",
    start_verb: "Mog",
    stop_verb: "It's over",
    footer_toggle: "to mog",
    footer_panic: "F8 when it's over",
    running_verb: "Mogging",
    running_suffix: " +1000 aura per click.",
    stopping_line: "Aura cooling down…",
    complete_prefix: "Mogging complete — everyone brutally frame-mogged",
    log_started_prefix: "Mogging session started — looksmaxxing",
    status_idle: "Ready to mog. Aura fully charged.",
};

/// Logical (point) size of the fixed main window; the Settings window's
/// scale option zooms the UI and re-applies this size so the physical
/// window grows or shrinks with the chosen percentage.
pub const MAIN_WINDOW_SIZE: [f32; 2] = [560.0, 456.0];

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

    // Window-chrome bookkeeping: retitle and re-icon on theme change,
    // resize once the zoom factor for the chosen scale has been applied.
    applied_window_title: &'static str,
    applied_mog_icon: bool,
    pending_resize: bool,

    // Persistence. The "last saved" fingerprints make the dirty check exact
    // so the disk is never written while nothing changed.
    settings_path: Option<PathBuf>,
    settings_dirty: bool,
    last_save: Instant,
    last_saved_config: EngineConfig,
    last_saved_prefs: AppPrefs,
    was_running: bool,

    status_kind: StatusKind,
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
            applied_window_title: "",
            applied_mog_icon: false,
            pending_resize: false,
            settings_path: Self::settings_file_path(),
            settings_dirty: false,
            last_save: Instant::now(),
            last_saved_config: EngineConfig::default(),
            last_saved_prefs: AppPrefs::default(),
            was_running: false,
            status_kind: StatusKind::Ready,
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
        self.status_kind = kind;
        self.status_text = text.into();
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
            AppTheme::Vaporwave => &VAPORWAVE_PALETTE,
            AppTheme::Mog => &MOG_PALETTE,
        }
    }

    /// The re-skinnable strings for the active theme. Only MogClicker
    /// deviates from the classic wording.
    fn theme_text(&self) -> &'static ThemeText {
        match self.prefs.theme {
            AppTheme::Mog => &MOG_TEXT,
            _ => &CLASSIC_TEXT,
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
        style.visuals.selection.stroke = Stroke::new(1.0_f32, pal.text);
        style.visuals.hyperlink_color = pal.accent;
        let rounding = Rounding::same(2.5_f32);
        style.visuals.widgets.noninteractive.rounding = rounding;
        style.visuals.widgets.inactive.rounding = rounding;
        style.visuals.widgets.hovered.rounding = rounding;
        style.visuals.widgets.active.rounding = rounding;
        style.visuals.widgets.open.rounding = rounding;
        style.spacing.item_spacing = Vec2::new(7.0_f32, 6.0_f32);
        style.spacing.button_padding = Vec2::new(10.0_f32, 5.0_f32);
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
            self.set_status(StatusKind::Stopping, self.theme_text().stopping_line);
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
                let text = self.theme_text();
                self.set_status(
                    StatusKind::Running,
                    format!(
                        "{} at {:.2} CPS — {} button, {} click.{}",
                        text.running_verb,
                        self.config.target_cps(),
                        self.config.mouse_button.label().to_lowercase(),
                        self.config.click_type.label().to_lowercase(),
                        text.running_suffix
                    ),
                );
                self.log(format!(
                    "{} at {:.2} CPS.",
                    text.log_started_prefix,
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

    // --------------------------------------------------------- classic widgets

    /// Win32 group box: a bordered frame whose title text breaks the top
    /// border, exactly like the native BS_GROUPBOX control.
    fn group_box<R>(
        ui: &mut egui::Ui,
        pal: &Palette,
        title: &str,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let response = Frame::none()
            .fill(pal.face)
            .stroke(Stroke::new(1.0_f32, pal.group_border))
            .rounding(Rounding::same(2.0_f32))
            .inner_margin(Margin {
                left: 11.0_f32,
                right: 11.0_f32,
                top: 17.0_f32,
                bottom: 11.0_f32,
            })
            .show(ui, add);
        let rect = response.response.rect;
        // The title rides the top border, half of it above rect.top().
        // `painter_at` clips to the given rect, so painting through a clip
        // region expanded upward is what keeps the glyphs whole; the frame's
        // own stroke was already painted by `Frame::show` above.
        let painter = ui.painter_at(rect.expand2(Vec2::new(12.0_f32, 16.0_f32)));
        let font = FontId::proportional(13.0_f32);
        let galley = painter.layout_no_wrap(title.to_string(), font.clone(), pal.group_title);
        let text_pos = rect.left_top() + Vec2::new(9.0_f32, -galley.size().y / 2.0_f32);
        // The face-colored backdrop must cover the 1 px border line entirely
        // so the title cleanly breaks it like a native BS_GROUPBOX caption.
        let backdrop = egui::Rect::from_min_size(
            text_pos - Vec2::new(4.0_f32, 1.0_f32),
            galley.size() + Vec2::new(8.0_f32, 2.0_f32),
        );
        painter.rect_filled(backdrop, 0.0_f32, pal.face);
        painter.text(text_pos, Align2::LEFT_TOP, title, font, pal.text);
        response.inner
    }

    fn classic_button(
        ui: &mut egui::Ui,
        pal: &Palette,
        text: &str,
        enabled: bool,
        width: f32,
    ) -> egui::Response {
        let text_color = if enabled { pal.text } else { pal.text_disabled };
        let button = egui::Button::new(egui::RichText::new(text).color(text_color).size(13.5_f32))
            .fill(if enabled {
                pal.button
            } else {
                pal.button_disabled
            })
            .stroke(Stroke::new(1.0_f32, pal.button_border))
            .min_size(Vec2::new(width, 31.0_f32));
        ui.add_enabled(enabled, button)
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
        let text = self.theme_text();
        Self::group_box(ui, pal, text.group_interval, |ui| {
            ui.horizontal(|ui| {
                let fields: [(&str, &mut u64, u64); 4] = [
                    ("hours", &mut self.hours, 99),
                    ("mins", &mut self.minutes, 59),
                    ("secs", &mut self.seconds, 59),
                    ("milliseconds", &mut self.millis, 999),
                ];
                for (label, value, max) in fields {
                    Self::numeric_field(ui, value, max, 56.0_f32);
                    ui.label(egui::RichText::new(label).color(pal.text));
                    ui.add_space(2.0_f32);
                }
            });
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

    fn draw_options_and_repeat(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let text = self.theme_text();
        ui.columns(2, |columns| {
            Self::group_box(&mut columns[0], pal, text.group_options, |ui| {
                Grid::new("click_options_grid")
                    .num_columns(2)
                    .spacing(Vec2::new(8.0_f32, 9.0_f32))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Mouse button:").color(pal.text));
                        egui::ComboBox::from_id_source("mouse_button_combo")
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
                            .selected_text(self.config.click_type.label())
                            .show_ui(ui, |ui| {
                                for kind in ClickType::all() {
                                    ui.selectable_value(
                                        &mut self.config.click_type,
                                        kind,
                                        kind.label(),
                                    );
                                }
                            });
                        ui.end_row();
                    });
            });

            Self::group_box(&mut columns[1], pal, text.group_repeat, |ui| {
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
            });
        });
    }

    fn draw_position_group(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let text = self.theme_text();
        Self::group_box(ui, pal, text.group_position, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.config.coordinate_mode,
                    CoordinateMode::CurrentCursor,
                    "Current location",
                );
            });
            ui.add_space(4.0_f32);
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.config.coordinate_mode,
                    CoordinateMode::FixedScreen,
                    "Pick location",
                );
                let pick_label = if self.picking_location {
                    "Cancel pick"
                } else {
                    "Pick location"
                };
                if Self::classic_button(ui, pal, pick_label, true, 104.0_f32).clicked() {
                    if self.picking_location {
                        self.picking_location = false;
                        self.set_status(StatusKind::Ready, "Location pick cancelled.");
                    } else {
                        self.arm_location_pick();
                    }
                }
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
            });
        });
    }

    fn draw_action_buttons(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let text = self.theme_text();
        let running = self.engine.is_running();
        let config_ok = self.validation_message().is_none();
        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
        let gap = ui.spacing().item_spacing.x;
        let button_width = (ui.available_width() - gap) / 2.0;

        Grid::new("action_button_grid")
            .num_columns(2)
            .spacing(Vec2::new(gap, 6.0_f32))
            .show(ui, |ui| {
                if Self::classic_button(
                    ui,
                    pal,
                    &format!("{} ({hotkey_name})", text.start_verb),
                    !running && config_ok,
                    button_width,
                )
                .clicked()
                {
                    self.handle_start();
                }
                if Self::classic_button(
                    ui,
                    pal,
                    &format!("{} ({hotkey_name})", text.stop_verb),
                    running,
                    button_width,
                )
                .clicked()
                {
                    self.engine.request_stop();
                    self.set_status(StatusKind::Stopping, self.theme_text().stopping_line);
                    self.log("Graceful stop requested.");
                    self.mark_dirty();
                    self.flush_settings();
                }
                ui.end_row();

                if Self::classic_button(ui, pal, "Hotkey setting", true, button_width).clicked() {
                    self.show_hotkey_dialog = true;
                    self.capturing = false;
                }
                if Self::classic_button(ui, pal, "Record & Playback", true, button_width).clicked()
                {
                    self.show_macro_window = true;
                }
                ui.end_row();
            });
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
                        "MogClicker is the meme theme: certified aura mogger. Every other app on your taskbar gets brutally frame-mogged.",
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
                        "The window resizes itself so every control stays visible.",
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
        self.apply_style(ctx);
        let pal = self.palette();

        // Keep the OS window title in sync with the theme ("MogClicker"
        // while the meme theme is active). Sent only when it changes.
        let title = self.theme_text().window_title;
        if self.applied_window_title != title {
            self.applied_window_title = title;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
        }

        // The taskbar mogs along: swap the procedural window icon to the
        // hot-pink Mog color way while the meme theme is active (and back).
        // Sent only when the theme family actually changes; the default
        // icon set at startup covers the initial non-Mog state.
        let want_mog_icon = self.prefs.theme == AppTheme::Mog;
        if self.applied_mog_icon != want_mog_icon {
            self.applied_mog_icon = want_mog_icon;
            let (rgba, width, height) = if want_mog_icon {
                icon::mog_icon_rgba()
            } else {
                icon::app_icon_rgba()
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
                egui::IconData {
                    rgba,
                    width,
                    height,
                },
            ))));
        }

        // UI scale: the zoom factor follows the persisted percentage; one
        // frame later (once egui reports the new factor as applied) the OS
        // window is re-sized to the same logical size so the physical
        // window actually grows or shrinks and no control is cut off.
        let desired_zoom = self.prefs.ui_scale_percent as f32 / 100.0_f32;
        if (ctx.zoom_factor() - desired_zoom).abs() > 0.001_f32 {
            ctx.set_zoom_factor(desired_zoom);
            self.pending_resize = true;
            ctx.request_repaint();
        } else if self.pending_resize {
            self.pending_resize = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                MAIN_WINDOW_SIZE[0],
                MAIN_WINDOW_SIZE[1],
            )));
            ctx.request_repaint();
        }

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
        // Detect natural session completion (repeat count reached) so the
        // status line and persistence react even without a button press.
        // Manual stops and worker errors keep their own status instead of
        // being misreported as a clean completion.
        let was_macro_playback = self.macro_playing;
        if self.was_running && !self.engine.is_running() && was_macro_playback {
            self.macro_playing = false;
            if self.engine.state() != STATE_PANIC {
                self.set_status(
                    StatusKind::Ready,
                    format!(
                        "Playback complete: {} clicks delivered.",
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
                        "{}: {} clicks delivered.",
                        self.theme_text().complete_prefix,
                        self.engine.click_count()
                    ),
                );
                self.log(format!(
                    "Session finished after {} clicks.",
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
        {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(50)
        };
        ctx.request_repaint_after(heartbeat);

        // Single fixed-size screen: no ScrollArea, everything fits. The top
        // margin leaves room for the first group title, which rides half
        // above its frame's top border.
        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal.face).inner_margin(Margin {
                left: 12.0_f32,
                right: 12.0_f32,
                top: 14.0_f32,
                bottom: 10.0_f32,
            }))
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(egui::Align::LEFT), |ui| {
                    self.draw_interval_group(ui);
                    ui.add_space(7.0_f32);
                    self.draw_options_and_repeat(ui);
                    ui.add_space(7.0_f32);
                    self.draw_position_group(ui);
                    ui.add_space(6.0_f32);

                    // Reserved one-line status strip keeps the layout fixed.
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
                        self.theme_text().status_idle
                    } else {
                        &self.status_text
                    };
                    ui.painter_at(rect).text(
                        rect.left_center(),
                        Align2::LEFT_CENTER,
                        shown,
                        FontId::proportional(12.5_f32),
                        self.status_kind.color(pal),
                    );

                    ui.add_space(2.0_f32);
                    self.draw_action_buttons(ui);
                    ui.add_space(5.0_f32);
                    // Status-bar style footer: key reminder on the left
                    // (tracking the real assigned hotkey, in the active
                    // theme's wording), Settings entry point on the right.
                    ui.horizontal(|ui| {
                        let text = self.theme_text();
                        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
                        ui.label(
                            egui::RichText::new(format!(
                                "{hotkey_name} {}  ·  {}",
                                text.footer_toggle, text.footer_panic
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
                            if ui.add(button).clicked() {
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
