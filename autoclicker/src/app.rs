//! OP Auto Clicker 4.1-style dashboard (classic Win32 look).
//!
//! The main window is a fixed-size, non-scrolling replica of the classic
//! OP Auto Clicker 4.x screen: four bordered group boxes (Click interval,
//! Click options, Click repeat, Cursor position) above a 2x2 grid of
//! standard push buttons. Power-user features (background window delivery,
//! Gaussian humanization and live telemetry) live in a separate Advanced
//! window so the main screen stays pixel-faithful to the reference.
//!
//! All worker interaction goes through atomics and the lock-free telemetry
//! store. The update thread never blocks on the worker.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Frame, Grid, Layout, Margin, Rounding, Sense, Stroke, Vec2};

use crate::config::{
    interval_parts_to_ms, ms_to_interval_parts, ClickType, CoordinateMode, EngineConfig, InputMode,
    MouseButton,
};
use crate::engine::{ClickEngine, STATE_PANIC};
use crate::hotkey::{
    self, is_reserved_panic_vk, is_supported_toggle_vk, vk_display_name, AppPrefs, VK_ESCAPE,
};
use crate::settings;
use crate::telemetry::{
    compute_stats, ClickRecord, TelemetryStats, GRAPH_HISTORY_SIZE, HISTOGRAM_BINS,
    SLIDING_WINDOW_LIMIT,
};
use crate::win32::{self, WindowInfo};

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

    // Background targeting (Advanced window).
    windows: Vec<WindowInfo>,
    selected_window: Option<usize>,

    // Telemetry aggregation.
    logs: Vec<String>,
    last_seen_error: Option<String>,
    sliding_window: Vec<ClickRecord>,
    cps_history: [f64; GRAPH_HISTORY_SIZE],
    cps_head: usize,
    cps_count: usize,
    stats: TelemetryStats,
    previous_elapsed_ms: f64,
    previous_clicks: u64,

    // Hotkey edge detection.
    toggle_was_down: bool,
    f8_was_down: bool,
    capture_key_was_down: bool,

    // Dialog state.
    show_hotkey_dialog: bool,
    capturing: bool,
    show_advanced: bool,

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
            windows: Vec::new(),
            selected_window: None,
            logs: vec![
                "OP Auto Clicker initialized. F6 starts/stops, F8 is the panic stop key."
                    .to_string(),
            ],
            last_seen_error: None,
            sliding_window: Vec::new(),
            cps_history: [0.0; GRAPH_HISTORY_SIZE],
            cps_head: 0,
            cps_count: 0,
            stats: TelemetryStats::default(),
            previous_elapsed_ms: 0.0,
            previous_clicks: 0,
            toggle_was_down: false,
            f8_was_down: false,
            capture_key_was_down: false,
            show_hotkey_dialog: false,
            capturing: false,
            show_advanced: false,
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
        app.last_saved_config = app.config.clone();
        app.last_saved_prefs = app.prefs;
        app
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
        if self.prefs.dark_mode {
            &DARK_PALETTE
        } else {
            &LIGHT_PALETTE
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

    fn drain_and_aggregate(&mut self) {
        let telemetry = self.engine.telemetry();
        telemetry.drain_into(&mut self.sliding_window);
        if self.sliding_window.len() > SLIDING_WINDOW_LIMIT {
            let drop_count = self.sliding_window.len() - SLIDING_WINDOW_LIMIT;
            self.sliding_window.drain(0..drop_count);
        }

        let elapsed = self.engine.elapsed_ms();
        self.stats = compute_stats(
            &self.sliding_window,
            self.engine.click_count(),
            self.engine.missed_count(),
            elapsed,
        );

        let delta_ms = elapsed - self.previous_elapsed_ms;
        let delta_clicks = self.engine.click_count() - self.previous_clicks;
        let mut instant_cps = 0.0;
        if delta_ms > 0.0 && delta_clicks > 0 {
            instant_cps = delta_clicks as f64 * 1000.0 / delta_ms;
        }
        self.previous_elapsed_ms = elapsed;
        self.previous_clicks = self.engine.click_count();
        if !self.sliding_window.is_empty() || self.engine.is_running() {
            self.cps_history[self.cps_head] = instant_cps;
            self.cps_head = (self.cps_head + 1) % self.cps_history.len();
            self.cps_count = (self.cps_count + 1).min(self.cps_history.len());
        }
    }

    fn toggle_session(&mut self) {
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
        if self.config.input_mode == InputMode::BackgroundHwnd {
            let Some(index) = self.selected_window else {
                self.set_status(
                    StatusKind::Warning,
                    "Select a target window in Advanced first.",
                );
                self.log("Start rejected: select a target window first.");
                return;
            };
            let Some(info) = self.windows.get(index) else {
                self.set_status(
                    StatusKind::Warning,
                    "Target list is stale; press Refresh in Advanced.",
                );
                self.log("Start rejected: target list is stale, refresh it.");
                return;
            };
            if !win32::is_valid_window(info.hwnd) {
                self.set_status(
                    StatusKind::Warning,
                    "Selected window no longer exists; pick another.",
                );
                self.log("Start rejected: selected window handle is invalid.");
                return;
            }
            self.config.target_hwnd = info.hwnd;
        } else {
            self.config.target_hwnd = 0;
        }
        match self.engine.start(&self.config) {
            Ok(()) => {
                self.cps_head = 0;
                self.cps_count = 0;
                self.cps_history.fill(0.0);
                self.sliding_window.clear();
                self.previous_elapsed_ms = 0.0;
                self.previous_clicks = 0;
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
        self.repeat_limited = false;
        self.offset_ms = 0.0;
        self.selected_window = None;
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
        let galley = painter.layout_no_wrap(title.to_string(), font.clone(), pal.text);
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
        Self::group_box(ui, pal, "Click interval", |ui| {
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
        ui.columns(2, |columns| {
            Self::group_box(&mut columns[0], pal, "Click options", |ui| {
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

            Self::group_box(&mut columns[1], pal, "Click repeat", |ui| {
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
        Self::group_box(ui, pal, "Cursor position", |ui| {
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
                    &format!("Start ({hotkey_name})"),
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
                    &format!("Stop ({hotkey_name})"),
                    running,
                    button_width,
                )
                .clicked()
                {
                    self.engine.request_stop();
                    self.set_status(StatusKind::Stopping, "Stopping…");
                    self.log("Graceful stop requested.");
                    self.mark_dirty();
                    self.flush_settings();
                }
                ui.end_row();

                if Self::classic_button(ui, pal, "Hotkey setting", true, button_width).clicked() {
                    self.show_hotkey_dialog = true;
                    self.capturing = false;
                }
                if Self::classic_button(ui, pal, "Advanced…", true, button_width).clicked() {
                    self.show_advanced = true;
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

    // ------------------------------------------------------------ advanced UI

    fn draw_advanced_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_advanced;
        egui::Window::new("Advanced — delivery, humanization & telemetry")
            .open(&mut open)
            .default_size(Vec2::new(520.0_f32, 620.0_f32))
            .min_width(440.0_f32)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_delivery_section(ui);
                    ui.add_space(8.0_f32);
                    self.draw_humanization_section(ui);
                    ui.add_space(8.0_f32);
                    self.draw_telemetry_section(ui);
                    ui.add_space(8.0_f32);
                    if ui
                        .button("Reset all settings to factory defaults")
                        .clicked()
                    {
                        self.reset_to_defaults();
                    }
                });
            });
        if !open {
            self.show_advanced = false;
        }
    }

    fn draw_delivery_section(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        Self::group_box(ui, pal, "Input delivery & targeting", |ui| {
            ui.radio_value(
                &mut self.config.input_mode,
                InputMode::GlobalSendInput,
                "Global (SendInput) — works in any application",
            );
            ui.radio_value(
                &mut self.config.input_mode,
                InputMode::BackgroundHwnd,
                "Background (posted window messages)",
            );
            if self.config.input_mode == InputMode::BackgroundHwnd {
                ui.add_space(4.0_f32);
                ui.horizontal(|ui| {
                    let selected = self
                        .selected_window
                        .and_then(|i| self.windows.get(i))
                        .map(|w| format!("{} [PID {}]", w.title, w.process_id))
                        .unwrap_or_else(|| "No window selected".to_string());
                    egui::ComboBox::from_id_source("target_combo")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if self.windows.is_empty() {
                                ui.label(
                                    egui::RichText::new("List is empty — press Refresh")
                                        .color(pal.text_dim),
                                );
                            }
                            for (index, info) in self.windows.iter().enumerate() {
                                let label = format!(
                                    "{} [PID {}]{}",
                                    info.title,
                                    info.process_id,
                                    if info.minimized { " (minimized)" } else { "" }
                                );
                                if ui
                                    .selectable_label(self.selected_window == Some(index), label)
                                    .clicked()
                                {
                                    // The per-frame fingerprint check flushes
                                    // this change to disk; no mutable call can
                                    // live inside the immutable window iter.
                                    self.selected_window = Some(index);
                                    self.config.target_hwnd = info.hwnd;
                                }
                            }
                        });
                    if ui.button("Refresh").clicked() {
                        self.windows = win32::enumerate_windows();
                        if self.selected_window.unwrap_or(0) >= self.windows.len() {
                            self.selected_window = None;
                            self.config.target_hwnd = 0;
                        }
                        self.log(format!(
                            "Target window list refreshed: {} candidates.",
                            self.windows.len()
                        ));
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [110.0_f32, 22.0_f32],
                        egui::DragValue::new(&mut self.config.client_x)
                            .clamp_range(0..=100_000)
                            .prefix("Client X: "),
                    );
                    ui.add_sized(
                        [110.0_f32, 22.0_f32],
                        egui::DragValue::new(&mut self.config.client_y)
                            .clamp_range(0..=100_000)
                            .prefix("Client Y: "),
                    );
                });
                ui.checkbox(
                    &mut self.config.background_synchronous,
                    "Synchronous delivery (SendMessageTimeoutW)",
                );
            }
        });
    }

    fn draw_humanization_section(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        Self::group_box(ui, pal, "Gaussian humanization (expert)", |ui| {
            ui.checkbox(
                &mut self.config.humanize_enabled,
                "Enable Gaussian humanization",
            );
            ui.collapsing("Advanced timing & humanization fields", |ui| {
                ui.add_space(3.0_f32);
                let fields: [(&str, &mut f64, f64, f64); 9] = [
                    (
                        "Base interval (ms)",
                        &mut self.config.base_interval_ms,
                        0.05,
                        10_000.0,
                    ),
                    (
                        "Interval sigma (ms)",
                        &mut self.config.sigma_ms,
                        0.0,
                        1_000.0,
                    ),
                    (
                        "Interval sigma (%)",
                        &mut self.config.sigma_percent,
                        0.0,
                        100.0,
                    ),
                    (
                        "Min interval (ms)",
                        &mut self.config.min_interval_ms,
                        0.05,
                        10_000.0,
                    ),
                    (
                        "Max interval (ms)",
                        &mut self.config.max_interval_ms,
                        1.0,
                        60_000.0,
                    ),
                    (
                        "Hold mean (ms)",
                        &mut self.config.hold_mean_ms,
                        0.0,
                        5_000.0,
                    ),
                    (
                        "Hold sigma (ms)",
                        &mut self.config.hold_sigma_ms,
                        0.0,
                        1_000.0,
                    ),
                    ("Hold min (ms)", &mut self.config.hold_min_ms, 0.0, 5_000.0),
                    ("Hold max (ms)", &mut self.config.hold_max_ms, 1.0, 10_000.0),
                ];
                for (label, value, low, high) in fields {
                    ui.add(
                        egui::DragValue::new(value)
                            .speed(0.5)
                            .clamp_range(low..=high)
                            .fixed_decimals(4)
                            .prefix(format!("{label}: ")),
                    );
                }
                ui.add_space(6.0_f32);
                match self.config.validate() {
                    Ok(()) => ui.label(
                        egui::RichText::new(format!(
                            "Valid. Effective sigma: {:.3} ms",
                            self.config.effective_interval_sigma_ms()
                        ))
                        .color(pal.success)
                        .small(),
                    ),
                    Err(error) => ui.label(
                        egui::RichText::new(format!("Configuration issue: {error}"))
                            .color(pal.warning)
                            .small(),
                    ),
                }
            });
        });
    }

    fn draw_telemetry_section(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        Self::group_box(ui, pal, "Live metrics", |ui| {
            Grid::new("metrics_grid")
                .num_columns(2)
                .spacing(Vec2::new(18.0_f32, 8.0_f32))
                .show(ui, |ui| {
                    let metric = |ui: &mut egui::Ui, label: &str, value: String| {
                        ui.label(egui::RichText::new(label).color(pal.text_dim));
                        ui.label(egui::RichText::new(value).color(pal.text));
                        ui.end_row();
                    };
                    metric(ui, "Actual CPS", format!("{:.3}", self.stats.actual_cps));
                    metric(
                        ui,
                        "Interval mean",
                        format!("{:.4} ms", self.stats.mean_interval_ms),
                    );
                    metric(
                        ui,
                        "Interval sigma",
                        format!("{:.4} ms", self.stats.sigma_interval_ms),
                    );
                    metric(
                        ui,
                        "Consistency (+/-10%)",
                        format!("{:.2} %", self.stats.consistency_pct),
                    );
                    metric(
                        ui,
                        "Missed deadlines",
                        self.stats.missed_deadlines.to_string(),
                    );
                    metric(
                        ui,
                        "Ring buffer dropped",
                        self.engine.telemetry().dropped_records().to_string(),
                    );
                });
        });
        ui.add_space(8.0_f32);
        Self::group_box(ui, pal, "Real-time CPS", |ui| self.draw_cps_graph(ui));
        ui.add_space(8.0_f32);
        Self::group_box(ui, pal, "Interval histogram (ms)", |ui| {
            self.draw_histogram(ui)
        });
        ui.add_space(8.0_f32);
        Self::group_box(ui, pal, "Recent activity", |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(170.0_f32)
                .show(ui, |ui| {
                    for line in &self.logs {
                        let warning = line.contains("PANIC")
                            || line.contains("rejected")
                            || line.contains("error")
                            || line.contains("Warning")
                            || line.contains("failed");
                        let color = if warning { pal.warning } else { pal.text_dim };
                        ui.label(egui::RichText::new(line).color(color).small());
                    }
                });
        });
    }

    fn draw_cps_graph(&self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0_f32), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0_f32, pal.chart_bg);
        painter.rect_stroke(rect, 6.0_f32, Stroke::new(1.0_f32, pal.chart_grid));

        for step in 1..4 {
            let y = rect.top() + rect.height() * step as f32 / 4.0_f32;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 8.0_f32, y),
                    egui::pos2(rect.right() - 8.0_f32, y),
                ],
                Stroke::new(1.0_f32, pal.chart_grid),
            );
        }

        let mut max_cps = (self.config.target_cps() * 1.25).max(1.0);
        for sample in self.cps_history {
            max_cps = max_cps.max(sample);
        }

        if self.cps_count > 1 {
            let mut previous = egui::Pos2::ZERO;
            for i in 0..self.cps_count {
                let index = (self.cps_head + self.cps_history.len() - self.cps_count + i)
                    % self.cps_history.len();
                let x = rect.left() + rect.width() * i as f32 / (self.cps_history.len() - 1) as f32;
                let normalized = (self.cps_history[index] / max_cps).clamp(0.0, 1.0) as f32;
                let y = rect.bottom() - 10.0_f32 - normalized * (rect.height() - 22.0_f32);
                let current = egui::pos2(x, y);
                if i > 0 {
                    painter.line_segment([previous, current], Stroke::new(2.0_f32, pal.accent));
                }
                previous = current;
            }
        }
    }

    fn draw_histogram(&self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0_f32), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0_f32, pal.chart_bg);
        painter.rect_stroke(rect, 6.0_f32, Stroke::new(1.0_f32, pal.chart_grid));

        let max_bin = self
            .stats
            .histogram
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .max(1) as f32;
        let gap = 3.0_f32;
        let bar_width = rect.width() / HISTOGRAM_BINS as f32 - gap;
        for bin in 0..HISTOGRAM_BINS {
            let height = rect.height() * self.stats.histogram[bin] as f32 / max_bin * 0.78_f32;
            let x = rect.left() + 8.0_f32 + bin as f32 * (bar_width + gap);
            let color = if bin % 2 == 0 {
                pal.accent
            } else {
                pal.chart_bar_alt
            };
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x, rect.bottom() - height - 10.0_f32),
                    Vec2::new(bar_width, height),
                ),
                3.0_f32,
                color,
            );
        }
    }
}

impl eframe::App for OpAutoClickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_style(ctx);
        let pal = self.palette();
        self.poll_hotkeys();
        self.poll_pick_capture();
        self.surface_engine_errors();
        if self.engine.is_running() {
            if let Err(error) = self.engine.update_config(&self.config) {
                if self.last_seen_error.as_deref() != Some(error.as_str()) {
                    self.log(format!("Engine error: {error}"));
                    self.set_status(StatusKind::Error, format!("Error: {error}"));
                    self.last_seen_error = Some(error);
                }
            }
        }
        self.drain_and_aggregate();

        // Detect natural session completion (repeat count reached) so the
        // status line and persistence react even without a button press.
        // Manual stops and worker errors keep their own status instead of
        // being misreported as a clean completion.
        if self.was_running && !self.engine.is_running() {
            let count_reached = self.repeat_limited
                && self.config.max_clicks > 0
                && self.engine.click_count() >= self.config.max_clicks;
            if count_reached && self.engine.state() != STATE_PANIC {
                self.set_status(
                    StatusKind::Ready,
                    format!(
                        "Session complete: {} clicks delivered.",
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
        if self.engine.is_running() || self.capturing || self.picking_location {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

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
                    if self.status_kind != StatusKind::Ready {
                        ui.painter_at(rect).text(
                            rect.left_center(),
                            Align2::LEFT_CENTER,
                            &self.status_text,
                            FontId::proportional(12.5_f32),
                            self.status_kind.color(pal),
                        );
                    }

                    ui.add_space(2.0_f32);
                    self.draw_action_buttons(ui);
                    ui.add_space(5.0_f32);
                    // Status-bar style footer: key reminder on the left
                    // (tracking the real assigned hotkey), theme switch on
                    // the right.
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
                            let mut dark = self.prefs.dark_mode;
                            if ui
                                .checkbox(
                                    &mut dark,
                                    egui::RichText::new("Dark mode").size(12.0_f32),
                                )
                                .changed()
                            {
                                self.prefs.dark_mode = dark;
                                self.mark_dirty();
                            }
                        });
                    });
                });
            });

        if self.show_hotkey_dialog {
            self.draw_hotkey_dialog(ctx);
        }
        if self.show_advanced {
            self.draw_advanced_window(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings_dirty = true;
        self.flush_settings();
    }
}
