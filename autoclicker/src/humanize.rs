//! Gaussian humanization engine.
//!
//! A deterministic SplitMix64 PRNG feeds a Box-Muller transform. A thread-local
//! instance is seeded from OS-visible entropy (system clock, thread identity,
//! a process-wide counter and, on Windows, QueryPerformanceCounter). Every
//! produced delay is clamped into the configured legal range, so negative or
//! non-finite delays can never reach the scheduler.

use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::config::EngineConfig;

static ENTROPY_COUNTER: AtomicU64 = AtomicU64::new(0x9e3779b97f4a7c15);

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn entropy_seed() -> u64 {
    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    let counter = ENTROPY_COUNTER.fetch_add(1, Ordering::Relaxed);
    counter.hash(&mut hasher);
    let mut seed = hasher.finish();
    seed ^= splitmix64(counter);
    seed
}

pub struct GaussianHumanizer {
    state: Cell<u64>,
    spare: Cell<Option<f64>>,
}

impl Default for GaussianHumanizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GaussianHumanizer {
    pub fn new() -> Self {
        Self {
            state: Cell::new(entropy_seed()),
            spare: Cell::new(None),
        }
    }

    /// Deterministic constructor used by the unit tests.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            state: Cell::new(splitmix64(seed)),
            spare: Cell::new(None),
        }
    }

    /// Folds external entropy (on Windows: a QueryPerformanceCounter reading)
    /// into the PRNG state and discards any cached normal sample.
    pub fn reseed(&self, extra_entropy: u64) {
        self.state.set(splitmix64(extra_entropy ^ self.state.get()));
        self.spare.set(None);
    }

    fn next_u64(&self) -> u64 {
        let next = splitmix64(self.state.get());
        self.state.set(next);
        next
    }

    /// Uniform sample in [0, 1).
    fn next_unit(&self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Pure Box-Muller transform. Inputs outside (0, 1] are defended against
    /// so the logarithm can never receive zero.
    pub fn box_muller_standard(u1: f64, u2: f64) -> f64 {
        let safe_u1 = if u1.is_finite() && u1 > 0.0 && u1 <= 1.0 {
            u1
        } else {
            f64::MIN_POSITIVE
        };
        let angle = if u2.is_finite() {
            u2.clamp(0.0, 1.0)
        } else {
            0.5
        } * 2.0
            * std::f64::consts::TAU;
        (-2.0 * safe_u1.ln()).sqrt() * angle.cos()
    }

    fn next_gaussian(&self, mean: f64, sigma: f64) -> f64 {
        if sigma <= 0.0 {
            return mean;
        }
        if let Some(cached) = self.spare.take() {
            return mean + sigma * cached;
        }
        let mut u1 = 0.0;
        while u1 <= 0.0 {
            u1 = self.next_unit();
        }
        let u2 = self.next_unit();
        let radius = (-2.0 * u1.ln()).sqrt();
        let angle = std::f64::consts::TAU * u2;
        self.spare.set(Some(radius * angle.sin()));
        mean + sigma * radius * angle.cos()
    }

    pub fn next_interval_ms(&self, config: &EngineConfig) -> f64 {
        let raw = if config.humanize_enabled {
            self.next_gaussian(
                config.base_interval_ms,
                config.effective_interval_sigma_ms(),
            )
        } else {
            config.base_interval_ms
        };
        clamp_finite(raw, config.min_interval_ms, config.max_interval_ms)
    }

    /// Uniform jitter in `[base - offset, base + offset]`, matching the
    /// "Random offset +- ms" control of OP Auto Clicker 4.1. A zero or
    /// negative offset returns the exact base interval. The result is clamped
    /// into the legal interval range so it can never reach zero or negatives.
    pub fn next_offset_interval_ms(&self, config: &EngineConfig) -> f64 {
        if config.random_offset_ms <= 0.0 {
            return config.base_interval_ms;
        }
        let unit = self.next_unit();
        let raw = config.base_interval_ms - config.random_offset_ms
            + 2.0 * config.random_offset_ms * unit;
        clamp_finite(raw, config.min_interval_ms, config.max_interval_ms)
    }

    pub fn next_hold_ms(&self, config: &EngineConfig) -> f64 {
        let raw = if config.hold_sigma_ms <= 0.0 {
            config.hold_mean_ms
        } else {
            self.next_gaussian(config.hold_mean_ms, config.hold_sigma_ms)
        };
        clamp_finite(raw, config.hold_min_ms, config.hold_max_ms)
    }
}

fn clamp_finite(value: f64, low: f64, high: f64) -> f64 {
    if !value.is_finite() {
        return low;
    }
    value.clamp(low, high)
}

thread_local! {
    static THREAD_HUMANIZER: GaussianHumanizer = GaussianHumanizer::new();
}

pub fn with_thread_humanizer<R>(f: impl FnOnce(&GaussianHumanizer) -> R) -> R {
    THREAD_HUMANIZER.with(f)
}

/// Re-seeds the calling thread's humanizer with additional entropy. The click
/// worker calls this at every session start with a fresh QPC reading so the
/// thread-local PRNG is seeded from both OS-visible entropy and QPC.
pub fn reseed_thread_humanizer(extra_entropy: u64) {
    THREAD_HUMANIZER.with(|humanizer| humanizer.reseed(extra_entropy));
}
