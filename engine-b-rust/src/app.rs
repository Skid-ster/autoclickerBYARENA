//! egui cyberpunk dashboard for Engine B.
//!
//! All worker interaction goes through atomics and the lock-free telemetry
//! store. The update thread never blocks on the worker and only requests
//! continuous repaints while a click session is actually running.

use std::time::Duration;

use egui::{Align, Color32, FontId, Grid, Sense, Stroke, Vec2};

use crate::config::{CoordinateMode, EngineConfig, InputMode};
use crate::engine::{ClickEngine, STATE_PANIC, STATE_RUNNING, STATE_STOPPING};
use crate::telemetry::{
    compute_stats, ClickRecord, GRAPH_HISTORY_SIZE, HISTOGRAM_BINS,
    SLIDING_WINDOW_LIMIT, TelemetryStats,
};
use crate::win32::{self, WindowInfo};

const COLOR_BG: Color32 = Color32::from_rgb(10, 12, 24);
const COLOR_PANEL: Color32 = Color32::from_rgb(18, 22, 40);
const COLOR_CYAN: Color32 = Color32::from_rgb(0, 229, 255);
const COLOR_MAGENTA: Color32 = Color32::from_rgb(255, 43, 214);
const COLOR_GREEN: Color32 = Color32::from_rgb(57, 255, 136);
const COLOR_AMBER: Color32 = Color32::from_rgb(255, 191, 0);
const COLOR_RED: Color32 = Color32::from_rgb(255, 70, 90);
const COLOR_DIM: Color32 = Color32::from_rgb(130, 140, 165);

const LOG_LIMIT: usize = 200;

pub struct EngineBApp {
    engine: ClickEngine,
    config: EngineConfig,
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
            cps_editor,
            windows: Vec::new(),
            selected_window: None,
            logs: vec![
                "OP 4.1 Killer Engine B initialized. F8 is the panic stop key."
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
        style.visuals.window_fill = COLOR_PANEL;
        style.visuals.extreme_bg_color = Color32::from_rgb(13, 16, 32);
        style.visuals.widgets.noninteractive.bg_fill = COLOR_PANEL;
        style.visuals.widgets.noninteractive.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(224, 230, 245));
        style.visuals.widgets.inactive.bg_fill =
            Color32::from_rgb(26, 32, 56);
        style.visuals.widgets.inactive.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(224, 230, 245));
        style.visuals.widgets.hovered.bg_fill =
            Color32::from_rgb(36, 44, 74);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, COLOR_CYAN);
        style.visuals.widgets.active.bg_fill =
            Color32::from_rgb(46, 56, 92);
        style.visuals.widgets.active.fg_stroke =
            Stroke::new(1.0, COLOR_CYAN);
        style.visuals.selection.bg_fill =
            Color32::from_rgba_unmultiplied(255, 43, 214, 90);
        style.visuals.hyperlink_color = COLOR_CYAN;
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style.visuals.widgets.noninteractive.rounding = 8.0.into();
        style.visuals.widgets.inactive.rounding = 6.0.into();
        style.visuals.widgets.hovered.rounding = 6.0.into();
        style.visuals.widgets.active.rounding = 6.0.into();
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

    fn draw_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new(
                "OP 4.1 KILLER  -  ENGINE B (Rust / egui / windows-sys)",
            ).color(COLOR_CYAN));
            let (text, color) = match self.engine.state() {
                STATE_RUNNING => ("RUNNING", COLOR_GREEN),
                STATE_STOPPING => ("STOPPING", COLOR_AMBER),
                STATE_PANIC => ("PANIC STOP", COLOR_RED),
                _ => ("IDLE", COLOR_CYAN),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("STATE: {text}")).color(color).strong());
            });
        });
        ui.label(format!(
            "Clicks: {}   Elapsed: {:.2} s   Actual CPS: {:.2}   Missed: {}",
            self.engine.click_count(),
            self.engine.elapsed_ms() / 1000.0,
            self.stats.actual_cps,
            self.engine.missed_count()
        ));
        if self.engine.priority_denied() {
            ui.label(
                egui::RichText::new(
                    "WARNING: THREAD_PRIORITY_TIME_CRITICAL was denied by the OS.",
                )
                .color(COLOR_RED),
            );
        }
        ui.separator();
    }

    fn draw_timing_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).fill(COLOR_PANEL).show(ui, |ui| {
            ui.label(egui::RichText::new("TIMING AND GAUSSIAN HUMANIZATION").color(COLOR_DIM));
            ui.separator();
            if ui
                .add(
                    egui::DragValue::new(&mut self.cps_editor)
                        .speed(0.5)
                        .clamp_range(0.1..=10000.0)
                        .fixed_decimals(3)
                        .prefix("Target CPS: "),
                )
                .changed()
            {
                if self.cps_editor > 0.0 {
                    self.config.base_interval_ms = 1000.0 / self.cps_editor;
                }
            }
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
            ui.checkbox(&mut self.config.humanize_enabled, "Enable Gaussian humanization");

            ui.separator();
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

            match self.config.validate() {
                Ok(()) => ui.label(
                    egui::RichText::new(format!(
                        "Config valid. Effective interval sigma: {:.3} ms",
                        self.config.effective_interval_sigma_ms()
                    ))
                    .color(COLOR_GREEN),
                ),
                Err(error) => ui.label(egui::RichText::new(format!("Config: {error}")).color(COLOR_AMBER)),
            }
        });
    }

    fn draw_input_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).fill(COLOR_PANEL).show(ui, |ui| {
            ui.label(egui::RichText::new("INPUT DISPATCH").color(COLOR_DIM));
            ui.separator();
            egui::ComboBox::from_label("Input mode")
                .selected_text(match self.config.input_mode {
                    InputMode::GlobalSendInput => "Global SendInput",
                    InputMode::BackgroundHwnd => "Background HWND messages",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.config.input_mode,
                        InputMode::GlobalSendInput,
                        "Global SendInput",
                    );
                    ui.selectable_value(
                        &mut self.config.input_mode,
                        InputMode::BackgroundHwnd,
                        "Background HWND messages",
                    );
                });
            egui::ComboBox::from_label("Coordinate mode")
                .selected_text(match self.config.coordinate_mode {
                    CoordinateMode::CurrentCursor => "Current cursor position",
                    CoordinateMode::FixedScreen => "Fixed screen coordinates",
                    CoordinateMode::WindowClient => "Target window client coordinates",
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
                    ui.selectable_value(
                        &mut self.config.coordinate_mode,
                        CoordinateMode::WindowClient,
                        "Target window client coordinates",
                    );
                });
            ui.add(
                egui::DragValue::new(&mut self.config.fixed_x)
                    .clamp_range(-100000..=100000)
                    .prefix("Fixed screen X: "),
            );
            ui.add(
                egui::DragValue::new(&mut self.config.fixed_y)
                    .clamp_range(-100000..=100000)
                    .prefix("Fixed screen Y: "),
            );
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
            ui.checkbox(
                &mut self.config.background_synchronous,
                "Use SendMessageTimeoutW for background delivery",
            );
            ui.label(
                egui::RichText::new(
                    "Global mode uses arrayed SendInput; background mode posts WM_MOUSE messages.",
                )
                .color(COLOR_DIM)
                .small(),
            );
        });
    }

    fn draw_target_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).fill(COLOR_PANEL).show(ui, |ui| {
            ui.label(egui::RichText::new("BACKGROUND TARGET WINDOW").color(COLOR_DIM));
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Refresh window list").clicked() {
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
                if ui.button("Validate selected HWND").clicked() {
                    match self.selected_window.and_then(|i| self.windows.get(i)) {
                        Some(info) if win32::is_valid_window(info.hwnd) => {
                            self.log("Selected HWND is valid.")
                        }
                        _ => self.log("Selected HWND is invalid."),
                    }
                }
            });
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    if self.windows.is_empty() {
                        ui.label(egui::RichText::new("No windows enumerated yet.").color(COLOR_DIM));
                    }
                    for (index, info) in self.windows.iter().enumerate() {
                        let label = format!(
                            "{}  [PID {}]{}{}",
                            info.title,
                            info.process_id,
                            if info.minimized { "  (minimized)" } else { "" },
                            if info.client_width > 0 {
                                format!("  {}x{}", info.client_width, info.client_height)
                            } else {
                                String::new()
                            }
                        );
                        if ui
                            .selectable_label(self.selected_window == Some(index), label)
                            .clicked()
                        {
                            self.selected_window = Some(index);
                            self.config.target_hwnd = info.hwnd;
                        }
                    }
                });
        });
    }

    fn draw_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let running = self.engine.is_running();
            ui.add_enabled_ui(!running, |ui| {
                if ui
                    .add(egui::Button::new("START").fill(Color32::from_rgba_unmultiplied(57, 255, 136, 90)).min_size(Vec2::new(140.0, 36.0)))
                    .clicked()
                {
                    self.handle_start();
                }
            });
            if ui
                .add(egui::Button::new("STOP").fill(Color32::from_rgba_unmultiplied(255, 191, 0, 90)).min_size(Vec2::new(140.0, 36.0)))
                .clicked()
            {
                self.engine.request_stop();
                self.log("Graceful stop requested.");
            }
            if ui
                .add(egui::Button::new("PANIC STOP (F8)").fill(Color32::from_rgba_unmultiplied(255, 70, 90, 110)).min_size(Vec2::new(180.0, 36.0)))
                .clicked()
            {
                self.engine.panic_stop();
                self.log("PANIC STOP triggered from dashboard.");
            }
        });
    }

    fn draw_cps_graph(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 130.0),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, COLOR_PANEL);
        for step in 1..4 {
            let y = rect.top() + rect.height() * step as f32 / 4.0;
            painter.line_segment(
                [egui::pos2(rect.left() + 4.0, y), egui::pos2(rect.right() - 4.0, y)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 229, 255, 28)),
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
                let x = rect.left()
                    + rect.width() * i as f32 / (self.cps_history.len() - 1) as f32;
                let normalized = (self.cps_history[index] / max_cps).clamp(0.0, 1.0);
                let y = rect.bottom() - 6.0 - normalized * (rect.height() - 16.0);
                let current = egui::pos2(x, y);
                if i > 0 {
                    painter.line_segment([previous, current], Stroke::new(2.0, COLOR_CYAN));
                }
                previous = current;
            }
        }
        painter.text(
            rect.left_top() + Vec2::new(8.0, 4.0),
            Align::LEFT_TOP,
            "REAL-TIME CPS",
            FontId::proportional(12.0),
            COLOR_DIM,
        );
    }

    fn draw_histogram(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 130.0),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, COLOR_PANEL);
        let max_bin = self.stats.histogram.iter().copied().max().unwrap_or(0).max(1) as f32;
        let gap = 2.0;
        let bar_width = rect.width() / HISTOGRAM_BINS as f32 - gap;
        for (bin, count) in self.stats.histogram.iter().enumerate() {
            let height = rect.height() * *count as f32 / max_bin * 0.82;
            let x = rect.left() + 4.0 + bin as f32 * (bar_width + gap);
            let color = if bin % 2 == 0 { COLOR_MAGENTA } else { COLOR_CYAN };
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x, rect.bottom() - height - 6.0),
                    Vec2::new(bar_width, height),
                ),
                3.0,
                color,
            );
        }
        painter.text(
            rect.left_top() + Vec2::new(8.0, 4.0),
            Align::LEFT_TOP,
            "INTERVAL HISTOGRAM (ms)",
            FontId::proportional(12.0),
            COLOR_DIM,
        );
    }

    fn draw_analytics(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).fill(COLOR_PANEL).show(ui, |ui| {
            ui.label(egui::RichText::new("LIVE PERFORMANCE ANALYTICS").color(COLOR_DIM));
            ui.separator();
            Grid::new("stat_grid").num_columns(2).spacing(Vec2::new(20.0, 6.0)).show(ui, |ui| {
                ui.label("Actual CPS");
                ui.label(format!("{:.3}", self.stats.actual_cps));
                ui.end_row();
                ui.label("Interval mean");
                ui.label(format!("{:.4} ms", self.stats.mean_interval_ms));
                ui.end_row();
                ui.label("Interval sigma");
                ui.label(format!("{:.4} ms", self.stats.sigma_interval_ms));
                ui.end_row();
                ui.label("Consistency (+/-10 % band)");
                ui.label(format!("{:.2} %", self.stats.consistency_pct));
                ui.end_row();
                ui.label("Observed interval range");
                ui.label(format!(
                    "{:.4} .. {:.4} ms",
                    self.stats.min_interval_ms, self.stats.max_interval_ms
                ));
                ui.end_row();
                ui.label("Missed deadlines");
                ui.label(self.stats.missed_deadlines.to_string());
                ui.end_row();
                ui.label("Ring buffer dropped");
                ui.label(self.engine.telemetry().dropped_records().to_string());
                ui.end_row();
            });
            ui.separator();
            self.draw_cps_graph(ui);
            ui.add_space(6.0);
            self.draw_histogram(ui);
        });
    }

    fn draw_log(&self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).fill(COLOR_PANEL).show(ui, |ui| {
            ui.label(egui::RichText::new("STATUS AND ERROR LOG").color(COLOR_DIM));
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(140.0)
                .show(ui, |ui| {
                    for line in &self.logs {
                        let warning = line.contains("PANIC")
                            || line.contains("rejected")
                            || line.contains("error")
                            || line.contains("WARNING");
                        let color = if warning { COLOR_AMBER } else { Color32::LIGHT_GRAY };
                        ui.label(egui::RichText::new(line).color(color).small());
                    }
                });
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

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                self.draw_header(ui);
                ui.columns(2, |columns| {
                    self.draw_timing_panel(&mut columns[0]);
                    self.draw_input_panel(&mut columns[0]);
                    self.draw_target_panel(&mut columns[0]);
                    self.draw_controls(&mut columns[0]);
                    self.draw_analytics(&mut columns[1]);
                });
                ui.separator();
                self.draw_log(ui);
            });
        });
    }
}
