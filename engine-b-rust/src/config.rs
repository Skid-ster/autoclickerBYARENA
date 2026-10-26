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

#[derive(Debug, Clone)]
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
            humanize_enabled: true,
            max_clicks: 0,
        }
    }
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
            return Err("Minimum interval must be a finite positive millisecond value.".to_string());
        }
        if !self.max_interval_ms.is_finite() || self.max_interval_ms < self.min_interval_ms {
            return Err("Maximum interval must be finite and not below the minimum.".to_string());
        }
        if self.base_interval_ms < self.min_interval_ms
            || self.base_interval_ms > self.max_interval_ms
        {
            return Err("Base interval must lie inside the min/max clamp range.".to_string());
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
        self.sigma_ms.store(config.sigma_ms.to_bits(), Ordering::Release);
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
        self.humanize_enabled
            .store(config.humanize_enabled, Ordering::Release);
        self.max_clicks.store(config.max_clicks, Ordering::Release);
    }

    pub fn load(&self) -> EngineConfig {
        EngineConfig {
            base_interval_ms: f64::from_bits(
                self.base_interval_ms.load(Ordering::Acquire),
            ),
            sigma_ms: f64::from_bits(self.sigma_ms.load(Ordering::Acquire)),
            sigma_percent: f64::from_bits(
                self.sigma_percent.load(Ordering::Acquire),
            ),
            min_interval_ms: f64::from_bits(
                self.min_interval_ms.load(Ordering::Acquire),
            ),
            max_interval_ms: f64::from_bits(
                self.max_interval_ms.load(Ordering::Acquire),
            ),
            hold_mean_ms: f64::from_bits(
                self.hold_mean_ms.load(Ordering::Acquire),
            ),
            hold_sigma_ms: f64::from_bits(
                self.hold_sigma_ms.load(Ordering::Acquire),
            ),
            hold_min_ms: f64::from_bits(
                self.hold_min_ms.load(Ordering::Acquire),
            ),
            hold_max_ms: f64::from_bits(
                self.hold_max_ms.load(Ordering::Acquire),
            ),
            input_mode: InputMode::from_i32(
                self.input_mode.load(Ordering::Acquire),
            ),
            coordinate_mode: CoordinateMode::from_i32(
                self.coordinate_mode.load(Ordering::Acquire),
            ),
            fixed_x: self.fixed_x.load(Ordering::Acquire),
            fixed_y: self.fixed_y.load(Ordering::Acquire),
            client_x: self.client_x.load(Ordering::Acquire),
            client_y: self.client_y.load(Ordering::Acquire),
            target_hwnd: self.target_hwnd.load(Ordering::Acquire),
            background_synchronous: self
                .background_synchronous
                .load(Ordering::Acquire),
            humanize_enabled: self.humanize_enabled.load(Ordering::Acquire),
            max_clicks: self.max_clicks.load(Ordering::Acquire),
        }
    }
}
