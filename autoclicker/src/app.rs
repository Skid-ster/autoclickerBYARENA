//! Single-screen dark dashboard for the OP Auto Clicker.
//!
//! All worker interaction goes through atomics and the lock-free telemetry
//! store. The update thread never blocks on the worker and only requests
//! continuous repaints while a click session is actually running.

use std::time::Duration;

use egui::{
    Align2, Color32, FontId, Frame, Grid, Margin, Rounding, Sense, Slider,
    Stroke, Vec2,
};

use crate::config::{CoordinateMode, EngineConfig, InputMode};
use crate::engine::{ClickEngine, STATE_PANIC, STATE_RUNNING, STATE_STOPPING};
use crate::telemetry::{
    compute_stats, ClickRecord, GRAPH_HISTORY_SIZE, HISTOGRAM_BINS,
    SLIDING_WINDOW_LIMIT, TelemetryStats,
};
use crate::win32::{self, WindowInfo};

// Modern dark palette. Opaque colors use the const `from_rgb`; translucent
// ones use the const-capable premultiplied constructor.
const COLOR_BG: Color32 = Color32::from_rgb(14, 17, 22);
const COLOR_CARD: Color32 = Color32::from_rgb(22, 27, 34);
const COLOR_INPUT: Color32 = Color32::from_rgb(28, 35, 48);
const COLOR_BORDER: Color32 = Color32::from_rgb(43, 53, 70);
const COLOR_ACCENT: Color32 = Color32::from_rgb(76, 141, 255);
const COLOR_SUCCESS: Color32 = Color32::from_rgb(63, 185, 80);
const COLOR_DANGER: Color32 = Color32::from_rgb(248, 81, 73);
const COLOR_WARNING: Color32 = Color32::from_rgb(210, 153, 34);
const COLOR_TEXT: Color32 = Color32::from_rgb(230, 237, 243);
const COLOR_TEXT_DIM: Color32 = Color32::from_rgb(139, 148, 158);
const COLOR_CHART_GRID: Color32 =
    Color32::from_rgba_premultiplied(48, 58, 76, 90);
const COLOR_CHART_BAR_ALT: Color32 =
    Color32::from_rgba_premultiplied(76, 141, 255, 110);
const COLOR_PILL_IDLE: Color32 =
    Color32::from_rgba_premultiplied(139, 148, 158, 40);

const LOG_LIMIT: usize = 200;
const CPS_MIN: f64 = 0.1;
const CPS_MAX: f64 = 50.0;
const SIDE_PANEL_WIDTH: f32 = 372.0_f32;

pub struct EngineBApp {
    engine: ClickEngine,
    config: EngineConfig,
    repeat_limited: bool,
    cps_editor: f64,
    windows: Vec<WindowInfo>,
    selected_window: Option<usize>,
    logs: Vec<String>,
    last_seen_error: Option<String>,
    sliding_window: Vec<ClickRecord>,
    cps_history: [f64; GRAPH_HISTORY_SIZE],
    cps_head: usize,
    cps_count: usize,
    stats: TelemetryStats,
    f8_was_down: bool,
    previous_elapsed_ms: f64,
    previous_clicks: u64,
}

impl Default for EngineBApp {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineBApp {
    pub fn new() -> Self {
        let config = EngineConfig::default();
        let cps_editor = config.target_cps();
        Self {
            engine: ClickEngine::new(),
            config,
            repeat_limited: false,
            cps_editor,
            windows: Vec::new(),
            selected_window: None,
            logs: vec![
                "OP Auto Clicker initialized. F8 is the panic stop key."
                    .to_string(),
            ],
            last_seen_error: None,
            sliding_window: Vec::new(),
            cps_history: [0.0; GRAPH_HISTORY_SIZE],
            cps_head: 0,
            cps_count: 0,
            stats: TelemetryStats::default(),
            f8_was_down: false,
            previous_elapsed_ms: 0.0,
            previous_clicks: 0,
        }
    }

    fn log(&mut self, line: impl Into<String>) {
        self.logs.push(line.into());
        if self.logs.len() > LOG_LIMIT {
            let drop_count = self.logs.len() - LOG_LIMIT;
            self.logs.drain(0..drop_count);
        }
    }

    fn apply_style(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = COLOR_BG;
        style.visuals.window_fill = COLOR_CARD;
        style.visuals.extreme_bg_color = COLOR_INPUT;
        style.visuals.window_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
        style.visuals.widgets.noninteractive.bg_fill = COLOR_CARD;
        style.visuals.widgets.noninteractive.fg_stroke =
            Stroke::new(1.0_f32, COLOR_TEXT);
        style.visuals.widgets.inactive.bg_fill = COLOR_INPUT;
        style.visuals.widgets.inactive.fg_stroke =
            Stroke::new(1.0_f32, COLOR_TEXT);
        style.visuals.widgets.hovered.bg_fill =
            Color32::from_rgb(38, 47, 64);
        style.visuals.widgets.hovered.fg_stroke =
            Stroke::new(1.0_f32, COLOR_ACCENT);
        style.visuals.widgets.active.bg_fill =
            Color32::from_rgb(46, 57, 78);
        style.visuals.widgets.active.fg_stroke =
            Stroke::new(1.0_f32, COLOR_ACCENT);
        style.visuals.selection.bg_fill =
            Color32::from_rgba_premultiplied(76, 141, 255, 70);
        style.visuals.hyperlink_color = COLOR_ACCENT;
        style.visuals.widgets.noninteractive.rounding =
            Rounding::same(10.0_f32);
        style.visuals.widgets.inactive.rounding = Rounding::same(8.0_f32);
        style.visuals.widgets.hovered.rounding = Rounding::same(8.0_f32);
        style.visuals.widgets.active.rounding = Rounding::same(8.0_f32);
        style.spacing.item_spacing = Vec2::new(9.0_f32, 9.0_f32);
        style.spacing.button_padding = Vec2::new(12.0_f32, 7.0_f32);
        ctx.set_style(style);
    }

    fn poll_panic_hotkey(&mut self) {
        let down = win32::is_f8_down();
        if down && !self.f8_was_down && self.engine.is_running() {
            self.engine.panic_stop();
            self.log("PANIC STOP triggered by F8.");
        }
        self.f8_was_down = down;
    }

    fn surface_engine_errors(&mut self) {
        if let Some(error) = self.engine.last_error() {
            if Some(&error) != self.last_seen_error.as_ref() {
                self.log(format!("Engine error: {error}"));
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

    fn handle_start(&mut self) {
        if self.config.input_mode == InputMode::BackgroundHwnd {
            let Some(index) = self.selected_window else {
                self.log("Start rejected: select a target window first.");
                return;
            };
            let Some(info) = self.windows.get(index) else {
                self.log("Start rejected: target list is stale, refresh it.");
                return;
            };
            if !win32::is_valid_window(info.hwnd) {
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
                self.log(format!(
                    "Click session started at {:.2} CPS.",
                    self.config.target_cps()
                ));
            }
            Err(error) => self.log(format!("Start rejected: {error}")),
        }
    }

    fn card<R>(
        ui: &mut egui::Ui,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        Frame::group(ui.style())
            .fill(COLOR_CARD)
            .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
            .rounding(Rounding::same(10.0_f32))
            .inner_margin(Margin::same(13.0_f32))
            .show(ui, add)
            .inner
    }

    fn section_title(ui: &mut egui::Ui, title: &str) {
        ui.label(
            egui::RichText::new(title)
                .color(COLOR_TEXT_DIM)
                .strong()
                .size(12.5_f32),
        );
        ui.add_space(5.0_f32);
    }

    fn hint(ui: &mut egui::Ui, text: &str) {
        ui.add_space(3.0_f32);
        ui.label(
            egui::RichText::new(text)
                .color(COLOR_TEXT_DIM)
                .small(),
        );
    }

    fn status_pill(ui: &mut egui::Ui, text: &str, color: Color32, soft: Color32) {
        let font = FontId::proportional(13.5_f32);
        let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
        let size = galley.size() + Vec2::new(26.0_f32, 10.0_f32);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 9.0_f32, soft);
        painter.rect_stroke(rect, 9.0_f32, Stroke::new(1.0_f32, color));
        painter.text(
            rect.min + Vec2::new(13.0_f32, 5.0_f32),
            Align2::LEFT_TOP,
            text,
            FontId::proportional(13.5_f32),
            color,
        );
    }

    // ------------------------------------------------------------------ header

    fn draw_header(&mut self, ui: &mut egui::Ui) {
        let (text, color, soft) = match self.engine.state() {
            STATE_RUNNING => (
                "RUNNING",
                COLOR_SUCCESS,
                Color32::from_rgba_premultiplied(63, 185, 80, 42),
            ),
            STATE_STOPPING => (
                "STOPPING",
                COLOR_WARNING,
                Color32::from_rgba_premultiplied(210, 153, 34, 42),
            ),
            STATE_PANIC => (
                "PANIC STOP",
                COLOR_DANGER,
                Color32::from_rgba_premultiplied(248, 81, 73, 42),
            ),
            _ => ("IDLE", COLOR_TEXT_DIM, COLOR_PILL_IDLE),
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("OP Auto Clicker")
                        .color(COLOR_TEXT)
                        .size(21.0_f32)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new("Rust edition")
                        .color(COLOR_TEXT_DIM)
                        .small(),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Self::status_pill(ui, text, color, soft);
            });
        });
        ui.add_space(6.0_f32);
        ui.horizontal_wrapped(|ui| {
            let counters = [
                format!("Clicks: {}", self.engine.click_count()),
                format!("Elapsed: {:.2} s", self.engine.elapsed_ms() / 1000.0),
                format!("Actual CPS: {:.2}", self.stats.actual_cps),
                format!("Missed: {}", self.engine.missed_count()),
            ];
            for (i, counter) in counters.iter().enumerate() {
                if i > 0 {
                    ui.add_space(14.0_f32);
                }
                ui.label(
                    egui::RichText::new(counter).color(COLOR_TEXT_DIM),
                );
            }
        });
        if self.engine.priority_denied() {
            ui.add_space(3.0_f32);
            ui.label(
                egui::RichText::new(
                    "Warning: THREAD_PRIORITY_TIME_CRITICAL was denied by the OS.",
                )
                .color(COLOR_WARNING)
                .small(),
            );
        }
    }

    // ------------------------------------------------------------- left column

    fn draw_controls(&mut self, ui: &mut egui::Ui) {
        // Click rate.
        Self::card(ui, |ui| {
            Self::section_title(ui, "CLICK RATE");
            if ui
                .add(
                    Slider::new(&mut self.cps_editor, CPS_MIN..=CPS_MAX)
                        .text("Clicks per second")
                        .fixed_decimals(2),
                )
                .changed()
                && self.cps_editor > 0.0
            {
                self.config.base_interval_ms = 1000.0 / self.cps_editor;
            }
            ui.label(
                egui::RichText::new(format!(
                    "{:.2} ms between clicks",
                    self.config.base_interval_ms
                ))
                .color(COLOR_TEXT_DIM)
                .small(),
            );
        });
        ui.add_space(9.0_f32);

        // Click options.
        Self::card(ui, |ui| {
            Self::section_title(ui, "CLICK OPTIONS");
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.config.input_mode,
                    InputMode::GlobalSendInput,
                    "Global (SendInput)",
                );
                ui.radio_value(
                    &mut self.config.input_mode,
                    InputMode::BackgroundHwnd,
                    "Background (window)",
                );
            });
            ui.add_space(4.0_f32);

            if self.config.input_mode == InputMode::GlobalSendInput {
                let fixed =
                    self.config.coordinate_mode == CoordinateMode::FixedScreen;
                egui::ComboBox::from_label("Position")
                    .selected_text(if fixed {
                        "Fixed screen coordinates"
                    } else {
                        "Current cursor position"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.config.coordinate_mode,
                            CoordinateMode::CurrentCursor,
                            "Current cursor position",
                        );
                        ui.selectable_value(
                            &mut self.config.coordinate_mode,
                            CoordinateMode::FixedScreen,
                            "Fixed screen coordinates",
                        );
                    });
                if fixed {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.config.fixed_x)
                                .clamp_range(-100000..=100000)
                                .prefix("X: "),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.config.fixed_y)
                                .clamp_range(-100000..=100000)
                                .prefix("Y: "),
                        );
                    });
                }
                Self::hint(
                    ui,
                    "Hardware-level input that works in any application.",
                );
            } else {
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
                                    egui::RichText::new(
                                        "List is empty — press Refresh",
                                    )
                                    .color(COLOR_TEXT_DIM),
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
                                    .selectable_label(
                                        self.selected_window == Some(index),
                                        label,
                                    )
                                    .clicked()
                                {
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
                    ui.add(
                        egui::DragValue::new(&mut self.config.client_x)
                            .clamp_range(0..=100000)
                            .prefix("Client X: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.config.client_y)
                            .clamp_range(0..=100000)
                            .prefix("Client Y: "),
                    );
                });
                ui.checkbox(
                    &mut self.config.background_synchronous,
                    "Synchronous delivery (SendMessageTimeoutW)",
                );
                Self::hint(
                    ui,
                    "Clicks land in unfocused or minimized windows via posted WM_MOUSE messages.",
                );
            }
        });
        ui.add_space(9.0_f32);

        // Repeat.
        Self::card(ui, |ui| {
            Self::section_title(ui, "REPEAT");
            ui.radio_value(&mut self.repeat_limited, false, "Until stopped");
            ui.radio_value(
                &mut self.repeat_limited,
                true,
                "Stop after a number of clicks",
            );
            if self.repeat_limited {
                if self.config.max_clicks == 0 {
                    self.config.max_clicks = 100;
                }
                ui.add(
                    egui::DragValue::new(&mut self.config.max_clicks)
                        .clamp_range(1..=1_000_000_000)
                        .prefix("Click count: "),
                );
            } else {
                self.config.max_clicks = 0;
            }
        });
        ui.add_space(9.0_f32);

        // Humanization with advanced fields behind one disclosure.
        Self::card(ui, |ui| {
            Self::section_title(ui, "HUMANIZATION");
            ui.checkbox(
                &mut self.config.humanize_enabled,
                "Enable Gaussian humanization",
            );
            ui.collapsing("Advanced timing & humanization", |ui| {
                ui.add_space(3.0_f32);
                ui.add(
                    egui::DragValue::new(&mut self.config.base_interval_ms)
                        .speed(0.5)
                        .clamp_range(0.05..=10000.0)
                        .fixed_decimals(4)
                        .suffix(" ms base interval"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.sigma_ms)
                        .speed(0.5)
                        .clamp_range(0.0..=1000.0)
                        .fixed_decimals(4)
                        .suffix(" ms interval sigma"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.sigma_percent)
                        .speed(1.0)
                        .clamp_range(0.0..=100.0)
                        .fixed_decimals(2)
                        .suffix(" % interval sigma"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.min_interval_ms)
                        .speed(0.5)
                        .clamp_range(0.05..=10000.0)
                        .fixed_decimals(4)
                        .suffix(" ms min interval"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.max_interval_ms)
                        .speed(10.0)
                        .clamp_range(1.0..=60000.0)
                        .fixed_decimals(4)
                        .suffix(" ms max interval"),
                );
                ui.add_space(6.0_f32);
                ui.add(
                    egui::DragValue::new(&mut self.config.hold_mean_ms)
                        .speed(1.0)
                        .clamp_range(0.0..=5000.0)
                        .fixed_decimals(3)
                        .suffix(" ms hold mean"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.hold_sigma_ms)
                        .speed(0.5)
                        .clamp_range(0.0..=1000.0)
                        .fixed_decimals(3)
                        .suffix(" ms hold sigma"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.hold_min_ms)
                        .speed(0.5)
                        .clamp_range(0.0..=5000.0)
                        .fixed_decimals(3)
                        .suffix(" ms hold min"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.config.hold_max_ms)
                        .speed(5.0)
                        .clamp_range(1.0..=10000.0)
                        .fixed_decimals(3)
                        .suffix(" ms hold max"),
                );
                ui.add_space(6.0_f32);
                match self.config.validate() {
                    Ok(()) => ui.label(
                        egui::RichText::new(format!(
                            "Valid. Effective sigma: {:.3} ms",
                            self.config.effective_interval_sigma_ms()
                        ))
                        .color(COLOR_SUCCESS)
                        .small(),
                    ),
                    Err(error) => ui.label(
                        egui::RichText::new(format!("Configuration issue: {error}"))
                            .color(COLOR_WARNING)
                            .small(),
                    ),
                }
            });
        });
    }

    // ------------------------------------------------------------ right column

    fn draw_cps_graph(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 168.0_f32),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 10.0_f32, COLOR_INPUT);

        for step in 1..4 {
            let y = rect.top() + rect.height() * step as f32 / 4.0_f32;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 8.0_f32, y),
                    egui::pos2(rect.right() - 8.0_f32, y),
                ],
                Stroke::new(1.0_f32, COLOR_CHART_GRID),
            );
        }

        let mut max_cps = (self.config.target_cps() * 1.25).max(1.0);
        for sample in self.cps_history {
            max_cps = max_cps.max(sample);
        }

        if self.cps_count > 1 {
            let mut previous = egui::Pos2::ZERO;
            for i in 0..self.cps_count {
                let index = (self.cps_head
                    + self.cps_history.len()
                    - self.cps_count
                    + i)
                    % self.cps_history.len();
                let x = rect.left()
                    + rect.width() * i as f32
                        / (self.cps_history.len() - 1) as f32;
                let normalized =
                    (self.cps_history[index] / max_cps).clamp(0.0, 1.0) as f32;
                let y = rect.bottom()
                    - 10.0_f32
                    - normalized * (rect.height() - 22.0_f32);
                let current = egui::pos2(x, y);
                if i > 0 {
                    painter.line_segment(
                        [previous, current],
                        Stroke::new(2.0_f32, COLOR_ACCENT),
                    );
                }
                previous = current;
            }
        }
        painter.text(
            rect.left_top() + Vec2::new(12.0_f32, 9.0_f32),
            Align2::LEFT_TOP,
            "REAL-TIME CPS",
            FontId::proportional(12.0_f32),
            COLOR_TEXT_DIM,
        );
    }

    fn draw_histogram(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 150.0_f32),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 10.0_f32, COLOR_INPUT);

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
            let height = rect.height()
                * self.stats.histogram[bin] as f32
                / max_bin
                * 0.78_f32;
            let x =
                rect.left() + 8.0_f32 + bin as f32 * (bar_width + gap);
            let color =
                if bin % 2 == 0 { COLOR_ACCENT } else { COLOR_CHART_BAR_ALT };
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x, rect.bottom() - height - 10.0_f32),
                    Vec2::new(bar_width, height),
                ),
                4.0_f32,
                color,
            );
        }
        painter.text(
            rect.left_top() + Vec2::new(12.0_f32, 9.0_f32),
            Align2::LEFT_TOP,
            "INTERVAL HISTOGRAM (ms)",
            FontId::proportional(12.0_f32),
            COLOR_TEXT_DIM,
        );
    }

    fn draw_analytics(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            Self::section_title(ui, "LIVE METRICS");
            Grid::new("metrics_grid")
                .num_columns(2)
                .spacing(Vec2::new(22.0_f32, 9.0_f32))
                .show(ui, |ui| {
                    let metric =
                        |ui: &mut egui::Ui, label: &str, value: String| {
                            ui.label(
                                egui::RichText::new(label).color(COLOR_TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new(value).color(COLOR_TEXT),
                            );
                            ui.end_row();
                        };
                    metric(ui, "Actual CPS", format!("{:.3}", self.stats.actual_cps));
                    metric(ui, "Interval mean", format!("{:.4} ms", self.stats.mean_interval_ms));
                    metric(ui, "Interval sigma", format!("{:.4} ms", self.stats.sigma_interval_ms));
                    metric(ui, "Consistency (+/-10%)", format!("{:.2} %", self.stats.consistency_pct));
                    metric(ui, "Missed deadlines", self.stats.missed_deadlines.to_string());
                    metric(ui, "Ring buffer dropped", self.engine.telemetry().dropped_records().to_string());
                });
            ui.label(
                egui::RichText::new(format!(
                    "Observed range: {:.4} .. {:.4} ms",
                    self.stats.min_interval_ms, self.stats.max_interval_ms
                ))
                .color(COLOR_TEXT_DIM)
                .small(),
            );
        });
        ui.add_space(9.0_f32);
        Self::card(ui, |ui| self.draw_cps_graph(ui));
        ui.add_space(9.0_f32);
        Self::card(ui, |ui| self.draw_histogram(ui));
    }

    fn draw_activity(&self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            Self::section_title(ui, "RECENT ACTIVITY");
            let height = (ui.available_height() - 8.0_f32).max(80.0_f32);
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(height)
                .show(ui, |ui| {
                    for line in &self.logs {
                        let warning = line.contains("PANIC")
                            || line.contains("rejected")
                            || line.contains("error")
                            || line.contains("Warning");
                        let color =
                            if warning { COLOR_WARNING } else { COLOR_TEXT_DIM };
                        ui.label(
                            egui::RichText::new(line).color(color).small(),
                        );
                    }
                });
        });
    }

    fn draw_footer(&mut self, ui: &mut egui::Ui) {
        let running = self.engine.is_running();
        let width = ui.available_width();
        let gap = ui.spacing().item_spacing.x;
        let start_width = (width - 2.0_f32 * gap) * 0.5_f32;
        let small_width = (width - 2.0_f32 * gap) * 0.25_f32;

        ui.horizontal(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("START")
                                .strong()
                                .color(Color32::WHITE)
                                .size(15.0_f32),
                        )
                        .fill(COLOR_SUCCESS)
                        .min_size(Vec2::new(start_width, 42.0_f32)),
                    )
                    .clicked()
                {
                    self.handle_start();
                }
            });
            ui.add_enabled_ui(running, |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("STOP")
                                .strong()
                                .color(COLOR_WARNING)
                                .size(15.0_f32),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.5_f32, COLOR_WARNING))
                        .min_size(Vec2::new(small_width, 42.0_f32)),
                    )
                    .clicked()
                {
                    self.engine.request_stop();
                    self.log("Graceful stop requested.");
                }
            });
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("PANIC STOP (F8)")
                            .strong()
                            .color(Color32::WHITE)
                            .size(15.0_f32),
                    )
                    .fill(COLOR_DANGER)
                    .min_size(Vec2::new(small_width, 42.0_f32)),
                )
                .clicked()
            {
                self.engine.panic_stop();
                self.log("PANIC STOP triggered from dashboard.");
            }
        });
    }
}

impl eframe::App for EngineBApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_style(ctx);
        self.poll_panic_hotkey();
        self.surface_engine_errors();
        if self.engine.is_running() {
            if let Err(error) = self.engine.update_config(&self.config) {
                if self.last_seen_error.as_deref() != Some(error.as_str()) {
                    self.log(format!("Engine error: {error}"));
                    self.last_seen_error = Some(error);
                }
            }
        }
        self.drain_and_aggregate();
        if self.engine.is_running() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        egui::TopBottomPanel::top("header_panel")
            .frame(
                Frame::none()
                    .fill(COLOR_BG)
                    .inner_margin(Margin::symmetric(16.0_f32, 12.0_f32)),
            )
            .show(ctx, |ui| self.draw_header(ui));

        egui::TopBottomPanel::bottom("control_footer")
            .frame(
                Frame::none()
                    .fill(COLOR_BG)
                    .inner_margin(Margin::symmetric(16.0_f32, 10.0_f32)),
            )
            .show(ctx, |ui| self.draw_footer(ui));

        egui::SidePanel::left("controls_panel")
            .exact_width(SIDE_PANEL_WIDTH)
            .resizable(true)
            .frame(
                Frame::none()
                    .fill(COLOR_BG)
                    .inner_margin(Margin::symmetric(14.0_f32, 6.0_f32)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_controls(ui);
                });
            });

        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(COLOR_BG)
                    .inner_margin(Margin::symmetric(6.0_f32, 6.0_f32)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_analytics(ui);
                    ui.add_space(9.0_f32);
                    self.draw_activity(ui);
                });
            });
    }
}
