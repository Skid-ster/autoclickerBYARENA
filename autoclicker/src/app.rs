//! MacroClicker — a resizable OP Auto Clicker 4.1-style dashboard with
//! Classic and Modern appearances, each in Light or Dark.
//!
//! The main window is a naturally resizable, non-scrolling rendition of
//! the classic OP Auto Clicker 4.x controls: four group cards (Click
//! interval, Click options, Click repeat, Cursor position) above the
//! authentic 2x2 grid of push buttons. The classic 560x456 size is the
//! enforced minimum; larger windows scale and reflow the dashboard to
//! fit, using stacked portrait or three-abreast panoramic compositions.
//! There is no diagnostics surface: every control on screen
//! configures clicking, and the engine's telemetry stays an internal,
//! test-verified concern.
//!
//! All worker interaction goes through atomics. The update thread never
//! blocks on the worker.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Frame, Grid, Layout, Margin, Rounding, Sense, Stroke, Vec2};

use crate::config::{
    commit_ui_interval, interval_parts_to_ms, ms_to_interval_parts, ClickType, CoordinateMode,
    EngineConfig, InputMode, MouseButton,
};
use crate::engine::{ClickEngine, SessionOutcome, STATE_RUNNING, STATE_STOPPING};
use crate::hotkey::{
    self, is_reserved_panic_vk, is_supported_toggle_vk, vk_display_name, AppPrefs, AppTheme,
    AppearanceMode, AppearanceStyle, VK_ESCAPE,
};
use crate::macro_rec::{self, MacroRecorder};
use crate::self_update::{self, InstallKind, UpdatePaths};
use crate::settings;
use crate::update::{self, ReleaseAsset, UpdateOutcome};
use crate::win32;
use crate::window_layout::{self, MainLayout};

/// Complete color and surface treatment for one appearance. The modern
/// treatment uses the same responsive compositions, with its own card
/// captions, typography, controls and colors.
struct Palette {
    dark: bool,
    modern: bool,
    face: Color32,
    button: Color32,
    button_disabled: Color32,
    button_border: Color32,
    hover_fill: Color32,
    pressed_fill: Color32,
    input_bg: Color32,
    input_disabled: Color32,
    /// Group-card surface. One step above `face` in every theme (white
    /// over the #f0f0f0 dialog in Classic, exactly like the Updates
    /// window's status plate) so each group reads as an elevated card.
    group_fill: Color32,
    /// Status-strip plate surface: recessed one step below the panel in
    /// the dark themes; in Classic it matches the group cards' white.
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
    modern: false,
    face: Color32::from_rgb(240, 240, 240),
    button: Color32::from_rgb(236, 236, 236),
    button_disabled: Color32::from_rgb(245, 245, 245),
    button_border: Color32::from_rgb(173, 173, 173),
    hover_fill: Color32::from_rgb(229, 241, 251),
    pressed_fill: Color32::from_rgb(204, 228, 247),
    input_bg: Color32::WHITE,
    input_disabled: Color32::from_rgb(250, 250, 250),
    group_fill: Color32::WHITE,
    well_fill: Color32::WHITE,
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
    modern: false,
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

// Modern light: porcelain surfaces, ink text and a restrained teal accent.
static MODERN_LIGHT_PALETTE: Palette = Palette {
    dark: false,
    modern: true,
    face: Color32::from_rgb(238, 243, 245),
    button: Color32::from_rgb(247, 250, 251),
    button_disabled: Color32::from_rgb(231, 239, 241),
    button_border: Color32::from_rgb(111, 135, 144),
    hover_fill: Color32::from_rgb(223, 238, 240),
    pressed_fill: Color32::from_rgb(209, 233, 234),
    input_bg: Color32::from_rgb(236, 243, 244),
    input_disabled: Color32::from_rgb(231, 239, 241),
    group_fill: Color32::WHITE,
    well_fill: Color32::from_rgb(233, 241, 242),
    group_border: Color32::from_rgb(193, 209, 214),
    input_border: Color32::from_rgb(111, 135, 144),
    text: Color32::from_rgb(19, 43, 54),
    group_title: Color32::from_rgb(0, 105, 111),
    text_dim: Color32::from_rgb(67, 94, 105),
    text_disabled: Color32::from_rgb(80, 101, 111),
    accent: Color32::from_rgb(0, 108, 112),
    success: Color32::from_rgb(15, 105, 73),
    danger: Color32::from_rgb(175, 37, 55),
    warning: Color32::from_rgb(121, 75, 5),
    chart_bg: Color32::from_rgb(236, 243, 244),
    chart_grid: Color32::from_rgb(200, 215, 219),
    chart_bar_alt: Color32::from_rgba_premultiplied(12, 65, 78, 120),
    selection_bg: Color32::from_rgba_premultiplied(0, 18, 22, 40),
};

// Modern dark: layered blue-ink neutrals with a luminous teal accent.
static MODERN_DARK_PALETTE: Palette = Palette {
    dark: true,
    modern: true,
    face: Color32::from_rgb(11, 18, 26),
    button: Color32::from_rgb(25, 40, 50),
    button_disabled: Color32::from_rgb(24, 35, 43),
    button_border: Color32::from_rgb(97, 120, 135),
    hover_fill: Color32::from_rgb(39, 60, 72),
    pressed_fill: Color32::from_rgb(48, 78, 90),
    input_bg: Color32::from_rgb(15, 27, 36),
    input_disabled: Color32::from_rgb(24, 35, 43),
    group_fill: Color32::from_rgb(21, 35, 43),
    well_fill: Color32::from_rgb(13, 28, 34),
    group_border: Color32::from_rgb(64, 84, 98),
    input_border: Color32::from_rgb(112, 133, 149),
    text: Color32::from_rgb(237, 245, 246),
    group_title: Color32::from_rgb(168, 217, 213),
    text_dim: Color32::from_rgb(175, 192, 199),
    text_disabled: Color32::from_rgb(161, 176, 187),
    accent: Color32::from_rgb(112, 216, 207),
    success: Color32::from_rgb(128, 223, 179),
    danger: Color32::from_rgb(255, 148, 160),
    warning: Color32::from_rgb(243, 206, 121),
    chart_bg: Color32::from_rgb(15, 27, 36),
    chart_grid: Color32::from_rgb(48, 78, 90),
    chart_bar_alt: Color32::from_rgba_premultiplied(32, 98, 100, 170),
    selection_bg: Color32::from_rgba_premultiplied(12, 43, 48, 100),
};

// Pure-black dark variant for OLED screens.
static OLED_PALETTE: Palette = Palette {
    dark: true,
    modern: false,
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
    modern: false,
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
    modern: false,
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

/// Fit-independent floor size of the main window: the exact classic
/// footprint in points with the window-driven fit at 1.0. The size
/// actually declared to the OS divides this by the applied fit
/// (`live_window_floor_for`), because egui-winit multiplies a declared
/// minimum by the total zoom when converting it to physical pixels.
/// That keeps the physical floor tied to any legacy scale alone,
/// so enlarging the window can never strand it above a minimum created
/// by the transient fit. Larger sizes are restored from the persisted
/// preferences.
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

/// Maximum extra gap in the classic and wide compositions. Portrait
/// compositions use a smaller cap and distribute the rest inside cards.
const MAIN_GAP_EXTRA_MAX: f32 = 72.0_f32;

/// Widest content column in the wide layout. Three group boxes across
/// need more elbow room than the stacked classic column before the
/// side margin takes over.
const MAIN_CONTENT_MAX_WIDTH_WIDE: f32 = 880.0_f32;

/// Window height at which the wide layout's column exactly fills the
/// window (its three-across row and single button row are shorter than
/// the stacked classic column, whose fill height is the 456-point
/// floor). The vertical rhythm measures spare height from here.
const MAIN_WIDE_FILL_HEIGHT: f32 = 372.0_f32;

/// Portrait cards are stacked and interval/position controls reflow into
/// two rows, so their width remains legible as the total zoom grows.
const MAIN_CONTENT_MAX_WIDTH_PORTRAIT: f32 = 480.0_f32;
/// Natural portrait-column height (including margins); surplus uses
/// the bounded portrait rhythm shared with the narrower compositions.
const MAIN_PORTRAIT_FILL_HEIGHT: f32 = 596.0_f32;
/// Narrower portrait compositions reflow more controls onto their own
/// rows, letting height contribute to zoom instead of enormous gaps.
const MAIN_NARROW_FILL_HEIGHT: f32 = 660.0_f32;
const MAIN_SLENDER_FILL_HEIGHT: f32 = 710.0_f32;

/// Reference display for the one-time display-aware opening size:
/// on a fresh install the classic dialog opens occupying the same
/// fraction of the monitor it occupies on a 1920x1080 screen at
/// 100 % display scaling, so a 1440p or 4K desktop no longer starts
/// with a comparatively tiny window. The factor is clamped to
/// [1, 2] and measured in neutral points, so Windows' own DPI
/// scaling is never double-counted.
const DISPLAY_DEFAULT_REFERENCE: [f32; 2] = [1920.0_f32, 1080.0_f32];
const DISPLAY_DEFAULT_MAX_SCALE: f32 = 2.0_f32;

/// Smallest side margin around the content column, kept at the floor
/// size so the classic layout is pixel-identical there.
const MAIN_SIDE_MARGIN: f32 = 12.0_f32;

const LOG_LIMIT: usize = 200;
const SAVE_THROTTLE_MS: u64 = 1_000;
/// The badge uses white text on a saturated red that clears 4.5:1.
const UPDATE_BADGE_RED: Color32 = Color32::from_rgb(190, 20, 36);

/// What the Updates window can currently say about the newest
/// published release. `Checking` always resolves into exactly one of
/// the other states: the worker thread reports once on every path,
/// and a dropped channel counts as `Failed` (see
/// `poll_update_check`), so the window can never claim to be
/// checking forever.
#[derive(Clone, Debug, PartialEq, Eq)]
enum UpdateUiState {
    NotChecked,
    Checking,
    UpToDate,
    /// Newer release version, without the leading "v".
    Available(String),
    /// User-facing reason the check could not complete.
    Failed(String),
}

/// Whether the Updates window's transient action notice reports a
/// successful browser/clipboard action or one the user must recover
/// from manually. Shape and wording carry the distinction; the
/// color only reinforces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateNoticeKind {
    Info,
    Error,
}

/// One finished update check: the upgrade verdict plus the asset
/// expectations that came with it, or why there is no verdict.
type UpdateCheckResult = Result<(UpdateOutcome, Option<ReleaseAsset>), String>;

/// Phases reported by the install worker through its shared counter.
const UPDATE_PHASE_DOWNLOADING: u8 = 0;
const UPDATE_PHASE_VERIFYING: u8 = 1;
const UPDATE_PHASE_SOURCE: u8 = 2;
const UPDATE_PHASE_STOPPING: u8 = 3;

/// The self-update pipeline's place in its one-way journey:
/// idle, a download/verify worker in flight (with the shared
/// progress counters the Updates window reads), or the brief
/// moment after the replacement program has been launched and
/// this process is on its way out. Every failure resolves back
/// to Idle with a notice — never a half-installed state.
enum UpdateInstallState {
    Idle,
    Downloading {
        version: String,
        total: u64,
        downloaded: Arc<AtomicU64>,
        phase: Arc<AtomicU8>,
        cancel: Arc<AtomicBool>,
        temp_path: PathBuf,
        source_expected: bool,
        source_advanced: Arc<AtomicBool>,
        rx: mpsc::Receiver<UpdateInstallMessage>,
    },
    WaitingForStop {
        ready: ReadyUpdate,
        cancel: Arc<AtomicBool>,
    },
    Restarting,
}

/// The download/verify worker's single report. Verification runs
/// on the worker so the UI thread never hashes a file; only the
/// millisecond-fast rename swap and relaunch happen here.
struct ReadyUpdate {
    paths: UpdatePaths,
    version: String,
    source_advanced: bool,
}

enum UpdateInstallMessage {
    Verified(ReadyUpdate),
    Cancelled { source_advanced: bool },
    Failed(String),
}

/// Original vector marks used beside the dashboard's four main actions.
#[derive(Clone, Copy)]
enum ModernActionIcon {
    Start,
    Stop,
    Hotkeys,
    Playback,
}

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
    show_updates: bool,
    show_font_license: bool,

    // Update checker: the Updates window's state, the latch for the
    // once-per-launch background check, and the channel its detached
    // worker reports through (None while no check is in flight).
    // The newest version GitHub has ever reported this session and
    // when the last attempt finished are kept separately, so a
    // later transient failure never erases a verdict the user can
    // still act on and the window can say exactly how fresh its
    // answer is. The notice is the one-line outcome of the last
    // browser/clipboard action, so no button ever feels dead.
    update_state: UpdateUiState,
    update_auto_check_started: bool,
    update_rx: Option<mpsc::Receiver<UpdateCheckResult>>,
    update_latest_version: Option<String>,
    update_last_checked: Option<Instant>,
    update_notice: Option<(UpdateNoticeKind, String)>,
    // Self-update: the asset expectations (size + SHA-256) from
    // the last successful check, the install pipeline's state,
    // this process's own EXE path (the file an install replaces),
    // and the latch that suppresses the exit-time settings save
    // after a relaunch — the new instance has already read the
    // file this one flushed before swapping, and two writers on
    // one temp file would only race.
    update_asset: Option<ReleaseAsset>,
    update_install: UpdateInstallState,
    exe_path: Option<PathBuf>,
    // Classified once at startup: portable EXE, recognized source
    // checkout, unrecognized Cargo artifact, or unknown. The Updates
    // window and footer both read this one verdict, so the install
    // action they offer can never disagree mid-session.
    install_kind: InstallKind,
    /// A failed rollback needs manual recovery before another in-app update.
    update_recovery_backup: Option<PathBuf>,
    skip_exit_save: bool,

    // Whether a settings file existed at load. A fresh install (no
    // file) earns the one-time display-aware opening size; any
    // existing file's size — including an explicit classic floor —
    // is respected exactly.
    window_size_was_persisted: bool,
    // Window-chrome bookkeeping: once the zoom factor for the chosen
    // scale has been applied, re-declare the minimum size and apply the
    // floor-aware size correction for that scale.
    pending_resize: bool,
    // Apply the persisted window size once (at startup and after a
    // factory reset) as soon as the zoom factor has settled.
    restore_window_size: bool,
    // The native minimum is expressed in physical pixels by egui-winit;
    // track display density so it can be re-declared after a DPI change.
    last_native_pixels_per_point: Option<f32>,
    // Live-size observation stays paused until this instant so the
    // transient point sizes seen while a zoom, DPI change, or restore
    // resize is in flight are never mistaken for a size the user chose. The deadline
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
    // Adopted window-driven scale factor (1.0 when the feature is
    // off or the window fits the design canvas at the user's own
    // percentage). The total egui zoom is the user's percentage
    // times this factor.
    auto_zoom: f32,
    // The dashboard's displayed fit factor. Secondary windows may
    // temporarily apply a smaller actual egui zoom. When
    // `adapt_to_window_size` adopts a larger `auto_zoom`, this eases
    // toward it over ~180 ms instead of snapping, so an enlargement
    // lands as one smooth zoom. A smaller fit is applied at once so
    // an inward drag never has to fight a canvas that is too large
    // for the shrinking window. It equals `auto_zoom` at rest —
    // including at startup, where the opening size must never animate.
    displayed_auto_zoom: f32,
    // In-flight zoom transition: (paint-clock start, factor it
    // started from). `None` while the displayed factor is at rest.
    // The transition is pure presentation: the neutral-point size
    // the fit reads is live-points x *applied* zoom, exact at every
    // intermediate value, and the evaluator only runs once the zoom
    // machinery is idle again — so easing can never feed back into
    // the fit, the persisted size, or itself.
    zoom_transition: Option<(f64, f32)>,
    // The legacy scale for which `auto_zoom` was last derived. A reset
    // can change it, so the fit is re-derived from the invariant neutral
    // window size before applying the new total zoom.
    applied_user_factor: f32,
    // Shape-aware classic, portrait, or panoramic composition.
    main_layout: MainLayout,
    // Upward fit-zoom debounce: the candidate factor and how many
    // consecutive frames the neutral-point window size has held
    // still while asking for it. Growth applies only once the size
    // settles; a downward fit bypasses this debounce so an inward
    // drag is never resisted by a stale, too-large factor.
    zoom_candidate: f32,
    stable_frames: u32,
    // Last neutral-point window size (live points x total zoom).
    last_base_size: Vec2,

    // Persistence. The "last saved" fingerprints make the dirty check exact
    // so the disk is never written while nothing changed.
    settings_path: Option<PathBuf>,
    settings_dirty: bool,
    /// If the existing file could not be read, never overwrite it with
    /// defaults during this run; the user can repair it before restarting.
    settings_load_error: Option<String>,
    last_save_error: Option<String>,
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
        // A relaunch after a verified self-update carries
        // `--updated-from=<old version>`; recognizing it earns the
        // user a confirmation line (and triggers the leftover-file
        // sweep below). args_os because args() would panic on a
        // non-UTF-8 argument some launcher invented.
        let exe_path = std::env::current_exe().ok();
        let install_kind = exe_path
            .as_deref()
            .map(self_update::install_kind_for_exe)
            .unwrap_or(InstallKind::Unknown);
        let updated_from = std::env::args_os().find_map(|arg| {
            let text = arg.to_string_lossy().into_owned();
            text.strip_prefix("--updated-from=")
                .filter(|version| update::parse_version(version).is_some())
                .map(|version| version.strip_prefix('v').unwrap_or(version).to_string())
        });
        let updated_attempt = std::env::args_os().find_map(|arg| {
            let text = arg.to_string_lossy().into_owned();
            text.strip_prefix("--update-attempt=")
                .filter(|id| self_update::valid_update_id(id))
                .map(str::to_string)
        });
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
            show_updates: false,
            show_font_license: false,
            update_state: UpdateUiState::NotChecked,
            update_auto_check_started: false,
            update_rx: None,
            update_latest_version: None,
            update_last_checked: None,
            update_notice: None,
            update_asset: None,
            update_install: UpdateInstallState::Idle,
            exe_path,
            install_kind,
            update_recovery_backup: None,
            skip_exit_save: false,
            window_size_was_persisted: false,
            pending_resize: false,
            restore_window_size: true,
            last_native_pixels_per_point: None,
            window_settle_until: Instant::now(),
            row_group_height: 60.0_f32,
            interval_group_widths: [0.0_f32; 4],
            auto_zoom: 1.0_f32,
            displayed_auto_zoom: 1.0_f32,
            zoom_transition: None,
            applied_user_factor: 1.0_f32,
            main_layout: MainLayout::Classic,
            zoom_candidate: 1.0_f32,
            stable_frames: 0,
            last_base_size: Vec2::ZERO,
            settings_path: Self::settings_file_path(),
            settings_dirty: false,
            settings_load_error: None,
            last_save_error: None,
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
        // Only clean this successful relaunch's exact attempt. Failed
        // rollbacks and other instances' in-flight files stay recoverable.
        if let (Some(exe), Some(id)) = (app.exe_path.clone(), updated_attempt) {
            if updated_from.is_some() {
                let paths = self_update::update_paths(&exe, &id);
                self_update::cleanup_update_attempt(&paths);
                let _ = std::thread::Builder::new()
                    .name("macroclicker-update-cleanup".to_string())
                    .spawn(move || {
                        for _attempt in 0..8 {
                            if !paths.backup.exists() && !paths.download.exists() {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(750));
                            self_update::cleanup_update_attempt(&paths);
                        }
                    });
            }
        }
        if let Some(previous) = updated_from {
            app.set_status(
                StatusKind::Ready,
                format!(
                    "Updated from v{previous} — now running v{}.",
                    env!("CARGO_PKG_VERSION")
                ),
            );
            app.log(format!(
                "Self-update complete: v{previous} was replaced by v{} and restarted.",
                env!("CARGO_PKG_VERSION")
            ));
        }
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
    /// Persisted sizes are fit-independent points; the builder wants
    /// live points, so the opening fit factor is divided back out.
    /// A missing file (fresh install) returns the classic floor
    /// here; the running app upgrades that first size to the
    /// display-aware default once the monitor size is known.
    pub fn persisted_window_size() -> [f32; 2] {
        let Some(path) = Self::settings_file_path() else {
            return MAIN_WINDOW_SIZE;
        };
        let Ok(text) = settings::read_settings(&path) else {
            return MAIN_WINDOW_SIZE;
        };
        let (_, prefs) = settings::parse_settings(&text);
        // Persisted sizes are fit-independent points; the fit
        // speaks neutral points, one user-factor further.
        let user_factor = prefs.ui_scale_percent as f32 / 100.0_f32;
        let neutral = Vec2::new(
            prefs.window_width as f32 * user_factor,
            prefs.window_height as f32 * user_factor,
        );
        let auto = Self::initial_auto_zoom(neutral, user_factor, prefs.auto_scale);
        let floor = Self::live_window_floor_for(auto);
        [
            (prefs.window_width as f32 / auto).max(floor.x),
            (prefs.window_height as f32 / auto).max(floor.y),
        ]
    }

    fn load_persisted(&mut self) {
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        let text = match settings::read_settings(&path) {
            Ok(text) => text,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && matches!(std::fs::symlink_metadata(&path), Err(ref metadata_error) if metadata_error.kind() == std::io::ErrorKind::NotFound) =>
            {
                return;
            }
            Err(error) => {
                // A file that cannot be read (including invalid UTF-8 or an
                // oversized file) is not a fresh install. Never silently
                // replace it with defaults during this app session.
                let detail = format!(
                    "Could not read {}: {error}. The original file was preserved. Close MacroClicker, repair or move the file, then reopen the app.",
                    path.display()
                );
                self.window_size_was_persisted = true;
                self.settings_load_error = Some(detail.clone());
                self.last_save_error = Some(detail.clone());
                self.set_status(
                    StatusKind::Warning,
                    "Settings could not be read; open Settings for details.",
                );
                self.log(detail);
                return;
            }
        };
        // A file existed, so its size (or its defaults) is the
        // user's to keep: no display-aware first-run upgrade.
        self.window_size_was_persisted = true;
        let (mut config, mut prefs) = settings::parse_settings(&text);
        // The dashboard no longer exposes these legacy modes; neutralize
        // them before validation so an old HWND cannot discard unrelated
        // valid preferences and controls.
        config.input_mode = InputMode::GlobalSendInput;
        config.target_hwnd = 0;
        config.background_synchronous = false;
        config.humanize_enabled = false;
        if config.validate().is_err() {
            let mut repaired = config.clone();
            repaired.random_offset_ms = 0.0;
            if repaired.validate().is_ok() {
                config = repaired;
                self.log("Invalid saved random offset reset; other settings kept.");
                self.set_status(StatusKind::Warning, "Saved offset invalid; reset to zero.");
            } else {
                config = EngineConfig::default();
                self.log("Invalid saved click settings reset; preferences kept.");
                self.set_status(StatusKind::Warning, "Invalid saved click settings reset.");
            }
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
        // Open at the fit factor (and layout) the persisted size
        // calls for, so the first frames don't start small and then
        // scale up. Persisted sizes are fit-independent points; the
        // fit speaks neutral points, one user-factor further.
        let user_factor = self.prefs.ui_scale_percent as f32 / 100.0_f32;
        let neutral = Vec2::new(
            self.prefs.window_width as f32 * user_factor,
            self.prefs.window_height as f32 * user_factor,
        );
        self.auto_zoom = Self::initial_auto_zoom(neutral, user_factor, self.prefs.auto_scale);
        self.displayed_auto_zoom = self.auto_zoom;
        self.applied_user_factor = user_factor;
        self.zoom_candidate = self.auto_zoom;
        self.main_layout = Self::layout_for(
            neutral,
            user_factor,
            MainLayout::Classic,
            self.prefs.auto_scale,
        );
        self.sync_interval_editors();
        self.log(format!("Settings loaded from {}.", path.display()));
    }

    fn mark_dirty(&mut self) {
        self.settings_dirty = true;
    }

    /// An invalid editor should not poison the entire settings file.
    /// Random offset is the only cross-field validation the dashboard can
    /// make invalid; keep all other edited fields and persist zero instead.
    /// Otherwise use the last saved configuration if valid, or defaults.
    fn settings_config_to_save(&self) -> EngineConfig {
        if self.config.validate().is_ok() {
            return self.config.clone();
        }
        let mut repaired = self.config.clone();
        repaired.random_offset_ms = 0.0;
        if repaired.validate().is_ok() {
            repaired
        } else if self.last_saved_config.validate().is_ok() {
            self.last_saved_config.clone()
        } else {
            EngineConfig::default()
        }
    }

    fn flush_settings(&mut self) -> bool {
        // A close/update can occur before the next normal frame commits
        // the editors. Persist the values currently visible to the user.
        self.commit_interval_editors();
        self.last_save = Instant::now();
        let config_to_save = self.settings_config_to_save();
        let result = if let Some(error) = &self.settings_load_error {
            Err(error.clone())
        } else {
            match self.settings_path.as_deref() {
                Some(path) => settings::write_settings_atomic(
                    path,
                    &settings::serialize_settings(&config_to_save, &self.prefs),
                )
                .map_err(|error| error.to_string()),
                None => Err("No settings path is available.".to_string()),
            }
        };
        match result {
            Ok(()) => {
                self.settings_dirty = false;
                self.last_saved_config = self.config.clone();
                self.last_saved_prefs = self.prefs;
                if self.last_save_error.take().is_some()
                    && self.status_kind == StatusKind::Warning
                    && self.status_text == "Settings could not be saved; open Settings for details."
                {
                    self.set_status(StatusKind::Ready, "Settings saved.");
                }
                true
            }
            Err(error) => {
                self.settings_dirty = true;
                if self.last_save_error.as_deref() != Some(error.as_str()) {
                    self.log(format!("Settings not saved: {error}"));
                }
                if !self.engine.is_running()
                    && !matches!(self.status_kind, StatusKind::Panic | StatusKind::Error)
                {
                    self.set_status(
                        StatusKind::Warning,
                        "Settings could not be saved; open Settings for details.",
                    );
                }
                self.last_save_error = Some(error);
                false
            }
        }
    }

    fn maybe_autosave(&mut self) {
        if self.settings_dirty
            && self.settings_load_error.is_none()
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
            AppTheme::ModernLight => &MODERN_LIGHT_PALETTE,
            AppTheme::ModernDark => &MODERN_DARK_PALETTE,
            AppTheme::Oled => &OLED_PALETTE,
            AppTheme::Matrix => &MATRIX_PALETTE,
            AppTheme::Aurora => &AURORA_PALETTE,
        }
    }

    fn apply_style(&self, ctx: &egui::Context) {
        let pal = self.palette();
        // Font definitions take effect at the next frame. Keep the standard
        // proportional family present so the first frame (and a switch back
        // to Classic) is safe, then install the bundled modern face ahead of
        // egui's default fallbacks only when the style actually changes.
        let font_id = egui::Id::new("macroclicker_modern_font_installed");
        let had_modern_font = ctx.data(|data| data.get_temp::<bool>(font_id).unwrap_or(false));
        if had_modern_font != pal.modern {
            let mut fonts = egui::FontDefinitions::default();
            if pal.modern {
                fonts.font_data.insert(
                    "Outfit".to_owned(),
                    egui::FontData::from_static(include_bytes!(
                        "../assets/fonts/Outfit-Regular.ttf"
                    )),
                );
                if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional)
                {
                    proportional.insert(0, "Outfit".to_owned());
                }
            }
            ctx.set_fonts(fonts);
            ctx.data_mut(|data| data.insert_temp(font_id, pal.modern));
            ctx.request_repaint();
        }
        let mut style = (*ctx.style()).clone();
        // Preserve the original egui colors that only Modern customizes;
        // switching back must not leave traces on Classic or older themes.
        let base_colors_id = egui::Id::new("macroclicker_base_visual_colors");
        let base_colors = ctx.data_mut(|data| {
            if let Some(colors) =
                data.get_temp::<(Color32, Color32, Color32, Color32)>(base_colors_id)
            {
                colors
            } else {
                let colors = (
                    style.visuals.faint_bg_color,
                    style.visuals.code_bg_color,
                    style.visuals.warn_fg_color,
                    style.visuals.error_fg_color,
                );
                data.insert_temp(base_colors_id, colors);
                colors
            }
        });
        style.visuals.dark_mode = pal.dark;
        style.visuals.panel_fill = pal.face;
        style.visuals.window_fill = if pal.modern { pal.group_fill } else { pal.face };
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
        if pal.modern {
            style.visuals.faint_bg_color = pal.well_fill;
            style.visuals.code_bg_color = pal.input_bg;
            style.visuals.warn_fg_color = pal.warning;
            style.visuals.error_fg_color = pal.danger;
        } else {
            style.visuals.faint_bg_color = base_colors.0;
            style.visuals.code_bg_color = base_colors.1;
            style.visuals.warn_fg_color = base_colors.2;
            style.visuals.error_fg_color = base_colors.3;
        }
        let rounding = Rounding::same(if pal.modern { 8.0_f32 } else { 5.0_f32 });
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
        // Modern uses a clean unblurred drop shadow; Classic keeps its
        // original shadow exactly. Neither style blurs the background.
        let shadow_alpha: u8 = if pal.dark { 120 } else { 48 };
        style.visuals.window_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0_f32, 6.0_f32),
            blur: if pal.modern { 0.0_f32 } else { 18.0_f32 },
            spread: if pal.modern { 1.0_f32 } else { 0.0_f32 },
            color: Color32::from_black_alpha(shadow_alpha),
        };
        style.visuals.popup_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0_f32, 3.0_f32),
            blur: if pal.modern { 0.0_f32 } else { 10.0_f32 },
            spread: 0.0_f32,
            color: Color32::from_black_alpha(shadow_alpha),
        };
        style.visuals.window_rounding = Rounding::same(if pal.modern { 12.0_f32 } else { 8.0_f32 });
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
        if self.update_is_finishing() {
            self.set_status(
                StatusKind::Warning,
                "The update is waiting to restart; cancel it before recording.",
            );
            return;
        }
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
        if self.update_is_finishing() {
            self.set_status(
                StatusKind::Warning,
                "The update is waiting to restart; cancel it before playback.",
            );
            return;
        }
        if self.engine.is_running() || self.macro_recording || self.macro_recorder.is_empty() {
            return;
        }
        self.commit_interval_editors();
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
        // Each recorded step is one physical click with its own recorded
        // delay. Dashboard burst and random-offset choices are irrelevant to
        // playback and must neither multiply nor invalidate recorded steps.
        config.click_type = ClickType::Single;
        config.random_offset_ms = 0.0;
        config.max_clicks = macro_rec::clicks_for_loops(steps.len(), loops);
        match self.engine.start_with_macro(&config, steps) {
            Ok(()) => {
                // A one-click macro can finish before the next egui frame.
                self.was_running = true;
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
                // A nonfatal timer/priority warning must not hide F8 while
                // the worker is draining. A failed dispatch is reported by
                // the terminal outcome after it has finished.
                if self.status_kind != StatusKind::Panic {
                    self.set_status(StatusKind::Error, format!("Error: {error}"));
                }
                self.last_seen_error = Some(error);
            }
        }
    }

    fn report_session_outcome(&mut self, outcome: SessionOutcome, was_macro: bool) {
        let count = self.engine.click_count();
        match outcome {
            SessionOutcome::Completed => {
                let label = if was_macro {
                    "Playback complete"
                } else {
                    "Session complete"
                };
                self.set_status(
                    StatusKind::Ready,
                    format!("{label} — {count} clicks delivered."),
                );
                self.log(format!("{label} after {count} clicks."));
            }
            SessionOutcome::Stopped => {
                let label = if was_macro {
                    "Playback stopped"
                } else {
                    "Stopped"
                };
                self.set_status(
                    StatusKind::Ready,
                    format!("{label} — {count} clicks delivered."),
                );
                self.log(format!("{label} after {count} clicks."));
            }
            SessionOutcome::Panicked => {
                self.set_status(StatusKind::Panic, "Panic stop triggered by F8.");
            }
            SessionOutcome::DispatchFailed => {
                let error = self.engine.last_error().unwrap_or_else(|| {
                    "Click delivery failed; the session was aborted.".to_string()
                });
                if Some(&error) != self.last_seen_error.as_ref() {
                    self.log(format!("Engine error: {error}"));
                }
                self.last_seen_error = Some(error.clone());
                self.set_status(StatusKind::Error, format!("Error: {error}"));
            }
            SessionOutcome::WorkerLost | SessionOutcome::Pending => {
                self.set_status(
                    StatusKind::Error,
                    "Click worker lost — session aborted. Restart the app before clicking again.",
                );
                self.log("Worker thread found dead or ended without a result; session aborted.");
            }
        }
    }

    fn reconcile_session_status(&mut self) {
        // Use one state snapshot: if the worker finishes immediately after
        // this read, the next frame will still observe the running-to-idle
        // transition. A successful start sets was_running immediately.
        let running = self.engine.is_running();
        if self.was_running && !running {
            let was_macro = self.macro_playing;
            self.macro_playing = false;
            self.report_session_outcome(self.engine.session_outcome(), was_macro);
            self.mark_dirty();
            self.flush_settings();
        }
        self.was_running = running;
    }

    fn toggle_session(&mut self) {
        if self.macro_recording {
            self.stop_macro_recording("hotkey");
            return;
        }
        if self.engine.state() == STATE_STOPPING {
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
        if self.update_is_finishing() {
            self.set_status(
                StatusKind::Warning,
                "The update is waiting to restart; cancel it before starting.",
            );
            return;
        }
        // Hotkeys are polled before this frame's normal editor commit.
        // Commit and validate here as well so F6 never starts with stale
        // values, and a zero interval cannot bypass the disabled button.
        self.commit_interval_editors();
        if let Some(error) = self.validation_message() {
            self.set_status(StatusKind::Error, format!("Error: {error}"));
            self.log(format!("Click session rejected: {error}"));
            return;
        }
        // Only global SendInput delivery is exposed by the UI; the target
        // handle is pinned clear so a stale persisted value can never leak
        // into a session.
        self.config.input_mode = InputMode::GlobalSendInput;
        self.config.target_hwnd = 0;
        match self.engine.start(&self.config) {
            Ok(()) => {
                // Track the start even if the worker finishes before repaint.
                self.was_running = true;
                self.last_seen_error = None;
                let cps = self.config.target_cps();
                let pace = if cps < 0.01 {
                    format!("every {} ms", self.interval_total_ms())
                } else {
                    format!("at {cps:.2} CPS")
                };
                self.set_status(
                    StatusKind::Running,
                    format!(
                        "Running {pace} — {} button, {} click.",
                        self.config.mouse_button.label().to_lowercase(),
                        self.config.click_type.label().to_lowercase()
                    ),
                );
                self.log(format!("Click session started {pace}."));
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
        // (floor) persisted size once the 100 % zoom has settled. The
        // reset itself is a deliberate size choice (and flushes one to
        // disk below), so the one-time fresh-install display default
        // must not override it when the restore runs.
        self.window_size_was_persisted = true;
        self.auto_zoom = 1.0_f32;
        self.displayed_auto_zoom = 1.0_f32;
        self.zoom_transition = None;
        self.applied_user_factor = 1.0_f32;
        self.zoom_candidate = 1.0_f32;
        self.main_layout = MainLayout::Classic;
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
        // The 4.1 checkbox gates the offset; zero means exact timing.
        let offset = if self.prefs.random_offset_enabled {
            self.offset_ms
        } else {
            0.0
        };
        commit_ui_interval(&mut self.config, total, offset);
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

    /// The classic floor in live points at a given window-driven fit.
    /// egui-winit will multiply this by the total zoom when it declares
    /// the OS minimum, so the division is what keeps the *physical*
    /// floor tied to the user's percentage alone instead of to the
    /// transient size of the window.
    fn live_window_floor_for(auto_zoom: f32) -> Vec2 {
        let (width, height) = hotkey::window_floor_for_auto_zoom(auto_zoom);
        Vec2::new(width, height)
    }

    /// How spare window height is spent. The groups always flow from
    /// the top of the panel, exactly as they do at the floor size;
    /// surplus height first loosens every breathing gap between the
    /// control clusters by an equal share (bounded by
    /// `MAIN_GAP_EXTRA_MAX` so neighbouring clusters stay visually
    /// related), and whatever remains collects as one spring directly
    /// above the status strip. The status strip, action buttons and
    /// footer therefore ride at the window's bottom edge like a
    /// status bar at *every* height, instead of the whole column
    /// floating centered between two voids once the window is much
    /// taller than the dialog. The returned values always sum (with
    /// the gaps) to the full surplus, so the column ends flush with
    /// the panel's bottom margin at any size. `fill_height` is the
    /// window height at which the active layout's column exactly
    /// fills the window, so both values are zero there and the
    /// classic composition is reproduced exactly.
    fn vertical_rhythm(window_height: f32, fill_height: f32, gaps: f32) -> (f32, f32) {
        if !window_height.is_finite() || !fill_height.is_finite() || gaps <= 0.0_f32 {
            return (0.0_f32, 0.0_f32);
        }
        let extra = (window_height - fill_height).max(0.0_f32);
        // The spring counts as one share alongside the gaps, so the
        // surplus spreads over every seam in the column instead of
        // pooling above the status strip; only what the bounded
        // gaps cannot absorb gathers there.
        let gap_extra = (extra / (gaps + 1.0_f32)).min(MAIN_GAP_EXTRA_MAX);
        let spring = (extra - gaps * gap_extra).max(0.0_f32);
        (gap_extra, spring)
    }

    /// In a width-limited portrait, use excess space within the four
    /// cards rather than letting one enormous spring separate the footer.
    /// The progressively narrower canvases do the actual zoom/reflow;
    /// this only balances the remaining space when width limits the fit.
    fn portrait_vertical_rhythm(window_height: f32, fill_height: f32) -> (f32, f32, f32) {
        const MAX_PORTRAIT_GAP: f32 = 8.0_f32;
        if !window_height.is_finite() || !fill_height.is_finite() {
            return (0.0_f32, 0.0_f32, 0.0_f32);
        }
        let extra = (window_height - fill_height).max(0.0_f32);
        let gap = (extra / 5.0_f32).min(MAX_PORTRAIT_GAP);
        let remaining = (extra - 4.0_f32 * gap).max(0.0_f32);
        let spring = remaining.min(MAX_PORTRAIT_GAP);
        // Reserve a point per card for rounding so the footer still
        // fits at every pixel density.
        let extra_per_card = ((remaining - spring) / 4.0_f32 - 1.0_f32).max(0.0_f32);
        (gap, spring, extra_per_card)
    }

    /// Shared, platform-independent fit and shape logic uses neutral points.
    fn fit_auto_zoom(base: Vec2, user_factor: f32, layout: MainLayout) -> f32 {
        window_layout::fit_auto_zoom((base.x, base.y), user_factor, layout)
    }

    fn initial_auto_zoom(base: Vec2, user_factor: f32, enabled: bool) -> f32 {
        window_layout::initial_auto_zoom((base.x, base.y), user_factor, enabled)
    }

    fn layout_for(base: Vec2, user_factor: f32, current: MainLayout, enabled: bool) -> MainLayout {
        window_layout::choose_layout((base.x, base.y), user_factor, current, enabled)
    }

    /// The one-time display-aware opening size for a fresh install,
    /// in fit-independent persisted points: the classic dialog
    /// scaled to occupy the same fraction of this monitor as it
    /// occupies of the 1920x1080 reference, clamped to
    /// [1, `DISPLAY_DEFAULT_MAX_SCALE`]. The monitor extent is in
    /// neutral points, so a display Windows already scales at
    /// 125 % or 150 % is credited for that scaling instead of being
    /// scaled twice. Any unusable input falls back to the exact
    /// classic size.
    fn display_default_window_size(monitor_neutral: Vec2, user_factor: f32) -> Vec2 {
        let fallback = Vec2::new(MAIN_WINDOW_SIZE[0], MAIN_WINDOW_SIZE[1]);
        if !monitor_neutral.x.is_finite()
            || !monitor_neutral.y.is_finite()
            || monitor_neutral.x <= 0.0_f32
            || monitor_neutral.y <= 0.0_f32
            || !user_factor.is_finite()
            || user_factor <= 0.0_f32
        {
            return fallback;
        }
        let scale = (monitor_neutral.x / DISPLAY_DEFAULT_REFERENCE[0])
            .min(monitor_neutral.y / DISPLAY_DEFAULT_REFERENCE[1])
            .clamp(1.0_f32, DISPLAY_DEFAULT_MAX_SCALE);
        fallback * scale / user_factor
    }

    /// Records the live inner size as the size to restore next launch,
    /// in fit-independent points (live points times the applied
    /// window-driven factor), so the physical size round-trips no
    /// matter which fit factor is active when it is written or read.
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
        // Displayed (not adopted) factor: live points are produced
        // by the zoom actually applied, so this stays exact even
        // while a zoom transition is easing between factors. The
        // floor test must happen in these fit-independent points:
        // at a 2.8x fit a perfectly valid floor window is only
        // 200x163 live points, and testing the live size against
        // 560x456 would discard real user-chosen sizes.
        // A secondary window may temporarily lower the applied zoom to
        // fit. Persist the actual viewport size, not the dashboard target.
        let user_factor = self.prefs.ui_scale_percent as f32 / 100.0_f32;
        let applied_auto_zoom = ctx.zoom_factor() / user_factor;
        let (base_width, base_height) =
            hotkey::fit_independent_window_size((size.x, size.y), applied_auto_zoom);
        // The OS minimum is enforced in physical pixels, so the
        // converted size can land a fraction below the floor after
        // rounding; allow that much slack, and treat anything further
        // below (or not finite) as a transient that must not be
        // recorded.
        if !base_width.is_finite()
            || !base_height.is_finite()
            || base_width < MAIN_WINDOW_SIZE[0] - 2.0_f32
            || base_height < MAIN_WINDOW_SIZE[1] - 2.0_f32
        {
            return;
        }
        let base = Vec2::new(base_width, base_height);
        let (width, height) = hotkey::sanitize_window_size(f64::from(base.x), f64::from(base.y));
        if width != self.prefs.window_width || height != self.prefs.window_height {
            self.prefs.window_width = width;
            self.prefs.window_height = height;
            self.mark_dirty();
        }
    }

    /// Re-derives the fit if a legacy saved scale changes, for example
    /// when restoring defaults. The neutral size (live points times the
    /// applied zoom) is unchanged by the scale decision, so the fit can
    /// be solved without growing the physical window from a stale factor.
    fn sync_fit_for_user_factor(&mut self, ctx: &egui::Context, user_factor: f32) {
        if self.prefs.auto_scale {
            let (size, minimized) = ctx.input(|i| (i.screen_rect.size(), i.viewport().minimized));
            let applied_zoom = ctx.zoom_factor();
            if minimized != Some(true)
                && size.x.is_finite()
                && size.y.is_finite()
                && size.x > 0.0_f32
                && size.y > 0.0_f32
                && applied_zoom.is_finite()
                && applied_zoom > 0.0_f32
            {
                let neutral = size * applied_zoom;
                self.main_layout = Self::layout_for(neutral, user_factor, self.main_layout, true);
                self.auto_zoom = Self::fit_auto_zoom(neutral, user_factor, self.main_layout);
                self.last_base_size = neutral;
            }
        } else {
            self.auto_zoom = 1.0_f32;
            self.main_layout = MainLayout::Classic;
        }
        self.displayed_auto_zoom = self.auto_zoom;
        self.zoom_candidate = self.auto_zoom;
        self.zoom_transition = None;
        self.stable_frames = 0;
        self.pending_resize = true;
        self.window_settle_until = Instant::now() + Duration::from_millis(750);
        ctx.request_repaint();
    }

    /// Makes a shrinking window safe on the frame it shrinks.
    ///
    /// Growth can wait for the settle/ease path because extra room
    /// never clips a control. Shrinkage is the opposite: if the old
    /// fit stayed applied throughout a continuous inward drag, the
    /// live canvas would fall below the design size (and, with an
    /// OS minimum declared from that old fit, Windows could stop
    /// the drag altogether). A lower target therefore snaps the
    /// displayed fit down immediately. If the window shrinks during
    /// an upward ease but the new target is still above the
    /// displayed factor, the ease is simply retargeted so it cannot
    /// overshoot into a size the window no longer has.
    fn apply_shrink_fit_immediately(&mut self, ctx: &egui::Context, user_factor: f32) {
        const FIT_DEADBAND: f32 = 0.011_f32;
        if !self.prefs.auto_scale || self.restore_window_size {
            return;
        }
        let (size, minimized) = ctx.input(|i| (i.screen_rect.size(), i.viewport().minimized));
        let applied_zoom = ctx.zoom_factor();
        if minimized == Some(true)
            || !size.x.is_finite()
            || !size.y.is_finite()
            || size.x <= 0.0_f32
            || size.y <= 0.0_f32
            || !applied_zoom.is_finite()
            || applied_zoom <= 0.0_f32
        {
            return;
        }
        let neutral = size * applied_zoom;
        // Re-evaluate the shape during the drag too: a portrait canvas
        // that no longer fits must reflow back to classic before waiting
        // for the normal settled-size / growth-easing machinery.
        let layout = Self::layout_for(neutral, user_factor, self.main_layout, true);
        if layout != self.main_layout {
            self.main_layout = layout;
            self.stable_frames = 0;
            ctx.request_repaint();
        }
        let target = Self::fit_auto_zoom(neutral, user_factor, self.main_layout);
        if target >= self.auto_zoom - FIT_DEADBAND {
            return;
        }
        self.auto_zoom = target;
        self.zoom_candidate = target;
        self.last_base_size = neutral;
        self.stable_frames = 0;
        if target < self.displayed_auto_zoom {
            self.displayed_auto_zoom = target;
            self.zoom_transition = None;
        } else if self.zoom_transition.is_some() {
            self.zoom_transition = Some((self.ui_clock_secs, self.displayed_auto_zoom));
        }
        self.window_settle_until = Instant::now() + Duration::from_millis(750);
        ctx.request_repaint();
    }

    /// Window-driven presentation: derives the layout mode and the
    /// target fit factor from the neutral-point window size (live
    /// points times the total applied zoom — invariant under the
    /// very zoom changes it drives, which is what makes feedback
    /// impossible). The layout mode follows immediately (its aspect
    /// and width bands carry their own hysteresis); a *growing* fit
    /// applies only once the size has held still for a few
    /// consecutive frames, so it lands just after a drag ends
    /// rather than jumping mid-drag. A *shrinking* fit is adopted
    /// at once (see `apply_shrink_fit_immediately`): waiting would
    /// leave the design canvas squeezed below its floor while the
    /// user is still pulling the window inward. A one-percent dead
    /// band around the applied
    /// factor keeps physical-pixel rounding from nudging the zoom
    /// back and forth across a quantization boundary. Runs only
    /// when the zoom machinery is idle (see `update`), so a factor
    /// never compounds with an in-flight zoom or restore.
    fn adapt_to_window_size(&mut self, ctx: &egui::Context) {
        const SETTLE_FRAMES: u32 = 6;
        const FIT_DEADBAND: f32 = 0.011_f32;
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
        // this only runs once the applied zoom equals user x fit, so
        // the measure is the physical window size at 100 % zoom.
        let base = size * ctx.zoom_factor();
        let user_factor = self.prefs.ui_scale_percent as f32 / 100.0_f32;
        self.main_layout =
            Self::layout_for(base, user_factor, self.main_layout, self.prefs.auto_scale);
        let target = if self.prefs.auto_scale {
            Self::fit_auto_zoom(base, user_factor, self.main_layout)
        } else {
            1.0_f32
        };
        let size_held = (base.x - self.last_base_size.x).abs() <= 1.5_f32
            && (base.y - self.last_base_size.y).abs() <= 1.5_f32;
        if size_held && (target - self.zoom_candidate).abs() <= 0.001_f32 {
            self.stable_frames = self.stable_frames.saturating_add(1);
        } else {
            self.stable_frames = 0;
            self.zoom_candidate = target;
        }
        self.last_base_size = base;
        let shrink_to_fit = target < self.auto_zoom - FIT_DEADBAND;
        if (self.stable_frames >= SETTLE_FRAMES || shrink_to_fit)
            && (target - self.auto_zoom).abs() > FIT_DEADBAND
        {
            self.auto_zoom = target;
            if shrink_to_fit {
                // Downward fits track the user's drag immediately;
                // easing down over 180 ms would spend that time with
                // a canvas smaller than the design floor.
                self.displayed_auto_zoom = target;
                self.zoom_transition = None;
            } else {
                // Ease the displayed factor to the adopted one (~180 ms,
                // advanced at the top of `update`) instead of snapping
                // the whole interface in a single frame.
                self.zoom_transition = Some((self.ui_clock_secs, self.displayed_auto_zoom));
            }
            self.stable_frames = 0;
            // The live point size this frame still belongs to the old
            // factor (the new zoom applies next frame), so size
            // observation must pause from this instant — otherwise the
            // observer in this very frame would multiply pre-fit
            // points by the new factor and persist a phantom size.
            // The zoom bookkeeping refreshes the same deadline when it
            // runs.
            self.window_settle_until = Instant::now() + Duration::from_millis(750);
            ctx.request_repaint();
        }
    }

    // --------------------------------------------------------- classic widgets

    fn modern_section_tile_color(pal: &Palette) -> Color32 {
        if pal.dark {
            Color32::from_rgb(30, 61, 65)
        } else {
            Color32::from_rgb(222, 242, 240)
        }
    }

    /// Small original vector glyphs; no icon font or external library is used.
    fn paint_section_icon(
        painter: &egui::Painter,
        title: &str,
        center: egui::Pos2,
        color: Color32,
    ) {
        let stroke = Stroke::new(1.2_f32, color);
        let at = |x, y| center + Vec2::new(x, y);
        match title {
            "Click interval" => {
                painter.circle_stroke(center, 4.8_f32, stroke);
                painter.line_segment([center, at(0.0, -3.0)], stroke);
                painter.line_segment([center, at(2.4, 1.2)], stroke);
            }
            "Click options" => {
                painter.rect_stroke(
                    egui::Rect::from_center_size(center, Vec2::new(8.0, 11.0)),
                    Rounding::same(4.0),
                    stroke,
                );
                painter.line_segment([at(0.0, -4.0), at(0.0, -1.0)], stroke);
            }
            "Click repeat" => {
                // Two short circular arcs with arrowheads read as a loop.
                for segment in [
                    [at(-4.2, -0.7), at(-3.0, -3.4)],
                    [at(-3.0, -3.4), at(0.4, -4.5)],
                    [at(0.4, -4.5), at(3.8, -2.9)],
                    [at(3.8, -2.9), at(4.4, 0.3)],
                    [at(4.2, 0.7), at(3.0, 3.4)],
                    [at(3.0, 3.4), at(-0.4, 4.5)],
                    [at(-0.4, 4.5), at(-3.8, 2.9)],
                    [at(-3.8, 2.9), at(-4.4, -0.3)],
                    [at(2.3, -1.0), at(4.4, 0.3)],
                    [at(4.4, 0.3), at(5.1, -2.1)],
                    [at(-2.3, 1.0), at(-4.4, -0.3)],
                    [at(-4.4, -0.3), at(-5.1, 2.1)],
                ] {
                    painter.line_segment(segment, stroke);
                }
            }
            "Cursor position" => {
                painter.circle_stroke(center, 3.3_f32, stroke);
                painter.line_segment([at(-5.4, 0.0), at(5.4, 0.0)], stroke);
                painter.line_segment([at(0.0, -5.4), at(0.0, 5.4)], stroke);
            }
            _ => {}
        }
    }

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
    /// Both styles use elevated cards. Classic captions break the top
    /// border; Modern puts the caption inside a rounded, clean-edged card.
    /// The modern margins are slightly more compact to leave room for the
    /// typeface's line metrics at the existing window floor.
    /// A reserved shadow slot is filled behind the real frame, and is
    /// skipped on a pure-black face where a black shadow cannot be seen.
    fn group_box<R>(
        ui: &mut egui::Ui,
        pal: &Palette,
        title: &str,
        min_height: f32,
        extra_vertical: f32,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (R, f32) {
        let shadow_slot = if pal.face != Color32::BLACK {
            Some(ui.painter().add(egui::Shape::Noop))
        } else {
            None
        };
        let card_rounding = Rounding::same(if pal.modern { 11.0_f32 } else { 8.0_f32 });
        let response = Frame::none()
            .fill(pal.group_fill)
            .stroke(Stroke::new(1.0_f32, pal.group_border))
            .rounding(card_rounding)
            .inner_margin(Margin {
                left: 12.0_f32,
                right: 12.0_f32,
                top: (if pal.modern { 24.0_f32 } else { 18.0_f32 }) + extra_vertical / 2.0_f32,
                bottom: (if pal.modern { 4.0_f32 } else { 12.0_f32 }) + extra_vertical / 2.0_f32,
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
            let shadow_alpha: u8 = if pal.modern {
                if pal.dark {
                    54
                } else {
                    24
                }
            } else if pal.dark {
                72
            } else {
                36
            };
            ui.painter().set(
                slot,
                egui::Shape::rect_filled(
                    shadow,
                    card_rounding,
                    Color32::from_black_alpha(shadow_alpha),
                ),
            );
        }
        if pal.modern {
            // Keep the existing in-place hover rim without changing the card's
            // geometry or registering a separate interactive widget.
            if ui.rect_contains_pointer(rect) {
                let rim = if pal.dark {
                    Color32::from_rgb(91, 126, 137)
                } else {
                    Color32::from_rgb(124, 160, 170)
                };
                ui.painter()
                    .rect_stroke(rect, card_rounding, Stroke::new(1.0_f32, rim));
            }
            let painter = ui.painter_at(rect);
            // A quiet top edge and a compact icon tile organize the card.
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 12.0_f32, rect.top() + 1.5_f32),
                    egui::pos2(rect.right() - 12.0_f32, rect.top() + 1.5_f32),
                ],
                Stroke::new(
                    1.0_f32,
                    if pal.dark {
                        Color32::from_rgb(42, 61, 69)
                    } else {
                        Color32::from_rgb(244, 249, 250)
                    },
                ),
            );
            let tile = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 8.0_f32, rect.top() + 3.0_f32),
                Vec2::splat(18.0_f32),
            );
            painter.rect_filled(
                tile,
                Rounding::same(5.0_f32),
                Self::modern_section_tile_color(pal),
            );
            Self::paint_section_icon(&painter, title, tile.center(), pal.accent);
            let text = egui::RichText::new(title.to_ascii_uppercase())
                .size(11.0_f32)
                .extra_letter_spacing(0.45_f32)
                .color(pal.group_title);
            let job = egui::WidgetText::from(text).into_layout_job(
                ui.style(),
                egui::FontSelection::Default,
                egui::Align::Center,
            );
            let galley = ui.fonts(|fonts| fonts.layout_job(job));
            painter.galley(
                egui::pos2(
                    rect.left() + 32.0_f32,
                    rect.top() + 12.0_f32 - galley.size().y / 2.0_f32,
                ),
                galley,
                pal.group_title,
            );
            return response.inner;
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
        let font = FontId::proportional(13.5_f32);
        let galley = painter.layout_no_wrap(title.to_string(), font.clone(), pal.group_title);
        // A short accent tick leads the caption — the card's identity
        // mark, in the same accent as the primary action. The caption
        // block (tick + text) rides the border together.
        let tick_width = 3.0_f32;
        let tick_gap = 5.0_f32;
        let block_top = rect.top() - galley.size().y / 2.0_f32;
        let tick_pos = egui::pos2(rect.left() + 10.0_f32, block_top);
        let text_pos = egui::pos2(tick_pos.x + tick_width + tick_gap, block_top);
        // The backdrop must cover the 1 px border line entirely so the
        // caption block cleanly breaks it like a native BS_GROUPBOX
        // caption. It straddles two surfaces — panel `face` above the
        // border, card `group_fill` below it — so it is painted in two
        // halves split at the border's midline; a single color would
        // leave a visible patch on one side of the line.
        let backdrop = egui::Rect::from_min_max(
            egui::pos2(tick_pos.x - 4.0_f32, block_top - 1.0_f32),
            egui::pos2(
                text_pos.x + galley.size().x + 4.0_f32,
                block_top + galley.size().y + 1.0_f32,
            ),
        );
        let seam = rect.top() + 0.5_f32;
        let upper = egui::Rect::from_min_max(backdrop.min, egui::pos2(backdrop.max.x, seam));
        let lower = egui::Rect::from_min_max(egui::pos2(backdrop.min.x, seam), backdrop.max);
        painter.rect_filled(upper, 0.0_f32, pal.face);
        painter.rect_filled(lower, 0.0_f32, pal.group_fill);
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(tick_pos.x, block_top + 1.5_f32),
                Vec2::new(tick_width, (galley.size().y - 3.0_f32).max(4.0_f32)),
            ),
            Rounding::same(1.5_f32),
            pal.accent,
        );
        // Paint the token the galley was laid out with: the palette's
        // accent-toned group title (identical to `text` in the Classic
        // theme, where the tick carries the accent instead).
        painter.text(text_pos, Align2::LEFT_TOP, title, font, pal.group_title);
        response.inner
    }

    /// Hand-painted push button with full interaction feedback: the fill,
    /// border and elevation all track the pointer state, and the hover lift
    /// is eased over ~180 ms (inside the 150–250 ms window where motion
    /// reads as responsive rather than sluggish). The pressed state drops
    /// the lift so the button visibly sits down under the pointer. Tones
    /// follow the Updates window's hierarchy: `primary` paints the one
    /// accent-filled call to action (Start, like Install), `danger`
    /// outlines the destructive action (Stop while a session runs) in
    /// the theme's danger color while keeping the neutral fill and the
    /// proven text contrast.
    fn classic_button(
        ui: &mut egui::Ui,
        pal: &Palette,
        text: &str,
        enabled: bool,
        width: f32,
    ) -> egui::Response {
        Self::toned_button(ui, pal, text, enabled, width, false, false)
    }

    /// Brighten/darken an opaque accent without changing its alpha.
    /// `Color32::gamma_multiply` is defined only for factors up to 1.0
    /// and scales alpha too, so it is not suitable for a hover highlight.
    fn accent_variant(accent: Color32, factor: f32) -> Color32 {
        let channel = |value: u8| (f32::from(value) * factor).round().clamp(0.0, 255.0) as u8;
        Color32::from_rgb(
            channel(accent.r()),
            channel(accent.g()),
            channel(accent.b()),
        )
    }

    /// The Modern Start gradient is intentionally light enough for the same
    /// dark label to remain readable at both ends, including hover/press.
    fn modern_start_colors(hovered: bool, pressed: bool) -> (Color32, Color32) {
        let factor = if pressed {
            0.90
        } else if hovered {
            1.08
        } else {
            1.0
        };
        (
            Self::accent_variant(Color32::from_rgb(69, 213, 194), factor),
            Self::accent_variant(Color32::from_rgb(112, 166, 250), factor),
        )
    }

    fn modern_stop_tone(
        dark: bool,
        enabled: bool,
        hovered: bool,
        pressed: bool,
    ) -> (Color32, Color32, Color32) {
        if !enabled {
            return if dark {
                (
                    Color32::from_rgb(27, 37, 45),
                    Color32::from_rgb(76, 94, 105),
                    Color32::from_rgb(160, 176, 188),
                )
            } else {
                (
                    Color32::from_rgb(230, 237, 239),
                    Color32::from_rgb(162, 178, 184),
                    Color32::from_rgb(82, 105, 116),
                )
            };
        }
        let factor = if pressed {
            0.90
        } else if hovered {
            1.06
        } else {
            1.0
        };
        let red = Self::accent_variant(Color32::from_rgb(176, 37, 55), factor);
        (red, red, Color32::WHITE)
    }

    /// A bounded triangle fan clips the horizontal gradient to a rounded
    /// rectangle. It is a small vector mesh, not a texture or backdrop blur.
    fn paint_modern_gradient(
        painter: &egui::Painter,
        rect: egui::Rect,
        radius: f32,
        left: Color32,
        right: Color32,
    ) {
        let radius = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
        let color_at = |x: f32| {
            let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            let channel =
                |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
            Color32::from_rgb(
                channel(left.r(), right.r()),
                channel(left.g(), right.g()),
                channel(left.b(), right.b()),
            )
        };
        let corners = [
            (
                egui::pos2(rect.right() - radius, rect.top() + radius),
                -std::f32::consts::FRAC_PI_2,
            ),
            (
                egui::pos2(rect.right() - radius, rect.bottom() - radius),
                0.0,
            ),
            (
                egui::pos2(rect.left() + radius, rect.bottom() - radius),
                std::f32::consts::FRAC_PI_2,
            ),
            (
                egui::pos2(rect.left() + radius, rect.top() + radius),
                std::f32::consts::PI,
            ),
        ];
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.center(), color_at(rect.center().x));
        const STEPS: usize = 5;
        for (center, start) in corners {
            for step in 0..=STEPS {
                let angle = start + std::f32::consts::FRAC_PI_2 * step as f32 / STEPS as f32;
                let point = center + Vec2::new(angle.cos(), angle.sin()) * radius;
                mesh.colored_vertex(point, color_at(point.x));
            }
        }
        let points = (corners.len() * (STEPS + 1)) as u32;
        for index in 1..=points {
            mesh.add_triangle(0, index, if index == points { 1 } else { index + 1 });
        }
        painter.add(egui::Shape::mesh(mesh));
    }

    fn modern_action_icon(text: &str, primary: bool, danger: bool) -> Option<ModernActionIcon> {
        if primary {
            Some(ModernActionIcon::Start)
        } else if danger {
            Some(ModernActionIcon::Stop)
        } else {
            match text {
                "Hotkeys" => Some(ModernActionIcon::Hotkeys),
                "Record & Playback" | "Record / Play" => Some(ModernActionIcon::Playback),
                _ => None,
            }
        }
    }

    /// Familiar, single-color vector marks. The visible label remains the
    /// accessible button name, and icons are omitted when space is tight.
    fn paint_modern_action_icon(
        painter: &egui::Painter,
        icon: ModernActionIcon,
        center: egui::Pos2,
        color: Color32,
    ) {
        let at = |x, y| center + Vec2::new(x, y);
        let stroke = Stroke::new(1.2_f32, color);
        match icon {
            ModernActionIcon::Start => {
                painter.add(egui::Shape::convex_polygon(
                    vec![at(-4.0, -5.0), at(5.0, 0.0), at(-4.0, 5.0)],
                    color,
                    Stroke::NONE,
                ));
            }
            ModernActionIcon::Stop => {
                painter.rect_filled(
                    egui::Rect::from_center_size(center, Vec2::splat(9.0)),
                    Rounding::same(1.5),
                    color,
                );
            }
            ModernActionIcon::Hotkeys => {
                painter.rect_stroke(
                    egui::Rect::from_center_size(center, Vec2::new(14.0, 10.0)),
                    Rounding::same(2.0),
                    stroke,
                );
                for x in [-4.0, 0.0, 4.0] {
                    painter.circle_filled(at(x, -1.5), 0.8, color);
                }
                painter.line_segment([at(-3.0, 2.0), at(3.0, 2.0)], stroke);
            }
            ModernActionIcon::Playback => {
                painter.circle_stroke(center, 5.5, stroke);
                painter.add(egui::Shape::convex_polygon(
                    vec![at(-1.5, -2.5), at(2.5, 0.0), at(-1.5, 2.5)],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }

    fn toned_button(
        ui: &mut egui::Ui,
        pal: &Palette,
        text: &str,
        enabled: bool,
        width: f32,
        primary: bool,
        danger: bool,
    ) -> egui::Response {
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 31.0_f32), sense);
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, text);
            info.enabled = enabled && ui.is_enabled();
            info
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered() && !pressed;
        let on_accent = if pal.modern && primary {
            Color32::from_rgb(7, 25, 35)
        } else if pal.dark {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        let (fill, border, text_color) = if pal.modern && danger {
            Self::modern_stop_tone(pal.dark, enabled, hovered, pressed)
        } else if !enabled {
            (pal.button_disabled, pal.button_border, pal.text_disabled)
        } else if primary {
            if pressed {
                (
                    Self::accent_variant(pal.accent, 0.9_f32),
                    pal.accent,
                    on_accent,
                )
            } else if hovered {
                (
                    Self::accent_variant(pal.accent, 1.12_f32),
                    pal.accent,
                    on_accent,
                )
            } else {
                (pal.accent, pal.accent, on_accent)
            }
        } else if pressed {
            (
                pal.pressed_fill,
                if danger { pal.danger } else { pal.accent },
                pal.text,
            )
        } else if hovered {
            (
                pal.hover_fill,
                if danger { pal.danger } else { pal.accent },
                pal.text,
            )
        } else {
            (
                pal.button,
                if danger {
                    pal.danger
                } else {
                    pal.button_border
                },
                pal.text,
            )
        };
        // Eased hover lift: 0 at rest and while pressed, 1 px when hovered.
        let lift = ui.ctx().animate_value_with_time(
            response.id.with("hover_lift"),
            if hovered { 1.0_f32 } else { 0.0_f32 },
            0.18_f32,
        );
        let face_rect = rect.translate(Vec2::new(0.0_f32, -lift));
        let rounding = Rounding::same(if pal.modern { 9.0_f32 } else { 6.0_f32 });
        let painter = ui.painter();
        // A restrained, unblurred drop shadow lifts Modern's primary and
        // active Stop buttons; secondary actions stay visually quiet.
        if enabled && !pressed && (!pal.modern || primary || danger) {
            let shadow_alpha = if pal.dark { 110 } else { 26 };
            painter.rect_filled(
                face_rect.translate(Vec2::new(0.0_f32, 1.5_f32 + lift)),
                rounding,
                Color32::from_black_alpha(shadow_alpha),
            );
        }
        if pal.modern && primary && enabled {
            let (left, right) = Self::modern_start_colors(hovered, pressed);
            Self::paint_modern_gradient(painter, face_rect, 9.0_f32, left, right);
            painter.rect_stroke(
                face_rect,
                rounding,
                Stroke::new(1.0_f32, Color32::from_white_alpha(50)),
            );
        } else {
            painter.rect(face_rect, rounding, fill, Stroke::new(1.0_f32, border));
        }
        // Classic 3D face cue: a one-pixel inner highlight along the top
        // edge, the same bevel the native BS_PUSHBUTTON control carries.
        // While pressed the cue inverts to a dark line — a pushed button
        // catches shadow on its top edge, not light.
        if enabled && !pal.modern {
            let bevel = if pressed {
                Color32::from_black_alpha(if pal.dark { 90 } else { 60 })
            } else {
                Color32::from_white_alpha(if pal.dark { 18 } else { 70 })
            };
            painter.line_segment(
                [
                    egui::pos2(face_rect.left() + 3.0_f32, face_rect.top() + 1.5_f32),
                    egui::pos2(face_rect.right() - 3.0_f32, face_rect.top() + 1.5_f32),
                ],
                Stroke::new(1.0_f32, bevel),
            );
        }
        if enabled && response.has_focus() {
            let focus_color = if pal.modern && danger {
                Color32::WHITE
            } else if primary {
                on_accent
            } else {
                pal.accent
            };
            painter.rect_stroke(
                face_rect.shrink(2.0_f32),
                rounding,
                Stroke::new(2.0_f32, focus_color),
            );
        }
        let icon = if pal.modern {
            Self::modern_action_icon(text, primary, danger)
        } else {
            None
        };
        if let Some(icon) = icon {
            let galley =
                painter.layout_no_wrap(text.to_owned(), FontId::proportional(13.5_f32), text_color);
            const ICON_WIDTH: f32 = 14.0;
            const ICON_GAP: f32 = 6.0;
            const SIDE_PADDING: f32 = 8.0;
            let content_width = ICON_WIDTH + ICON_GAP + galley.size().x;
            if content_width + 2.0 * SIDE_PADDING <= face_rect.width() {
                let left = face_rect.center().x - content_width / 2.0;
                Self::paint_modern_action_icon(
                    painter,
                    icon,
                    egui::pos2(left + ICON_WIDTH / 2.0, face_rect.center().y),
                    text_color,
                );
                painter.galley(
                    egui::pos2(
                        left + ICON_WIDTH + ICON_GAP,
                        face_rect.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    text_color,
                );
            } else {
                // Never crowd the label just to fit an ornament.
                painter.text(
                    face_rect.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(13.5_f32),
                    text_color,
                );
            }
        } else {
            painter.text(
                face_rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(13.5_f32),
                text_color,
            );
        }
        response
    }

    fn numeric_field(ui: &mut egui::Ui, value: &mut u64, max: u64, width: f32) {
        ui.add_sized(
            [width, 22.0_f32],
            egui::DragValue::new(value).clamp_range(0..=max),
        );
    }

    /// egui 0.27's disabled `DragValue` still registers for keyboard focus.
    /// Tab can land on it, lose focus, and cycle back to the first field. Paint
    /// the same disabled button face without registering an interactive sense;
    /// enabled fields continue to use the normal editable `DragValue`.
    fn disabled_numeric_field(ui: &mut egui::Ui, text: String, value: f64, size: [f32; 2]) {
        // `DragValue` paints a button with the inactive visual state. A
        // hover-only button uses noninteractive visuals instead, so copy the
        // inactive state locally to keep the disabled appearance unchanged.
        let inactive = ui.visuals().widgets.inactive;
        ui.style_mut().visuals.widgets.noninteractive = inactive;
        let text = egui::RichText::new(text).text_style(ui.style().drag_value_text_style.clone());
        let response = ui.add_sized(
            size,
            egui::Button::new(text)
                .wrap(false)
                .sense(Sense::hover())
                .min_size(ui.spacing().interact_size),
        );
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::drag_value(value);
            info.enabled = false;
            info
        });
    }

    /// Like an egui button, but an inactive action does not enter the Tab
    /// focus order. Its paint is unchanged, and enabled actions stay normal.
    fn focus_safe_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
        ui.add_enabled_ui(enabled, |ui| {
            let interactive = ui.is_enabled();
            let mut button = egui::Button::new(text);
            if !interactive {
                let inactive = ui.visuals().widgets.inactive;
                ui.style_mut().visuals.widgets.noninteractive = inactive;
                button = button.sense(Sense::hover());
            }
            let response = ui.add(button);
            if !interactive {
                response.widget_info(|| {
                    let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, text);
                    info.enabled = false;
                    info
                });
            }
            response
        })
        .inner
    }

    /// Disabled appearance choices retain egui's selectable-label paint but
    /// are skipped by Tab until a Classic or Modern style is selected.
    fn disabled_selectable_label(ui: &mut egui::Ui, selected: bool, text: &str) {
        let inactive = ui.visuals().widgets.inactive;
        ui.style_mut().visuals.widgets.noninteractive = inactive;
        let padding = ui.spacing().button_padding;
        let galley = egui::WidgetText::from(text).into_galley(
            ui,
            None,
            ui.available_width() - 2.0 * padding.x,
            egui::TextStyle::Button,
        );
        let mut desired = 2.0 * padding + galley.size();
        desired.y = desired.y.max(ui.spacing().interact_size.y);
        let (rect, response) = ui.allocate_at_least(desired, Sense::hover());
        response.widget_info(|| {
            let mut info =
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, selected, text);
            info.enabled = false;
            info
        });
        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact_selectable(&response, selected);
            if selected || response.hovered() || response.highlighted() || response.has_focus() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                );
            }
            let text_pos = ui
                .layout()
                .align_size_within_rect(galley.size(), rect.shrink2(padding))
                .min;
            ui.painter().galley(text_pos, galley, visuals.text_color());
        }
    }

    // ------------------------------------------------------------- the screen

    fn draw_interval_group(&mut self, ui: &mut egui::Ui, extra_vertical: f32) {
        let pal = self.palette();
        let _ = Self::group_box(ui, pal, "Click interval", 0.0_f32, extra_vertical, |ui| {
            if self.main_layout == MainLayout::Slender {
                // Stacked unit labels allow two fields per row even on the
                // slimmest canvas, without abbreviating milliseconds.
                Grid::new("slender_interval_fields")
                    .num_columns(2)
                    .spacing(Vec2::new(12.0_f32, 6.0_f32))
                    .show(ui, |ui| {
                        for (index, (label, value, max)) in [
                            ("hours", &mut self.hours, 99),
                            ("mins", &mut self.minutes, 59),
                            ("secs", &mut self.seconds, 59),
                            ("milliseconds", &mut self.millis, 999),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(label).color(pal.text));
                                Self::numeric_field(ui, value, max, 56.0_f32);
                            });
                            if index % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
            } else if self.main_layout.is_portrait() {
                // Four units need two rows at this width. Grid columns keep
                // the fields and their labels aligned without clipping.
                Grid::new("portrait_interval_fields")
                    .num_columns(4)
                    .spacing(Vec2::new(7.0_f32, 6.0_f32))
                    .show(ui, |ui| {
                        Self::numeric_field(ui, &mut self.hours, 99, 56.0_f32);
                        ui.label(egui::RichText::new("hours").color(pal.text));
                        Self::numeric_field(ui, &mut self.minutes, 59, 56.0_f32);
                        ui.label(egui::RichText::new("mins").color(pal.text));
                        ui.end_row();
                        Self::numeric_field(ui, &mut self.seconds, 59, 56.0_f32);
                        ui.label(egui::RichText::new("secs").color(pal.text));
                        Self::numeric_field(ui, &mut self.millis, 999, 56.0_f32);
                        ui.label(egui::RichText::new("milliseconds").color(pal.text));
                        ui.end_row();
                    });
            } else {
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
            }
            ui.add_space(7.0_f32);
            if self.main_layout.is_compact_portrait() {
                let enabled = self.random_offset_checkbox(ui);
                ui.horizontal(|ui| self.random_offset_value(ui, pal, enabled));
            } else {
                ui.horizontal(|ui| {
                    let enabled = self.random_offset_checkbox(ui);
                    self.random_offset_value(ui, pal, enabled);
                });
            }
        });
    }

    fn random_offset_checkbox(&mut self, ui: &mut egui::Ui) -> bool {
        let mut enabled = self.prefs.random_offset_enabled;
        if ui.checkbox(&mut enabled, "Random offset ±").changed() {
            self.prefs.random_offset_enabled = enabled;
            if enabled && self.offset_ms <= 0.0 {
                self.offset_ms = 40.0;
            }
            self.mark_dirty();
        }
        enabled
    }

    fn random_offset_value(&mut self, ui: &mut egui::Ui, pal: &Palette, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            if ui.is_enabled() {
                ui.add_sized(
                    [72.0_f32, 22.0_f32],
                    egui::DragValue::new(&mut self.offset_ms)
                        .speed(1.0)
                        .clamp_range(0.0..=10_000.0)
                        .fixed_decimals(1),
                );
            } else {
                self.offset_ms = self.offset_ms.clamp(0.0, 10_000.0);
                let text = if self.offset_ms == 0.0 {
                    "0".to_string()
                } else {
                    format!("{:.1}", self.offset_ms)
                };
                Self::disabled_numeric_field(ui, text, self.offset_ms, [72.0_f32, 22.0_f32]);
            }
        });
        ui.label(egui::RichText::new("milliseconds").color(pal.text));
    }

    fn options_group_contents(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        Grid::new("click_options_grid")
            .num_columns(2)
            .spacing(Vec2::new(8.0_f32, 9.0_f32))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Mouse button:").color(pal.text));
                self.mouse_button_selector(ui);
                ui.end_row();

                ui.label(egui::RichText::new("Click type:").color(pal.text));
                self.click_type_selector(ui);
                ui.end_row();
            });
    }

    fn mouse_button_selector(&mut self, ui: &mut egui::Ui) {
        let width = if self.main_layout == MainLayout::Slender {
            70.0_f32
        } else {
            ui.available_width()
        };
        egui::ComboBox::from_id_source("mouse_button_combo")
            .width(width)
            .selected_text(self.config.mouse_button.label())
            .show_ui(ui, |ui| {
                for button in MouseButton::all() {
                    ui.selectable_value(&mut self.config.mouse_button, button, button.label());
                }
            });
    }

    fn click_type_selector(&mut self, ui: &mut egui::Ui) {
        let width = if self.main_layout == MainLayout::Slender {
            70.0_f32
        } else {
            ui.available_width()
        };
        egui::ComboBox::from_id_source("click_type_combo")
            .width(width)
            .selected_text(self.config.click_type.label())
            .show_ui(ui, |ui| {
                for kind in ClickType::all() {
                    ui.selectable_value(&mut self.config.click_type, kind, kind.label());
                }
            });
    }

    fn repeat_group_contents(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            if self.main_layout == MainLayout::Slender {
                // The longest supported count still fits beside its radio
                // and unit label when the two gaps are slightly tighter.
                ui.spacing_mut().item_spacing.x = 3.0_f32;
                if pal.modern {
                    // The modern face is wider: keep both labels visible
                    // without letting this row widen the entire dashboard.
                    for text_style in [egui::TextStyle::Body, egui::TextStyle::Button] {
                        ui.style_mut()
                            .text_styles
                            .insert(text_style, FontId::proportional(11.0_f32));
                    }
                }
            }
            ui.radio_value(&mut self.repeat_limited, true, "Repeat");
            self.repeat_count_editor(ui, pal);
        });
        ui.add_space(4.0_f32);
        ui.radio_value(&mut self.repeat_limited, false, "Repeat until stopped");
        if !self.repeat_limited {
            self.config.max_clicks = 0;
        }
    }

    fn repeat_count_editor(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.add_enabled_ui(self.repeat_limited, |ui| {
            if self.repeat_limited && self.config.max_clicks == 0 {
                self.config.max_clicks = 1;
            }
            if pal.modern && matches!(self.main_layout, MainLayout::Slender | MainLayout::Wide) {
                // Ten digits in the modern face would expand these narrower
                // cards past their columns. A compact numeric face keeps
                // the complete value editable without hiding its unit.
                Self::compact_numeric_style(ui);
            }
            if ui.is_enabled() {
                ui.add_sized(
                    [76.0_f32, 22.0_f32],
                    egui::DragValue::new(&mut self.config.max_clicks)
                        .clamp_range(1..=1_000_000_000),
                );
            } else {
                self.config.max_clicks = self.config.max_clicks.clamp(1, 1_000_000_000);
                Self::disabled_numeric_field(
                    ui,
                    self.config.max_clicks.to_string(),
                    self.config.max_clicks as f64,
                    [76.0_f32, 22.0_f32],
                );
            }
        });
        ui.label(egui::RichText::new("times").color(pal.text));
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

    fn compact_numeric_style(ui: &mut egui::Ui) {
        // Keep Outfit in the compact layouts as well; only the size and
        // padding change to fit the widest supported values in each card.
        ui.style_mut().drag_value_text_style = egui::TextStyle::Small;
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(10.5_f32));
        ui.spacing_mut().button_padding.x = 4.0_f32;
    }

    fn fixed_xy_fields(&mut self, ui: &mut egui::Ui) {
        let fixed = self.config.coordinate_mode == CoordinateMode::FixedScreen;
        let compact = self.palette().modern
            && matches!(self.main_layout, MainLayout::Slender | MainLayout::Wide);
        ui.add_enabled_ui(fixed, |ui| {
            if compact {
                Self::compact_numeric_style(ui);
            }
            if ui.is_enabled() {
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
            } else {
                self.config.fixed_x = self.config.fixed_x.clamp(-100_000, 100_000);
                self.config.fixed_y = self.config.fixed_y.clamp(-100_000, 100_000);
                for (label, value) in [("X", self.config.fixed_x), ("Y", self.config.fixed_y)] {
                    Self::disabled_numeric_field(
                        ui,
                        format!("{label}: {value}"),
                        value as f64,
                        [78.0_f32, 22.0_f32],
                    );
                }
            }
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
            let (_, left_natural) = Self::group_box(
                &mut columns[0],
                pal,
                "Click options",
                row_height,
                0.0_f32,
                |ui| self.options_group_contents(ui, pal),
            );
            let (_, right_natural) = Self::group_box(
                &mut columns[1],
                pal,
                "Click repeat",
                row_height,
                0.0_f32,
                |ui| self.repeat_group_contents(ui, pal),
            );
            taller_natural = left_natural.max(right_natural);
        });
        self.store_row_group_height(taller_natural);
    }

    fn draw_portrait_options_and_repeat(
        &mut self,
        ui: &mut egui::Ui,
        gap_extra: f32,
        extra_vertical: f32,
    ) {
        let pal = self.palette();
        let _ = Self::group_box(ui, pal, "Click options", 0.0_f32, extra_vertical, |ui| {
            self.options_group_contents(ui, pal)
        });
        ui.add_space(7.0_f32 + gap_extra);
        let _ = Self::group_box(ui, pal, "Click repeat", 0.0_f32, extra_vertical, |ui| {
            self.repeat_group_contents(ui, pal)
        });
    }

    /// Wide layout's middle row: Click options, Click repeat and
    /// Cursor position three-abreast, still meeting at one bottom
    /// edge through the same shared-height mechanism.
    fn draw_wide_group_row(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let row_height = self.row_group_height.max(0.0_f32);
        let mut tallest_natural = 0.0_f32;
        ui.columns(3, |columns| {
            let (_, options_natural) = Self::group_box(
                &mut columns[0],
                pal,
                "Click options",
                row_height,
                0.0_f32,
                |ui| self.options_group_contents(ui, pal),
            );
            let (_, repeat_natural) = Self::group_box(
                &mut columns[1],
                pal,
                "Click repeat",
                row_height,
                0.0_f32,
                |ui| self.repeat_group_contents(ui, pal),
            );
            let (_, position_natural) = Self::group_box(
                &mut columns[2],
                pal,
                "Cursor position",
                row_height,
                0.0_f32,
                |ui| self.position_group_contents(ui, pal, true),
            );
            tallest_natural = options_natural.max(repeat_natural).max(position_natural);
        });
        self.store_row_group_height(tallest_natural);
    }

    fn draw_position_group(&mut self, ui: &mut egui::Ui, extra_vertical: f32) {
        let pal = self.palette();
        let _ = Self::group_box(ui, pal, "Cursor position", 0.0_f32, extra_vertical, |ui| {
            if self.main_layout.is_portrait() {
                if self.main_layout == MainLayout::Slender {
                    ui.radio_value(
                        &mut self.config.coordinate_mode,
                        CoordinateMode::CurrentCursor,
                        "Current location",
                    );
                    ui.radio_value(
                        &mut self.config.coordinate_mode,
                        CoordinateMode::FixedScreen,
                        "Pick location",
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.config.coordinate_mode,
                            CoordinateMode::CurrentCursor,
                            "Current location",
                        );
                        ui.radio_value(
                            &mut self.config.coordinate_mode,
                            CoordinateMode::FixedScreen,
                            "Pick location",
                        );
                    });
                }
                ui.add_space(4.0_f32);
                if self.main_layout.is_compact_portrait() {
                    self.pick_location_button(ui, pal);
                    ui.horizontal(|ui| self.fixed_xy_fields(ui));
                } else {
                    ui.horizontal(|ui| {
                        self.pick_location_button(ui, pal);
                        self.fixed_xy_fields(ui);
                    });
                }
            } else {
                self.position_group_contents(ui, pal, false);
            }
        });
    }

    fn start_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let running = self.engine.is_running();
        let validation = self.validation_message();
        let finishing = self.update_is_finishing();
        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
        let response = Self::toned_button(
            ui,
            pal,
            &format!("Start ({hotkey_name})"),
            !running && validation.is_none() && !finishing,
            width,
            true,
            false,
        );
        let reason = if running {
            Some("Stop the current session first.".to_string())
        } else if finishing {
            Some("The update is waiting to restart.".to_string())
        } else {
            validation
        };
        let response = if let Some(reason) = reason {
            // This hand-painted disabled control senses hover (but not
            // clicks), so use the hover tooltip rather than egui's
            // disabled-widget tooltip, which requires a disabled Ui.
            response.on_hover_text(reason)
        } else {
            response
        };
        if response.clicked() {
            self.handle_start();
        }
    }

    fn stop_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let running = self.engine.state() == STATE_RUNNING;
        let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
        if Self::toned_button(
            ui,
            pal,
            &format!("Stop ({hotkey_name})"),
            running,
            width,
            false,
            true,
        )
        .clicked()
        {
            self.engine.request_stop();
            self.set_status(StatusKind::Stopping, "Stopping…");
            self.log("Graceful stop requested.");
            self.mark_dirty();
            self.flush_settings();
        }
    }

    fn hotkey_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let label = if pal.modern {
            "Hotkeys"
        } else {
            "Hotkey setting"
        };
        let response = Self::classic_button(ui, pal, label, true, width)
            .on_hover_text("Choose the key that starts and stops the clicker");
        if response.clicked() {
            self.show_hotkey_dialog = true;
            self.capturing = false;
        }
    }

    fn record_button(&mut self, ui: &mut egui::Ui, pal: &Palette, width: f32) {
        let label = if self.main_layout == MainLayout::Slender {
            "Record / Play"
        } else {
            "Record & Playback"
        };
        let response = Self::classic_button(ui, pal, label, true, width)
            .on_hover_text("Record your own clicks and play them back as a macro");
        if response.clicked() {
            self.show_macro_window = true;
        }
    }

    fn draw_action_buttons(&mut self, ui: &mut egui::Ui) {
        let pal = self.palette();
        let gap = ui.spacing().item_spacing.x;
        if self.main_layout == MainLayout::Wide {
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

    /// A single-line label that keeps its full text for accessibility
    /// and tooltips while remaining inside a fixed-width control.
    fn truncated_galley(
        ui: &egui::Ui,
        text: egui::RichText,
        max_width: f32,
    ) -> std::sync::Arc<egui::Galley> {
        let mut job = egui::WidgetText::from(text).into_layout_job(
            ui.style(),
            egui::FontSelection::Default,
            egui::Align::Center,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0_f32));
        ui.fonts(|fonts| fonts.layout_job(job))
    }

    fn draw_footer_actions(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().button_padding = Vec2::new(8.0_f32, 2.0_f32);
            let settings_hover = pal.modern.then(|| ui.painter().add(egui::Shape::Noop));
            let button = egui::Button::new(
                egui::RichText::new("Settings…")
                    .size(12.0_f32)
                    .color(pal.text),
            )
            .fill(if pal.modern {
                Color32::TRANSPARENT
            } else {
                pal.button
            })
            .stroke(Stroke::new(1.0_f32, pal.button_border))
            .rounding(Rounding::same(if pal.modern { 8.0_f32 } else { 5.0_f32 }));
            let settings_response = ui
                .add(button)
                .on_hover_text("Appearance style and mode, automatic layout and settings");
            if let Some(slot) = settings_hover {
                if settings_response.hovered() {
                    ui.painter().set(
                        slot,
                        egui::Shape::rect_filled(
                            settings_response.rect,
                            Rounding::same(8.0),
                            pal.hover_fill,
                        ),
                    );
                }
            }
            if pal.modern && settings_response.has_focus() {
                ui.painter().rect_stroke(
                    settings_response.rect.shrink(2.0),
                    Rounding::same(6.0),
                    Stroke::new(1.2_f32, pal.accent),
                );
            }
            if settings_response.clicked() {
                self.show_settings = true;
            }
            // The Updates entry point reports the
            // checker's state in place — including the
            // waiting and ready-to-download states — so
            // a background verdict is visible without
            // opening anything. When a verified in-app
            // update is ready and no click/macro session
            // is active, this one button starts the whole
            // update; the Updates window opens as its
            // progress and cancellation surface.
            // The badge is an actionable notification, not an update counter:
            // the latest confirmed release is either available or it isn't.
            // While installing, the "Updating…" label takes over; a failed
            // or cancelled install restores the badge when it returns to Idle.
            let show_update_badge = matches!(self.update_state, UpdateUiState::Available(_))
                && matches!(self.update_install, UpdateInstallState::Idle);
            let direct_update = show_update_badge
                && self.self_update_possible()
                && !self.engine.is_running()
                && !self.macro_recording
                && !self.macro_playing;
            let (updates_label, updates_color, updates_stroke, updates_hover) =
                match &self.update_state {
                    UpdateUiState::Available(version) => (
                        format!("Update v{version}"),
                        pal.success,
                        pal.success,
                        if direct_update && self.source_checkout().is_some() {
                            format!("Update this source checkout and restart into v{version}")
                        } else if direct_update {
                            format!("Update now and restart into v{version}")
                        } else {
                            format!("MacroClicker v{version} is available — open update details")
                        },
                    ),
                    UpdateUiState::Checking => (
                        "Checking…".to_string(),
                        pal.text_dim,
                        pal.button_border,
                        "Checking GitHub for a newer release".to_string(),
                    ),
                    UpdateUiState::Failed(_) => (
                        "Updates…".to_string(),
                        pal.warning,
                        pal.button_border,
                        "The last update check couldn't finish — open for details".to_string(),
                    ),
                    _ => (
                        "Updates…".to_string(),
                        pal.text,
                        pal.button_border,
                        "See whether a newer MacroClicker release is available".to_string(),
                    ),
                };
            let (updates_label, updates_color, updates_stroke, updates_hover) = if matches!(
                self.update_install,
                UpdateInstallState::Downloading { .. } | UpdateInstallState::WaitingForStop { .. }
            ) {
                (
                    "Updating…".to_string(),
                    pal.success,
                    pal.success,
                    "Updating MacroClicker — open for progress".to_string(),
                )
            } else {
                (updates_label, updates_color, updates_stroke, updates_hover)
            };
            // Reserve space inside the same button for the badge. Nothing is
            // added or moved when no update is available, and the full label
            // remains available to the tooltip and accessibility event.
            let normal_padding_x = ui.spacing().button_padding.x;
            if show_update_badge {
                ui.spacing_mut().button_padding.x += 8.0_f32;
            }
            let update_text = Self::truncated_galley(
                ui,
                egui::RichText::new(updates_label)
                    .size(12.0_f32)
                    .color(updates_color),
                ui.available_width() - 2.0_f32 * ui.spacing().button_padding.x,
            );
            let updates_hover_slot = pal.modern.then(|| ui.painter().add(egui::Shape::Noop));
            let updates_button = egui::Button::new(update_text)
                .fill(if pal.modern {
                    Color32::TRANSPARENT
                } else {
                    pal.button
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if pal.modern {
                        pal.button_border
                    } else {
                        updates_stroke
                    },
                ))
                .rounding(Rounding::same(if pal.modern { 8.0_f32 } else { 5.0_f32 }));
            let updates_response = ui.add(updates_button).on_hover_text(updates_hover);
            ui.spacing_mut().button_padding.x = normal_padding_x;
            if let Some(slot) = updates_hover_slot {
                if updates_response.hovered() {
                    ui.painter().set(
                        slot,
                        egui::Shape::rect_filled(
                            updates_response.rect,
                            Rounding::same(8.0),
                            pal.hover_fill,
                        ),
                    );
                }
            }
            if pal.modern && updates_response.has_focus() {
                ui.painter().rect_stroke(
                    updates_response.rect.shrink(2.0),
                    Rounding::same(6.0),
                    Stroke::new(1.2_f32, pal.accent),
                );
            }
            if show_update_badge {
                // Keep the full circle inside the button's existing hit area
                // so clicking the badge has exactly the same behavior as the
                // rest of Updates. The white rim echoes a notification bubble.
                let center = egui::pos2(
                    updates_response.rect.right() - 7.5_f32,
                    updates_response.rect.top() + 7.5_f32,
                );
                ui.painter().circle_filled(center, 7.0_f32, Color32::WHITE);
                ui.painter()
                    .circle_filled(center, 6.0_f32, UPDATE_BADGE_RED);
                ui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    "1",
                    FontId::proportional(9.0_f32),
                    Color32::WHITE,
                );
            }
            if updates_response.clicked() {
                self.show_updates = true;
                if direct_update {
                    self.start_update_install();
                }
            }
        });
    }

    fn any_dialog_open(&self) -> bool {
        self.show_settings
            || self.show_updates
            || self.show_hotkey_dialog
            || self.show_macro_window
            || self.show_font_license
    }

    /// Dim only the Modern dashboard, on the background layer underneath all
    /// windows. This is a visual treatment rather than a new modal lock: the
    /// existing dashboard and emergency-stop interaction stays available.
    fn draw_dialog_backdrop(&self, ctx: &egui::Context) {
        let pal = self.palette();
        if pal.modern && self.any_dialog_open() {
            let alpha = if pal.dark { 110 } else { 72 };
            ctx.layer_painter(egui::LayerId::background()).rect_filled(
                ctx.screen_rect(),
                Rounding::ZERO,
                Color32::from_black_alpha(alpha),
            );
        }
    }

    /// Leave room for the native egui window's title and margins. Long
    /// diagnostics can then scroll instead of pushing the close button
    /// or the bottom actions outside a short viewport.
    fn dialog_content_height(ctx: &egui::Context) -> f32 {
        (ctx.screen_rect().height() - 48.0_f32).max(1.0_f32)
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
            .max_height(Self::dialog_content_height(ctx))
            .vscroll(true)
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
            .max_height(Self::dialog_content_height(ctx))
            .vscroll(true)
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
                    } else if Self::focus_safe_button(ui, "Record", !engine_running).clicked() {
                        self.start_macro_recording();
                    }
                    if Self::focus_safe_button(
                        ui,
                        "Clear",
                        steps > 0 && !self.macro_recording && !engine_running,
                    )
                    .clicked()
                    {
                        self.macro_recorder.clear();
                        self.set_status(StatusKind::Ready, "Macro cleared.");
                        self.log("Macro cleared.");
                    }
                });
                ui.add_space(6.0_f32);

                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.macro_until_stopped, true, "Play until stopped");
                    ui.radio_value(&mut self.macro_until_stopped, false, "Play");
                    let mut loops = self.macro_loops.max(1);
                    ui.add_enabled_ui(!self.macro_until_stopped, |ui| {
                        if ui.is_enabled() {
                            ui.add_sized(
                                [64.0_f32, 20.0_f32],
                                egui::DragValue::new(&mut loops).clamp_range(1..=1_000_000),
                            );
                        } else {
                            loops = loops.clamp(1, 1_000_000);
                            Self::disabled_numeric_field(
                                ui,
                                loops.to_string(),
                                loops as f64,
                                [64.0_f32, 20.0_f32],
                            );
                        }
                    });
                    self.macro_loops = loops;
                    ui.label("times");
                });
                ui.add_space(8.0_f32);

                ui.horizontal(|ui| {
                    if Self::focus_safe_button(
                        ui,
                        "Play",
                        steps > 0 && !self.macro_recording && !engine_running,
                    )
                    .clicked()
                    {
                        self.start_macro_playback();
                    }
                    if Self::focus_safe_button(
                        ui,
                        "Stop playback",
                        engine_running && self.macro_playing,
                    )
                    .clicked()
                    {
                        self.engine.request_stop();
                        self.set_status(StatusKind::Stopping, "Stopping playback…");
                        self.log("Playback stop requested.");
                    }
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
            .max_height(Self::dialog_content_height(ctx))
            .vscroll(true)
            .default_width(280.0_f32)
            .show(ctx, |ui| {
                if pal.modern {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Appearance").color(pal.text).strong());
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.link("Outfit license").clicked() {
                                self.show_font_license = true;
                            }
                        });
                    });
                } else {
                    ui.label(egui::RichText::new("Appearance").color(pal.text).strong());
                }
                ui.horizontal(|ui| {
                    ui.add_sized([40.0_f32, 22.0_f32], egui::Label::new("Style"));
                    for (style, label) in [
                        (AppearanceStyle::Classic, "Classic"),
                        (AppearanceStyle::Modern, "Modern"),
                    ] {
                        let selected = self.prefs.theme.style() == Some(style);
                        if ui.selectable_label(selected, label).clicked() && !selected {
                            self.prefs.theme =
                                AppTheme::from_appearance(style, self.prefs.theme.mode());
                            self.mark_dirty();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_sized([40.0_f32, 22.0_f32], egui::Label::new("Mode"));
                    ui.add_enabled_ui(self.prefs.theme.style().is_some(), |ui| {
                        for (mode, label) in [
                            (AppearanceMode::Light, "Light"),
                            (AppearanceMode::Dark, "Dark"),
                        ] {
                            let selected = self.prefs.theme.mode() == mode;
                            if ui.is_enabled() {
                                if ui.selectable_label(selected, label).clicked() && !selected {
                                    if let Some(style) = self.prefs.theme.style() {
                                        self.prefs.theme = AppTheme::from_appearance(style, mode);
                                        self.mark_dirty();
                                    }
                                }
                            } else {
                                Self::disabled_selectable_label(ui, selected, label);
                            }
                        }
                    });
                });
                if self.prefs.theme.style().is_none() {
                    ui.label(
                        egui::RichText::new(format!(
                            "Previous theme: {}. Choose Classic or Modern to change it.",
                            self.prefs.theme.label()
                        ))
                        .color(pal.text_dim)
                        .small(),
                    );
                }
                egui::CollapsingHeader::new("Previous themes")
                    .id_source("settings_previous_themes")
                    .show(ui, |ui| {
                        for theme in AppTheme::previous() {
                            if ui
                                .selectable_value(&mut self.prefs.theme, theme, theme.label())
                                .changed()
                            {
                                self.mark_dirty();
                            }
                        }
                    });
                ui.add_space(8.0_f32);
                if ui
                    .checkbox(
                        &mut self.prefs.auto_scale,
                        "Scale and rearrange with window size",
                    )
                    .changed()
                {
                    if !self.prefs.auto_scale {
                        // Back to the fixed classic composition at once;
                        // the evaluator would retire the fit on its own,
                        // just a few frames later.
                        self.auto_zoom = 1.0_f32;
                        self.displayed_auto_zoom = 1.0_f32;
                        self.zoom_transition = None;
                        self.zoom_candidate = 1.0_f32;
                        self.main_layout = MainLayout::Classic;
                        self.stable_frames = 0;
                    }
                    self.mark_dirty();
                }
                ui.label(
                    egui::RichText::new(
                        "The interface scales as the window grows or shrinks. \
                         Tall windows stack the controls; panoramic windows \
                         arrange them side by side.",
                    )
                    .color(pal.text_dim)
                    .small(),
                );
                if let Some(error) = &self.settings_load_error {
                    ui.add_space(8.0_f32);
                    ui.label(egui::RichText::new(error).color(pal.warning).small());
                } else if let Some(error) = &self.last_save_error {
                    ui.add_space(8.0_f32);
                    ui.label(
                        egui::RichText::new(format!(
                            "Settings not saved: {error}. Retrying automatically."
                        ))
                        .color(pal.warning)
                        .small(),
                    );
                }
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

    /// The executable embeds Outfit, so include its complete license in the
    /// executable too and make it readable without a network or separate file.
    fn draw_font_license_window(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        let mut open = self.show_font_license;
        egui::Window::new("Outfit font license")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(400.0_f32)
            .max_height(Self::dialog_content_height(ctx))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(include_str!("../assets/fonts/OFL-Outfit.txt"))
                        .size(12.0_f32)
                        .color(pal.text),
                );
            });
        self.show_font_license = open;
    }

    // ----------------------------------------------------------- updates

    /// Starts a background update check unless one is already in
    /// flight. The detached worker performs a single HTTPS GET of
    /// GitHub's latest-release metadata and sends exactly one
    /// result; if the app closes mid-check the send simply fails
    /// and the thread exits.
    fn start_update_check(&mut self) {
        if self.update_rx.is_some() {
            return;
        }
        self.update_state = UpdateUiState::Checking;
        self.update_notice = None;
        let (sender, receiver) = mpsc::channel();
        self.update_rx = Some(receiver);
        let spawned = std::thread::Builder::new()
            .name("macroclicker-update-check".to_string())
            .spawn(move || {
                let result = win32::https_get_text(
                    update::UPDATE_API_HOST,
                    update::UPDATE_API_PATH,
                    update::UPDATE_API_HEADERS,
                )
                .map_err(|error| error.to_string())
                .and_then(|body| update::evaluate_release(&body, env!("CARGO_PKG_VERSION")));
                let _ = sender.send(result);
            });
        if let Err(error) = spawned {
            self.apply_update_failure(
                format!("could not start the update check: {error}"),
                Instant::now(),
            );
        }
    }

    /// Refreshes the verdict when the Updates window is opened and
    /// the last completed attempt is missing or stale. Opening the
    /// window is itself an explicit request for a current answer,
    /// so this runs even when the startup check is disabled; the
    /// staleness window keeps ordinary reopenings from spending
    /// GitHub's unauthenticated request budget.
    fn refresh_update_check_if_stale(&mut self) {
        if self.update_rx.is_some()
            || matches!(self.update_state, UpdateUiState::Checking)
            || !matches!(self.update_install, UpdateInstallState::Idle)
        {
            return;
        }
        if update::should_refresh(self.update_last_checked.map(|checked| checked.elapsed())) {
            self.start_update_check();
        }
    }

    /// Applies a finished worker's verdict. A disconnected channel
    /// (worker gone without reporting) also resolves the state, as
    /// Failed, so the Updates window can never sit in Checking
    /// forever.
    fn poll_update_check(&mut self) {
        let Some(result) = self.update_rx.as_ref().map(|receiver| receiver.try_recv()) else {
            return;
        };
        let finished_at = Instant::now();
        match result {
            Ok(Ok((UpdateOutcome::UpToDate(latest), asset))) => {
                self.update_latest_version = Some(latest);
                self.update_asset = asset;
                self.update_last_checked = Some(finished_at);
                self.update_state = UpdateUiState::UpToDate;
                self.update_rx = None;
            }
            Ok(Ok((UpdateOutcome::Available(version), asset))) => {
                self.update_latest_version = Some(version.clone());
                self.update_asset = asset;
                self.update_last_checked = Some(finished_at);
                self.update_state = UpdateUiState::Available(version);
                self.update_rx = None;
            }
            Ok(Err(message)) => {
                self.apply_update_failure(message, finished_at);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.apply_update_failure(
                    "the check stopped unexpectedly".to_string(),
                    finished_at,
                );
            }
        }
    }

    /// Records a failed attempt without throwing away a verdict
    /// GitHub already confirmed this session: with a known latest
    /// version the window keeps that (still actionable) result and
    /// reports the refresh problem as a notice, so one transient
    /// network blip can never strand a user who was about to
    /// download. Only a failure with nothing confirmed yet becomes
    /// the window's headline state.
    fn apply_update_failure(&mut self, message: String, finished_at: Instant) {
        self.update_last_checked = Some(finished_at);
        self.update_rx = None;
        match self.update_latest_version.clone() {
            Some(latest) => {
                self.update_state = if update::is_newer(&latest, env!("CARGO_PKG_VERSION")) {
                    UpdateUiState::Available(latest)
                } else {
                    UpdateUiState::UpToDate
                };
                self.update_notice = Some((
                    UpdateNoticeKind::Error,
                    format!(
                        "Couldn't refresh: {}. Showing the last confirmed result.",
                        update::error_headline(&message)
                    ),
                ));
            }
            None => {
                self.update_state = UpdateUiState::Failed(message);
            }
        }
    }

    /// Whether the confirmed update may be installed in-app:
    /// GitHub must have published the asset's SHA-256 (no
    /// fingerprint, no code execution), this process must know
    /// its own EXE path, and the installation kind must have a
    /// defined update path. Portable builds install directly;
    /// recognized source checkouts first fast-forward to the exact
    /// release tag; unrecognized Cargo artifacts keep the browser.
    fn self_update_possible(&self) -> bool {
        let asset_ready = self
            .update_asset
            .as_ref()
            .is_some_and(|asset| asset.sha256.is_some());
        let install_ready = matches!(
            self.install_kind,
            InstallKind::Portable | InstallKind::SourceCheckout(_)
        );
        asset_ready
            && self.exe_path.is_some()
            && install_ready
            && self.update_recovery_backup.is_none()
    }

    fn source_checkout(&self) -> Option<&self_update::SourceCheckout> {
        match &self.install_kind {
            InstallKind::SourceCheckout(checkout) => Some(checkout),
            _ => None,
        }
    }

    fn install_kind_label(&self) -> &'static str {
        match &self.install_kind {
            InstallKind::Portable => "Portable release build",
            InstallKind::SourceCheckout(_) => "Source checkout build",
            InstallKind::CargoBuild => "Development build",
            InstallKind::Unknown => "Unknown",
        }
    }

    /// Why the browser path is being offered instead of an
    /// in-app update, for the Updates window's fine print.
    fn self_update_unavailable_reason(&self) -> &'static str {
        if self.update_recovery_backup.is_some() {
            return "A previous rollback needs manual recovery; inspect the backup path shown above and restart MacroClicker before retrying.";
        }
        match &self.install_kind {
            InstallKind::CargoBuild => {
                "Development build outside a recognized MacroClicker source checkout — \
                 downloads open in your browser. Settings are kept."
            }
            InstallKind::Unknown => {
                "MacroClicker could not locate its own program file, so the download \
                 opens in your browser. Settings are kept."
            }
            InstallKind::Portable | InstallKind::SourceCheckout(_) => {
                "GitHub didn't publish a verifiable SHA-256 fingerprint for this \
                 release, so the download opens in your browser. Settings are kept."
            }
        }
    }

    fn update_is_finishing(&self) -> bool {
        matches!(
            self.update_install,
            UpdateInstallState::WaitingForStop { .. } | UpdateInstallState::Restarting
        )
    }

    fn source_advanced_context(version: &str, advanced: bool) -> String {
        if advanced {
            format!("The source checkout was fast-forwarded to v{version}. ")
        } else {
            String::new()
        }
    }

    fn show_update_cancelled(&mut self, version: &str, advanced: bool) {
        self.update_notice = Some((
            UpdateNoticeKind::Info,
            format!(
                "Update cancelled. {}The running program was not replaced.",
                Self::source_advanced_context(version, advanced)
            ),
        ));
    }

    /// Cancellation is checked on both sides of the potentially lengthy
    /// hash. It is checked once more on the UI thread before any rename.
    fn finish_update_verification<F>(
        paths: UpdatePaths,
        version: String,
        source_advanced: bool,
        cancel: &AtomicBool,
        verify: F,
    ) -> UpdateInstallMessage
    where
        F: FnOnce() -> Result<(), String>,
    {
        if cancel.load(Ordering::Acquire) {
            let _ = std::fs::remove_file(&paths.download);
            return UpdateInstallMessage::Cancelled { source_advanced };
        }
        let result = verify();
        if cancel.load(Ordering::Acquire) {
            let _ = std::fs::remove_file(&paths.download);
            return UpdateInstallMessage::Cancelled { source_advanced };
        }
        match result {
            Ok(()) => UpdateInstallMessage::Verified(ReadyUpdate {
                paths,
                version,
                source_advanced,
            }),
            Err(message) => {
                let _ = std::fs::remove_file(&paths.download);
                UpdateInstallMessage::Failed(format!(
                    "{}{message}",
                    Self::source_advanced_context(&version, source_advanced)
                ))
            }
        }
    }

    /// Starts only from an explicit user action. Source preparation and
    /// verification happen off the UI thread; nothing is replaced until a
    /// verified report has passed the final cancellation check.
    fn start_update_install(&mut self) {
        if let Some(backup) = &self.update_recovery_backup {
            self.update_notice = Some((
                UpdateNoticeKind::Error,
                format!(
                    "Restore or inspect the recovery backup at {} and restart MacroClicker before another in-app update.",
                    backup.display()
                ),
            ));
            return;
        }
        if !matches!(self.update_install, UpdateInstallState::Idle) {
            return;
        }
        let UpdateUiState::Available(version) = self.update_state.clone() else {
            return;
        };
        let (Some(asset), Some(exe)) = (self.update_asset.clone(), self.exe_path.clone()) else {
            return;
        };
        let Some(sha256) = asset.sha256 else { return };
        let Some((host, path)) = update::download_request_for_version(&version) else {
            return;
        };
        let paths = match self_update::reserve_update_paths(&exe) {
            Ok(paths) => paths,
            Err(error) => {
                self.update_notice = Some((UpdateNoticeKind::Error, error));
                return;
            }
        };
        let source_checkout = self.source_checkout().cloned();
        let source_expected = source_checkout.is_some();
        let initial_phase = if source_expected {
            UPDATE_PHASE_SOURCE
        } else {
            UPDATE_PHASE_DOWNLOADING
        };
        let downloaded = Arc::new(AtomicU64::new(0));
        let phase = Arc::new(AtomicU8::new(initial_phase));
        let cancel = Arc::new(AtomicBool::new(false));
        let source_advanced = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let temp_path = paths.download.clone();
        self.update_install = UpdateInstallState::Downloading {
            version: version.clone(),
            total: asset.size,
            downloaded: Arc::clone(&downloaded),
            phase: Arc::clone(&phase),
            cancel: Arc::clone(&cancel),
            temp_path,
            source_expected,
            source_advanced: Arc::clone(&source_advanced),
            rx: receiver,
        };
        self.update_notice = None;
        let spawned = std::thread::Builder::new()
            .name("macroclicker-update".to_string())
            .spawn(move || {
                let source_result = match &source_checkout {
                    Some(checkout) => {
                        self_update::fast_forward_source_checkout(checkout, &version, &cancel)
                            .map(|result| result.advanced)
                    }
                    None => Ok(false),
                };
                let message = match source_result {
                    Err(failure)
                        if failure.cancelled()
                            && failure.change == self_update::SourceChange::Unchanged =>
                    {
                        UpdateInstallMessage::Cancelled {
                            source_advanced: false,
                        }
                    }
                    Err(failure) => UpdateInstallMessage::Failed(failure.description(&version)),
                    Ok(advanced) => {
                        source_advanced.store(advanced, Ordering::Release);
                        if cancel.load(Ordering::Acquire) {
                            UpdateInstallMessage::Cancelled {
                                source_advanced: advanced,
                            }
                        } else {
                            phase.store(UPDATE_PHASE_DOWNLOADING, Ordering::Relaxed);
                            match win32::https_download_to_file(
                                host,
                                &path,
                                &paths.download,
                                asset.size,
                                &downloaded,
                                &cancel,
                            ) {
                                Ok(_bytes) => {
                                    phase.store(UPDATE_PHASE_VERIFYING, Ordering::Relaxed);
                                    let candidate = paths.download.clone();
                                    Self::finish_update_verification(
                                        paths,
                                        version,
                                        advanced,
                                        &cancel,
                                        || {
                                            self_update::verify_downloaded_exe(
                                                &candidate, asset.size, &sha256,
                                            )
                                        },
                                    )
                                }
                                Err(error) => {
                                    let _ = std::fs::remove_file(&paths.download);
                                    if cancel.load(Ordering::Acquire) {
                                        UpdateInstallMessage::Cancelled {
                                            source_advanced: advanced,
                                        }
                                    } else {
                                        UpdateInstallMessage::Failed(format!(
                                            "{}{error}",
                                            Self::source_advanced_context(&version, advanced)
                                        ))
                                    }
                                }
                            }
                        }
                    }
                };
                let _ = sender.send(message);
            });
        if let Err(error) = spawned {
            if let UpdateInstallState::Downloading { temp_path, .. } =
                std::mem::replace(&mut self.update_install, UpdateInstallState::Idle)
            {
                let _ = std::fs::remove_file(temp_path);
            }
            self.update_notice = Some((
                UpdateNoticeKind::Error,
                format!("Could not start the update worker: {error}"),
            ));
        }
    }

    /// Apply terminal worker reports; never replace an EXE after cancellation
    /// or while a click worker is still releasing its current session.
    fn poll_update_install(&mut self, ctx: &egui::Context) {
        if let UpdateInstallState::WaitingForStop { cancel, .. } = &self.update_install {
            let cancelled = cancel.load(Ordering::Acquire);
            let worker_lost = self.engine.is_running() && !self.engine.worker_alive();
            if !cancelled && !worker_lost && self.engine.is_running() {
                ctx.request_repaint_after(Duration::from_millis(16));
                return;
            }
            let previous = std::mem::replace(&mut self.update_install, UpdateInstallState::Idle);
            if let UpdateInstallState::WaitingForStop { ready, .. } = previous {
                if cancelled {
                    let _ = std::fs::remove_file(&ready.paths.download);
                    self.show_update_cancelled(&ready.version, ready.source_advanced);
                } else if worker_lost {
                    self.engine.force_idle();
                    let _ = std::fs::remove_file(&ready.paths.download);
                    self.update_notice = Some((UpdateNoticeKind::Error, format!(
                        "{}The click worker exited unexpectedly. Restart the app before retrying; the program was not replaced.",
                        Self::source_advanced_context(&ready.version, ready.source_advanced)
                    )));
                } else {
                    self.install_verified_update(ctx, &ready);
                }
            }
            return;
        }
        let message = match &self.update_install {
            UpdateInstallState::Downloading {
                rx,
                source_expected,
                source_advanced,
                version,
                ..
            } => match rx.try_recv() {
                Ok(message) => Some(message),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let context = if source_advanced.load(Ordering::Acquire) {
                        Self::source_advanced_context(version, true)
                    } else if *source_expected {
                        "The source checkout may have changed; inspect git status and HEAD before retrying. ".to_string()
                    } else {
                        String::new()
                    };
                    Some(UpdateInstallMessage::Failed(format!("{context}The update worker stopped unexpectedly; the running program was not replaced.")))
                }
            },
            _ => None,
        };
        let Some(message) = message else {
            if matches!(self.update_install, UpdateInstallState::Downloading { .. }) {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            return;
        };
        let (version, temp_path, cancel) =
            match std::mem::replace(&mut self.update_install, UpdateInstallState::Idle) {
                UpdateInstallState::Downloading {
                    version,
                    temp_path,
                    cancel,
                    ..
                } => (version, temp_path, cancel),
                _ => return,
            };
        match message {
            UpdateInstallMessage::Cancelled { source_advanced } => {
                let _ = std::fs::remove_file(&temp_path);
                self.show_update_cancelled(&version, source_advanced);
            }
            UpdateInstallMessage::Failed(reason) => {
                let _ = std::fs::remove_file(&temp_path);
                self.update_notice = Some((UpdateNoticeKind::Error, format!(
                    "Couldn't update automatically: {reason} You can still download v{version} in your browser."
                )));
            }
            UpdateInstallMessage::Verified(ready) => {
                if cancel.load(Ordering::Acquire) {
                    let _ = std::fs::remove_file(&ready.paths.download);
                    self.show_update_cancelled(&ready.version, ready.source_advanced);
                } else if self.engine.is_running() {
                    self.engine.request_stop();
                    self.set_status(StatusKind::Stopping, "Stopping before update restart…");
                    self.update_install = UpdateInstallState::WaitingForStop { ready, cancel };
                    ctx.request_repaint();
                } else {
                    self.install_verified_update(ctx, &ready);
                }
            }
        }
    }

    /// Swap only after the click worker has drained. Errors retain recovery
    /// paths and never claim rollback succeeded when a rename failed.
    fn install_verified_update(&mut self, ctx: &egui::Context, ready: &ReadyUpdate) {
        let paths = &ready.paths;
        let version = &ready.version;
        let source_context = Self::source_advanced_context(version, ready.source_advanced);
        if self.macro_recording {
            self.stop_macro_recording("update restart");
        }
        self.macro_playing = false;
        if !self.flush_settings() {
            let _ = std::fs::remove_file(&paths.download);
            self.update_notice = Some((UpdateNoticeKind::Error, format!(
                "{source_context}Settings could not be saved, so the update was not installed. Open Settings for details and retry."
            )));
            return;
        }
        match self_update::swap_in_new_exe(paths) {
            Err(reason) => {
                if paths.backup.exists() {
                    self.update_recovery_backup = Some(paths.backup.clone());
                } else if paths.exe.is_file() {
                    // No recovery backup is needed and the original is still
                    // in place; do not accumulate a discarded candidate.
                    let _ = std::fs::remove_file(&paths.download);
                }
                self.update_notice = Some((UpdateNoticeKind::Error, format!(
                    "{source_context}v{version} was verified but could not be installed: {reason}. This running instance remains open."
                )));
            }
            Ok(()) => {
                let spawned = std::process::Command::new(&paths.exe)
                    .arg(format!("--updated-from={}", env!("CARGO_PKG_VERSION")))
                    .arg(format!("--update-attempt={}", paths.id))
                    .spawn();
                match spawned {
                    Ok(_child) => {
                        self.skip_exit_save = true;
                        self.update_install = UpdateInstallState::Restarting;
                        self.log(format!(
                            "MacroClicker v{version} installed; restarting into it."
                        ));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        let rollback = match self_update::undo_swap(paths) {
                            Ok(()) => "The current program file was restored.".to_string(),
                            Err(reason) => {
                                self.update_recovery_backup = Some(paths.backup.clone());
                                format!("Automatic restoration failed: {reason}. This running instance remains open.")
                            }
                        };
                        self.update_notice = Some((UpdateNoticeKind::Error, format!(
                            "{source_context}The new version couldn't be started ({error}). {rollback}"
                        )));
                    }
                }
            }
        }
    }

    /// One of the Updates window's action buttons. Primary actions
    /// use the theme accent (with black/white text chosen per theme
    /// family — both pairings clear 5.6:1 on every palette), while
    /// secondary actions keep the classic button face.
    fn update_action_button(
        ui: &mut egui::Ui,
        pal: &Palette,
        label: &str,
        enabled: bool,
        primary: bool,
        width: f32,
        height: f32,
    ) -> egui::Response {
        let (fill, border, plain_color) = if !enabled {
            (pal.button_disabled, pal.button_border, pal.text_disabled)
        } else if primary {
            (
                pal.accent,
                pal.accent,
                if pal.dark {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                },
            )
        } else {
            (
                if pal.modern {
                    Color32::TRANSPARENT
                } else {
                    pal.button
                },
                pal.button_border,
                pal.text,
            )
        };
        let text = egui::RichText::new(label).color(plain_color).size(13.0_f32);
        let text = if primary { text.strong() } else { text };
        let galley =
            Self::truncated_galley(ui, text, width - 2.0_f32 * ui.spacing().button_padding.x);
        let elided = galley.elided;
        let ghost_hover =
            (pal.modern && enabled && !primary).then(|| ui.painter().add(egui::Shape::Noop));
        let button = egui::Button::new(galley)
            .fill(fill)
            .stroke(Stroke::new(1.0_f32, border))
            .rounding(Rounding::same(if pal.modern { 8.0_f32 } else { 5.0_f32 }))
            .min_size(Vec2::new(width, height));
        let response = if enabled && ui.is_enabled() {
            ui.add(button)
        } else {
            ui.add_enabled_ui(false, |ui| {
                let inactive = ui.visuals().widgets.inactive;
                ui.style_mut().visuals.widgets.noninteractive = inactive;
                let response = ui.add(button.sense(Sense::hover()));
                response.widget_info(|| {
                    let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, label);
                    info.enabled = false;
                    info
                });
                response
            })
            .inner
        };
        if let Some(slot) = ghost_hover {
            if response.hovered() {
                ui.painter().set(
                    slot,
                    egui::Shape::rect_filled(response.rect, Rounding::same(8.0), pal.hover_fill),
                );
            }
        }
        if pal.modern && enabled && response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.shrink(2.0),
                Rounding::same(6.0),
                Stroke::new(1.2_f32, if primary { plain_color } else { pal.accent }),
            );
        }
        if elided && enabled {
            response.on_hover_text(label)
        } else if elided {
            response.on_disabled_hover_text(label)
        } else {
            response
        }
    }

    /// State glyph for the Updates status card: a spinner while the
    /// request is in flight, otherwise a hand-drawn check, download
    /// arrow, warning mark or idle dot inside a ring — so the state
    /// reads by shape as well as by color.
    fn paint_update_status_icon(ui: &mut egui::Ui, pal: &Palette, state: &UpdateUiState) {
        let (rect, _response) = ui.allocate_exact_size(Vec2::splat(30.0_f32), Sense::hover());
        if matches!(state, UpdateUiState::Checking) {
            ui.put(rect.shrink(3.0_f32), egui::Spinner::new());
            return;
        }
        if !ui.is_rect_visible(rect) {
            return;
        }
        let color = match state {
            UpdateUiState::NotChecked => pal.accent,
            UpdateUiState::UpToDate | UpdateUiState::Available(_) => pal.success,
            UpdateUiState::Failed(_) => pal.warning,
            UpdateUiState::Checking => pal.accent,
        };
        let painter = ui.painter_at(rect.expand(1.0_f32));
        let center = rect.center();
        let at = |x: f32, y: f32| center + Vec2::new(x, y);
        let stroke = Stroke::new(2.0_f32, color);
        painter.circle_stroke(center, 11.0_f32, stroke);
        match state {
            UpdateUiState::UpToDate => {
                painter.line_segment([at(-5.0, 0.0), at(-1.5, 4.0)], stroke);
                painter.line_segment([at(-1.5, 4.0), at(5.5, -4.0)], stroke);
            }
            UpdateUiState::Available(_) => {
                painter.line_segment([at(0.0, -6.0), at(0.0, 3.0)], stroke);
                painter.line_segment([at(-4.0, -0.5), at(0.0, 4.0)], stroke);
                painter.line_segment([at(4.0, -0.5), at(0.0, 4.0)], stroke);
                painter.line_segment([at(-5.5, 6.5), at(5.5, 6.5)], stroke);
            }
            UpdateUiState::Failed(_) => {
                painter.line_segment([at(0.0, -5.5), at(0.0, 1.5)], stroke);
                painter.circle_filled(at(0.0, 5.0), 1.7_f32, color);
            }
            _ => {
                painter.circle_filled(center, 2.2_f32, color);
            }
        }
    }

    /// Compact progress path for the one-click updater. Glyphs carry
    /// completed/current/upcoming as shapes (check, filled marker,
    /// hollow marker); color is reinforcement, not the only signal.
    fn paint_update_steps(ui: &mut egui::Ui, pal: &Palette, steps: &[&str], current: usize) {
        ui.horizontal_wrapped(|ui| {
            for (index, label) in steps.iter().enumerate() {
                if index > 0 {
                    ui.label(
                        egui::RichText::new("\u{2192}")
                            .color(pal.text_dim)
                            .size(11.0_f32),
                    );
                }
                let (glyph, color) = if index < current {
                    ("\u{2713}", pal.success)
                } else if index == current {
                    // Outfit and egui's fallback lack U+25CF, so use the
                    // supported bullet in Modern rather than a missing-glyph box.
                    (if pal.modern { "\u{2022}" } else { "\u{25cf}" }, pal.accent)
                } else {
                    ("\u{25cb}", pal.text_dim)
                };
                let mut text = egui::RichText::new(format!("{glyph} {label}"))
                    .color(color)
                    .size(11.0_f32);
                if index <= current {
                    text = text.strong();
                }
                ui.label(text);
            }
        });
    }

    fn draw_updates_window(&mut self, ctx: &egui::Context) {
        self.refresh_update_check_if_stale();
        let pal = self.palette();
        let mut open = self.show_updates;
        let state = self.update_state.clone();
        let current_version = env!("CARGO_PKG_VERSION");
        let latest_known = self.update_latest_version.clone();
        let last_checked_label = self
            .update_last_checked
            .map(|checked| update::checked_ago_label(checked.elapsed()))
            .unwrap_or_else(|| "Never".to_string());
        let notice = self.update_notice.clone();
        let install_kind_label = self.install_kind_label();
        let is_source_checkout = self.source_checkout().is_some();
        let download_size_label = self
            .update_asset
            .as_ref()
            .map(|asset| update::byte_size_label(asset.size))
            .unwrap_or_else(|| "Not checked yet".to_string());
        let mut check_on_startup = self.prefs.check_updates;

        let available_version = match &state {
            UpdateUiState::Available(version) => Some(version.clone()),
            _ => None,
        };
        let can_install = available_version.is_some() && self.self_update_possible();
        // (version, bytes done, bytes total, phase) while the
        // worker prepares source, streams the release, or verifies it.
        let install_progress = match &self.update_install {
            UpdateInstallState::Downloading {
                version,
                total,
                downloaded,
                phase,
                ..
            } => Some((
                version.clone(),
                downloaded.load(Ordering::Relaxed),
                *total,
                phase.load(Ordering::Relaxed),
            )),
            UpdateInstallState::WaitingForStop { ready, .. } => {
                Some((ready.version.clone(), 0, 0, UPDATE_PHASE_STOPPING))
            }
            _ => None,
        };
        let update_steps: &[&str] = if is_source_checkout {
            &["Source", "Download", "Verify", "Restart"]
        } else {
            &["Download", "Verify", "Install", "Restart"]
        };
        let update_step = match &install_progress {
            Some((_version, _done, _total, UPDATE_PHASE_SOURCE)) => 0,
            Some((_version, _done, _total, UPDATE_PHASE_STOPPING)) => {
                update_steps.len().saturating_sub(1)
            }
            Some((_version, _done, _total, UPDATE_PHASE_VERIFYING)) => {
                if is_source_checkout {
                    2
                } else {
                    1
                }
            }
            Some((_version, _done, _total, _phase)) => {
                if is_source_checkout {
                    1
                } else {
                    0
                }
            }
            None if matches!(self.update_install, UpdateInstallState::Restarting) => {
                update_steps.len().saturating_sub(1)
            }
            None => 0,
        };
        let download_url = available_version
            .as_deref()
            .and_then(update::download_url_for_version);
        let notes_url = available_version
            .as_deref()
            .and_then(update::release_page_url_for_version)
            .unwrap_or_else(|| update::RELEASES_PAGE_URL.to_string());

        let (heading, detail) = match &state {
            UpdateUiState::NotChecked => (
                "Ready to check".to_string(),
                "MacroClicker hasn't asked GitHub yet in this session.".to_string(),
            ),
            UpdateUiState::Checking => (
                "Checking GitHub…".to_string(),
                "Reading the public latest-release metadata — no click settings are sent."
                    .to_string(),
            ),
            UpdateUiState::UpToDate => match latest_known.as_deref() {
                Some(latest) if update::is_newer(current_version, latest) => (
                    "You're ahead of the latest release".to_string(),
                    format!(
                        "GitHub's newest published build is v{latest}; this build is v{current_version}."
                    ),
                ),
                Some(latest) => (
                    "You're up to date".to_string(),
                    format!("v{latest} is the latest published release."),
                ),
                None => (
                    "You're up to date".to_string(),
                    "No newer release is published.".to_string(),
                ),
            },
            UpdateUiState::Available(version) => (
                format!("MacroClicker v{version} is ready"),
                if can_install && is_source_checkout {
                    "Source fast-forward, verified download and restart — one action."
                        .to_string()
                } else if can_install {
                    "Download, fingerprint check and restart — all in here.".to_string()
                } else {
                    "Ready to download from GitHub Releases.".to_string()
                },
            ),
            UpdateUiState::Failed(message) => (
                update::error_headline(message).to_string(),
                message.clone(),
            ),
        };
        let (heading, detail) = match &install_progress {
            Some((version, _done, _total, UPDATE_PHASE_STOPPING)) => (
                format!("Stopping clicks before v{version}…"),
                "Waiting for the click worker to release and finish safely.".to_string(),
            ),
            Some((version, _done, _total, UPDATE_PHASE_SOURCE)) => (
                format!("Updating source to v{version}\u{2026}"),
                "Fast-forwarding the clean main checkout to the exact release tag.".to_string(),
            ),
            Some((version, _done, _total, UPDATE_PHASE_VERIFYING)) => (
                format!("Verifying v{version}\u{2026}"),
                "Checking the download against GitHub's SHA-256 fingerprint.".to_string(),
            ),
            Some((version, _done, _total, _phase)) => (
                format!("Downloading v{version}\u{2026}"),
                "The verified update installs itself, then restarts MacroClicker.".to_string(),
            ),
            None => (heading, detail),
        };
        let latest_label = latest_known
            .as_deref()
            .map(|version| format!("v{version}"))
            .unwrap_or_else(|| "Not checked yet".to_string());
        let checking = matches!(state, UpdateUiState::Checking);
        let check_label = if matches!(state, UpdateUiState::Failed(_)) {
            "Try again".to_string()
        } else if matches!(state, UpdateUiState::NotChecked) {
            "Check for updates".to_string()
        } else {
            "Check now".to_string()
        };
        let download_label = available_version
            .as_deref()
            .map(|version| {
                if can_install {
                    format!("Update to v{version}")
                } else {
                    format!("Download v{version}")
                }
            })
            .unwrap_or_else(|| "Download update…".to_string());
        let download_hover = if can_install && is_source_checkout {
            "Fast-forward this source checkout, download and verify the release, install it, then restart"
        } else if can_install {
            "Download and verify the update, install it, then restart MacroClicker"
        } else {
            "Download the new macro_clicker.exe in your browser"
        };

        let mut check_clicked = false;
        let mut download_clicked = false;
        let mut notes_clicked = false;
        let mut copy_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new(
            egui::RichText::new("Updates")
                .color(pal.text)
                .size(16.0_f32)
                .strong(),
        )
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .max_height(Self::dialog_content_height(ctx))
        .vscroll(true)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .default_width(364.0_f32)
        .min_width(340.0_f32)
        .max_width(400.0_f32)
        .frame(
            Frame::none()
                .fill(if pal.modern { pal.group_fill } else { pal.face })
                .stroke(Stroke::new(1.0_f32, pal.group_border))
                .rounding(Rounding::same(if pal.modern { 12.0_f32 } else { 8.0_f32 }))
                .shadow(if pal.modern {
                    ctx.style().visuals.window_shadow
                } else {
                    egui::epaint::Shadow::NONE
                })
                .inner_margin(Margin::same(12.0_f32)),
        )
        .show(ctx, |ui| {
            // Status plate: glyph, headline and the facts a user
            // needs to trust the verdict — what they run, what
            // GitHub reported, how this build is installed, and how
            // fresh that answer is.
            Frame::none()
                .fill(pal.input_bg)
                .stroke(Stroke::new(1.0_f32, pal.input_border))
                .rounding(Rounding::same(7.0_f32))
                .inner_margin(Margin::same(10.0_f32))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        Self::paint_update_status_icon(ui, pal, &state);
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&heading)
                                        .color(pal.text)
                                        .size(15.0_f32)
                                        .strong(),
                                )
                                .truncate(true),
                            );
                            ui.label(
                                egui::RichText::new(&detail)
                                    .color(pal.text_dim)
                                    .size(12.0_f32),
                            );
                        });
                    });
                    if available_version.is_some() || install_progress.is_some() {
                        ui.add_space(7.0_f32);
                        Self::paint_update_steps(ui, pal, update_steps, update_step);
                    }
                    ui.add_space(8.0_f32);
                    ui.separator();
                    ui.add_space(6.0_f32);
                    // Measure the other fact columns in the active font:
                    // a wider typeface (or installation label) must not let
                    // a long version expand this window beyond the viewport.
                    let fact_font = FontId::proportional(12.0_f32);
                    let measure = |text: &str| {
                        ui.fonts(|fonts| {
                            fonts
                                .layout_no_wrap(text.to_owned(), fact_font.clone(), pal.text)
                                .size()
                                .x
                        })
                    };
                    let first_column = ["Installed", "Installation", "Checked"]
                        .into_iter()
                        .map(measure)
                        .fold(0.0_f32, f32::max);
                    let installed = format!("v{current_version}");
                    let second_column =
                        [installed.as_str(), install_kind_label, &last_checked_label]
                            .into_iter()
                            .map(measure)
                            .fold(0.0_f32, f32::max);
                    let third_column = ["Latest", "Size", "Channel"]
                        .into_iter()
                        .map(measure)
                        .fold(0.0_f32, f32::max);
                    let latest_width = (ui.available_width()
                        - first_column
                        - second_column
                        - third_column
                        - 3.0_f32 * 8.0_f32
                        - 2.0_f32)
                        .max(0.0_f32);
                    Grid::new("updates_facts")
                        .num_columns(4)
                        .spacing([8.0_f32, 3.0_f32])
                        .show(ui, |ui| {
                            let facts = [
                                ("Installed", format!("v{current_version}")),
                                ("Latest", latest_label.clone()),
                                ("Installation", install_kind_label.to_string()),
                                ("Size", download_size_label.clone()),
                                ("Checked", last_checked_label.clone()),
                                ("Channel", "Stable".to_string()),
                            ];
                            for (index, (label, value)) in facts.into_iter().enumerate() {
                                ui.label(
                                    egui::RichText::new(label)
                                        .color(pal.text_dim)
                                        .size(12.0_f32),
                                );
                                let text = egui::RichText::new(value)
                                    .color(pal.text)
                                    .size(12.0_f32)
                                    .strong();
                                if label == "Latest" {
                                    let galley = Self::truncated_galley(ui, text, latest_width);
                                    ui.add(egui::Label::new(galley));
                                } else {
                                    ui.label(text);
                                }
                                if index % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                });

            ui.add_space(10.0_f32);
            let gap = ui.spacing().item_spacing.x;
            let button_width = ((ui.available_width() - gap) / 2.0_f32).max(0.0_f32);
            if let Some((_version, done, total, phase)) = &install_progress {
                if *phase == UPDATE_PHASE_STOPPING {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Waiting for clicks to stop…")
                                .color(pal.text)
                                .size(12.0_f32),
                        );
                    });
                } else if *phase == UPDATE_PHASE_SOURCE {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Updating the source checkout\u{2026}")
                                .color(pal.text)
                                .size(12.0_f32),
                        );
                    });
                } else if *phase == UPDATE_PHASE_VERIFYING {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Verifying the SHA-256 fingerprint\u{2026}")
                                .color(pal.text)
                                .size(12.0_f32),
                        );
                    });
                } else {
                    let fraction = if *total > 0 {
                        (*done as f32 / *total as f32).clamp(0.0_f32, 1.0_f32)
                    } else {
                        0.0_f32
                    };
                    ui.add(egui::ProgressBar::new(fraction));
                    ui.add_space(4.0_f32);
                    ui.label(
                        egui::RichText::new(update::download_progress_label(*done, *total))
                            .color(pal.text_dim)
                            .size(12.0_f32),
                    );
                }
                ui.add_space(8.0_f32);
                ui.horizontal(|ui| {
                    if Self::update_action_button(
                        ui,
                        pal,
                        "Cancel update",
                        true,
                        false,
                        button_width,
                        30.0_f32,
                    )
                    .on_hover_text("Stop this update; the running version stays in place")
                    .clicked()
                    {
                        cancel_clicked = true;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if Self::update_action_button(
                        ui,
                        pal,
                        &check_label,
                        !checking,
                        available_version.is_none(),
                        button_width,
                        34.0_f32,
                    )
                    .on_hover_text("Ask GitHub for the newest published release")
                    .clicked()
                    {
                        check_clicked = true;
                    }
                    if Self::update_action_button(
                        ui,
                        pal,
                        &download_label,
                        download_url.is_some(),
                        available_version.is_some(),
                        button_width,
                        34.0_f32,
                    )
                    .on_hover_text(download_hover)
                    .clicked()
                    {
                        download_clicked = true;
                    }
                });

                if available_version.is_some() {
                    ui.add_space(8.0_f32);
                    ui.horizontal(|ui| {
                        if Self::update_action_button(
                            ui,
                            pal,
                            "Release notes",
                            true,
                            false,
                            button_width,
                            28.0_f32,
                        )
                        .clicked()
                        {
                            notes_clicked = true;
                        }
                        if Self::update_action_button(
                            ui,
                            pal,
                            "Copy download link",
                            true,
                            false,
                            button_width,
                            28.0_f32,
                        )
                        .clicked()
                        {
                            copy_clicked = true;
                        }
                    });
                }
            }

            if let Some((kind, message)) = &notice {
                let color = match kind {
                    UpdateNoticeKind::Info => pal.success,
                    UpdateNoticeKind::Error => pal.warning,
                };
                ui.add_space(7.0_f32);
                ui.label(egui::RichText::new(message).color(color).size(12.0_f32));
            }

            ui.add_space(7.0_f32);
            let fine_print = if matches!(
                &install_progress,
                Some((_version, _done, _total, UPDATE_PHASE_SOURCE))
            ) {
                "Only a clean main checkout is fast-forwarded. Local tracked \
                 changes stop the update before any program file is replaced."
            } else if install_progress.is_some() {
                "Downloading never interrupts a running session; clicking \
                 stops only for the restart itself."
            } else if can_install && is_source_checkout {
                "Update fast-forwards this clean source checkout to the release \
                 tag, verifies the SHA-256 fingerprint, then restarts. Settings are kept."
            } else if can_install {
                "Update downloads, verifies the SHA-256 fingerprint, then \
                 restarts MacroClicker. Settings are kept."
            } else if available_version.is_some() {
                self.self_update_unavailable_reason()
            } else {
                "Nothing installs without a click here; the check only reads \
                 GitHub's public release metadata."
            };
            ui.label(egui::RichText::new(fine_print).color(pal.text_dim).small());

            ui.add_space(8.0_f32);
            ui.separator();
            ui.add_space(6.0_f32);
            ui.checkbox(&mut check_on_startup, "Check for updates on startup");
            ui.label(
                egui::RichText::new("One check per launch; reopening refreshes stale results.")
                    .color(pal.text_dim)
                    .small(),
            );
        });

        if check_on_startup != self.prefs.check_updates {
            self.prefs.check_updates = check_on_startup;
            self.mark_dirty();
        }
        if check_clicked {
            self.start_update_check();
        }
        if cancel_clicked {
            match &self.update_install {
                UpdateInstallState::Downloading { cancel, .. }
                | UpdateInstallState::WaitingForStop { cancel, .. } => {
                    cancel.store(true, Ordering::Release)
                }
                _ => {}
            }
        }
        if download_clicked && can_install {
            self.start_update_install();
        } else if download_clicked {
            match download_url.as_deref() {
                Some(url) if win32::open_url(url) => {
                    let version = available_version.as_deref().unwrap_or("the new version");
                    self.update_notice = Some((
                        UpdateNoticeKind::Info,
                        format!("Download started in your browser for MacroClicker v{version}."),
                    ));
                }
                Some(_) => {
                    self.update_notice = Some((
                        UpdateNoticeKind::Error,
                        "Windows couldn't open your browser. Use “Copy download link” and paste it into a browser.".to_string(),
                    ));
                }
                None => {
                    self.update_notice = Some((
                        UpdateNoticeKind::Error,
                        "That version couldn't be turned into a safe download link. Open the release notes and download from GitHub.".to_string(),
                    ));
                }
            }
        }
        if notes_clicked {
            if win32::open_url(&notes_url) {
                self.update_notice = None;
            } else {
                self.update_notice = Some((
                    UpdateNoticeKind::Error,
                    "Windows couldn't open your browser for the release notes.".to_string(),
                ));
            }
        }
        if copy_clicked {
            if let Some(url) = download_url {
                ctx.copy_text(url);
                self.update_notice = Some((
                    UpdateNoticeKind::Info,
                    "Download link copied to the clipboard.".to_string(),
                ));
            }
        }
        if !open {
            self.show_updates = false;
        }
    }
    /// Draw the dashboard independently of native-window bookkeeping.
    fn draw_dashboard(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        // Single non-scrolling screen: no ScrollArea, everything fits at
        // the OS-enforced floor and still fits above it. Growing the
        // window widens the group boxes and the button grid (all field
        // widths and control heights stay fixed) up to the content cap
        // and centers the column beyond it; past the wide threshold
        // (`adapt_to_window_size` owns the switch) the middle groups go
        // three-abreast and the buttons form one row. Tall windows
        // reflow into narrower compositions. Their extra height is
        // shared within cards after small bounded gaps, keeping the
        // footer close to the last card; classic and wide layouts use
        // the original vertical rhythm. The top margin leaves room for
        // the first group title, which rides half above its frame's top
        // border.
        let window_size = ctx.screen_rect().size();
        let wide = self.main_layout == MainLayout::Wide;
        let portrait = self.main_layout.is_portrait();
        let content_cap = match self.main_layout {
            MainLayout::Classic => MAIN_CONTENT_MAX_WIDTH,
            MainLayout::Portrait | MainLayout::Narrow | MainLayout::Slender => {
                MAIN_CONTENT_MAX_WIDTH_PORTRAIT
            }
            MainLayout::Wide => MAIN_CONTENT_MAX_WIDTH_WIDE,
        };
        let side_margin = Self::centered_side_margin(window_size.x, content_cap);
        let (fill_height, gap_count) = match self.main_layout {
            MainLayout::Classic => (MAIN_WINDOW_SIZE[1], 3.0_f32),
            MainLayout::Portrait => (MAIN_PORTRAIT_FILL_HEIGHT, 4.0_f32),
            MainLayout::Narrow => (MAIN_NARROW_FILL_HEIGHT, 4.0_f32),
            MainLayout::Slender => (MAIN_SLENDER_FILL_HEIGHT, 4.0_f32),
            MainLayout::Wide => (MAIN_WIDE_FILL_HEIGHT, 2.0_f32),
        };
        let (gap_extra, spring, extra_per_card) = if portrait {
            Self::portrait_vertical_rhythm(window_size.y, fill_height)
        } else {
            let (gap, spring) = Self::vertical_rhythm(window_size.y, fill_height, gap_count);
            (gap, spring, 0.0_f32)
        };
        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal.face).inner_margin(Margin {
                left: side_margin,
                right: side_margin,
                // Classic captions straddle the first border; Modern's
                // inset caption needs less space above the first card.
                top: if pal.modern { 8.0_f32 } else { 14.0_f32 },
                bottom: 10.0_f32,
            }))
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(egui::Align::LEFT), |ui| {
                    self.draw_interval_group(ui, extra_per_card);
                    if wide {
                        ui.add_space(7.0_f32 + gap_extra);
                        self.draw_wide_group_row(ui);
                        ui.add_space(6.0_f32 + gap_extra);
                    } else {
                        ui.add_space(7.0_f32 + gap_extra);
                        if portrait {
                            self.draw_portrait_options_and_repeat(ui, gap_extra, extra_per_card);
                        } else {
                            self.draw_options_and_repeat(ui);
                        }
                        ui.add_space(7.0_f32 + gap_extra);
                        self.draw_position_group(ui, extra_per_card);
                        ui.add_space(6.0_f32 + gap_extra);
                    }
                    // Classic and wide layouts keep their status
                    // cluster at the bottom; portrait caps this seam
                    // and balances remaining height inside the cards.
                    ui.add_space(spring);

                    // Reserved one-line status strip; its height never
                    // changes, so the rows below it stay anchored.
                    let (rect, status_response) = ui.allocate_exact_size(
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
                    // Status plate in every theme — the Updates window's
                    // card at strip scale: recessed below the panel in
                    // the dark themes, the cards' white in Classic. It
                    // lives entirely inside the reserved 16 px strip,
                    // so the dot, the text and the rows below never move.
                    painter.rect(
                        rect.shrink2(Vec2::new(0.0_f32, 1.0_f32)),
                        Rounding::same(6.0_f32),
                        pal.well_fill,
                        Stroke::new(1.0_f32, pal.group_border),
                    );
                    let dot_center = egui::pos2(rect.left() + 9.0_f32, rect.center().y);
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
                    // Long errors must not silently run off a narrow
                    // status plate. Ellipsize the visible line and expose
                    // the complete, unchanged message on hover and to
                    // accessibility integrations.
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        shown.to_owned(),
                        FontId::proportional(12.5_f32),
                        state_color,
                    );
                    job.wrap = egui::text::TextWrapping::truncate_at_width(
                        (rect.width() - 21.0_f32).max(0.0_f32),
                    );
                    let galley = ui.fonts(|fonts| fonts.layout_job(job));
                    status_response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, shown));
                    if galley.elided {
                        status_response.on_hover_text(shown);
                    }
                    painter.galley(
                        egui::pos2(
                            rect.left() + 17.0_f32,
                            rect.center().y - galley.size().y / 2.0_f32,
                        ),
                        galley,
                        state_color,
                    );

                    ui.add_space(2.0_f32);
                    self.draw_action_buttons(ui);
                    ui.add_space(5.0_f32);
                    // Portrait puts the reminder on its own line so the
                    // update and settings buttons never collide with it.
                    let hotkey_name = vk_display_name(self.prefs.toggle_hotkey_vk);
                    let reminder = format!("{hotkey_name} start/stop  ·  F8 emergency stop");
                    if portrait {
                        ui.label(egui::RichText::new(&reminder).color(pal.text_dim).small());
                        ui.horizontal(|ui| self.draw_footer_actions(ui, pal));
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&reminder).color(pal.text_dim).small());
                            self.draw_footer_actions(ui, pal);
                        });
                    }
                });
            });
    }

    /// Reconcile fit, dialog zoom, the physical window floor and saved size.
    /// Keeping this independent of eframe's native Frame lets geometry tests
    /// exercise the same resize bookkeeping as the real update loop.
    fn sync_window_layout(&mut self, ctx: &egui::Context) {
        // UI scale and window size: the zoom factor follows the persisted
        // percentage times the window-driven fit (`adapt_to_window_size`
        // owns the fit; it only moves on settled sizes, and because it
        // changes no physical size the point size simply rescales inside
        // the same window). One frame after a zoom change (once egui
        // reports the new factor as applied) the window bookkeeping runs
        // exactly once. The minimum size is re-declared in points —
        // egui-winit multiplies viewport sizes by the current
        // pixels-per-point (zoom x native scale) when it talks to the
        // OS, so the physical floor rescales with the chosen percentage
        // without any multiplication here (doing it in both places would
        // double-scale the floor). It is *not* the raw 560x456 in live
        // points once an auto-fit is active: that would multiply the
        // physical minimum by the transient fit and could leave a
        // large window unable to shrink until restart. The declared
        // live floor is therefore divided by the actually applied auto-fit,
        // including the temporary fit used while a dialog is open. A
        // pending restore (startup, factory reset) applies the persisted
        // size converted to live points. A plain scale change only ever
        // grows the window if the new zoom left it below the floor, so a
        // window the user enlarged is never forced back down.
        let user_factor = self.prefs.ui_scale_percent as f32 / 100.0_f32;
        if let Some(native) = ctx
            .input(|i| i.viewport().native_pixels_per_point)
            .filter(|value| value.is_finite() && *value > 0.0_f32)
        {
            if self
                .last_native_pixels_per_point
                .is_none_or(|previous| (previous - native).abs() > 0.000_1_f32)
            {
                // egui-winit translates this command to physical pixels.
                // Refresh the floor at the *current* applied zoom, but do
                // not request an inner resize: Windows owns the DPI move,
                // maximized windows and Snap geometry.
                let applied_auto = ctx.zoom_factor() / user_factor;
                let floor = Self::live_window_floor_for(applied_auto);
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(floor));
                self.window_settle_until = Instant::now() + Duration::from_millis(750);
                ctx.request_repaint();
            }
            self.last_native_pixels_per_point = Some(native);
        }
        if (user_factor - self.applied_user_factor).abs() > 0.000_1_f32 {
            self.applied_user_factor = user_factor;
            if !self.restore_window_size {
                self.sync_fit_for_user_factor(ctx, user_factor);
            }
        }
        self.apply_shrink_fit_immediately(ctx, user_factor);

        // Advance an in-flight zoom transition (cubic ease-out,
        // 180 ms — the same state-change budget as every other
        // motion here). The target is the adopted factor; landing
        // sets the displayed factor exactly, so the zoom always
        // terminates on the fit value, never near it. While a
        // transition is in motion (including its landing frame) the
        // desired zoom is applied unconditionally: the ease-out's
        // final steps are smaller than the resting dead band below,
        // and without the override the applied zoom would freeze a
        // fraction short of the fit factor forever.
        let mut zoom_in_motion = false;
        if let Some((started, from)) = self.zoom_transition {
            const ZOOM_TRANSITION_SECS: f64 = 0.18_f64;
            zoom_in_motion = true;
            let t = ((self.ui_clock_secs - started) / ZOOM_TRANSITION_SECS).clamp(0.0_f64, 1.0_f64)
                as f32;
            let ease = 1.0_f32 - (1.0_f32 - t).powi(3);
            self.displayed_auto_zoom = from + (self.auto_zoom - from) * ease;
            if t >= 1.0_f32 {
                self.displayed_auto_zoom = self.auto_zoom;
                self.zoom_transition = None;
            } else {
                ctx.request_repaint();
            }
        }
        let dashboard_zoom = user_factor * self.displayed_auto_zoom;
        let desired_zoom = if self.any_dialog_open() {
            let neutral = ctx.screen_rect().size() * ctx.zoom_factor();
            window_layout::dialog_safe_zoom((neutral.x, neutral.y), dashboard_zoom)
        } else {
            dashboard_zoom
        };
        if zoom_in_motion || (ctx.zoom_factor() - desired_zoom).abs() > 0.001_f32 {
            ctx.set_zoom_factor(desired_zoom);
            self.pending_resize = true;
            ctx.request_repaint();
        } else if self.pending_resize || self.restore_window_size {
            self.pending_resize = false;
            // Use the zoom actually applied, which may be temporarily
            // smaller while a dialog is open, to keep the physical floor
            // and restored window size independent of that zoom.
            let applied_auto_zoom = ctx.zoom_factor() / user_factor;
            let applied_floor = Self::live_window_floor_for(applied_auto_zoom);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(applied_floor));
            if self.restore_window_size {
                self.restore_window_size = false;
                // Fresh install (no settings file at load) at the
                // classic floor: upgrade the first size to the
                // display-aware default for this monitor, so a
                // 1440p desktop opens with the dialog at the same
                // screen fraction it has on the 1080p reference
                // instead of comparatively tiny. Any existing file
                // — including one that deliberately stores the
                // floor — skips this entirely, and the size chosen
                // here is then observed and remembered like any
                // other, so it happens exactly once.
                if !self.window_size_was_persisted
                    && self.prefs.window_width == hotkey::WINDOW_MIN_WIDTH
                    && self.prefs.window_height == hotkey::WINDOW_MIN_HEIGHT
                {
                    if let Some(monitor_size) = ctx.input(|i| i.viewport().monitor_size) {
                        let user_factor = self.prefs.ui_scale_percent as f32 / 100.0_f32;
                        let target = Self::display_default_window_size(
                            monitor_size * ctx.zoom_factor(),
                            user_factor,
                        );
                        let (width, height) =
                            hotkey::sanitize_window_size(f64::from(target.x), f64::from(target.y));
                        if width != self.prefs.window_width || height != self.prefs.window_height {
                            self.prefs.window_width = width;
                            self.prefs.window_height = height;
                            self.mark_dirty();
                        }
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                    self.prefs.window_width as f32 / applied_auto_zoom,
                    self.prefs.window_height as f32 / applied_auto_zoom,
                )));
            } else {
                let live = ctx.input(|i| i.screen_rect.size());
                if live.x.is_finite() && live.y.is_finite() && live.x > 0.0_f32 && live.y > 0.0_f32
                {
                    let grown = Vec2::new(live.x.max(applied_floor.x), live.y.max(applied_floor.y));
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
    }
}

impl eframe::App for OpAutoClickerApp {
    fn persist_egui_memory(&self) -> bool {
        // Our tolerant settings file is the sole source of window and
        // scale state; stale egui memory must not restore a second zoom.
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_clock_secs = ctx.input(|i| i.time);
        self.apply_style(ctx);

        // One background update check per launch (unless the user
        // switched it off in the Updates window; manual checks are
        // always available there). The worker resolves the state
        // exactly once and the poll applies it on whatever frame
        // happens to run next, so the UI never waits on the network.
        if self.prefs.check_updates && !self.update_auto_check_started {
            self.update_auto_check_started = true;
            self.start_update_check();
        }
        self.poll_update_check();
        self.poll_update_install(ctx);

        self.sync_window_layout(ctx);

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
            self.was_running = true;
        }
        self.reconcile_session_status();

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

        self.draw_dashboard(ctx);
        self.draw_dialog_backdrop(ctx);

        // Opening a dialog can happen while drawing the dashboard, after
        // this frame's zoom decision. Wait until the safe zoom is actually
        // applied before drawing the dialog, so it never appears clipped.
        if self.any_dialog_open() {
            let applied_zoom = ctx.zoom_factor();
            let neutral = ctx.screen_rect().size() * applied_zoom;
            let safe_zoom = window_layout::dialog_safe_zoom((neutral.x, neutral.y), applied_zoom);
            if safe_zoom + 0.001_f32 < applied_zoom {
                ctx.request_repaint();
                return;
            }
        }

        if self.show_settings {
            self.draw_settings_window(ctx);
        }
        if self.show_updates {
            self.draw_updates_window(ctx);
        }
        if self.show_hotkey_dialog {
            self.draw_hotkey_dialog(ctx);
        }
        if self.show_macro_window {
            self.draw_macro_window(ctx);
        }
        if self.show_font_license {
            self.draw_font_license_window(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // After an update relaunch the replacement process has
        // already read the settings flushed before the swap; a
        // second write from this dying process could only race it.
        if !self.skip_exit_save {
            self.settings_dirty = true;
            self.flush_settings();
        }
    }
}

#[cfg(test)]
mod dashboard_layout_tests {
    use super::*;
    use egui::{epaint::Shape, Event, FullOutput, Modifiers, PointerButton, Pos2, RawInput, Rect};

    fn test_app(layout: MainLayout) -> OpAutoClickerApp {
        let mut app = OpAutoClickerApp::new();
        app.prefs = AppPrefs::default();
        app.config = EngineConfig::default();
        app.sync_interval_editors();
        app.main_layout = layout;
        app.install_kind = InstallKind::Unknown;
        app.exe_path = None;
        app.settings_path = None;
        app.settings_load_error = None;
        app.last_save_error = None;
        app.update_state = UpdateUiState::Available("1.6.0".to_string());
        app.status_text = "Ready.".to_string();
        app
    }

    fn render(
        app: &mut OpAutoClickerApp,
        ctx: &egui::Context,
        size: Vec2,
        ppp: f32,
        events: Vec<Event>,
    ) -> FullOutput {
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        ctx.run(input, |ctx| {
            app.apply_style(ctx);
            app.draw_dashboard(ctx);
        })
    }

    fn collect_text(shape: &Shape, clip: Rect, result: &mut Vec<(String, Rect, Rect)>) {
        match shape {
            Shape::Vec(shapes) => {
                for child in shapes {
                    collect_text(child, clip, result);
                }
            }
            Shape::Text(text) if !text.galley.job.text.is_empty() => {
                result.push((
                    text.galley.job.text.clone(),
                    text.visual_bounding_rect(),
                    clip,
                ));
            }
            _ => {}
        }
    }

    fn text_bounds(output: &FullOutput) -> Vec<(String, Rect, Rect)> {
        let mut result = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, clipped.clip_rect, &mut result);
        }
        result
    }

    #[test]
    fn dashboard_text_fits_and_does_not_overlap_in_every_composition() {
        for (layout, size) in [
            (MainLayout::Classic, Vec2::new(560.0, 456.0)),
            (MainLayout::Portrait, Vec2::new(400.0, 600.0)),
            (MainLayout::Portrait, Vec2::new(440.0, 680.0)),
            (MainLayout::Portrait, Vec2::new(400.0, 710.0)),
            (MainLayout::Narrow, Vec2::new(280.0, 700.0)),
            (MainLayout::Narrow, Vec2::new(287.0, 701.0)),
            (MainLayout::Narrow, Vec2::new(280.0, 760.0)),
            (MainLayout::Slender, Vec2::new(220.0, 740.0)),
            (MainLayout::Slender, Vec2::new(235.0, 780.0)),
            (MainLayout::Slender, Vec2::new(220.6, 856.8)),
            (MainLayout::Wide, Vec2::new(700.0, 456.0)),
            (MainLayout::Wide, Vec2::new(904.0, 456.0)),
        ] {
            for ppp in [1.0, 1.25, 1.5, 2.0] {
                for theme in AppTheme::all() {
                    for hotkey in [0x87, 0x20] {
                        // F24 and Space, the longest supported names.
                        let ctx = egui::Context::default();
                        let mut app = test_app(layout);
                        app.prefs.theme = theme;
                        app.prefs.toggle_hotkey_vk = hotkey;
                        app.config.coordinate_mode = CoordinateMode::FixedScreen;
                        app.config.mouse_button = MouseButton::Middle;
                        app.config.click_type = ClickType::Triple;
                        app.config.fixed_x = -100_000;
                        app.config.fixed_y = 100_000;
                        app.repeat_limited = true;
                        app.config.max_clicks = 1_000_000_000;
                        let mut output = render(&mut app, &ctx, size, ppp, vec![]);
                        for _ in 0..5 {
                            output = render(&mut app, &ctx, size, ppp, vec![]);
                        }
                        let text = text_bounds(&output);
                        let modern = app.palette().modern;
                        for expected in [
                            if modern {
                                "CLICK INTERVAL"
                            } else {
                                "Click interval"
                            },
                            if modern {
                                "CLICK OPTIONS"
                            } else {
                                "Click options"
                            },
                            if modern {
                                "CLICK REPEAT"
                            } else {
                                "Click repeat"
                            },
                            if modern {
                                "CURSOR POSITION"
                            } else {
                                "Cursor position"
                            },
                            if layout == MainLayout::Slender {
                                "Record / Play"
                            } else {
                                "Record & Playback"
                            },
                            "Update v1.6.0",
                            "Settings…",
                        ] {
                            assert!(
                                text.iter().any(|(label, _, _)| label == expected),
                                "{layout:?} / {theme:?} at {ppp}: missing {expected}"
                            );
                        }
                        let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
                        for (index, (label, rect, clip)) in text.iter().enumerate() {
                            assert!(
                            screen.contains_rect(*rect),
                            "{layout:?} / {theme:?} at {ppp}: {label:?} outside window: {rect:?}"
                        );
                            assert!(
                        clip.expand(0.5).contains_rect(*rect),
                        "{layout:?} / {theme:?} at {ppp}: {label:?} clipped: {rect:?} by {clip:?}"
                    );
                            for (other, other_rect, _) in &text[index + 1..] {
                                let overlap = rect.intersect(*other_rect);
                                assert!(
                            overlap.width() <= 0.5 || overlap.height() <= 0.5,
                            "{layout:?} / {theme:?} at {ppp}: {label:?} overlaps {other:?}: {overlap:?}"
                        );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn modern_slender_cards_stay_aligned_with_long_values() {
        fn collect_cards(shape: &Shape, pal: &Palette, cards: &mut Vec<Rect>) {
            match shape {
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_cards(shape, pal, cards);
                    }
                }
                Shape::Rect(rect)
                    if rect.fill == pal.group_fill
                        && rect.stroke.color == pal.group_border
                        && rect.rect.height() > 40.0 =>
                {
                    cards.push(rect.rect);
                }
                _ => {}
            }
        }

        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            for size in [Vec2::new(220.0, 740.0), Vec2::new(235.0, 780.0)] {
                for ppp in [1.0, 1.25, 1.5, 2.0] {
                    let ctx = egui::Context::default();
                    let mut app = test_app(MainLayout::Slender);
                    app.prefs.theme = theme;
                    app.repeat_limited = true;
                    app.config.max_clicks = 1_000_000_000;
                    app.config.coordinate_mode = CoordinateMode::FixedScreen;
                    app.config.fixed_x = -100_000;
                    app.config.fixed_y = 100_000;
                    let mut output = render(&mut app, &ctx, size, ppp, vec![]);
                    for _ in 0..5 {
                        output = render(&mut app, &ctx, size, ppp, vec![]);
                    }
                    let mut cards = Vec::new();
                    for clipped in &output.shapes {
                        collect_cards(&clipped.shape, app.palette(), &mut cards);
                    }
                    assert_eq!(cards.len(), 4, "{theme:?} at {size:?}, {ppp}");
                    for rect in cards {
                        assert!(
                            (rect.left() - 12.0).abs() <= 1.0
                                && (rect.right() - (size.x - 12.0)).abs() <= 1.0,
                            "{theme:?} at {size:?}, {ppp}: misaligned card {rect:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn vertical_resize_keeps_card_and_footer_gaps_bounded() {
        fn collect_cards(shape: &Shape, pal: &Palette, width: f32, cards: &mut Vec<Rect>) {
            match shape {
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_cards(shape, pal, width, cards);
                    }
                }
                Shape::Rect(rect)
                    if rect.fill == pal.group_fill
                        && rect.stroke.color == pal.group_border
                        && rect.rect.width() > width - 30.0
                        && rect.rect.height() > 40.0 =>
                {
                    cards.push(rect.rect);
                }
                _ => {}
            }
        }

        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            let mut current = MainLayout::Classic;
            for height in [
                700.0, 840.0, 900.0, 1000.0, 1010.0, 1200.0, 1360.0, 1450.0, 1490.0, 1500.0,
                1700.0, 1824.0, 2160.0,
            ] {
                let neutral = Vec2::new(560.0, height);
                let layout = OpAutoClickerApp::layout_for(neutral, 1.0, current, true);
                let zoom = OpAutoClickerApp::fit_auto_zoom(neutral, 1.0, layout);
                let size = neutral / zoom;
                let ctx = egui::Context::default();
                let mut app = test_app(layout);
                app.prefs.theme = theme;
                let mut output = render(&mut app, &ctx, size, zoom, vec![]);
                for _ in 0..5 {
                    output = render(&mut app, &ctx, size, zoom, vec![]);
                }
                let mut cards = Vec::new();
                for clipped in &output.shapes {
                    collect_cards(&clipped.shape, app.palette(), size.x, &mut cards);
                }
                cards.sort_by(|a, b| a.top().total_cmp(&b.top()));
                assert_eq!(cards.len(), 4, "{height}px / {layout:?}: {cards:?}");
                for pair in cards.windows(2) {
                    let gap_pixels = (pair[1].top() - pair[0].bottom()) * zoom;
                    assert!(
                        (0.0..60.0).contains(&gap_pixels),
                        "{height}px / {layout:?}: card gap {gap_pixels}px"
                    );
                }
                let text = text_bounds(&output);
                let bounds = |label: &str| {
                    text.iter()
                        .find(|(text, _, _)| text == label)
                        .map(|(_, rect, _)| *rect)
                        .unwrap_or_else(|| panic!("{height}px: missing {label:?}"))
                };
                let status_gap = (bounds("Ready.").top() - cards[3].bottom()) * zoom;
                assert!(
                    (0.0..100.0).contains(&status_gap),
                    "{height}px / {layout:?}: footer gap {status_gap}px"
                );
                let bottom_gap = (size.y - bounds("Settings…").bottom()) * zoom;
                assert!(
                    (0.0..50.0).contains(&bottom_gap),
                    "{height}px / {layout:?}: bottom gap {bottom_gap}px"
                );
                current = layout;
            }
        }
    }

    #[test]
    fn long_status_is_ellipsized_and_hover_reveals_the_full_message() {
        let message = "Click worker lost — session aborted. Restart the app before clicking again.";
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for (layout, size) in [
                (MainLayout::Classic, Vec2::new(560.0, 456.0)),
                (MainLayout::Narrow, Vec2::new(280.0, 700.0)),
                (MainLayout::Slender, Vec2::new(220.0, 740.0)),
            ] {
                let ctx = egui::Context::default();
                let mut app = test_app(layout);
                app.prefs.theme = theme;
                app.status_kind = StatusKind::Error;
                app.status_text = message.to_owned();
                let mut output = render(&mut app, &ctx, size, 1.0, vec![]);
                for _ in 0..5 {
                    output = render(&mut app, &ctx, size, 1.0, vec![]);
                }
                let (_, rect, clip) = text_bounds(&output)
                    .into_iter()
                    .find(|(label, _, _)| label == message)
                    .expect("status text");
                assert!(
                    clip.expand(0.5).contains_rect(rect),
                    "{layout:?}: {rect:?} by {clip:?}"
                );
                assert!(Rect::from_min_size(Pos2::ZERO, size)
                    .expand(0.5)
                    .contains_rect(rect));
                if layout == MainLayout::Slender {
                    let pointer = Pos2::new(clip.center().x, clip.center().y);
                    for _ in 0..50 {
                        output = render(
                            &mut app,
                            &ctx,
                            size,
                            1.0,
                            vec![Event::PointerMoved(pointer)],
                        );
                    }
                    let copies = text_bounds(&output)
                        .into_iter()
                        .filter(|(text, _, _)| text == message)
                        .count();
                    assert!(
                        copies >= 2,
                        "truncated status should reveal its full text on hover"
                    );
                }
            }
        }
    }

    #[test]
    fn screenshot_sized_window_scales_controls_and_avoids_the_footer_void() {
        // The reported image is 556x1388 including the native title bar.
        // Use a realistic 556x1360 inner viewport to exercise the defect.
        let neutral = Vec2::new(556.0, 1360.0);
        let layout = OpAutoClickerApp::layout_for(neutral, 1.0, MainLayout::Classic, true);
        let zoom = OpAutoClickerApp::fit_auto_zoom(neutral, 1.0, layout);
        let old_portrait_zoom = OpAutoClickerApp::fit_auto_zoom(neutral, 1.0, MainLayout::Portrait);
        assert_eq!(layout, MainLayout::Narrow);
        assert!(zoom >= 1.90 && zoom >= old_portrait_zoom * 1.35);

        let size = neutral / zoom;
        let ctx = egui::Context::default();
        let mut app = test_app(layout);
        app.config.coordinate_mode = CoordinateMode::FixedScreen;
        let mut output = render(&mut app, &ctx, size, zoom, vec![]);
        for _ in 0..5 {
            output = render(&mut app, &ctx, size, zoom, vec![]);
        }
        let text = text_bounds(&output);
        let bounds = |label: &str| {
            text.iter()
                .find(|(text, _, _)| text == label)
                .map(|(_, rect, _)| *rect)
                .unwrap_or_else(|| panic!("missing {label:?}"))
        };
        let last_coordinate = bounds("Y: 0");
        let status = bounds("Ready.");
        let footer = bounds("Settings…");
        let footer_gap_pixels = (status.top() - last_coordinate.bottom()) * zoom;
        assert!(
            footer_gap_pixels < 120.0,
            "footer gap: {footer_gap_pixels}px"
        );
        assert!(
            (size.y - footer.bottom()) * zoom < 50.0,
            "footer not near bottom"
        );
    }

    fn render_dialog(
        app: &mut OpAutoClickerApp,
        ctx: &egui::Context,
        size: Vec2,
        ppp: f32,
        name: &str,
        events: Vec<Event>,
    ) -> FullOutput {
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        ctx.run(input, |ctx| {
            app.apply_style(ctx);
            match name {
                "settings" => app.draw_settings_window(ctx),
                "hotkey" => app.draw_hotkey_dialog(ctx),
                "macro" => app.draw_macro_window(ctx),
                "updates" => app.draw_updates_window(ctx),
                _ => unreachable!(),
            }
        })
    }

    #[test]
    fn ordinary_update_labels_remain_full_length_in_each_layout() {
        fn elided(shape: &Shape, expected: &str) -> Option<bool> {
            match shape {
                Shape::Vec(shapes) => shapes.iter().find_map(|shape| elided(shape, expected)),
                Shape::Text(text) if text.galley.job.text == expected => Some(text.galley.elided),
                _ => None,
            }
        }
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for (layout, size) in [
                (MainLayout::Classic, Vec2::new(560.0, 456.0)),
                (MainLayout::Portrait, Vec2::new(400.0, 600.0)),
                (MainLayout::Narrow, Vec2::new(280.0, 700.0)),
                (MainLayout::Slender, Vec2::new(220.0, 740.0)),
                (MainLayout::Wide, Vec2::new(700.0, 456.0)),
            ] {
                let ctx = egui::Context::default();
                let mut app = test_app(layout);
                app.prefs.theme = theme;
                let mut output = render(&mut app, &ctx, size, 1.0, vec![]);
                for _ in 0..5 {
                    output = render(&mut app, &ctx, size, 1.0, vec![]);
                }
                let is_elided = output
                    .shapes
                    .iter()
                    .find_map(|clipped| elided(&clipped.shape, "Update v1.6.0"));
                assert_eq!(is_elided, Some(false), "{layout:?}");
            }
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            app.show_updates = true;
            app.update_last_checked = Some(Instant::now());
            app.exe_path = Some(std::path::PathBuf::from("macro_clicker.exe"));
            app.install_kind = InstallKind::Portable;
            app.update_asset = Some(ReleaseAsset {
                size: 4_000_000,
                sha256: Some([0; 32]),
            });
            let size = Vec2::new(420.0, 420.0);
            let mut output = render_dialog(&mut app, &ctx, size, 1.0, "updates", vec![]);
            for _ in 0..5 {
                output = render_dialog(&mut app, &ctx, size, 1.0, "updates", vec![]);
            }
            let is_elided = output
                .shapes
                .iter()
                .find_map(|clipped| elided(&clipped.shape, "Update to v1.6.0"));
            assert_eq!(is_elided, Some(false));
        }
    }

    #[test]
    fn long_valid_version_keeps_update_labels_and_controls_within_view() {
        let version = "4294967295.4294967295.4294967295";
        assert!(update::parse_version(version).is_some());
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for name in ["dashboard", "updates"] {
                let size = if name == "dashboard" {
                    Vec2::new(220.0, 740.0)
                } else {
                    Vec2::new(420.0, 420.0)
                };
                let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Slender);
                app.prefs.theme = theme;
                app.update_state = UpdateUiState::Available(version.to_owned());
                app.update_latest_version = Some(version.to_owned());
                app.update_last_checked = Some(Instant::now());
                app.show_updates = name == "updates";
                app.exe_path = Some(std::path::PathBuf::from("macro_clicker.exe"));
                app.install_kind = InstallKind::Portable;
                app.update_asset = Some(ReleaseAsset {
                    size: 4_000_000,
                    sha256: Some([0; 32]),
                });
                let frame = |app: &mut OpAutoClickerApp, events| {
                    if name == "dashboard" {
                        render(app, &ctx, size, 1.0, events)
                    } else {
                        render_dialog(app, &ctx, size, 1.0, name, events)
                    }
                };
                let mut output = frame(&mut app, vec![]);
                for _ in 0..5 {
                    output = frame(&mut app, vec![]);
                }
                let expected = if name == "dashboard" {
                    format!("Update v{version}")
                } else {
                    format!("Update to v{version}")
                };
                let text = text_bounds(&output);
                let (_, button_text, _) = text
                    .iter()
                    .find(|(label, _, _)| label == &expected)
                    .unwrap_or_else(|| panic!("missing {name} action {expected}"));
                let pointer = button_text.center();
                for (label, rect, clip) in text {
                    assert!(screen.contains_rect(rect), "{name}: {label:?} at {rect:?}");
                    assert!(
                        clip.expand(0.5).contains_rect(rect),
                        "{name}: {label:?}: {rect:?} by {clip:?}"
                    );
                }
                for _ in 0..50 {
                    output = frame(&mut app, vec![Event::PointerMoved(pointer)]);
                }
                assert!(
                    text_bounds(&output)
                        .iter()
                        .any(|(label, _, _)| label != &expected && label.contains(version)),
                    "{name} must reveal the full release version on hover"
                );
                if name == "dashboard" {
                    // The test avoids actually starting an update, but verifies
                    // the adjacent Settings action stays reachable at this width.
                    click_text_at(&mut app, &ctx, size, "Settings…");
                    assert!(app.show_settings);
                }
            }
        }
    }

    #[test]
    fn update_dialog_bounds_hold_for_each_appearance_and_install_type() {
        let size = Vec2::new(420.0, 420.0);
        let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source = self_update::SourceCheckout {
            root: crate_dir.clone(),
            manifest: crate_dir.join("Cargo.toml"),
            crate_dir,
            profile: "release".to_owned(),
        };
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for ppp in [1.0, 1.25, 1.5, 2.0] {
                for kind in [
                    InstallKind::Portable,
                    InstallKind::SourceCheckout(source.clone()),
                    InstallKind::CargoBuild,
                    InstallKind::Unknown,
                ] {
                    for version in ["1.6.0", "4294967295.4294967295.4294967295"] {
                        for asset_size in [None, Some(4_000_000), Some(u64::MAX)] {
                            let ctx = egui::Context::default();
                            let mut app = test_app(MainLayout::Slender);
                            app.prefs.theme = theme;
                            app.install_kind = kind.clone();
                            app.exe_path = Some(PathBuf::from("macro_clicker.exe"));
                            app.update_state = UpdateUiState::Available(version.to_owned());
                            app.update_latest_version = Some(version.to_owned());
                            app.update_last_checked = Some(Instant::now());
                            app.update_asset = asset_size.map(|size| ReleaseAsset {
                                size,
                                sha256: Some([0; 32]),
                            });
                            app.show_updates = true;
                            let mut output =
                                render_dialog(&mut app, &ctx, size, ppp, "updates", vec![]);
                            for _ in 0..5 {
                                output =
                                    render_dialog(&mut app, &ctx, size, ppp, "updates", vec![]);
                            }
                            for (label, rect, clip) in text_bounds(&output) {
                                // Long source-update content may legitimately
                                // scroll vertically; horizontal overflow cannot.
                                assert!(
                                rect.left() >= screen.left()
                                    && rect.right() <= screen.right()
                                    && rect.left() >= clip.left() - 0.5
                                    && rect.right() <= clip.right() + 0.5,
                                "{theme:?} / {kind:?} / {ppp} / {version} / {asset_size:?}: {label:?} at {rect:?}, clipped by {clip:?}"
                            );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn long_latest_version_reveals_its_full_label_on_hover() {
        let version = "4294967295.4294967295.4294967295";
        let full_label = format!("v{version}");
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            let size = Vec2::new(420.0, 420.0);
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            app.show_updates = true;
            app.update_latest_version = Some(version.to_owned());
            app.update_last_checked = Some(Instant::now());
            app.install_kind = InstallKind::Portable;
            let mut output = render_dialog(&mut app, &ctx, size, 1.0, "updates", vec![]);
            for _ in 0..6 {
                output = render_dialog(&mut app, &ctx, size, 1.0, "updates", vec![]);
            }
            let pointer = text_bounds(&output)
                .into_iter()
                .find(|(label, _, _)| label == &full_label)
                .map(|(_, rect, _)| rect.center())
                .expect("full source text must remain in the facts label");
            let button = |pressed| Event::PointerButton {
                pos: pointer,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::default(),
            };
            let _ = render_dialog(
                &mut app,
                &ctx,
                size,
                1.0,
                "updates",
                vec![Event::PointerMoved(pointer), button(true)],
            );
            output = render_dialog(&mut app, &ctx, size, 1.0, "updates", vec![button(false)]);
            assert!(
                output.platform_output.events.iter().any(|event| {
                    matches!(event, egui::output::OutputEvent::Clicked(info)
                    if info.label.as_deref() == Some(full_label.as_str()))
                }),
                "{theme:?}: the egui accessibility event must retain the full label"
            );
            for _ in 0..50 {
                output = render_dialog(
                    &mut app,
                    &ctx,
                    size,
                    1.0,
                    "updates",
                    vec![Event::PointerMoved(pointer)],
                );
            }
            let copies = text_bounds(&output)
                .into_iter()
                .filter(|(label, _, _)| label == &full_label)
                .count();
            assert!(
                copies >= 2,
                "{theme:?}: the elided facts label must show its complete text on hover"
            );
        }
    }

    #[test]
    fn long_errors_keep_dialog_title_and_bottom_controls_reachable() {
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for name in ["settings", "updates"] {
                let size = Vec2::new(420.0, 420.0);
                let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Classic);
                app.prefs.theme = theme;
                app.update_last_checked = Some(Instant::now());
                app.show_settings = name == "settings";
                app.show_updates = name == "updates";
                if name == "settings" {
                    app.last_save_error = Some("Access denied while saving settings. ".repeat(30));
                } else {
                    app.update_state = UpdateUiState::Failed(
                        "The update request timed out, please try again. ".repeat(30),
                    );
                }
                let title = if name == "settings" {
                    "Settings"
                } else {
                    "Updates"
                };
                let bottom = if name == "settings" {
                    "Reset all settings to factory defaults"
                } else {
                    "Check for updates on startup"
                };
                let mut output = render_dialog(&mut app, &ctx, size, 1.0, name, vec![]);
                for _ in 0..6 {
                    output = render_dialog(&mut app, &ctx, size, 1.0, name, vec![]);
                }
                let top = text_bounds(&output)
                    .into_iter()
                    .find(|(label, _, _)| label == title)
                    .expect("dialog title must remain visible");
                assert!(screen.contains_rect(top.1));
                assert!(top.2.expand(0.5).contains_rect(top.1));
                for _ in 0..5 {
                    output = render_dialog(
                        &mut app,
                        &ctx,
                        size,
                        1.0,
                        name,
                        vec![
                            Event::PointerMoved(Pos2::new(210.0, 210.0)),
                            Event::Scroll(Vec2::new(0.0, -1000.0)),
                        ],
                    );
                }
                for _ in 0..8 {
                    output = render_dialog(&mut app, &ctx, size, 1.0, name, vec![]);
                }
                let (label, rect, clip) = text_bounds(&output)
                    .into_iter()
                    .find(|(label, _, _)| label == bottom)
                    .unwrap_or_else(|| panic!("{name} bottom control was not reachable"));
                assert!(screen.contains_rect(rect), "{name}: {label:?}: {rect:?}");
                assert!(
                    clip.expand(0.5).contains_rect(rect),
                    "{name}: {label:?}: {rect:?} by {clip:?}"
                );
                let title_rect = text_bounds(&output)
                    .into_iter()
                    .find(|(label, _, _)| label == title)
                    .expect("title after scroll")
                    .1;
                assert!(
                    screen.contains_rect(title_rect),
                    "{name} title left the viewport"
                );
                if name == "updates" {
                    assert!(app.prefs.check_updates);
                    click_dialog_text(&mut app, &ctx, size, name, bottom);
                    assert!(!app.prefs.check_updates);
                } else {
                    click_dialog_text(&mut app, &ctx, size, name, bottom);
                    assert_eq!(app.prefs.theme, AppTheme::Classic);
                    assert!(
                        app.settings_dirty,
                        "an unavailable settings path must keep the retry pending"
                    );
                    assert!(app.status_kind == StatusKind::Warning);
                }
            }
        }
    }

    #[test]
    fn settings_recovery_details_wrap_and_keep_the_reset_control_reachable() {
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            let size = Vec2::new(420.0, 420.0);
            let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            app.show_settings = true;
            app.settings_load_error = Some(format!(
                "Could not read C:/{}: settings file is not UTF-8. The original file was preserved. Close MacroClicker, repair or move the file, then reopen the app.",
                "long-profile-name/".repeat(15)
            ));
            for _ in 0..8 {
                let _ = render_dialog(&mut app, &ctx, size, 1.0, "settings", vec![]);
            }
            let mut output = render_dialog(&mut app, &ctx, size, 1.0, "settings", vec![]);
            let title = text_bounds(&output)
                .into_iter()
                .find(|(label, _, _)| label == "Settings")
                .expect("settings title");
            assert!(screen.contains_rect(title.1));
            for _ in 0..10 {
                output = render_dialog(
                    &mut app,
                    &ctx,
                    size,
                    1.0,
                    "settings",
                    vec![
                        Event::PointerMoved(Pos2::new(210.0, 210.0)),
                        Event::Scroll(Vec2::new(0.0, -1000.0)),
                    ],
                );
            }
            let reset = text_bounds(&output)
                .into_iter()
                .find(|(label, _, _)| label == "Reset all settings to factory defaults")
                .expect("reset remains reachable");
            assert!(
                screen.contains_rect(reset.1),
                "{theme:?}: reset outside viewport"
            );
            assert!(
                reset.2.expand(0.5).contains_rect(reset.1),
                "{theme:?}: reset clipped"
            );
        }
    }

    #[test]
    fn secondary_windows_fit_at_the_tall_dashboard_dialog_zoom() {
        for neutral in [
            Vec2::new(560.0, 456.0),
            Vec2::new(556.0, 1360.0),
            Vec2::new(560.0, 2160.0),
        ] {
            let layout = OpAutoClickerApp::layout_for(neutral, 1.0, MainLayout::Classic, true);
            let dashboard_zoom = OpAutoClickerApp::fit_auto_zoom(neutral, 1.0, layout);
            let dialog_zoom =
                window_layout::dialog_safe_zoom((neutral.x, neutral.y), dashboard_zoom);
            let size = neutral / dialog_zoom;
            let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
            for ppp in [1.0, 1.25, 2.0] {
                for theme in AppTheme::all() {
                    for name in ["settings", "hotkey", "macro", "updates"] {
                        let ctx = egui::Context::default();
                        let mut app = test_app(layout);
                        app.prefs.theme = theme;
                        app.prefs.ui_scale_percent = 175; // Legacy value remains hidden in Settings.
                        app.update_last_checked = Some(Instant::now());
                        app.show_settings = name == "settings";
                        app.show_hotkey_dialog = name == "hotkey";
                        app.show_macro_window = name == "macro";
                        app.show_updates = name == "updates";
                        app.macro_until_stopped = false;
                        app.macro_loops = 1_000_000;
                        let mut output = render_dialog(&mut app, &ctx, size, ppp, name, vec![]);
                        for _ in 0..5 {
                            output = render_dialog(&mut app, &ctx, size, ppp, name, vec![]);
                        }
                        let text = text_bounds(&output);
                        let title = match name {
                            "settings" => "Settings",
                            "hotkey" => "Hotkey setting",
                            "macro" => "Record & Playback",
                            "updates" => "Updates",
                            _ => unreachable!(),
                        };
                        assert!(text.iter().any(|(label, _, _)| label == title));
                        assert!(!text.iter().any(|(label, _, _)| label == "Window scale"));
                        if name == "settings" {
                            assert!(!text.iter().any(|(label, _, _)| label == "175 %"));
                            assert!(text.iter().any(
                                |(label, _, _)| label == "Scale and rearrange with window size"
                            ));
                        }
                        for (label, rect, clip) in text {
                            assert!(
                            screen.contains_rect(rect),
                            "{name} / {theme:?} at {size:?}: {label:?} outside window: {rect:?}"
                        );
                            assert!(
                            clip.expand(0.5).contains_rect(rect),
                            "{name} / {theme:?} at {size:?}: {label:?} clipped: {rect:?} by {clip:?}"
                        );
                        }
                    }
                }
            }
        }
    }

    fn click_dialog_text(
        app: &mut OpAutoClickerApp,
        ctx: &egui::Context,
        size: Vec2,
        name: &str,
        label: &str,
    ) {
        let output = render_dialog(app, ctx, size, 1.0, name, vec![]);
        let position = text_bounds(&output)
            .into_iter()
            .find(|(text, _, _)| text == label)
            .map(|(_, rect, _)| rect.center())
            .unwrap_or_else(|| panic!("missing {name} control {label:?}"));
        let button = |pressed| Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        let _ = render_dialog(
            app,
            ctx,
            size,
            1.0,
            name,
            vec![Event::PointerMoved(position), button(true)],
        );
        let _ = render_dialog(app, ctx, size, 1.0, name, vec![button(false)]);
    }

    #[test]
    fn secondary_window_controls_remain_interactive_at_the_safe_zoom() {
        let size = Vec2::new(420.0, 1027.0);
        for name in ["settings", "hotkey", "macro", "updates"] {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Narrow);
            app.update_last_checked = Some(Instant::now());
            app.show_settings = name == "settings";
            app.show_hotkey_dialog = name == "hotkey";
            app.show_macro_window = name == "macro";
            app.show_updates = name == "updates";
            for _ in 0..5 {
                let _ = render_dialog(&mut app, &ctx, size, 1.0, name, vec![]);
            }
            match name {
                "settings" => {
                    assert!(app.prefs.auto_scale);
                    click_dialog_text(
                        &mut app,
                        &ctx,
                        size,
                        name,
                        "Scale and rearrange with window size",
                    );
                    assert!(!app.prefs.auto_scale);
                    assert_eq!(app.main_layout, MainLayout::Classic);
                    let resize_ctx = egui::Context::default();
                    let tall = Rect::from_min_size(Pos2::ZERO, Vec2::new(560.0, 1360.0));
                    for _ in 0..8 {
                        let _ = resize_ctx.run(
                            RawInput {
                                screen_rect: Some(tall),
                                ..Default::default()
                            },
                            |ctx| app.adapt_to_window_size(ctx),
                        );
                    }
                    assert_eq!(app.main_layout, MainLayout::Classic);
                    assert_eq!(app.auto_zoom, 1.0);
                    click_dialog_text(
                        &mut app,
                        &ctx,
                        size,
                        name,
                        "Scale and rearrange with window size",
                    );
                    assert!(app.prefs.auto_scale);
                    for _ in 0..8 {
                        let _ = resize_ctx.run(
                            RawInput {
                                screen_rect: Some(tall),
                                ..Default::default()
                            },
                            |ctx| app.adapt_to_window_size(ctx),
                        );
                    }
                    assert_eq!(app.main_layout, MainLayout::Narrow);
                    assert!(app.auto_zoom >= 1.90);
                    click_dialog_text(&mut app, &ctx, size, name, "Dark");
                    assert_eq!(app.prefs.theme, AppTheme::Dark);
                    click_dialog_text(&mut app, &ctx, size, name, "Modern");
                    assert_eq!(app.prefs.theme, AppTheme::ModernDark);
                    click_dialog_text(&mut app, &ctx, size, name, "Light");
                    assert_eq!(app.prefs.theme, AppTheme::ModernLight);
                    click_dialog_text(&mut app, &ctx, size, name, "Classic");
                    assert_eq!(app.prefs.theme, AppTheme::Classic);
                }
                "hotkey" => {
                    click_dialog_text(&mut app, &ctx, size, name, "Assign new key…");
                    assert!(app.capturing);
                    click_dialog_text(&mut app, &ctx, size, name, "Close");
                    assert!(!app.show_hotkey_dialog);
                    assert!(!app.capturing);
                }
                "macro" => {
                    assert!(app.macro_until_stopped);
                    click_dialog_text(&mut app, &ctx, size, name, "Play");
                    assert!(!app.macro_until_stopped);
                }
                "updates" => {
                    assert!(app.prefs.check_updates);
                    click_dialog_text(&mut app, &ctx, size, name, "Check for updates on startup");
                    assert!(!app.prefs.check_updates);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn secondary_dialog_controls_work_in_every_appearance_and_window_shape() {
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for (layout, size) in [
                (MainLayout::Classic, Vec2::new(420.0, 420.0)),
                (MainLayout::Narrow, Vec2::new(420.0, 1027.0)),
                (MainLayout::Wide, Vec2::new(700.0, 456.0)),
            ] {
                for name in ["settings", "hotkey", "macro", "updates"] {
                    let ctx = egui::Context::default();
                    let mut app = test_app(layout);
                    app.prefs.theme = theme;
                    app.update_last_checked = Some(Instant::now());
                    app.show_settings = name == "settings";
                    app.show_hotkey_dialog = name == "hotkey";
                    app.show_macro_window = name == "macro";
                    app.show_updates = name == "updates";
                    for _ in 0..6 {
                        let _ = render_dialog(&mut app, &ctx, size, 1.0, name, vec![]);
                    }
                    match name {
                        "settings" => {
                            let next = if theme.mode() == AppearanceMode::Light {
                                AppearanceMode::Dark
                            } else {
                                AppearanceMode::Light
                            };
                            click_dialog_text(
                                &mut app,
                                &ctx,
                                size,
                                name,
                                if next == AppearanceMode::Dark {
                                    "Dark"
                                } else {
                                    "Light"
                                },
                            );
                            assert_eq!(
                                app.prefs.theme,
                                AppTheme::from_appearance(
                                    theme.style().expect("current style"),
                                    next
                                )
                            );
                        }
                        "hotkey" => {
                            click_dialog_text(&mut app, &ctx, size, name, "Assign new key…");
                            assert!(app.capturing, "{theme:?} / {layout:?}");
                            click_dialog_text(&mut app, &ctx, size, name, "Close");
                            assert!(!app.show_hotkey_dialog && !app.capturing);
                        }
                        "macro" => {
                            click_dialog_text(&mut app, &ctx, size, name, "Play");
                            assert!(!app.macro_until_stopped, "{theme:?} / {layout:?}");
                        }
                        "updates" => {
                            click_dialog_text(
                                &mut app,
                                &ctx,
                                size,
                                name,
                                "Check for updates on startup",
                            );
                            assert!(!app.prefs.check_updates, "{theme:?} / {layout:?}");
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    fn sync_at_size(
        app: &mut OpAutoClickerApp,
        ctx: &egui::Context,
        neutral: Vec2,
        applied_zoom: f32,
    ) -> FullOutput {
        ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, neutral / applied_zoom)),
                ..Default::default()
            },
            |ctx| {
                app.ui_clock_secs = ctx.input(|input| input.time);
                app.sync_window_layout(ctx);
            },
        )
    }

    #[test]
    fn dialog_zoom_round_trips_window_size_and_restores_the_dashboard() {
        for percent in [75, 100, 125, 150, 175, 200] {
            let user_factor = percent as f32 / 100.0;
            let neutral = Vec2::new(560.0, 1360.0) * user_factor;
            let auto_zoom =
                OpAutoClickerApp::fit_auto_zoom(neutral, user_factor, MainLayout::Narrow);
            let dashboard_zoom = auto_zoom * user_factor;
            let dialog_zoom =
                window_layout::dialog_safe_zoom((neutral.x, neutral.y), dashboard_zoom);
            for name in ["settings", "hotkey", "macro", "updates"] {
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Narrow);
                app.prefs.ui_scale_percent = percent;
                app.applied_user_factor = user_factor;
                app.restore_window_size = false;
                app.pending_resize = false;
                app.auto_zoom = auto_zoom;
                app.displayed_auto_zoom = auto_zoom;
                app.zoom_transition = None;
                app.last_base_size = neutral;
                match name {
                    "settings" => app.show_settings = true,
                    "hotkey" => app.show_hotkey_dialog = true,
                    "macro" => app.show_macro_window = true,
                    "updates" => app.show_updates = true,
                    _ => unreachable!(),
                }

                ctx.set_zoom_factor(dashboard_zoom);
                // A zoom request is applied at the next frame boundary.
                let _ = ctx.run(RawInput::default(), |_| {});
                let _ = sync_at_size(&mut app, &ctx, neutral, dashboard_zoom);
                assert!(app.pending_resize, "{name} / {percent}%");
                let output = sync_at_size(&mut app, &ctx, neutral, dialog_zoom);
                assert!((ctx.zoom_factor() - dialog_zoom).abs() < 0.001);
                assert!(!app.pending_resize);
                let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
                let floor = commands
                    .iter()
                    .find_map(|command| match command {
                        egui::ViewportCommand::MinInnerSize(size) => Some(*size),
                        _ => None,
                    })
                    .expect("minimum-size command");
                assert!((floor.x * dialog_zoom - 560.0 * user_factor).abs() < 0.1);
                assert!((floor.y * dialog_zoom - 456.0 * user_factor).abs() < 0.1);
                assert!(
                    !commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_))),
                    "opening {name} changed the physical window size"
                );

                app.window_settle_until = Instant::now() - Duration::from_secs(1);
                let _ = sync_at_size(&mut app, &ctx, neutral, dialog_zoom);
                assert_eq!(
                    (app.prefs.window_width, app.prefs.window_height),
                    (560, 1360)
                );

                app.show_settings = false;
                app.show_hotkey_dialog = false;
                app.show_macro_window = false;
                app.show_updates = false;
                let _ = sync_at_size(&mut app, &ctx, neutral, dialog_zoom);
                let output = sync_at_size(&mut app, &ctx, neutral, dashboard_zoom);
                assert!((ctx.zoom_factor() - dashboard_zoom).abs() < 0.001);
                assert!(
                    !output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_))),
                    "closing {name} changed the physical window size"
                );
                app.window_settle_until = Instant::now() - Duration::from_secs(1);
                let _ = sync_at_size(&mut app, &ctx, neutral, dashboard_zoom);
                assert_eq!(
                    (app.prefs.window_width, app.prefs.window_height),
                    (560, 1360)
                );
            }
        }
    }

    #[test]
    fn resizing_and_resetting_while_a_dialog_is_open_uses_the_applied_zoom() {
        let neutral = Vec2::new(600.0, 1360.0);
        let dashboard_zoom = OpAutoClickerApp::fit_auto_zoom(neutral, 1.0, MainLayout::Narrow);
        let cap = window_layout::dialog_safe_zoom((neutral.x, neutral.y), dashboard_zoom);
        let ctx = egui::Context::default();
        let mut app = test_app(MainLayout::Narrow);
        app.restore_window_size = false;
        app.pending_resize = false;
        app.auto_zoom = dashboard_zoom;
        app.displayed_auto_zoom = dashboard_zoom;
        app.last_base_size = neutral;
        app.show_settings = true;
        ctx.set_zoom_factor(cap);
        let _ = ctx.run(RawInput::default(), |_| {});
        let _ = sync_at_size(&mut app, &ctx, neutral, cap);

        let grown = Vec2::new(640.0, 1500.0);
        let grown_cap = window_layout::dialog_safe_zoom((grown.x, grown.y), dashboard_zoom);
        let _ = sync_at_size(&mut app, &ctx, grown, cap);
        let output = sync_at_size(&mut app, &ctx, grown, grown_cap);
        assert!((ctx.zoom_factor() - grown_cap).abs() < 0.001);
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_))));
        app.window_settle_until = Instant::now() - Duration::from_secs(1);
        let _ = sync_at_size(&mut app, &ctx, grown, grown_cap);
        assert_eq!(
            (app.prefs.window_width, app.prefs.window_height),
            (640, 1500)
        );

        app.reset_to_defaults();
        let _ = sync_at_size(&mut app, &ctx, grown, grown_cap);
        let output = sync_at_size(&mut app, &ctx, grown, 1.0);
        let size = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .find_map(|command| match command {
                egui::ViewportCommand::InnerSize(size) => Some(*size),
                _ => None,
            })
            .expect("factory reset must restore the window size");
        assert_eq!(size, Vec2::new(560.0, 456.0));
        assert!(!app.restore_window_size);
    }

    #[test]
    fn dpi_changes_redeclare_the_floor_without_resizing_the_window() {
        for percent in [75, 100, 125, 150, 175, 200] {
            for dialog in [false, true] {
                let user = percent as f32 / 100.0;
                let neutral = Vec2::new(560.0, 1360.0) * user;
                let dashboard =
                    user * OpAutoClickerApp::fit_auto_zoom(neutral, user, MainLayout::Narrow);
                let zoom = if dialog {
                    window_layout::dialog_safe_zoom((neutral.x, neutral.y), dashboard)
                } else {
                    dashboard
                };
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Narrow);
                app.restore_window_size = false;
                app.prefs.ui_scale_percent = percent;
                app.applied_user_factor = user;
                app.auto_zoom = dashboard / user;
                app.displayed_auto_zoom = dashboard / user;
                app.last_base_size = neutral;
                app.show_settings = dialog;
                ctx.set_zoom_factor(zoom);
                let _ = ctx.run(RawInput::default(), |_| {});
                let frame = |app: &mut OpAutoClickerApp, density: f32, maximized: bool| {
                    let mut input = RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, neutral / zoom)),
                        ..Default::default()
                    };
                    let viewport = input.viewports.entry(egui::ViewportId::ROOT).or_default();
                    viewport.native_pixels_per_point = Some(density);
                    viewport.maximized = Some(maximized);
                    ctx.run(input, |ctx| app.sync_window_layout(ctx))
                };
                let initial = frame(&mut app, 1.0, false);
                let commands = &initial.viewport_output[&egui::ViewportId::ROOT].commands;
                assert!(commands
                    .iter()
                    .any(|cmd| matches!(cmd, egui::ViewportCommand::MinInnerSize(_))));
                assert!(!commands
                    .iter()
                    .any(|cmd| matches!(cmd, egui::ViewportCommand::InnerSize(_))));
                for density in [1.25, 1.5, 2.0, 1.0] {
                    let output = frame(&mut app, density, true);
                    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
                    let floor = commands
                        .iter()
                        .find_map(|cmd| match cmd {
                            egui::ViewportCommand::MinInnerSize(value) => Some(*value),
                            _ => None,
                        })
                        .unwrap_or_else(|| {
                            panic!("{percent}% dialog={dialog} density={density}: no floor update")
                        });
                    assert!((floor.x * zoom * density - 560.0 * user * density).abs() < 0.1);
                    assert!((floor.y * zoom * density - 456.0 * user * density).abs() < 0.1);
                    assert!(!commands
                        .iter()
                        .any(|cmd| matches!(cmd, egui::ViewportCommand::InnerSize(_))));
                    let unchanged = frame(&mut app, density, true);
                    assert!(!unchanged.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .any(|cmd| matches!(cmd, egui::ViewportCommand::MinInnerSize(_))));
                }
            }
        }
    }

    #[test]
    fn shrinking_with_a_dialog_open_reflows_and_keeps_the_chosen_size() {
        let neutral = Vec2::new(600.0, 1360.0);
        let dashboard_zoom = OpAutoClickerApp::fit_auto_zoom(neutral, 1.0, MainLayout::Narrow);
        let cap = window_layout::dialog_safe_zoom((neutral.x, neutral.y), dashboard_zoom);
        let ctx = egui::Context::default();
        let mut app = test_app(MainLayout::Narrow);
        app.restore_window_size = false;
        app.pending_resize = false;
        app.auto_zoom = dashboard_zoom;
        app.displayed_auto_zoom = dashboard_zoom;
        app.last_base_size = neutral;
        app.show_hotkey_dialog = true;
        ctx.set_zoom_factor(cap);
        let _ = ctx.run(RawInput::default(), |_| {});
        let _ = sync_at_size(&mut app, &ctx, neutral, cap);

        let shrunk = Vec2::new(560.0, 800.0);
        let target = OpAutoClickerApp::fit_auto_zoom(shrunk, 1.0, MainLayout::Portrait);
        let _ = sync_at_size(&mut app, &ctx, shrunk, cap);
        assert_eq!(app.main_layout, MainLayout::Portrait);
        assert!((app.displayed_auto_zoom - target).abs() < 0.001);
        let output = sync_at_size(&mut app, &ctx, shrunk, target);
        assert!((ctx.zoom_factor() - target).abs() < 0.001);
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_))));
        app.window_settle_until = Instant::now() - Duration::from_secs(1);
        let _ = sync_at_size(&mut app, &ctx, shrunk, target);
        assert_eq!(
            (app.prefs.window_width, app.prefs.window_height),
            (560, 800)
        );
        app.show_hotkey_dialog = false;
        let _ = sync_at_size(&mut app, &ctx, shrunk, target);
        assert!((ctx.zoom_factor() - target).abs() < 0.001);
    }

    fn click_text_at(app: &mut OpAutoClickerApp, ctx: &egui::Context, size: Vec2, label: &str) {
        let output = render(app, ctx, size, 1.0, vec![]);
        let position = text_bounds(&output)
            .into_iter()
            .find(|(text, _, _)| text == label)
            .map(|(_, rect, _)| rect.center())
            .unwrap_or_else(|| panic!("missing control {label:?}"));
        let button = |pressed| Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        let _ = render(
            app,
            ctx,
            size,
            1.0,
            vec![Event::PointerMoved(position), button(true)],
        );
        let _ = render(app, ctx, size, 1.0, vec![button(false)]);
    }

    fn click_text(app: &mut OpAutoClickerApp, ctx: &egui::Context, label: &str) {
        click_text_at(app, ctx, Vec2::new(400.0, 600.0), label);
    }

    #[test]
    fn new_warning_and_error_text_meet_contrast_on_each_surface() {
        fn luminance(color: Color32) -> f64 {
            let linear = |channel: u8| {
                let value = f64::from(channel) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
        }
        fn contrast(a: Color32, b: Color32) -> f64 {
            let a = luminance(a);
            let b = luminance(b);
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }
        assert!(
            contrast(Color32::WHITE, UPDATE_BADGE_RED) >= 4.5,
            "the update badge number must be readable on red"
        );
        for palette in [
            &LIGHT_PALETTE,
            &DARK_PALETTE,
            &MODERN_LIGHT_PALETTE,
            &MODERN_DARK_PALETTE,
            &OLED_PALETTE,
            &MATRIX_PALETTE,
            &AURORA_PALETTE,
        ] {
            for foreground in [palette.warning, palette.danger] {
                for background in [palette.face, palette.well_fill] {
                    assert!(contrast(foreground, background) >= 4.5);
                }
            }
        }

        // Modern's regular, muted, accent and status text is used on
        // several surfaces, including the in-card dialogs and the strip.
        for palette in [&MODERN_LIGHT_PALETTE, &MODERN_DARK_PALETTE] {
            for foreground in [
                palette.text,
                palette.text_dim,
                palette.group_title,
                palette.accent,
                palette.success,
                palette.danger,
                palette.warning,
            ] {
                for background in [
                    palette.face,
                    palette.group_fill,
                    palette.well_fill,
                    palette.button,
                    palette.input_bg,
                    palette.hover_fill,
                ] {
                    assert!(
                        contrast(foreground, background) >= 4.5,
                        "modern foreground {foreground:?} on {background:?}"
                    );
                }
            }
            assert!(contrast(palette.text_disabled, palette.button_disabled) >= 4.5);
            assert!(contrast(palette.text_disabled, palette.input_disabled) >= 4.5);
            assert!(contrast(palette.text, palette.pressed_fill) >= 4.5);
            assert!(
                contrast(
                    palette.accent,
                    OpAutoClickerApp::modern_section_tile_color(palette)
                ) >= 4.5,
                "section icon must be distinct from its tile"
            );
            // Resting input and button outlines must remain identifiable on
            // their actual adjacent surfaces, not just on the panel face.
            for (outline, surface) in [
                (palette.button_border, palette.face),
                (palette.button_border, palette.button),
                (palette.input_border, palette.input_bg),
                (palette.input_border, palette.group_fill),
            ] {
                assert!(
                    contrast(outline, surface) >= 3.0,
                    "control outline {outline:?} on {surface:?}"
                );
            }
            for surface in [
                palette.face,
                palette.group_fill,
                palette.input_bg,
                palette.button,
                palette.hover_fill,
                palette.pressed_fill,
            ] {
                assert!(
                    contrast(palette.accent, surface) >= 3.0,
                    "focus accent must be visible on {surface:?}"
                );
            }
            let on_accent = if palette.dark {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            for fill in [
                palette.accent,
                OpAutoClickerApp::accent_variant(palette.accent, 0.9),
                OpAutoClickerApp::accent_variant(palette.accent, 1.12),
            ] {
                assert!(contrast(on_accent, fill) >= 4.5);
            }
            // Selection colors are premultiplied and paint over the dialog
            // card; compare the accent text to the resulting visible fill.
            let alpha = f32::from(palette.selection_bg.a()) / 255.0;
            let composite = |front: u8, back: u8| {
                (f32::from(front) + f32::from(back) * (1.0 - alpha)).round() as u8
            };
            let selected = Color32::from_rgb(
                composite(palette.selection_bg.r(), palette.group_fill.r()),
                composite(palette.selection_bg.g(), palette.group_fill.g()),
                composite(palette.selection_bg.b(), palette.group_fill.b()),
            );
            assert!(contrast(palette.accent, selected) >= 4.5);

            let start_text = Color32::from_rgb(7, 25, 35);
            for (hovered, pressed) in [(false, false), (true, false), (false, true)] {
                let (left, right) = OpAutoClickerApp::modern_start_colors(hovered, pressed);
                for step in 0..=20 {
                    let t = step as f32 / 20.0;
                    let channel = |a: u8, b: u8| {
                        (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
                    };
                    let color = Color32::from_rgb(
                        channel(left.r(), right.r()),
                        channel(left.g(), right.g()),
                        channel(left.b(), right.b()),
                    );
                    assert!(
                        contrast(start_text, color) >= 4.5,
                        "gradient text on {color:?}"
                    );
                }
                let (fill, _, text) =
                    OpAutoClickerApp::modern_stop_tone(palette.dark, true, hovered, pressed);
                assert!(contrast(text, fill) >= 4.5, "active Stop text on {fill:?}");
            }
            let (fill, _, text) =
                OpAutoClickerApp::modern_stop_tone(palette.dark, false, false, false);
            assert!(
                contrast(text, fill) >= 4.5,
                "disabled Stop text on {fill:?}"
            );
        }
    }

    #[test]
    fn growing_tall_adopts_portrait_then_shrinking_fits_immediately() {
        let ctx = egui::Context::default();
        let mut app = test_app(MainLayout::Classic);
        app.restore_window_size = false;
        let raw = |size| RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        for _ in 0..8 {
            let _ = ctx.run(raw(Vec2::new(560.0, 840.0)), |ctx| {
                app.adapt_to_window_size(ctx);
            });
        }
        assert_eq!(app.main_layout, MainLayout::Portrait);
        assert!((app.auto_zoom - 1.4).abs() < 0.001);
        assert!(app.zoom_transition.is_some(), "growth eases after settling");

        // Model the landed zoom, then pull the physical window down to
        // 560x600. The live size is neutral size / applied zoom.
        app.displayed_auto_zoom = app.auto_zoom;
        app.zoom_transition = None;
        ctx.set_zoom_factor(1.4);
        // egui applies a requested zoom at the next frame boundary.
        let _ = ctx.run(raw(Vec2::new(400.0, 600.0)), |_| {});
        let _ = ctx.run(raw(Vec2::new(400.0, 600.0 / 1.4)), |ctx| {
            app.apply_shrink_fit_immediately(ctx, 1.0);
        });
        assert_eq!(app.main_layout, MainLayout::Classic);
        assert!((app.auto_zoom - 1.0).abs() < 0.001);
        assert!((app.displayed_auto_zoom - 1.0).abs() < 0.001);
        assert!(app.zoom_transition.is_none());
        assert_eq!(
            OpAutoClickerApp::live_window_floor_for(app.displayed_auto_zoom),
            Vec2::new(560.0, 456.0)
        );
    }

    #[test]
    fn extreme_portrait_reflows_grow_and_shrink_without_stale_fit() {
        let ctx = egui::Context::default();
        let mut app = test_app(MainLayout::Classic);
        app.restore_window_size = false;
        let raw = |size| RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        for _ in 0..8 {
            let _ = ctx.run(raw(Vec2::new(560.0, 1360.0)), |ctx| {
                app.adapt_to_window_size(ctx);
            });
        }
        assert_eq!(app.main_layout, MainLayout::Narrow);
        assert!(app.auto_zoom >= 1.90);
        assert!(app.zoom_transition.is_some());
        let narrow_zoom = app.auto_zoom;
        app.displayed_auto_zoom = narrow_zoom;
        app.zoom_transition = None;
        ctx.set_zoom_factor(narrow_zoom);
        let _ = ctx.run(raw(Vec2::new(560.0, 1360.0) / narrow_zoom), |_| {});
        for _ in 0..8 {
            let _ = ctx.run(raw(Vec2::new(560.0, 1800.0) / narrow_zoom), |ctx| {
                app.adapt_to_window_size(ctx);
            });
        }
        assert_eq!(app.main_layout, MainLayout::Slender);
        assert!(app.auto_zoom > narrow_zoom);
        assert!(app.zoom_transition.is_some());
        let slender_zoom = app.auto_zoom;
        app.displayed_auto_zoom = slender_zoom;
        app.zoom_transition = None;
        ctx.set_zoom_factor(slender_zoom);
        let _ = ctx.run(raw(Vec2::new(560.0, 1800.0) / slender_zoom), |_| {});
        let _ = ctx.run(raw(Vec2::new(560.0, 1360.0) / slender_zoom), |ctx| {
            app.apply_shrink_fit_immediately(ctx, 1.0);
        });
        assert_eq!(app.main_layout, MainLayout::Narrow);
        assert!((app.auto_zoom - narrow_zoom).abs() < 0.001);
        assert_eq!(app.displayed_auto_zoom, app.auto_zoom);
        assert!(app.zoom_transition.is_none());

        ctx.set_zoom_factor(narrow_zoom);
        let _ = ctx.run(raw(Vec2::new(560.0, 1360.0) / narrow_zoom), |_| {});
        let _ = ctx.run(raw(Vec2::new(560.0, 600.0) / narrow_zoom), |ctx| {
            app.apply_shrink_fit_immediately(ctx, 1.0);
        });
        assert_eq!(app.main_layout, MainLayout::Classic);
        assert_eq!(app.auto_zoom, 1.0);
        assert_eq!(app.displayed_auto_zoom, 1.0);
    }

    #[test]
    fn narrow_and_slender_controls_remain_interactive() {
        for (layout, size) in [
            (MainLayout::Narrow, Vec2::new(280.0, 700.0)),
            (MainLayout::Slender, Vec2::new(220.0, 740.0)),
        ] {
            let ctx = egui::Context::default();
            let mut app = test_app(layout);
            for _ in 0..5 {
                let _ = render(&mut app, &ctx, size, 1.0, vec![]);
            }
            click_text_at(&mut app, &ctx, size, "Repeat");
            assert!(app.repeat_limited);
            click_text_at(&mut app, &ctx, size, "Random offset ±");
            assert!(app.prefs.random_offset_enabled);
            assert!(app.offset_ms > 0.0);
            click_text_at(&mut app, &ctx, size, "Pick location");
            assert_eq!(app.config.coordinate_mode, CoordinateMode::FixedScreen);
            click_text_at(&mut app, &ctx, size, "Left");
            click_text_at(&mut app, &ctx, size, "Right");
            assert_eq!(app.config.mouse_button, MouseButton::Right);
            click_text_at(&mut app, &ctx, size, "Single");
            click_text_at(&mut app, &ctx, size, "Double");
            assert_eq!(app.config.click_type, ClickType::Double);
            click_text_at(&mut app, &ctx, size, "Hotkey setting");
            assert!(app.show_hotkey_dialog);
            let macro_label = if layout == MainLayout::Slender {
                "Record / Play"
            } else {
                "Record & Playback"
            };
            click_text_at(&mut app, &ctx, size, macro_label);
            assert!(app.show_macro_window);
            click_text_at(&mut app, &ctx, size, "Settings…");
            assert!(app.show_settings);
            click_text_at(&mut app, &ctx, size, "Update v1.6.0");
            assert!(app.show_updates);
        }
    }

    #[test]
    fn controls_work_in_each_appearance_at_compact_tall_and_wide_sizes() {
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for (layout, size) in [
                (MainLayout::Classic, Vec2::new(560.0, 456.0)),
                (MainLayout::Narrow, Vec2::new(280.0, 700.0)),
                (MainLayout::Slender, Vec2::new(220.0, 740.0)),
                (MainLayout::Wide, Vec2::new(700.0, 456.0)),
            ] {
                let ctx = egui::Context::default();
                let mut app = test_app(layout);
                app.prefs.theme = theme;
                for _ in 0..6 {
                    let _ = render(&mut app, &ctx, size, 1.0, vec![]);
                }
                click_text_at(&mut app, &ctx, size, "Repeat");
                assert!(app.repeat_limited, "{theme:?} / {layout:?}");
                click_text_at(&mut app, &ctx, size, "Random offset ±");
                assert!(app.prefs.random_offset_enabled);
                click_text_at(&mut app, &ctx, size, "Pick location");
                assert_eq!(app.config.coordinate_mode, CoordinateMode::FixedScreen);
                click_text_at(&mut app, &ctx, size, "Left");
                click_text_at(&mut app, &ctx, size, "Right");
                assert_eq!(app.config.mouse_button, MouseButton::Right);
                let hotkey_label = if matches!(theme, AppTheme::ModernLight | AppTheme::ModernDark)
                {
                    "Hotkeys"
                } else {
                    "Hotkey setting"
                };
                click_text_at(&mut app, &ctx, size, hotkey_label);
                assert!(app.show_hotkey_dialog);
                click_text_at(
                    &mut app,
                    &ctx,
                    size,
                    if layout == MainLayout::Slender {
                        "Record / Play"
                    } else {
                        "Record & Playback"
                    },
                );
                assert!(app.show_macro_window);
                click_text_at(&mut app, &ctx, size, "Settings…");
                assert!(app.show_settings);
                click_text_at(&mut app, &ctx, size, "Update v1.6.0");
                assert!(app.show_updates);

                // Ten minutes to the first deadline: exercise the real
                // start/stop handoff without delivering an input on CI.
                app.hours = 0;
                app.minutes = 10;
                app.seconds = 0;
                app.millis = 0;
                click_text_at(&mut app, &ctx, size, "Start (F6)");
                assert!(app.engine.is_running(), "{theme:?} / {layout:?}");
                click_text_at(&mut app, &ctx, size, "Stop (F6)");
                let deadline = Instant::now() + Duration::from_secs(5);
                while app.engine.is_running() {
                    assert!(Instant::now() < deadline, "worker did not drain");
                    std::thread::yield_now();
                }
                assert_eq!(app.engine.click_count(), 0);
            }
        }
    }

    #[test]
    fn portrait_controls_remain_interactive() {
        let ctx = egui::Context::default();
        let mut app = test_app(MainLayout::Portrait);
        for _ in 0..5 {
            let _ = render(&mut app, &ctx, Vec2::new(400.0, 600.0), 1.0, vec![]);
        }
        click_text(&mut app, &ctx, "Repeat");
        assert!(app.repeat_limited);
        click_text(&mut app, &ctx, "Random offset ±");
        assert!(app.prefs.random_offset_enabled);
        assert!(app.offset_ms > 0.0);
        click_text(&mut app, &ctx, "Pick location");
        assert_eq!(app.config.coordinate_mode, CoordinateMode::FixedScreen);
        click_text(&mut app, &ctx, "Left");
        click_text(&mut app, &ctx, "Right");
        assert_eq!(app.config.mouse_button, MouseButton::Right);
        click_text(&mut app, &ctx, "Settings…");
        assert!(app.show_settings);
        click_text(&mut app, &ctx, "Update v1.6.0");
        assert!(app.show_updates);
        click_text(&mut app, &ctx, "Record & Playback");
        assert!(app.show_macro_window);
    }

    fn scratch(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "macroclicker-app-test-{}-{label}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("scratch directory");
        path
    }

    #[test]
    fn dialog_backdrop_is_below_windows_and_keeps_controls_interactive() {
        fn composite(
            app: &mut OpAutoClickerApp,
            ctx: &egui::Context,
            events: Vec<Event>,
        ) -> FullOutput {
            ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(560.0, 456.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.apply_style(ctx);
                    app.draw_dashboard(ctx);
                    app.draw_dialog_backdrop(ctx);
                    app.draw_settings_window(ctx);
                    app.draw_updates_window(ctx);
                },
            )
        }
        fn shadowed_frame(shape: &Shape, color: Color32, fill: Color32) -> bool {
            match shape {
                Shape::Vec(children) => {
                    let mesh = children.iter().any(|child| {
                        matches!(child, Shape::Mesh(mesh)
                        if mesh.vertices.iter().any(|vertex| vertex.color == color))
                    });
                    let frame = children
                        .iter()
                        .any(|child| matches!(child, Shape::Rect(rect) if rect.fill == fill));
                    (mesh && frame)
                        || children
                            .iter()
                            .any(|child| shadowed_frame(child, color, fill))
                }
                _ => false,
            }
        }
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for name in ["settings", "updates"] {
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Classic);
                app.prefs.theme = theme;
                app.show_settings = name == "settings";
                app.show_updates = name == "updates";
                app.update_last_checked = Some(Instant::now());
                let mut output = composite(&mut app, &ctx, vec![]);
                for _ in 0..5 {
                    output = composite(&mut app, &ctx, vec![]);
                }
                let palette = app.palette();
                let shade = Color32::from_black_alpha(if palette.dark { 110 } else { 72 });
                let overlay = output.shapes.iter().position(|clipped| {
                    matches!(&clipped.shape,
                    Shape::Rect(rect) if rect.fill == shade
                        && rect.rect == Rect::from_min_size(Pos2::ZERO, Vec2::new(560.0, 456.0)))
                });
                if palette.modern {
                    let overlay = overlay.expect("Modern dialog background dimming");
                    let title = if name == "settings" {
                        "Settings"
                    } else {
                        "Updates"
                    };
                    let window = output
                        .shapes
                        .iter()
                        .position(|clipped| {
                            let mut texts = Vec::new();
                            collect_text(&clipped.shape, clipped.clip_rect, &mut texts);
                            texts.iter().any(|(text, _, _)| text == title)
                        })
                        .unwrap_or_else(|| panic!("missing {title} title"));
                    assert!(
                        overlay < window,
                        "the backdrop must paint behind the dialog"
                    );
                    let shadow = ctx.style().visuals.window_shadow;
                    assert_eq!(shadow.blur, 0.0, "Modern shadows are clean, not blurred");
                    assert!(shadow.offset.y > 0.0 && shadow.color.a() > 0);
                    assert!(
                        output
                            .shapes
                            .iter()
                            .skip(overlay + 1)
                            .any(|clipped| shadowed_frame(
                                &clipped.shape,
                                shadow.color,
                                palette.group_fill
                            )),
                        "{name} must paint its own drop shadow"
                    );
                } else {
                    assert!(overlay.is_none(), "Classic must not gain a backdrop");
                    assert_eq!(ctx.style().visuals.window_shadow.blur, 18.0);
                }
                let target = if name == "settings" {
                    if theme.mode() == AppearanceMode::Light {
                        "Dark"
                    } else {
                        "Light"
                    }
                } else {
                    "Check for updates on startup"
                };
                let position = text_bounds(&output)
                    .into_iter()
                    .find(|(text, _, _)| text == target)
                    .unwrap_or_else(|| panic!("missing dialog control {target}"))
                    .1
                    .center();
                let pointer = |pressed| Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                };
                let _ = composite(
                    &mut app,
                    &ctx,
                    vec![Event::PointerMoved(position), pointer(true)],
                );
                let _ = composite(&mut app, &ctx, vec![pointer(false)]);
                if name == "settings" {
                    assert_ne!(app.prefs.theme.mode(), theme.mode());
                    if palette.modern {
                        // The dimming layer is intentionally non-modal: safety
                        // and dashboard controls outside the window still work.
                        let output = composite(&mut app, &ctx, vec![]);
                        let position = text_bounds(&output)
                            .into_iter()
                            .find(|(text, _, _)| text == "Hotkeys")
                            .expect("dashboard control outside Settings")
                            .1
                            .center();
                        let pointer = |pressed| Event::PointerButton {
                            pos: position,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::default(),
                        };
                        let _ = composite(
                            &mut app,
                            &ctx,
                            vec![Event::PointerMoved(position), pointer(true)],
                        );
                        let _ = composite(&mut app, &ctx, vec![pointer(false)]);
                        assert!(app.show_hotkey_dialog);
                    }
                } else {
                    assert!(!app.prefs.check_updates);
                }
            }
        }
    }

    #[test]
    fn modern_outfit_license_is_available_offline_and_scrollable() {
        fn composite(
            app: &mut OpAutoClickerApp,
            ctx: &egui::Context,
            size: Vec2,
            events: Vec<Event>,
        ) -> FullOutput {
            ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.apply_style(ctx);
                    if app.show_settings {
                        app.draw_settings_window(ctx);
                    }
                    if app.show_font_license {
                        app.draw_font_license_window(ctx);
                    }
                },
            )
        }
        fn find_window(shape: &Shape, fill: Color32) -> Option<Rect> {
            match shape {
                Shape::Vec(children) => children.iter().find_map(|child| find_window(child, fill)),
                Shape::Rect(rect) if rect.fill == fill && rect.rect.width() > 350.0 => {
                    Some(rect.rect)
                }
                _ => None,
            }
        }
        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            for size in [
                Vec2::new(420.0, 420.0),
                Vec2::new(560.0, 456.0),
                Vec2::new(700.0, 456.0),
            ] {
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Classic);
                app.prefs.theme = theme;
                app.show_settings = true;
                let mut output = composite(&mut app, &ctx, size, vec![]);
                for _ in 0..5 {
                    output = composite(&mut app, &ctx, size, vec![]);
                }
                let labels = text_bounds(&output);
                let appearance = labels
                    .iter()
                    .find(|(text, _, _)| text == "Appearance")
                    .expect("appearance heading")
                    .1;
                let entry = labels
                    .iter()
                    .find(|(text, _, _)| text == "Outfit license")
                    .expect("license entry in Modern Settings")
                    .1;
                assert!(
                    appearance.right() < entry.left(),
                    "settings heading and license must not overlap"
                );
                let position = entry.center();
                let pointer = |pressed| Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                };
                let _ = composite(
                    &mut app,
                    &ctx,
                    size,
                    vec![Event::PointerMoved(position), pointer(true)],
                );
                let _ = composite(&mut app, &ctx, size, vec![pointer(false)]);
                assert!(app.show_font_license && app.any_dialog_open());
                for _ in 0..5 {
                    output = composite(&mut app, &ctx, size, vec![]);
                }
                let license = text_bounds(&output)
                    .into_iter()
                    .find(|(text, _, _)| text.contains("Copyright 2021 The Outfit Project Authors"))
                    .expect("bundled license text");
                assert!(license.0.contains("SIL OPEN FONT LICENSE Version 1.1"));
                assert!(license.0.contains("DISCLAIMER"));
                let scroll_point = license.2.center();
                let _ = composite(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        Event::PointerMoved(scroll_point),
                        Event::Scroll(Vec2::new(0.0, -1000.0)),
                    ],
                );
                output = composite(&mut app, &ctx, size, vec![]);
                let scrolled = text_bounds(&output)
                    .into_iter()
                    .find(|(text, _, _)| text.contains("Copyright 2021 The Outfit Project Authors"))
                    .expect("scrolled license text");
                assert!(
                    scrolled.1.top() < license.1.top(),
                    "full license must scroll"
                );
                let window = output
                    .shapes
                    .iter()
                    .find_map(|clipped| find_window(&clipped.shape, app.palette().group_fill))
                    .expect("license window frame");
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size)
                        .expand(0.5)
                        .contains_rect(window),
                    "{theme:?} / {size:?}: {window:?}"
                );
                let title = text_bounds(&output)
                    .into_iter()
                    .find(|(text, _, _)| text == "Outfit font license")
                    .expect("license title")
                    .1;
                let close = egui::pos2(window.right() - 18.0, title.center().y);
                let pointer = |pressed| Event::PointerButton {
                    pos: close,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                };
                let _ = composite(
                    &mut app,
                    &ctx,
                    size,
                    vec![Event::PointerMoved(close), pointer(true)],
                );
                let _ = composite(&mut app, &ctx, size, vec![pointer(false)]);
                assert!(
                    !app.show_font_license,
                    "license window close button must work"
                );
            }
        }
    }

    #[test]
    fn modern_card_headings_use_vector_icons_and_a_hover_rim() {
        fn shapes<'a>(shape: &'a Shape, output: &mut Vec<&'a Shape>) {
            match shape {
                Shape::Vec(children) => children.iter().for_each(|child| shapes(child, output)),
                _ => output.push(shape),
            }
        }
        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            let size = Vec2::new(560.0, 456.0);
            for _ in 0..5 {
                let _ = render(&mut app, &ctx, size, 1.0, vec![]);
            }
            ctx.fonts(|fonts| {
                for glyph in ['•', '○', '✓', '→', '±', '…', '×', '—', '–', '·', '“', '”']
                {
                    assert!(
                        fonts.has_glyph(&FontId::proportional(11.0), glyph),
                        "Modern must render its interface glyph {glyph}"
                    );
                }
            });
            let output = render(&mut app, &ctx, size, 1.0, vec![]);
            let labels = text_bounds(&output);
            let mut all = Vec::new();
            for clipped in &output.shapes {
                shapes(&clipped.shape, &mut all);
            }
            for title in [
                "CLICK INTERVAL",
                "CLICK OPTIONS",
                "CLICK REPEAT",
                "CURSOR POSITION",
            ] {
                let text = labels
                    .iter()
                    .find(|(label, _, _)| label == title)
                    .unwrap_or_else(|| panic!("missing {title} in {theme:?}"))
                    .1;
                let icon_area = Rect::from_min_max(
                    egui::pos2(text.left() - 20.0, text.center().y - 9.0),
                    egui::pos2(text.left() - 2.0, text.center().y + 9.0),
                );
                let tile = all.iter().find_map(|shape| match shape {
                    Shape::Rect(rect)
                        if rect.fill
                            == OpAutoClickerApp::modern_section_tile_color(app.palette())
                            && icon_area.contains(rect.rect.center()) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                });
                let tile = tile.unwrap_or_else(|| panic!("{title} needs its icon tile"));
                assert_eq!(tile.size(), Vec2::splat(18.0));
                assert!(tile.right() < text.left(), "tile must not crowd {title}");
                let count = all
                    .iter()
                    .filter(|shape| {
                        let color = match shape {
                            Shape::Circle(circle) => circle.stroke.color,
                            Shape::Rect(rect) => rect.stroke.color,
                            Shape::LineSegment { stroke, .. } => stroke.color,
                            _ => Color32::TRANSPARENT,
                        };
                        color == app.palette().accent
                            && icon_area.intersects(shape.visual_bounding_rect())
                    })
                    .count();
                assert!(
                    count >= 2,
                    "{title} must have its own vector icon, found {count}"
                );
            }
            let card = all
                .iter()
                .find_map(|shape| match shape {
                    Shape::Rect(rect)
                        if rect.fill == app.palette().group_fill
                            && rect.stroke.color == app.palette().group_border =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("first group card");
            let hovered = render(
                &mut app,
                &ctx,
                size,
                1.0,
                vec![Event::PointerMoved(card.min + Vec2::new(6.0, 6.0))],
            );
            let rim = if app.palette().dark {
                Color32::from_rgb(91, 126, 137)
            } else {
                Color32::from_rgb(124, 160, 170)
            };
            let mut all = Vec::new();
            for clipped in &hovered.shapes {
                shapes(&clipped.shape, &mut all);
            }
            assert!(
                all.iter().any(|shape| matches!(shape,
                Shape::Rect(rect) if rect.rect == card && rect.stroke.color == rim)),
                "hover must highlight the card without changing its geometry"
            );
        }
    }

    fn notification_badges(output: &FullOutput) -> Vec<Pos2> {
        fn collect(shape: &Shape, result: &mut Vec<Pos2>) {
            match shape {
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, result);
                    }
                }
                Shape::Circle(circle) if circle.fill == UPDATE_BADGE_RED => {
                    result.push(circle.center);
                }
                _ => {}
            }
        }
        let mut result = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut result);
        }
        result
    }

    #[test]
    fn update_notification_tracks_verdict_and_install_lifecycle() {
        let (major, minor, patch) =
            update::parse_version(env!("CARGO_PKG_VERSION")).expect("package version");
        let version = format!(
            "{major}.{minor}.{}",
            patch.checked_add(1).expect("next patch")
        );
        let ctx = egui::Context::default();
        let size = Vec2::new(560.0, 456.0);
        let mut app = test_app(MainLayout::Classic);
        for state in [
            UpdateUiState::NotChecked,
            UpdateUiState::Checking,
            UpdateUiState::UpToDate,
            UpdateUiState::Failed("Offline".to_string()),
        ] {
            app.update_state = state.clone();
            let output = render(&mut app, &ctx, size, 1.0, vec![]);
            assert!(notification_badges(&output).is_empty(), "{state:?}");
        }
        app.update_state = UpdateUiState::Available(version.clone());
        app.update_latest_version = Some(version.clone());
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert_eq!(notification_badges(&output).len(), 1);

        let dir = scratch("update-notification");
        let temp_path = dir.join("candidate.tmp");
        let downloading = |rx| UpdateInstallState::Downloading {
            version: version.clone(),
            total: 10,
            downloaded: Arc::new(AtomicU64::new(0)),
            phase: Arc::new(AtomicU8::new(UPDATE_PHASE_DOWNLOADING)),
            cancel: Arc::new(AtomicBool::new(false)),
            temp_path: temp_path.clone(),
            source_expected: false,
            source_advanced: Arc::new(AtomicBool::new(false)),
            rx,
        };
        std::fs::write(&temp_path, b"test candidate").expect("create candidate");
        let (tx, rx) = mpsc::channel();
        app.update_install = downloading(rx);
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert!(
            notification_badges(&output).is_empty(),
            "install in progress"
        );
        tx.send(UpdateInstallMessage::Cancelled {
            source_advanced: false,
        })
        .expect("send cancellation");
        app.poll_update_install(&ctx);
        assert!(matches!(app.update_install, UpdateInstallState::Idle));
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert_eq!(
            notification_badges(&output).len(),
            1,
            "cancelled update remains available"
        );
        assert!(!temp_path.exists(), "cancelled candidate is removed");

        std::fs::write(&temp_path, b"test candidate").expect("create candidate");
        let (tx, rx) = mpsc::channel();
        app.update_install = downloading(rx);
        tx.send(UpdateInstallMessage::Failed("Download failed".to_string()))
            .expect("send failure");
        app.poll_update_install(&ctx);
        assert!(matches!(app.update_install, UpdateInstallState::Idle));
        assert!(!temp_path.exists(), "failed candidate is removed");
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert_eq!(
            notification_badges(&output).len(),
            1,
            "failed install can be retried"
        );

        app.update_state = UpdateUiState::Checking;
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert!(
            notification_badges(&output).is_empty(),
            "checking has no current verdict"
        );
        app.apply_update_failure("Offline".to_string(), Instant::now());
        assert!(matches!(app.update_state, UpdateUiState::Available(_)));
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert_eq!(
            notification_badges(&output).len(),
            1,
            "last confirmed update survives a failed refresh"
        );

        app.update_install = UpdateInstallState::WaitingForStop {
            ready: ReadyUpdate {
                paths: self_update::update_paths(
                    std::path::Path::new("macro_clicker.exe"),
                    "badge-test",
                ),
                version: version.clone(),
                source_advanced: false,
            },
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert!(
            notification_badges(&output).is_empty(),
            "waiting to finish installing"
        );
        app.update_install = UpdateInstallState::Restarting;
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert!(
            notification_badges(&output).is_empty(),
            "installation already completed"
        );
        app.update_install = UpdateInstallState::Idle;
        app.update_state = UpdateUiState::UpToDate;
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        assert!(
            notification_badges(&output).is_empty(),
            "installed version is current"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn update_notification_fits_and_is_clickable_across_themes_and_layouts() {
        for theme in AppTheme::all() {
            for (layout, size) in [
                (MainLayout::Classic, Vec2::new(560.0, 456.0)),
                (MainLayout::Portrait, Vec2::new(400.0, 600.0)),
                (MainLayout::Narrow, Vec2::new(280.0, 700.0)),
                (MainLayout::Slender, Vec2::new(220.0, 740.0)),
                (MainLayout::Wide, Vec2::new(700.0, 456.0)),
            ] {
                for ppp in [1.0, 1.25, 1.5, 2.0] {
                    let ctx = egui::Context::default();
                    let mut app = test_app(layout);
                    app.prefs.theme = theme;
                    let mut output = render(&mut app, &ctx, size, ppp, vec![]);
                    for _ in 0..5 {
                        output = render(&mut app, &ctx, size, ppp, vec![]);
                    }
                    let badges = notification_badges(&output);
                    assert_eq!(badges.len(), 1, "{theme:?} / {layout:?} / {ppp}");
                    let center = badges[0];
                    let circle = Rect::from_center_size(center, Vec2::splat(14.0));
                    assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(circle));
                    let text = text_bounds(&output);
                    let label = text
                        .iter()
                        .find(|(label, _, _)| label == "Update v1.6.0")
                        .expect("full update label")
                        .1;
                    let settings = text
                        .iter()
                        .find(|(label, _, _)| label == "Settings…")
                        .expect("settings label")
                        .1;
                    assert!(
                        !circle.intersects(label),
                        "badge must not cover the update label"
                    );
                    assert!(
                        !circle.intersects(settings),
                        "badge must not cover Settings"
                    );
                    assert!(
                        text.iter()
                            .any(|(label, rect, _)| label == "1" && circle.contains(rect.center())),
                        "notification number must appear inside its circle"
                    );
                    let pointer = |pressed| Event::PointerButton {
                        pos: center,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::default(),
                    };
                    let _ = render(
                        &mut app,
                        &ctx,
                        size,
                        ppp,
                        vec![Event::PointerMoved(center), pointer(true)],
                    );
                    output = render(&mut app, &ctx, size, ppp, vec![pointer(false)]);
                    assert!(app.show_updates, "clicking the badge must activate Updates");
                    assert!(
                        output
                            .platform_output
                            .events
                            .iter()
                            .any(|event| matches!(event,
                        egui::output::OutputEvent::Clicked(info)
                            if info.label.as_deref() == Some("Update v1.6.0"))),
                        "the full accessible button label must be retained"
                    );
                }
            }
        }
    }

    #[test]
    fn update_notification_preserves_long_version_hover_and_accessibility() {
        let version = "4294967295.4294967295.4294967295";
        let label = format!("Update v{version}");
        let size = Vec2::new(220.0, 740.0);
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for ppp in [1.0, 2.0] {
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Slender);
                app.prefs.theme = theme;
                app.update_state = UpdateUiState::Available(version.to_string());
                app.update_latest_version = Some(version.to_string());
                let mut output = render(&mut app, &ctx, size, ppp, vec![]);
                for _ in 0..5 {
                    output = render(&mut app, &ctx, size, ppp, vec![]);
                }
                let badges = notification_badges(&output);
                assert_eq!(badges.len(), 1);
                let center = badges[0];
                for _ in 0..50 {
                    output = render(&mut app, &ctx, size, ppp, vec![Event::PointerMoved(center)]);
                }
                assert!(
                    text_bounds(&output)
                        .iter()
                        .any(|(text, _, _)| text != &label && text.contains(version)),
                    "hovering the badge must reveal the full version"
                );
                let pointer = |pressed| Event::PointerButton {
                    pos: center,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                };
                let _ = render(&mut app, &ctx, size, ppp, vec![pointer(true)]);
                output = render(&mut app, &ctx, size, ppp, vec![pointer(false)]);
                assert!(app.show_updates);
                assert!(output.platform_output.events.iter().any(|event| matches!(event,
                    egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some(&label))),
                    "the accessible label must contain the full version");
            }
        }
    }

    #[test]
    fn disabled_numeric_fields_keep_the_original_visuals() {
        for theme in AppTheme::all() {
            for ppp in [1.0, 1.25, 1.5, 2.0] {
                for compact in [false, true] {
                    for (kind, number) in [
                        ("offset", 0.0),
                        ("offset", 40.4),
                        ("repeat", 1.0),
                        ("repeat", 1_000_000_000.0),
                        ("loop", 1.0),
                        ("loop", 1_000_000.0),
                        ("X", -100_000.0),
                        ("Y", 53.0),
                    ] {
                        let ctx = egui::Context::default();
                        let mut app = test_app(MainLayout::Classic);
                        app.prefs.theme = theme;
                        let frame = |old: bool| {
                            let mut input = RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    Vec2::new(320.0, 120.0),
                                )),
                                ..Default::default()
                            };
                            input
                                .viewports
                                .entry(egui::ViewportId::ROOT)
                                .or_default()
                                .native_pixels_per_point = Some(ppp);
                            ctx.run(input, |ctx| {
                                app.apply_style(ctx);
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    ui.add_enabled_ui(false, |ui| {
                                        if compact {
                                            OpAutoClickerApp::compact_numeric_style(ui);
                                        }
                                        let mut value = number;
                                        let size = match kind {
                                            "offset" => [72.0, 22.0],
                                            "repeat" => [76.0, 22.0],
                                            "loop" => [64.0, 20.0],
                                            _ => [78.0, 22.0],
                                        };
                                        if old && kind == "loop" {
                                            let mut loops = number as u64;
                                            ui.add_sized(
                                                size,
                                                egui::DragValue::new(&mut loops)
                                                    .clamp_range(1..=1_000_000),
                                            );
                                        } else if old {
                                            let widget = egui::DragValue::new(&mut value);
                                            let widget = match kind {
                                                "offset" => widget
                                                    .speed(1.0)
                                                    .clamp_range(0.0..=10_000.0)
                                                    .fixed_decimals(1),
                                                "repeat" => widget
                                                    .max_decimals(0)
                                                    .clamp_range(1.0..=1_000_000_000.0),
                                                _ => widget
                                                    .max_decimals(0)
                                                    .clamp_range(-100_000.0..=100_000.0)
                                                    .prefix(format!("{kind}: ")),
                                            };
                                            ui.add_sized(size, widget);
                                        } else {
                                            let text = match kind {
                                                "offset" if number == 0.0 => "0".to_string(),
                                                "offset" => format!("{number:.1}"),
                                                "repeat" | "loop" => format!("{number:.0}"),
                                                _ => format!("{kind}: {number:.0}"),
                                            };
                                            OpAutoClickerApp::disabled_numeric_field(
                                                ui, text, number, size,
                                            );
                                        }
                                    });
                                });
                            })
                        };
                        for _ in 0..4 {
                            let _ = frame(true);
                        }
                        let original = frame(true);
                        let replacement = frame(false);
                        assert_eq!(original.shapes, replacement.shapes,
                            "{theme:?}/{ppp}/{compact}/{kind}/{number}: disabled field paint changed");
                    }
                }
            }
        }
    }

    #[test]
    fn dialog_focus_fixes_keep_the_original_control_paint() {
        for theme in AppTheme::all() {
            for ppp in [1.0, 1.5, 2.0] {
                for kind in ["button", "selected", "unselected", "update"] {
                    let ctx = egui::Context::default();
                    let mut app = test_app(MainLayout::Classic);
                    app.prefs.theme = theme;
                    let frame = |old: bool| {
                        let mut input = RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(320.0, 120.0),
                            )),
                            ..Default::default()
                        };
                        input
                            .viewports
                            .entry(egui::ViewportId::ROOT)
                            .or_default()
                            .native_pixels_per_point = Some(ppp);
                        ctx.run(input, |ctx| {
                            app.apply_style(ctx);
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let pal = app.palette();
                                match kind {
                                    "button" => {
                                        if old {
                                            ui.add_enabled_ui(false, |ui| {
                                                let _ = ui.button("Clear");
                                            });
                                        } else {
                                            OpAutoClickerApp::focus_safe_button(ui, "Clear", false);
                                        }
                                    }
                                    "selected" | "unselected" => {
                                        let selected = kind == "selected";
                                        ui.add_enabled_ui(false, |ui| {
                                            if old {
                                                let _ = ui.selectable_label(selected, "Dark");
                                            } else {
                                                OpAutoClickerApp::disabled_selectable_label(
                                                    ui, selected, "Dark",
                                                );
                                            }
                                        });
                                    }
                                    "update" => {
                                        if old {
                                            let text = egui::RichText::new("Update to v1.6.0")
                                                .color(pal.text_disabled)
                                                .size(13.0)
                                                .strong();
                                            let galley = OpAutoClickerApp::truncated_galley(
                                                ui,
                                                text,
                                                150.0 - 2.0 * ui.spacing().button_padding.x,
                                            );
                                            ui.add_enabled(
                                                false,
                                                egui::Button::new(galley)
                                                    .fill(pal.button_disabled)
                                                    .stroke(Stroke::new(1.0_f32, pal.button_border))
                                                    .rounding(Rounding::same(if pal.modern {
                                                        8.0
                                                    } else {
                                                        5.0
                                                    }))
                                                    .min_size(Vec2::new(150.0, 34.0)),
                                            );
                                        } else {
                                            OpAutoClickerApp::update_action_button(
                                                ui,
                                                pal,
                                                "Update to v1.6.0",
                                                false,
                                                true,
                                                150.0,
                                                34.0,
                                            );
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                            });
                        })
                    };
                    for _ in 0..4 {
                        let _ = frame(true);
                    }
                    let original = frame(true);
                    let replacement = frame(false);
                    assert_eq!(
                        original.shapes, replacement.shapes,
                        "{theme:?}/{ppp}/{kind}: disabled dialog control paint changed"
                    );
                }
            }
        }
    }

    #[test]
    fn secondary_dialog_keyboard_navigation_skips_disabled_controls() {
        let size = Vec2::new(560.0, 456.0);
        let mut cases = Vec::new();
        for theme in AppTheme::all() {
            cases.push(("macro", theme, "Play", "Record"));
            cases.push((
                "updates",
                theme,
                "Check for updates on startup",
                "Check now",
            ));
        }
        for theme in AppTheme::previous() {
            cases.push((
                "settings",
                theme,
                "Scale and rearrange with window size",
                "Classic",
            ));
        }
        for (name, theme, target, back_target) in cases {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            app.show_macro_window = name == "macro";
            app.show_updates = name == "updates";
            app.show_settings = name == "settings";
            app.update_state = UpdateUiState::UpToDate;
            app.update_last_checked = Some(Instant::now());
            for _ in 0..6 {
                let _ = render_dialog(&mut app, &ctx, size, 1.0, name, vec![]);
            }
            let tab = |shift, pressed| Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers {
                    shift,
                    ..Modifiers::default()
                },
            };
            let mut labels = Vec::new();
            for _ in 0..48 {
                let press = render_dialog(&mut app, &ctx, size, 1.0, name, vec![tab(false, true)]);
                let release =
                    render_dialog(&mut app, &ctx, size, 1.0, name, vec![tab(false, false)]);
                labels.extend(
                    press
                        .platform_output
                        .events
                        .iter()
                        .chain(&release.platform_output.events)
                        .filter_map(|event| {
                            if let egui::output::OutputEvent::FocusGained(info) = event {
                                info.label.clone()
                            } else {
                                None
                            }
                        }),
                );
                if labels.iter().any(|label| label == target) {
                    break;
                }
            }
            assert!(
                labels.iter().any(|label| label == target),
                "{name}/{theme:?}: Tab never reached {target:?}: {labels:?}"
            );
            let _ = render_dialog(
                &mut app,
                &ctx,
                size,
                1.0,
                name,
                vec![Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                }],
            );
            match name {
                "macro" => assert!(!app.macro_until_stopped),
                "updates" => assert!(!app.prefs.check_updates),
                "settings" => assert!(!app.prefs.auto_scale),
                _ => unreachable!(),
            }
            let mut backwards = Vec::new();
            for _ in 0..48 {
                let press = render_dialog(&mut app, &ctx, size, 1.0, name, vec![tab(true, true)]);
                let release =
                    render_dialog(&mut app, &ctx, size, 1.0, name, vec![tab(true, false)]);
                backwards.extend(
                    press
                        .platform_output
                        .events
                        .iter()
                        .chain(&release.platform_output.events)
                        .filter_map(|event| {
                            if let egui::output::OutputEvent::FocusGained(info) = event {
                                info.label.clone()
                            } else {
                                None
                            }
                        }),
                );
                if backwards.iter().any(|label| label == back_target) {
                    break;
                }
            }
            assert!(
                backwards.iter().any(|label| label == back_target),
                "{name}/{theme:?}: Shift+Tab never reached {back_target:?}: {backwards:?}"
            );
        }
    }

    #[test]
    fn macro_dialog_enabled_actions_remain_operable() {
        let size = Vec2::new(560.0, 456.0);
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            app.show_macro_window = true;
            let ctx = egui::Context::default();
            for _ in 0..6 {
                let _ = render_dialog(&mut app, &ctx, size, 1.0, "macro", vec![]);
            }
            click_dialog_text(&mut app, &ctx, size, "macro", "Record");
            assert!(app.macro_recording, "{theme:?}: Record");
            click_dialog_text(&mut app, &ctx, size, "macro", "Stop recording");
            assert!(!app.macro_recording, "{theme:?}: Stop recording");
            assert!(app
                .macro_recorder
                .record_click(600_000.0, 50, 60, MouseButton::Right));
            click_dialog_text(&mut app, &ctx, size, "macro", "Clear");
            assert!(app.macro_recorder.is_empty(), "{theme:?}: Clear");
            assert!(app
                .macro_recorder
                .record_click(600_000.0, 50, 60, MouseButton::Right));

            // A fresh context starts at the beginning of the focus order.
            // The second Play is the action after the disabled loop field;
            // the first is the radio button before it.
            let ctx = egui::Context::default();
            for _ in 0..6 {
                let _ = render_dialog(&mut app, &ctx, size, 1.0, "macro", vec![]);
            }
            let key = |key, pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::default(),
            };
            let mut labels = Vec::new();
            for _ in 0..16 {
                let press = render_dialog(
                    &mut app,
                    &ctx,
                    size,
                    1.0,
                    "macro",
                    vec![key(egui::Key::Tab, true)],
                );
                let release = render_dialog(
                    &mut app,
                    &ctx,
                    size,
                    1.0,
                    "macro",
                    vec![key(egui::Key::Tab, false)],
                );
                labels.extend(
                    press
                        .platform_output
                        .events
                        .iter()
                        .chain(&release.platform_output.events)
                        .filter_map(|event| {
                            if let egui::output::OutputEvent::FocusGained(info) = event {
                                info.label.clone()
                            } else {
                                None
                            }
                        }),
                );
                if labels.ends_with(&["Play".to_string(), "Play".to_string()]) {
                    break;
                }
            }
            assert!(
                labels.ends_with(&["Play".to_string(), "Play".to_string()]),
                "{theme:?}: disabled loop field trapped Tab before Play: {labels:?}"
            );
            let _ = render_dialog(
                &mut app,
                &ctx,
                size,
                1.0,
                "macro",
                vec![key(egui::Key::Enter, true)],
            );
            assert!(
                app.engine.is_running() && app.macro_playing,
                "{theme:?}: Play"
            );
            click_dialog_text(&mut app, &ctx, size, "macro", "Stop playback");
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.engine.is_running() {
                assert!(
                    Instant::now() < deadline,
                    "{theme:?}: playback did not stop"
                );
                std::thread::yield_now();
            }
            assert_eq!(app.engine.click_count(), 0, "test must not send input");
        }
    }

    #[test]
    fn numeric_fields_ignore_input_when_disabled_and_edit_after_enabling() {
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            let size = Vec2::new(560.0, 456.0);
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            app.update_state = UpdateUiState::UpToDate;
            for _ in 0..6 {
                let _ = render(&mut app, &ctx, size, 1.0, vec![]);
            }
            click_text_at(&mut app, &ctx, size, "X: 0");
            let _ = render(
                &mut app,
                &ctx,
                size,
                1.0,
                vec![Event::Text("900".to_string())],
            );
            assert_eq!(
                app.config.fixed_x, 0,
                "disabled coordinate must ignore typing"
            );

            click_text_at(&mut app, &ctx, size, "Random offset ±");
            assert!(app.prefs.random_offset_enabled);
            click_text_at(&mut app, &ctx, size, "40.0");
            let _ = render(
                &mut app,
                &ctx,
                size,
                1.0,
                vec![Event::Text("7".to_string())],
            );
            assert_eq!(app.offset_ms, 7.0, "enabled offset remains editable");

            click_text_at(&mut app, &ctx, size, "Repeat");
            assert!(app.repeat_limited);
            click_text_at(&mut app, &ctx, size, "1");
            let _ = render(
                &mut app,
                &ctx,
                size,
                1.0,
                vec![Event::Text("23".to_string())],
            );
            assert_eq!(app.config.max_clicks, 23, "enabled repeat remains editable");

            click_text_at(&mut app, &ctx, size, "Pick location");
            assert_eq!(app.config.coordinate_mode, CoordinateMode::FixedScreen);
            click_text_at(&mut app, &ctx, size, "X: 0");
            let _ = render(
                &mut app,
                &ctx,
                size,
                1.0,
                vec![Event::Text("12".to_string())],
            );
            assert_eq!(
                app.config.fixed_x, 12,
                "enabled coordinate remains editable"
            );
        }
    }

    #[test]
    fn keyboard_navigation_reaches_footer_and_returns_across_disabled_fields() {
        for theme in [
            AppTheme::Classic,
            AppTheme::Dark,
            AppTheme::ModernLight,
            AppTheme::ModernDark,
        ] {
            for (layout, size) in [
                (MainLayout::Classic, Vec2::new(560.0, 456.0)),
                (MainLayout::Slender, Vec2::new(220.0, 740.0)),
            ] {
                let ctx = egui::Context::default();
                let mut app = test_app(layout);
                app.prefs.theme = theme;
                app.prefs.random_offset_enabled = false;
                app.repeat_limited = false;
                app.config.coordinate_mode = CoordinateMode::CurrentCursor;
                for _ in 0..6 {
                    let _ = render(&mut app, &ctx, size, 1.0, vec![]);
                }
                let tab = |shift, pressed| Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers {
                        shift,
                        ..Modifiers::default()
                    },
                };
                let mut labels = Vec::new();
                for _ in 0..48 {
                    let output = render(&mut app, &ctx, size, 1.0, vec![tab(false, true)]);
                    labels.extend(output.platform_output.events.iter().filter_map(|event| {
                        if let egui::output::OutputEvent::FocusGained(info) = event {
                            info.label.clone()
                        } else {
                            None
                        }
                    }));
                    let _ = render(&mut app, &ctx, size, 1.0, vec![tab(false, false)]);
                    if labels.iter().any(|label| label == "Update v1.6.0") {
                        break;
                    }
                }
                assert!(
                    labels.iter().any(|label| label == "Update v1.6.0"),
                    "{theme:?}/{layout:?}: forward Tab never reached Updates: {labels:?}"
                );
                let output = render(
                    &mut app,
                    &ctx,
                    size,
                    1.0,
                    vec![Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::default(),
                    }],
                );
                assert!(app.show_updates, "focused Updates must activate by Enter");
                assert!(output
                    .platform_output
                    .events
                    .iter()
                    .any(|event| matches!(event,
                    egui::output::OutputEvent::Clicked(info)
                        if info.label.as_deref() == Some("Update v1.6.0"))));
                app.show_updates = false;
                let mut backward_labels = Vec::new();
                for _ in 0..48 {
                    let press = render(&mut app, &ctx, size, 1.0, vec![tab(true, true)]);
                    let release = render(&mut app, &ctx, size, 1.0, vec![tab(true, false)]);
                    backward_labels.extend(
                        press
                            .platform_output
                            .events
                            .iter()
                            .chain(&release.platform_output.events)
                            .filter_map(|event| {
                                if let egui::output::OutputEvent::FocusGained(info) = event {
                                    info.label.clone()
                                } else {
                                    None
                                }
                            }),
                    );
                    if backward_labels
                        .iter()
                        .any(|label| label == "Random offset ±")
                    {
                        break;
                    }
                }
                assert!(
                    backward_labels
                        .iter()
                        .any(|label| label == "Random offset ±"),
                    "{theme:?}/{layout:?}: Shift+Tab never returned: {backward_labels:?}"
                );
            }
        }
    }

    #[test]
    fn update_notification_retains_keyboard_and_direct_update_actions() {
        let ctx = egui::Context::default();
        let size = Vec2::new(560.0, 456.0);
        let mut app = test_app(MainLayout::Classic);
        app.prefs.theme = AppTheme::ModernDark;
        for _ in 0..5 {
            let _ = render(&mut app, &ctx, size, 1.0, vec![]);
        }
        let output = render(&mut app, &ctx, size, 1.0, vec![]);
        let center = notification_badges(&output)[0];
        let pointer = |pressed| Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        let _ = render(
            &mut app,
            &ctx,
            size,
            1.0,
            vec![Event::PointerMoved(center), pointer(true)],
        );
        let _ = render(&mut app, &ctx, size, 1.0, vec![pointer(false)]);
        assert!(app.show_updates);
        let update_button_id = ctx
            .interaction_snapshot(|interaction| interaction.clicked)
            .expect("click belongs to the Updates button");
        ctx.memory_mut(|memory| memory.request_focus(update_button_id));
        app.show_updates = false;
        let output = render(
            &mut app,
            &ctx,
            size,
            1.0,
            vec![Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert!(
            app.show_updates,
            "keyboard activation of Updates must still work"
        );
        assert!(output
            .platform_output
            .events
            .iter()
            .any(|event| matches!(event,
            egui::output::OutputEvent::Clicked(info)
                if info.label.as_deref() == Some("Update v1.6.0"))));

        // With an eligible portable update, clicking the badge follows the
        // existing direct-update path. A deliberately missing parent folder
        // stops the attempt at reservation, before any network or file swap.
        let dir = scratch("badge-direct");
        let missing = dir.join("missing");
        app.show_updates = false;
        app.install_kind = InstallKind::Portable;
        app.exe_path = Some(missing.join("macro_clicker.exe"));
        app.update_asset = Some(ReleaseAsset {
            size: 100,
            sha256: Some([0; 32]),
        });
        let _ = render(
            &mut app,
            &ctx,
            size,
            1.0,
            vec![Event::PointerMoved(center), pointer(true)],
        );
        let _ = render(&mut app, &ctx, size, 1.0, vec![pointer(false)]);
        assert!(app.show_updates);
        assert!(matches!(app.update_install, UpdateInstallState::Idle));
        assert!(
            app.update_notice.is_some(),
            "direct update was attempted and stopped safely"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn modern_footer_ghosts_highlight_on_hover() {
        fn hover_fill(shape: &Shape, point: Pos2, color: Color32) -> bool {
            match shape {
                Shape::Vec(children) => {
                    children.iter().any(|child| hover_fill(child, point, color))
                }
                Shape::Rect(rect) => rect.fill == color && rect.rect.contains(point),
                _ => false,
            }
        }
        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            let size = Vec2::new(560.0, 456.0);
            for _ in 0..5 {
                let _ = render(&mut app, &ctx, size, 1.0, vec![]);
            }
            for label in ["Settings…", "Update v1.6.0"] {
                let output = render(&mut app, &ctx, size, 1.0, vec![]);
                let position = text_bounds(&output)
                    .into_iter()
                    .find(|(text, _, _)| text == label)
                    .unwrap_or_else(|| panic!("missing {label}"))
                    .1
                    .center();
                let hovered = render(
                    &mut app,
                    &ctx,
                    size,
                    1.0,
                    vec![Event::PointerMoved(position)],
                );
                assert!(
                    hovered.shapes.iter().any(|clipped| hover_fill(
                        &clipped.shape,
                        position,
                        app.palette().hover_fill
                    )),
                    "{theme:?}: {label} needs visible hover feedback"
                );
            }
        }
    }

    #[test]
    fn modern_actions_paint_distinct_gradient_stop_and_secondary_states() {
        fn frame(
            app: &OpAutoClickerApp,
            ctx: &egui::Context,
            events: Vec<Event>,
        ) -> (egui::FullOutput, [Rect; 4]) {
            let mut rects = [Rect::NOTHING; 4];
            let output = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 240.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.apply_style(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for (index, (label, enabled, primary, danger)) in [
                            ("Start", true, true, false),
                            ("Stop", true, false, true),
                            ("Stop disabled", false, false, true),
                            ("Record", true, false, false),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            rects[index] = OpAutoClickerApp::toned_button(
                                ui,
                                app.palette(),
                                label,
                                enabled,
                                140.0,
                                primary,
                                danger,
                            )
                            .rect;
                        }
                    });
                },
            );
            (output, rects)
        }
        fn mesh_colors(output: &egui::FullOutput) -> Vec<Color32> {
            fn visit(shape: &Shape, colors: &mut Vec<Color32>) {
                match shape {
                    Shape::Vec(shapes) => shapes.iter().for_each(|shape| visit(shape, colors)),
                    Shape::Mesh(mesh) => {
                        colors.extend(mesh.vertices.iter().map(|vertex| vertex.color))
                    }
                    _ => {}
                }
            }
            let mut colors = Vec::new();
            for clipped in &output.shapes {
                visit(&clipped.shape, &mut colors);
            }
            colors
        }
        fn has_fill(
            output: &egui::FullOutput,
            target: Rect,
            fill: Color32,
            stroke: Color32,
        ) -> bool {
            fn matches(shape: &Shape, target: Rect, fill: Color32, stroke: Color32) -> bool {
                match shape {
                    Shape::Vec(shapes) => shapes
                        .iter()
                        .any(|shape| matches(shape, target, fill, stroke)),
                    Shape::Rect(rect) => {
                        rect.rect == target && rect.fill == fill && rect.stroke.color == stroke
                    }
                    _ => false,
                }
            }
            output
                .shapes
                .iter()
                .any(|clipped| matches(&clipped.shape, target, fill, stroke))
        }
        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            for _ in 0..4 {
                let _ = frame(&app, &ctx, vec![]);
            }
            let (normal, rects) = frame(&app, &ctx, vec![]);
            let (left, right) = OpAutoClickerApp::modern_start_colors(false, false);
            let colors = mesh_colors(&normal);
            assert!(
                colors.contains(&left) && colors.contains(&right),
                "Start paints both gradient endpoints"
            );
            let (fill, border, _) =
                OpAutoClickerApp::modern_stop_tone(app.palette().dark, true, false, false);
            assert!(has_fill(&normal, rects[1], fill, border));
            let (fill, border, _) =
                OpAutoClickerApp::modern_stop_tone(app.palette().dark, false, false, false);
            assert!(has_fill(&normal, rects[2], fill, border));
            assert!(has_fill(
                &normal,
                rects[3],
                app.palette().button,
                app.palette().button_border
            ));
            let (hovered, _) = frame(&app, &ctx, vec![Event::PointerMoved(rects[0].center())]);
            let (hover_left, hover_right) = OpAutoClickerApp::modern_start_colors(true, false);
            let colors = mesh_colors(&hovered);
            assert!(colors.contains(&hover_left) && colors.contains(&hover_right));
            let start_pointer = Event::PointerButton {
                pos: rects[0].center(),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            };
            let (pressed, _) = frame(&app, &ctx, vec![start_pointer]);
            let (pressed_left, pressed_right) = OpAutoClickerApp::modern_start_colors(false, true);
            let colors = mesh_colors(&pressed);
            assert!(colors.contains(&pressed_left) && colors.contains(&pressed_right));
            let _ = frame(
                &app,
                &ctx,
                vec![Event::PointerButton {
                    pos: rects[0].center(),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                }],
            );
            let (stop_hover, _) = frame(&app, &ctx, vec![Event::PointerMoved(rects[1].center())]);
            let (fill, border, _) =
                OpAutoClickerApp::modern_stop_tone(app.palette().dark, true, true, false);
            assert!(has_fill(&stop_hover, rects[1], fill, border));
            let (ghost_hover, _) = frame(&app, &ctx, vec![Event::PointerMoved(rects[3].center())]);
            assert!(has_fill(
                &ghost_hover,
                rects[3],
                app.palette().hover_fill,
                app.palette().accent
            ));
        }
    }

    #[test]
    fn modern_action_icons_fit_or_yield_to_the_label_and_keep_accessible_clicks() {
        fn frame(
            app: &OpAutoClickerApp,
            ctx: &egui::Context,
            ppp: f32,
            label: &str,
            width: f32,
            role: (bool, bool, bool),
            events: Vec<Event>,
        ) -> (FullOutput, Rect, bool) {
            let mut input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 120.0))),
                events,
                ..Default::default()
            };
            input
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .native_pixels_per_point = Some(ppp);
            let mut rect = Rect::NOTHING;
            let mut clicked = false;
            let output = ctx.run(input, |ctx| {
                app.apply_style(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = OpAutoClickerApp::toned_button(
                        ui,
                        app.palette(),
                        label,
                        role.0,
                        width,
                        role.1,
                        role.2,
                    );
                    rect = response.rect;
                    clicked = response.clicked();
                });
            });
            (output, rect, clicked)
        }
        fn has_glyph(
            shape: &Shape,
            icon: ModernActionIcon,
            expected_area: Rect,
            color: Color32,
        ) -> bool {
            if let Shape::Vec(children) = shape {
                return children
                    .iter()
                    .any(|child| has_glyph(child, icon, expected_area, color));
            }
            if !shape.visual_bounding_rect().intersects(expected_area) {
                return false;
            }
            match (icon, shape) {
                (ModernActionIcon::Start, Shape::Path(path)) => {
                    path.fill == color && path.points.len() == 3
                }
                (ModernActionIcon::Stop, Shape::Rect(rect)) => {
                    rect.fill == color && rect.rect.size() == Vec2::splat(9.0)
                }
                (ModernActionIcon::Hotkeys, Shape::Rect(rect)) => {
                    rect.stroke.color == color && rect.rect.size() == Vec2::new(14.0, 10.0)
                }
                (ModernActionIcon::Playback, Shape::Circle(circle)) => {
                    circle.stroke.color == color && (circle.radius - 5.5).abs() < 0.01
                }
                _ => false,
            }
        }
        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            for ppp in [1.0, 1.25, 1.5, 2.0] {
                for (label, primary, danger) in [
                    ("Start (F24)", true, false),
                    ("Stop (Space)", false, true),
                    ("Hotkeys", false, false),
                    ("Record & Playback", false, false),
                    ("Record / Play", false, false),
                ] {
                    for roomy in [false, true] {
                        let ctx = egui::Context::default();
                        let mut app = test_app(MainLayout::Classic);
                        app.prefs.theme = theme;
                        let role = (true, primary, danger);
                        for _ in 0..5 {
                            let _ = frame(&app, &ctx, ppp, label, 220.0, role, vec![]);
                        }
                        let color = if primary {
                            Color32::from_rgb(7, 25, 35)
                        } else if danger {
                            Color32::WHITE
                        } else {
                            app.palette().text
                        };
                        let text_width = ctx.fonts(|fonts| {
                            fonts
                                .layout_no_wrap(label.to_owned(), FontId::proportional(13.5), color)
                                .size()
                                .x
                        });
                        // Both widths fit the label. Only the second can also
                        // accommodate the mark, gap, and 8-point side insets.
                        let width = text_width + if roomy { 38.0 } else { 12.0 };
                        let (output, rect, _) = frame(&app, &ctx, ppp, label, width, role, vec![]);
                        assert!((rect.width() - width).abs() < 0.01);
                        assert!((rect.height() - 31.0).abs() < 0.01);
                        let bounds = text_bounds(&output)
                            .into_iter()
                            .find(|(text, _, _)| text == label)
                            .expect("complete visible button label")
                            .1;
                        assert!(
                            rect.expand(0.5).contains_rect(bounds),
                            "{theme:?}/{ppp}/{label}: label must fit its hit target"
                        );
                        let icon = OpAutoClickerApp::modern_action_icon(label, primary, danger)
                            .expect("dashboard action has a vector mark");
                        let icon_center = egui::pos2(
                            rect.center().x - (14.0 + 6.0 + text_width) / 2.0 + 7.0,
                            rect.center().y,
                        );
                        let expected_area = Rect::from_center_size(icon_center, Vec2::splat(16.0));
                        assert_eq!(
                            output.shapes.iter().any(|clipped| {
                                has_glyph(&clipped.shape, icon, expected_area, color)
                            }),
                            roomy,
                            "{theme:?}/{ppp}/{label}: mark must yield when space is tight"
                        );
                        if !roomy {
                            assert!((bounds.center().x - rect.center().x).abs() < 1.0);
                        }
                        let position = if roomy { icon_center } else { rect.center() };
                        let pointer = |pressed| Event::PointerButton {
                            pos: position,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::default(),
                        };
                        let _ = frame(
                            &app,
                            &ctx,
                            ppp,
                            label,
                            width,
                            role,
                            vec![Event::PointerMoved(position), pointer(true)],
                        );
                        let (output, _, clicked) =
                            frame(&app, &ctx, ppp, label, width, role, vec![pointer(false)]);
                        assert!(clicked, "the entire action remains clickable");
                        assert!(
                            output.platform_output.events.iter().any(|event| {
                                matches!(event, egui::output::OutputEvent::Clicked(info)
                                if info.label.as_deref() == Some(label) && info.enabled)
                            }),
                            "{label}: accessible name and enabled state must be unchanged"
                        );
                    }
                }
                // Disabled Stop still has a recognizable mark, but neither
                // that mark nor the rest of its hit area can activate it.
                let ctx = egui::Context::default();
                let mut app = test_app(MainLayout::Classic);
                app.prefs.theme = theme;
                let role = (false, false, true);
                for _ in 0..5 {
                    let _ = frame(&app, &ctx, ppp, "Stop (F6)", 160.0, role, vec![]);
                }
                let (_, _, color) =
                    OpAutoClickerApp::modern_stop_tone(app.palette().dark, false, false, false);
                let text_width = ctx.fonts(|fonts| {
                    fonts
                        .layout_no_wrap("Stop (F6)".to_owned(), FontId::proportional(13.5), color)
                        .size()
                        .x
                });
                let (output, rect, _) = frame(&app, &ctx, ppp, "Stop (F6)", 160.0, role, vec![]);
                let icon_center = egui::pos2(
                    rect.center().x - (14.0 + 6.0 + text_width) / 2.0 + 7.0,
                    rect.center().y,
                );
                let expected_area = Rect::from_center_size(icon_center, Vec2::splat(16.0));
                assert!(output.shapes.iter().any(|clipped| has_glyph(
                    &clipped.shape,
                    ModernActionIcon::Stop,
                    expected_area,
                    color
                )));
                let pointer = |pressed| Event::PointerButton {
                    pos: icon_center,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                };
                let _ = frame(
                    &app,
                    &ctx,
                    ppp,
                    "Stop (F6)",
                    160.0,
                    role,
                    vec![Event::PointerMoved(icon_center), pointer(true)],
                );
                let (output, _, clicked) = frame(
                    &app,
                    &ctx,
                    ppp,
                    "Stop (F6)",
                    160.0,
                    role,
                    vec![pointer(false)],
                );
                assert!(!clicked, "disabled Stop must remain inert");
                assert!(!output.platform_output.events.iter().any(|event| {
                    matches!(event, egui::output::OutputEvent::Clicked(info)
                        if info.label.as_deref() == Some("Stop (F6)"))
                }));
            }
        }
    }

    #[test]
    fn modern_numeric_fields_center_text_and_show_the_focus_accent() {
        fn rects(shape: &Shape, output: &mut Vec<(Rect, Color32, Color32)>) {
            match shape {
                Shape::Vec(shapes) => {
                    for child in shapes {
                        rects(child, output);
                    }
                }
                Shape::Rect(rect) => output.push((rect.rect, rect.fill, rect.stroke.color)),
                _ => {}
            }
        }
        for theme in [AppTheme::ModernLight, AppTheme::ModernDark] {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = theme;
            let mut value = 37_u64;
            let frame = |value: &mut u64, events| {
                let input = RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 120.0))),
                    events,
                    ..Default::default()
                };
                ctx.run(input, |ctx| {
                    app.apply_style(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        OpAutoClickerApp::numeric_field(ui, value, 99, 56.0);
                    });
                })
            };
            let mut output = frame(&mut value, vec![]);
            for _ in 0..5 {
                output = frame(&mut value, vec![]);
            }
            let mut shapes = Vec::new();
            for clipped in &output.shapes {
                rects(&clipped.shape, &mut shapes);
            }
            let field = shapes
                .into_iter()
                .find(|(rect, fill, _)| {
                    *fill == app.palette().input_bg && (rect.width() - 56.0).abs() < 1.0
                })
                .expect("inset numeric field")
                .0;
            let text = text_bounds(&output)
                .into_iter()
                .find(|(label, _, _)| label == "37")
                .expect("field value")
                .1;
            assert!((text.center().x - field.center().x).abs() < 1.5);
            let position = field.center();
            let button = |pressed| Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::default(),
            };
            let _ = frame(
                &mut value,
                vec![Event::PointerMoved(position), button(true)],
            );
            let _ = frame(&mut value, vec![button(false)]);
            output = frame(&mut value, vec![]);
            let mut shapes = Vec::new();
            for clipped in &output.shapes {
                rects(&clipped.shape, &mut shapes);
            }
            assert!(
                shapes.iter().any(|(rect, fill, stroke)| {
                    *fill == app.palette().input_bg
                        && *stroke == app.palette().accent
                        && (rect.width() - 56.0).abs() < 3.0
                }),
                "{theme:?}: focused field must use the accent outline"
            );
            let focused_text = text_bounds(&output)
                .into_iter()
                .find(|(label, _, _)| label == "37")
                .expect("focused value")
                .1;
            assert!((focused_text.center().x - field.center().x).abs() < 1.5);
            let _ = frame(&mut value, vec![Event::Text("5".to_owned())]);
            assert_eq!(value, 5, "editing remains functional after styling");
        }
    }

    #[test]
    fn custom_action_button_supports_keyboard_focus_and_accessible_label() {
        for palette in [
            &LIGHT_PALETTE,
            &DARK_PALETTE,
            &MODERN_LIGHT_PALETTE,
            &MODERN_DARK_PALETTE,
        ] {
            let ctx = egui::Context::default();
            let input = |events| RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 200.0))),
                events,
                ..Default::default()
            };
            let mut id = None;
            let _ = ctx.run(input(vec![]), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    id = Some(
                        OpAutoClickerApp::toned_button(
                            ui,
                            palette,
                            "Start (F6)",
                            true,
                            150.0,
                            true,
                            false,
                        )
                        .id,
                    );
                });
            });
            ctx.memory_mut(|memory| memory.request_focus(id.expect("button id")));
            let mut clicked = false;
            let mut focused = false;
            let output = ctx.run(
                input(vec![Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                }]),
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = OpAutoClickerApp::toned_button(
                            ui,
                            palette,
                            "Start (F6)",
                            true,
                            150.0,
                            true,
                            false,
                        );
                        clicked = response.clicked();
                        focused = response.has_focus();
                    });
                },
            );
            assert!(focused);
            assert!(clicked, "Enter activates the focused action");
            assert!(output.platform_output.events.iter().any(|event| {
                matches!(event, egui::output::OutputEvent::Clicked(info)
                    if info.label.as_deref() == Some("Start (F6)") && info.enabled)
            }));
        }
    }

    #[test]
    fn switching_back_restores_classic_font_and_visual_details() {
        let ctx = egui::Context::default();
        let size = Vec2::new(560.0, 456.0);
        let mut app = test_app(MainLayout::Classic);
        let frame = |app: &mut OpAutoClickerApp| {
            for _ in 0..6 {
                let _ = render(app, &ctx, size, 1.0, vec![]);
            }
        };
        let measure = || {
            ctx.fonts(|fonts| {
                fonts
                    .layout_no_wrap(
                        "Click interval".to_owned(),
                        FontId::proportional(13.5),
                        Color32::WHITE,
                    )
                    .size()
            })
        };
        frame(&mut app);
        let classic_size = measure();
        let classic_visuals = ctx.style().visuals.clone();
        app.prefs.theme = AppTheme::ModernDark;
        frame(&mut app);
        assert_ne!(measure(), classic_size, "Modern uses its own typeface");
        assert_ne!(
            ctx.style().visuals.window_rounding,
            classic_visuals.window_rounding
        );
        app.prefs.theme = AppTheme::Classic;
        frame(&mut app);
        assert_eq!(measure(), classic_size);
        let restored = ctx.style().visuals.clone();
        assert_eq!(restored.faint_bg_color, classic_visuals.faint_bg_color);
        assert_eq!(restored.code_bg_color, classic_visuals.code_bg_color);
        assert_eq!(restored.warn_fg_color, classic_visuals.warn_fg_color);
        assert_eq!(restored.error_fg_color, classic_visuals.error_fg_color);
        assert_eq!(restored.window_rounding, classic_visuals.window_rounding);
        assert_eq!(restored.window_fill, classic_visuals.window_fill);
    }

    #[test]
    fn appearance_choices_persist_without_changing_other_preferences() {
        let dir = scratch("appearance");
        let path = dir.join("settings.conf");
        let ctx = egui::Context::default();
        let size = Vec2::new(560.0, 456.0);
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path.clone());
        app.show_settings = true;
        app.prefs.ui_scale_percent = 150;
        app.prefs.window_width = 720;
        app.prefs.window_height = 560;
        app.prefs.toggle_hotkey_vk = hotkey::VK_F7;
        app.millis = 250;
        for _ in 0..5 {
            let _ = render_dialog(&mut app, &ctx, size, 1.0, "settings", vec![]);
        }
        for (label, expected) in [
            ("Dark", AppTheme::Dark),
            ("Modern", AppTheme::ModernDark),
            ("Light", AppTheme::ModernLight),
            ("Classic", AppTheme::Classic),
            ("Modern", AppTheme::ModernLight),
            ("Dark", AppTheme::ModernDark),
        ] {
            click_dialog_text(&mut app, &ctx, size, "settings", label);
            assert_eq!(app.prefs.theme, expected, "after choosing {label}");
            assert!(app.settings_dirty);
            assert!(app.flush_settings());
            let document = std::fs::read_to_string(&path).expect("saved appearance");
            let (config, prefs) = settings::parse_settings(&document);
            assert_eq!(prefs.theme, expected);
            assert_eq!(prefs.toggle_hotkey_vk, hotkey::VK_F7);
            assert_eq!(prefs.ui_scale_percent, 150);
            assert_eq!((prefs.window_width, prefs.window_height), (720, 560));
            assert_eq!(config.base_interval_ms, 250.0);

            let mut reloaded = test_app(MainLayout::Classic);
            reloaded.settings_path = Some(path.clone());
            reloaded.load_persisted();
            assert_eq!(reloaded.prefs, prefs);
            assert_eq!(reloaded.config.base_interval_ms, 250.0);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn previous_appearances_remain_until_a_deliberate_style_choice() {
        let size = Vec2::new(560.0, 456.0);
        for previous in AppTheme::previous() {
            let ctx = egui::Context::default();
            let mut app = test_app(MainLayout::Classic);
            app.prefs.theme = previous;
            app.show_settings = true;
            let mut output = render_dialog(&mut app, &ctx, size, 1.0, "settings", vec![]);
            for _ in 0..5 {
                output = render_dialog(&mut app, &ctx, size, 1.0, "settings", vec![]);
            }
            assert!(text_bounds(&output)
                .iter()
                .any(|(text, _, _)| text.contains(previous.label())));
            click_dialog_text(&mut app, &ctx, size, "settings", "Light");
            assert_eq!(app.prefs.theme, previous, "mode is disabled for old themes");
            click_dialog_text(&mut app, &ctx, size, "settings", "Modern");
            assert_eq!(app.prefs.theme, AppTheme::ModernDark);
            assert!(app.settings_dirty);
            click_dialog_text(&mut app, &ctx, size, "settings", "Previous themes");
            for _ in 0..20 {
                let _ = render_dialog(&mut app, &ctx, size, 1.0, "settings", vec![]);
            }
            click_dialog_text(&mut app, &ctx, size, "settings", "Terminal green");
            assert_eq!(app.prefs.theme, AppTheme::Matrix);
        }
    }

    #[test]
    fn hotkey_start_cannot_bypass_a_zero_interval() {
        let mut app = test_app(MainLayout::Portrait);
        app.hours = 0;
        app.minutes = 0;
        app.seconds = 0;
        app.millis = 0;
        app.toggle_session();
        assert!(!app.engine.is_running());
        assert!(app.status_text.contains("at least 1 millisecond"));
    }

    #[test]
    fn hotkey_toggle_commits_the_editors_and_rejects_invalid_offset() {
        let mut app = test_app(MainLayout::Classic);
        app.hours = 0;
        app.minutes = 10;
        app.seconds = 0;
        app.millis = 0;
        app.toggle_session();
        assert!(app.engine.is_running());
        assert_eq!(app.config.base_interval_ms, 600_000.0);
        assert!(app.config.max_interval_ms >= 600_000.0);
        assert!(app.status_text.contains("every 600000 ms"));
        assert!(!app.status_text.contains("0.00 CPS"));
        app.engine.request_stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.engine.is_running() {
            assert!(Instant::now() < deadline, "click worker did not drain");
            std::thread::yield_now();
        }
        app.minutes = 0;
        app.millis = 1;
        app.prefs.random_offset_enabled = true;
        app.offset_ms = 5.0;
        app.toggle_session();
        assert!(!app.engine.is_running());
        assert!(app.status_text.contains("Random offset is too large"));
    }

    #[test]
    fn sessions_started_after_a_frame_are_tracked_before_the_next_frame() {
        let mut app = test_app(MainLayout::Classic);
        app.hours = 0;
        app.minutes = 10;
        app.seconds = 0;
        app.millis = 0;
        app.handle_start();
        assert!(app.engine.is_running());
        assert!(
            app.was_running,
            "a quick session must be tracked immediately"
        );
        app.engine.request_stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.engine.is_running() {
            assert!(Instant::now() < deadline, "worker did not stop");
            std::thread::yield_now();
        }
        assert_eq!(app.engine.click_count(), 0, "test must not send input");

        app.was_running = false;
        assert!(app
            .macro_recorder
            .record_click(600_000.0, 50, 60, MouseButton::Right));
        app.start_macro_playback();
        assert!(app.engine.is_running());
        assert!(app.was_running, "a quick macro must be tracked immediately");
        app.engine.request_stop();
        while app.engine.is_running() {
            assert!(Instant::now() < deadline, "macro worker did not stop");
            std::thread::yield_now();
        }
        assert_eq!(app.engine.click_count(), 0, "test must not send input");
    }

    #[test]
    fn failed_playback_does_not_claim_to_have_completed() {
        let dir = scratch("playback-failure-status");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(dir.join("settings.conf"));
        // An invalid HWND fails before posting any input. The app's playback
        // flag exercises its completion handling without injecting a click.
        let config = EngineConfig {
            base_interval_ms: 1.0,
            input_mode: InputMode::BackgroundHwnd,
            target_hwnd: isize::MAX,
            ..EngineConfig::default()
        };
        app.engine.start(&config).expect("test session");
        app.macro_playing = true;
        app.was_running = true;
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.engine.is_running() {
            assert!(
                Instant::now() < deadline,
                "invalid target did not stop the worker"
            );
            std::thread::yield_now();
        }
        app.surface_engine_errors();
        assert!(app.status_kind == StatusKind::Error);
        app.reconcile_session_status();
        assert!(app.status_kind == StatusKind::Error);
        assert!(
            app.status_text.contains("Target window was lost"),
            "{}",
            app.status_text
        );
        assert!(!app.macro_playing);
        assert_eq!(app.engine.click_count(), 0, "test must not send input");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn off_desktop_playback_reports_the_failure_even_if_it_finishes_before_repaint() {
        let dir = scratch("off-desktop-macro");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(dir.join("settings.conf"));
        assert!(app
            .macro_recorder
            .record_click(1.0, i64::MAX, i64::MIN, MouseButton::Left));
        app.macro_until_stopped = false;
        app.macro_loops = 1;
        app.start_macro_playback();
        assert!(app.was_running);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.engine.is_running() {
            assert!(
                Instant::now() < deadline,
                "off-desktop macro did not terminate"
            );
            std::thread::yield_now();
        }
        assert_eq!(app.engine.session_outcome(), SessionOutcome::DispatchFailed);
        // No intervening surface_engine_errors call: the terminal result must
        // still expose the error if the worker wins the frame timing race.
        app.reconcile_session_status();
        assert!(app.status_kind == StatusKind::Error);
        assert!(
            app.status_text.contains("click was not sent"),
            "{}",
            app.status_text
        );
        assert!(!app.macro_playing);
        assert!(!app.was_running);
        assert_eq!(app.engine.click_count(), 0, "test must not send input");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn off_desktop_fixed_click_fails_without_a_mouse_input() {
        let dir = scratch("off-desktop-fixed");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(dir.join("settings.conf"));
        app.hours = 0;
        app.minutes = 0;
        app.seconds = 0;
        app.millis = 1;
        app.repeat_limited = true;
        app.config.max_clicks = 1;
        app.config.coordinate_mode = CoordinateMode::FixedScreen;
        app.config.fixed_x = i64::MAX;
        app.config.fixed_y = i64::MIN;
        app.handle_start();
        assert!(app.was_running);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.engine.is_running() {
            assert!(
                Instant::now() < deadline,
                "off-desktop click did not terminate"
            );
            std::thread::yield_now();
        }
        app.reconcile_session_status();
        assert!(app.status_kind == StatusKind::Error);
        assert!(
            app.status_text.contains("click was not sent"),
            "{}",
            app.status_text
        );
        assert!(!app.was_running);
        assert_eq!(app.engine.click_count(), 0, "test must not send input");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stopped_and_panicked_macro_sessions_finish_with_the_correct_status() {
        let dir = scratch("macro-terminal-stop");
        for panic in [false, true] {
            let mut app = test_app(MainLayout::Classic);
            app.settings_path = Some(dir.join(if panic { "panic.conf" } else { "stop.conf" }));
            assert!(app
                .macro_recorder
                .record_click(600_000.0, 50, 60, MouseButton::Left));
            app.start_macro_playback();
            assert!(app.was_running);
            if panic {
                app.engine.panic_stop();
            } else {
                app.engine.request_stop();
            }
            // Simulate an unrelated warning arriving while the worker drains.
            app.set_status(StatusKind::Warning, "Nonfatal timing warning");
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.engine.is_running() {
                assert!(Instant::now() < deadline, "macro did not drain");
                std::thread::yield_now();
            }
            app.reconcile_session_status();
            if panic {
                assert!(app.status_kind == StatusKind::Panic);
                assert!(app.status_text.contains("Panic stop"));
            } else {
                assert!(app.status_kind == StatusKind::Ready);
                assert!(app.status_text.starts_with("Playback stopped"));
            }
            assert!(!app.macro_playing);
            assert_eq!(app.engine.click_count(), 0, "test must not send input");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn terminal_outcomes_are_not_inferred_from_previous_ui_messages() {
        let mut app = test_app(MainLayout::Classic);
        // Timer/priority warnings are not terminal failures.
        app.last_seen_error = Some("timeBeginPeriod was rejected".to_string());
        app.set_status(StatusKind::Error, "An earlier nonfatal warning");
        app.report_session_outcome(SessionOutcome::Completed, true);
        assert!(app.status_kind == StatusKind::Ready);
        assert!(app.status_text.starts_with("Playback complete"));

        // A status can change while draining: the worker's result is the
        // source of truth, not whether the old message said "Stopping".
        app.set_status(StatusKind::Warning, "Another message while stopping");
        app.report_session_outcome(SessionOutcome::Stopped, true);
        assert!(app.status_kind == StatusKind::Ready);
        assert!(app.status_text.starts_with("Playback stopped"));
        app.report_session_outcome(SessionOutcome::Stopped, false);
        assert!(app.status_text.starts_with("Stopped"));
        app.report_session_outcome(SessionOutcome::Panicked, true);
        assert!(app.status_kind == StatusKind::Panic);
        app.report_session_outcome(SessionOutcome::WorkerLost, true);
        assert!(app.status_kind == StatusKind::Error);
        assert!(app.status_text.contains("worker lost"));
    }

    #[test]
    fn failed_settings_save_does_not_hide_a_critical_status() {
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = None;
        for kind in [StatusKind::Error, StatusKind::Panic] {
            app.set_status(kind, "Critical session result");
            app.mark_dirty();
            assert!(!app.flush_settings());
            assert!(app.status_kind == kind);
            assert_eq!(app.status_text, "Critical session result");
            assert!(app.settings_dirty);
            assert!(app.last_save_error.is_some());
        }
    }

    #[test]
    fn macro_playback_does_not_multiply_recorded_clicks() {
        let mut app = test_app(MainLayout::Classic);
        app.config.click_type = ClickType::Triple;
        app.hours = 0;
        app.minutes = 0;
        app.seconds = 0;
        app.millis = 1;
        app.prefs.random_offset_enabled = true;
        app.offset_ms = 40.0; // invalid for regular clicking, unused by macros
        assert!(app
            .macro_recorder
            .record_click(600_000.0, 50, 60, MouseButton::Right));
        app.macro_until_stopped = false;
        app.macro_loops = 2;
        app.start_macro_playback();
        assert!(app.engine.is_running());
        assert!(app.macro_playing);
        let config = app.engine.session_config_for_test();
        assert_eq!(config.click_type, ClickType::Single);
        assert_eq!(config.random_offset_ms, 0.0);
        assert_eq!(config.max_clicks, 2);
        assert_eq!(config.input_mode, InputMode::GlobalSendInput);
        assert_eq!(config.coordinate_mode, CoordinateMode::FixedScreen);
        app.engine.request_stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.engine.is_running() {
            assert!(Instant::now() < deadline, "macro worker did not drain");
            std::thread::yield_now();
        }
        assert_eq!(app.engine.click_count(), 0, "test must not send input");
    }

    #[test]
    fn unreadable_settings_are_preserved_instead_of_overwritten_by_defaults() {
        let dir = scratch("unreadable-settings");
        for (name, original) in [
            (
                "oversized.conf",
                vec![b'x'; settings::MAX_SETTINGS_BYTES + 1],
            ),
            ("invalid-utf8.conf", vec![0xff, 0xfe, b'X']),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, &original).expect("original file");
            let mut app = test_app(MainLayout::Classic);
            app.settings_path = Some(path.clone());
            app.load_persisted();
            assert!(app.settings_load_error.is_some());
            assert!(app.status_kind == StatusKind::Warning);
            assert!(app.status_text.contains("could not be read"));
            app.prefs.theme = AppTheme::ModernDark;
            app.mark_dirty();
            assert!(!app.flush_settings());
            assert!(app.settings_dirty);
            assert_eq!(std::fs::read(&path).expect("file preserved"), original);
            // Closing or explicitly resetting must not quietly erase the file.
            app.reset_to_defaults();
            eframe::App::on_exit(&mut app, None);
            assert_eq!(
                std::fs::read(&path).expect("file still preserved"),
                original
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_settings_file_remains_a_writable_fresh_install() {
        let dir = scratch("missing-settings");
        let path = dir.join("settings.conf");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path.clone());
        app.load_persisted();
        assert!(app.settings_load_error.is_none());
        app.mark_dirty();
        assert!(app.flush_settings());
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_failure_stays_dirty_and_a_later_save_recovers() {
        let dir = scratch("settings-retry");
        let path = dir.join("settings.conf");
        std::fs::create_dir(&path).expect("block target with directory");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path.clone());
        app.mark_dirty();
        assert!(!app.flush_settings());
        assert!(app.settings_dirty);
        assert!(app.last_save_error.is_some());
        std::fs::remove_dir(&path).expect("unblock target");
        assert!(app.flush_settings());
        assert!(!app.settings_dirty);
        assert!(app.last_save_error.is_none());
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_flush_commits_pending_interval_editors() {
        let dir = scratch("pending-editors");
        let path = dir.join("settings.conf");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path.clone());
        app.hours = 1;
        app.minutes = 2;
        app.seconds = 3;
        app.millis = 4;
        assert!(app.flush_settings());
        let document = std::fs::read_to_string(path).expect("saved document");
        let (config, _) = settings::parse_settings(&document);
        assert_eq!(config.base_interval_ms, 3_723_004.0);
        assert!(config.validate().is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_offset_cannot_discard_other_persisted_settings() {
        let dir = scratch("invalid-settings");
        let path = dir.join("settings.conf");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path.clone());
        app.prefs.theme = AppTheme::Aurora;
        app.prefs.random_offset_enabled = true;
        app.config.mouse_button = MouseButton::Right;
        app.hours = 0;
        app.minutes = 0;
        app.seconds = 0;
        app.millis = 1;
        app.offset_ms = 40.0;
        app.commit_interval_editors();
        assert!(app.validation_message().is_some());
        assert!(app.flush_settings());
        let mut reloaded = test_app(MainLayout::Classic);
        reloaded.settings_path = Some(path);
        reloaded.load_persisted();
        assert_eq!(reloaded.prefs.theme, AppTheme::Aurora);
        assert_eq!(reloaded.config.mouse_button, MouseButton::Right);
        assert_eq!(reloaded.config.base_interval_ms, 1.0);
        assert_eq!(reloaded.config.random_offset_ms, 0.0);
        assert!(reloaded.config.validate().is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_saved_offset_and_legacy_mode_do_not_erase_preferences() {
        let dir = scratch("legacy-settings");
        let path = dir.join("settings.conf");
        let config = EngineConfig {
            base_interval_ms: 1.0,
            random_offset_ms: 40.0,
            input_mode: InputMode::BackgroundHwnd,
            target_hwnd: 99,
            mouse_button: MouseButton::Right,
            ..EngineConfig::default()
        };
        let prefs = AppPrefs {
            theme: AppTheme::Matrix,
            ..AppPrefs::default()
        };
        std::fs::write(&path, settings::serialize_settings(&config, &prefs)).expect("fixture");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path);
        app.load_persisted();
        assert_eq!(app.prefs.theme, AppTheme::Matrix);
        assert_eq!(app.config.base_interval_ms, 1.0);
        assert_eq!(app.config.random_offset_ms, 0.0);
        assert_eq!(app.config.mouse_button, MouseButton::Right);
        assert_eq!(app.config.input_mode, InputMode::GlobalSendInput);
        assert_eq!(app.config.target_hwnd, 0);
        assert!(app.config.validate().is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_saved_engine_fields_keep_independent_preferences() {
        let dir = scratch("bad-engine-settings");
        let path = dir.join("settings.conf");
        let config = EngineConfig {
            sigma_ms: -1.0,
            ..EngineConfig::default()
        };
        let prefs = AppPrefs {
            theme: AppTheme::Aurora,
            ..AppPrefs::default()
        };
        std::fs::write(&path, settings::serialize_settings(&config, &prefs)).expect("fixture");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(path);
        app.load_persisted();
        assert_eq!(app.prefs.theme, AppTheme::Aurora);
        assert_eq!(app.config, EngineConfig::default());
        assert!(app.config.validate().is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cancelling_during_verification_deletes_candidate_and_keeps_source_context() {
        let dir = scratch("cancel-verify");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        let candidate = paths.download.clone();
        let cancel = AtomicBool::new(false);
        let result = OpAutoClickerApp::finish_update_verification(
            paths,
            "1.6.0".to_string(),
            true,
            &cancel,
            || {
                cancel.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            UpdateInstallMessage::Cancelled {
                source_advanced: true
            }
        ));
        assert!(!candidate.exists());
        assert_eq!(std::fs::read(&exe).expect("original"), b"current");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn send_verified(app: &mut OpAutoClickerApp, paths: UpdatePaths, cancel: Arc<AtomicBool>) {
        let (tx, rx) = mpsc::channel();
        let temp_path = paths.download.clone();
        tx.send(UpdateInstallMessage::Verified(ReadyUpdate {
            paths,
            version: "1.6.0".to_string(),
            source_advanced: true,
        }))
        .expect("send report");
        app.update_install = UpdateInstallState::Downloading {
            version: "1.6.0".to_string(),
            total: 1,
            downloaded: Arc::new(AtomicU64::new(1)),
            phase: Arc::new(AtomicU8::new(UPDATE_PHASE_VERIFYING)),
            cancel,
            temp_path,
            source_expected: true,
            source_advanced: Arc::new(AtomicBool::new(true)),
            rx,
        };
    }

    #[test]
    fn cancellation_after_verified_report_still_prevents_the_swap() {
        let dir = scratch("cancel-after-verify");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        std::fs::write(&paths.download, b"replacement").expect("candidate bytes");
        let candidate = paths.download.clone();
        let mut app = test_app(MainLayout::Classic);
        send_verified(&mut app, paths, Arc::new(AtomicBool::new(true)));
        app.poll_update_install(&egui::Context::default());
        assert!(matches!(app.update_install, UpdateInstallState::Idle));
        assert!(app
            .update_notice
            .as_ref()
            .is_some_and(|(_, text)| text.contains("fast-forwarded")));
        assert_eq!(std::fs::read(&exe).expect("current"), b"current");
        assert!(!candidate.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn verified_update_waits_for_click_worker_and_can_be_cancelled() {
        let dir = scratch("cancel-stop");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        std::fs::write(&paths.download, b"replacement").expect("candidate bytes");
        let cancel = Arc::new(AtomicBool::new(false));
        let mut app = test_app(MainLayout::Classic);
        let config = EngineConfig {
            base_interval_ms: 600_000.0,
            max_interval_ms: 600_000.0,
            ..EngineConfig::default()
        };
        assert!(app.engine.start(&config).is_ok());
        send_verified(&mut app, paths, Arc::clone(&cancel));
        let ctx = egui::Context::default();
        app.poll_update_install(&ctx);
        assert!(matches!(
            app.update_install,
            UpdateInstallState::WaitingForStop { .. }
        ));
        cancel.store(true, Ordering::Release);
        app.poll_update_install(&ctx);
        assert!(matches!(app.update_install, UpdateInstallState::Idle));
        assert_eq!(std::fs::read(&exe).expect("current"), b"current");
        assert!(app
            .update_notice
            .as_ref()
            .is_some_and(|(_, text)| text.contains("fast-forwarded")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_relaunch_restores_original_and_reports_source_change() {
        let dir = scratch("failed-relaunch");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        std::fs::write(&paths.download, b"not an executable").expect("candidate");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(dir.join("settings.conf"));
        app.install_verified_update(
            &egui::Context::default(),
            &ReadyUpdate {
                paths: paths.clone(),
                version: "1.6.0".to_string(),
                source_advanced: true,
            },
        );
        assert_eq!(std::fs::read(&exe).expect("restored exe"), b"current");
        assert!(!paths.backup.exists());
        let notice = &app.update_notice.as_ref().expect("failure notice").1;
        assert!(notice.contains("fast-forwarded"));
        assert!(notice.contains("was restored"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pre_cancelled_verification_never_runs_the_hash() {
        let dir = scratch("cancel-before-verify");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        let candidate = paths.download.clone();
        let cancel = AtomicBool::new(true);
        let result = OpAutoClickerApp::finish_update_verification(
            paths,
            "1.6.0".to_string(),
            false,
            &cancel,
            || panic!("cancelled attempt must not hash"),
        );
        assert!(matches!(
            result,
            UpdateInstallMessage::Cancelled {
                source_advanced: false
            }
        ));
        assert!(!candidate.exists());
        assert_eq!(std::fs::read(&exe).expect("current"), b"current");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_settings_save_prevents_install_and_preserves_original() {
        let dir = scratch("update-settings-failure");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        std::fs::write(&paths.download, b"replacement").expect("candidate");
        let settings = dir.join("settings.conf");
        std::fs::create_dir(&settings).expect("block settings target");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(settings);
        app.install_verified_update(
            &egui::Context::default(),
            &ReadyUpdate {
                paths: paths.clone(),
                version: "1.6.0".to_string(),
                source_advanced: true,
            },
        );
        assert_eq!(std::fs::read(&exe).expect("original"), b"current");
        assert!(!paths.backup.exists());
        assert!(!paths.download.exists());
        assert!(app
            .update_notice
            .as_ref()
            .is_some_and(|(_, text)| text.contains("Settings could not be saved")
                && text.contains("fast-forwarded")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recovery_backup_blocks_further_in_app_updates() {
        let dir = scratch("manual-recovery");
        let exe = dir.join("macro_clicker.exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        std::fs::write(&paths.backup, b"known-good original").expect("recovery backup");
        let mut app = test_app(MainLayout::Classic);
        app.settings_path = Some(dir.join("settings.conf"));
        app.install_verified_update(
            &egui::Context::default(),
            &ReadyUpdate {
                paths: paths.clone(),
                version: "1.6.0".to_string(),
                source_advanced: false,
            },
        );
        assert_eq!(
            app.update_recovery_backup.as_deref(),
            Some(paths.backup.as_path())
        );
        assert_eq!(
            std::fs::read(&paths.backup).expect("recovery retained"),
            b"known-good original"
        );
        app.start_update_install();
        assert!(app
            .update_notice
            .as_ref()
            .is_some_and(|(_, text)| text.contains("restart MacroClicker")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn disconnected_source_worker_resolves_with_checkout_warning() {
        let dir = scratch("update-worker-disconnect");
        let exe = dir.join("macro_clicker.exe");
        std::fs::write(&exe, b"current").expect("exe");
        let paths = self_update::reserve_update_paths(&exe).expect("candidate");
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        let mut app = test_app(MainLayout::Classic);
        app.update_install = UpdateInstallState::Downloading {
            version: "1.6.0".to_string(),
            total: 1,
            downloaded: Arc::new(AtomicU64::new(0)),
            phase: Arc::new(AtomicU8::new(UPDATE_PHASE_SOURCE)),
            cancel: Arc::new(AtomicBool::new(false)),
            temp_path: paths.download.clone(),
            source_expected: true,
            source_advanced: Arc::new(AtomicBool::new(false)),
            rx: receiver,
        };
        app.poll_update_install(&egui::Context::default());
        assert!(matches!(app.update_install, UpdateInstallState::Idle));
        assert!(app
            .update_notice
            .as_ref()
            .is_some_and(|(_, text)| text.contains("source checkout may have changed")));
        assert!(!paths.download.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
