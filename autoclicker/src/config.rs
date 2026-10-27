//! Configuration snapshot, validation and a fully atomic runtime snapshot.
//!
//! This module is platform-independent so the validation rules can be unit
//! tested on any operating system. The worker thread never takes a lock to
//! read its configuration: every field is stored in its own atomic.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicIsize, AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum InputMode {
    GlobalSendInput = 0,
    BackgroundHwnd = 1,
}

impl InputMode {
    fn from_i32(value: i32) -> Self {
        match value {
            1 => InputMode::BackgroundHwnd,
            _ => InputMode::GlobalSendInput,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CoordinateMode {
    CurrentCursor = 0,
    FixedScreen = 1,
    WindowClient = 2,
}

impl CoordinateMode {
    fn from_i32(value: i32) -> Self {
        match value {
            1 => CoordinateMode::FixedScreen,
            2 => CoordinateMode::WindowClient,
            _ => CoordinateMode::CurrentCursor,
        }
    }
}

/// Mouse button targeted by every click of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum MouseButton {
    #[default]
    Left = 0,
    Right = 1,
    Middle = 2,
}

impl MouseButton {
    fn from_i32(value: i32) -> Self {
        match value {
            1 => MouseButton::Right,
            2 => MouseButton::Middle,
            _ => MouseButton::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MouseButton::Left => "Left",
            MouseButton::Right => "Right",
            MouseButton::Middle => "Middle",
        }
    }

    // Consumed by the Windows-only dashboard (app.rs); the path-included
    // standalone test builds omit that module, so the narrow allow keeps
    // their `cargo test` output warning-free without weakening anything.
    #[allow(dead_code)]
    pub fn all() -> [MouseButton; 3] {
        [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
    }
}

/// Number of down/up bursts emitted per logical click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum ClickType {
    #[default]
    Single = 1,
    Double = 2,
    Triple = 3,
}

impl ClickType {
    fn from_i32(value: i32) -> Self {
        match value {
            2 => ClickType::Double,
            3 => ClickType::Triple,
            _ => ClickType::Single,
        }
    }

    pub fn burst_count(self) -> usize {
        self as usize
    }

    // Consumed by the Windows-only dashboard (app.rs); the path-included
    // standalone test builds omit that module, so the narrow allow keeps
    // their `cargo test` output warning-free without weakening anything.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            ClickType::Single => "Single",
            ClickType::Double => "Double",
            ClickType::Triple => "Triple",
        }
    }

    #[allow(dead_code)]
    pub fn all() -> [ClickType; 3] {
        [ClickType::Single, ClickType::Double, ClickType::Triple]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    pub base_interval_ms: f64,
    pub sigma_ms: f64,
    pub sigma_percent: f64,
    pub min_interval_ms: f64,
    pub max_interval_ms: f64,

    pub hold_mean_ms: f64,
    pub hold_sigma_ms: f64,
    pub hold_min_ms: f64,
    pub hold_max_ms: f64,

    pub input_mode: InputMode,
    pub coordinate_mode: CoordinateMode,
    pub fixed_x: i64,
    pub fixed_y: i64,
    pub client_x: i64,
    pub client_y: i64,
    pub target_hwnd: isize,
    pub background_synchronous: bool,

    pub mouse_button: MouseButton,
    pub click_type: ClickType,
    /// Uniform +/- jitter in milliseconds applied around the base interval.
    /// Zero disables the jitter and falls back to the Gaussian humanizer.
    pub random_offset_ms: f64,

    pub humanize_enabled: bool,
    pub max_clicks: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            base_interval_ms: 100.0,
            sigma_ms: 5.0,
            sigma_percent: 0.0,
            min_interval_ms: 1.0,
            max_interval_ms: 10_000.0,
            hold_mean_ms: 40.0,
            hold_sigma_ms: 8.0,
            hold_min_ms: 1.0,
            hold_max_ms: 500.0,
            input_mode: InputMode::GlobalSendInput,
            coordinate_mode: CoordinateMode::CurrentCursor,
            fixed_x: 0,
            fixed_y: 0,
            client_x: 100,
            client_y: 100,
            target_hwnd: 0,
            background_synchronous: false,
            mouse_button: MouseButton::Left,
            click_type: ClickType::Single,
            random_offset_ms: 0.0,
            humanize_enabled: false,
            max_clicks: 0,
        }
    }
}

/// Milliseconds in one hour / minute / second, used by the 4.1-style editors.
pub const MS_PER_SECOND: u64 = 1_000;
pub const MS_PER_MINUTE: u64 = 60_000;
pub const MS_PER_HOUR: u64 = 3_600_000;

/// Combines the four Click Interval fields into a total millisecond count.
pub fn interval_parts_to_ms(hours: u64, minutes: u64, seconds: u64, millis: u64) -> u64 {
    hours
        .saturating_mul(MS_PER_HOUR)
        .saturating_add(minutes.saturating_mul(MS_PER_MINUTE))
        .saturating_add(seconds.saturating_mul(MS_PER_SECOND))
        .saturating_add(millis)
}

/// Splits a total millisecond count back into the four Click Interval fields.
pub fn ms_to_interval_parts(total_ms: u64) -> (u64, u64, u64, u64) {
    let hours = total_ms / MS_PER_HOUR;
    let remainder = total_ms % MS_PER_HOUR;
    let minutes = remainder / MS_PER_MINUTE;
    let remainder = remainder % MS_PER_MINUTE;
    let seconds = remainder / MS_PER_SECOND;
    let millis = remainder % MS_PER_SECOND;
    (hours, minutes, seconds, millis)
}

impl EngineConfig {
    pub fn effective_interval_sigma_ms(&self) -> f64 {
        self.sigma_ms + (self.sigma_percent * 0.01) * self.base_interval_ms
    }

    pub fn target_cps(&self) -> f64 {
        if self.base_interval_ms > 0.0 {
            1000.0 / self.base_interval_ms
        } else {
            0.0
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.base_interval_ms.is_finite() || self.base_interval_ms <= 0.0 {
            return Err("Base interval must be a finite positive millisecond value.".to_string());
        }
        if !self.sigma_ms.is_finite() || self.sigma_ms < 0.0 {
            return Err("Interval sigma (ms) must be finite and non-negative.".to_string());
        }
        if !self.sigma_percent.is_finite() || self.sigma_percent < 0.0 {
            return Err("Interval sigma (%) must be finite and non-negative.".to_string());
        }
        if !self.min_interval_ms.is_finite() || self.min_interval_ms <= 0.0 {
            return Err(
                "Minimum interval must be a finite positive millisecond value.".to_string(),
            );
        }
        if !self.max_interval_ms.is_finite() || self.max_interval_ms < self.min_interval_ms {
            return Err("Maximum interval must be finite and not below the minimum.".to_string());
        }
        if self.base_interval_ms < self.min_interval_ms
            || self.base_interval_ms > self.max_interval_ms
        {
            return Err("Base interval must lie inside the min/max clamp range.".to_string());
        }
        if !self.random_offset_ms.is_finite() || self.random_offset_ms < 0.0 {
            return Err(
                "Random offset must be a finite non-negative millisecond value.".to_string(),
            );
        }
        if self.random_offset_ms > 0.0
            && self.base_interval_ms - self.random_offset_ms < self.min_interval_ms
        {
            return Err(
                "Random offset is too large: base minus offset must stay above the minimum interval."
                    .to_string(),
            );
        }

        if !self.hold_mean_ms.is_finite() || self.hold_mean_ms < 0.0 {
            return Err("Hold mean must be a finite non-negative millisecond value.".to_string());
        }
        if !self.hold_sigma_ms.is_finite() || self.hold_sigma_ms < 0.0 {
            return Err("Hold sigma must be finite and non-negative.".to_string());
        }
        if !self.hold_min_ms.is_finite() || self.hold_min_ms < 0.0 {
            return Err("Hold minimum must be finite and non-negative.".to_string());
        }
        if !self.hold_max_ms.is_finite() || self.hold_max_ms < self.hold_min_ms {
            return Err("Hold maximum must be finite and not below the hold minimum.".to_string());
        }
        if self.hold_mean_ms < self.hold_min_ms || self.hold_mean_ms > self.hold_max_ms {
            return Err("Hold mean must lie inside the hold min/max clamp range.".to_string());
        }

        if self.client_x < 0 || self.client_y < 0 {
            return Err("Window client coordinates must be non-negative.".to_string());
        }
        if self.input_mode == InputMode::BackgroundHwnd && self.target_hwnd == 0 {
            return Err("Background mode requires a selected target window.".to_string());
        }
        Ok(())
    }
}

/// Lock-free configuration mirror. The UI thread calls [`store`] while the
/// worker calls [`load`] on every click iteration; no mutex is involved.
pub struct AtomicConfigSnapshot {
    base_interval_ms: AtomicU64,
    sigma_ms: AtomicU64,
    sigma_percent: AtomicU64,
    min_interval_ms: AtomicU64,
    max_interval_ms: AtomicU64,
    hold_mean_ms: AtomicU64,
    hold_sigma_ms: AtomicU64,
    hold_min_ms: AtomicU64,
    hold_max_ms: AtomicU64,
    input_mode: AtomicI32,
    coordinate_mode: AtomicI32,
    fixed_x: AtomicI64,
    fixed_y: AtomicI64,
    client_x: AtomicI64,
    client_y: AtomicI64,
    target_hwnd: AtomicIsize,
    background_synchronous: AtomicBool,
    mouse_button: AtomicI32,
    click_type: AtomicI32,
    random_offset_ms: AtomicU64,
    humanize_enabled: AtomicBool,
    max_clicks: AtomicU64,
}

impl Default for AtomicConfigSnapshot {
    fn default() -> Self {
        let snapshot = Self {
            base_interval_ms: AtomicU64::new(0),
            sigma_ms: AtomicU64::new(0),
            sigma_percent: AtomicU64::new(0),
            min_interval_ms: AtomicU64::new(0),
            max_interval_ms: AtomicU64::new(0),
            hold_mean_ms: AtomicU64::new(0),
            hold_sigma_ms: AtomicU64::new(0),
            hold_min_ms: AtomicU64::new(0),
            hold_max_ms: AtomicU64::new(0),
            input_mode: AtomicI32::new(0),
            coordinate_mode: AtomicI32::new(0),
            fixed_x: AtomicI64::new(0),
            fixed_y: AtomicI64::new(0),
            client_x: AtomicI64::new(0),
            client_y: AtomicI64::new(0),
            target_hwnd: AtomicIsize::new(0),
            background_synchronous: AtomicBool::new(false),
            mouse_button: AtomicI32::new(0),
            click_type: AtomicI32::new(1),
            random_offset_ms: AtomicU64::new(0),
            humanize_enabled: AtomicBool::new(false),
            max_clicks: AtomicU64::new(0),
        };
        snapshot.store(&EngineConfig::default());
        snapshot
    }
}

impl AtomicConfigSnapshot {
    pub fn store(&self, config: &EngineConfig) {
        self.base_interval_ms
            .store(config.base_interval_ms.to_bits(), Ordering::Release);
        self.sigma_ms
            .store(config.sigma_ms.to_bits(), Ordering::Release);
        self.sigma_percent
            .store(config.sigma_percent.to_bits(), Ordering::Release);
        self.min_interval_ms
            .store(config.min_interval_ms.to_bits(), Ordering::Release);
        self.max_interval_ms
            .store(config.max_interval_ms.to_bits(), Ordering::Release);
        self.hold_mean_ms
            .store(config.hold_mean_ms.to_bits(), Ordering::Release);
        self.hold_sigma_ms
            .store(config.hold_sigma_ms.to_bits(), Ordering::Release);
        self.hold_min_ms
            .store(config.hold_min_ms.to_bits(), Ordering::Release);
        self.hold_max_ms
            .store(config.hold_max_ms.to_bits(), Ordering::Release);
        self.input_mode
            .store(config.input_mode as i32, Ordering::Release);
        self.coordinate_mode
            .store(config.coordinate_mode as i32, Ordering::Release);
        self.fixed_x.store(config.fixed_x, Ordering::Release);
        self.fixed_y.store(config.fixed_y, Ordering::Release);
        self.client_x.store(config.client_x, Ordering::Release);
        self.client_y.store(config.client_y, Ordering::Release);
        self.target_hwnd
            .store(config.target_hwnd, Ordering::Release);
        self.background_synchronous
            .store(config.background_synchronous, Ordering::Release);
        self.mouse_button
            .store(config.mouse_button as i32, Ordering::Release);
        self.click_type
            .store(config.click_type as i32, Ordering::Release);
        self.random_offset_ms
            .store(config.random_offset_ms.to_bits(), Ordering::Release);
        self.humanize_enabled
            .store(config.humanize_enabled, Ordering::Release);
        self.max_clicks.store(config.max_clicks, Ordering::Release);
    }

    pub fn load(&self) -> EngineConfig {
        EngineConfig {
            base_interval_ms: f64::from_bits(self.base_interval_ms.load(Ordering::Acquire)),
            sigma_ms: f64::from_bits(self.sigma_ms.load(Ordering::Acquire)),
            sigma_percent: f64::from_bits(self.sigma_percent.load(Ordering::Acquire)),
            min_interval_ms: f64::from_bits(self.min_interval_ms.load(Ordering::Acquire)),
            max_interval_ms: f64::from_bits(self.max_interval_ms.load(Ordering::Acquire)),
            hold_mean_ms: f64::from_bits(self.hold_mean_ms.load(Ordering::Acquire)),
            hold_sigma_ms: f64::from_bits(self.hold_sigma_ms.load(Ordering::Acquire)),
            hold_min_ms: f64::from_bits(self.hold_min_ms.load(Ordering::Acquire)),
            hold_max_ms: f64::from_bits(self.hold_max_ms.load(Ordering::Acquire)),
            input_mode: InputMode::from_i32(self.input_mode.load(Ordering::Acquire)),
            coordinate_mode: CoordinateMode::from_i32(self.coordinate_mode.load(Ordering::Acquire)),
            fixed_x: self.fixed_x.load(Ordering::Acquire),
            fixed_y: self.fixed_y.load(Ordering::Acquire),
            client_x: self.client_x.load(Ordering::Acquire),
            client_y: self.client_y.load(Ordering::Acquire),
            target_hwnd: self.target_hwnd.load(Ordering::Acquire),
            background_synchronous: self.background_synchronous.load(Ordering::Acquire),
            mouse_button: MouseButton::from_i32(self.mouse_button.load(Ordering::Acquire)),
            click_type: ClickType::from_i32(self.click_type.load(Ordering::Acquire)),
            random_offset_ms: f64::from_bits(self.random_offset_ms.load(Ordering::Acquire)),
            humanize_enabled: self.humanize_enabled.load(Ordering::Acquire),
            max_clicks: self.max_clicks.load(Ordering::Acquire),
        }
    }
}
