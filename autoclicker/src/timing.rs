//! QueryPerformanceCounter scheduling primitives and RAII timer resolution.
//!
//! The active click path contains no sleep calls: waiting is a QPC spin loop
//! backed by [`std::hint::spin_loop`], while idle parking happens on a kernel
//! event in [`crate::engine`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinResult {
    DeadlineReached,
    StopRequested,
    PanicRequested,
}

fn frequency() -> i64 {
    static FREQUENCY: OnceLock<i64> = OnceLock::new();
    *FREQUENCY.get_or_init(|| {
        let mut freq = 0i64;
        // SAFETY: `freq` is a valid local i64 for the lifetime of the call.
        let ok = unsafe { QueryPerformanceFrequency(&mut freq) };
        if ok == 0 || freq <= 0 {
            10_000_000 // documented QPC fallback frequency
        } else {
            freq
        }
    })
}

pub fn qpc_now() -> i64 {
    let mut counter = 0i64;
    // SAFETY: `counter` is a valid local i64 for the lifetime of the call.
    let ok = unsafe { QueryPerformanceCounter(&mut counter) };
    debug_assert_ne!(ok, 0);
    counter
}

pub fn ticks_to_ms(ticks: i64) -> f64 {
    ticks as f64 * 1000.0 / frequency() as f64
}

pub fn ms_to_ticks(milliseconds: f64) -> i64 {
    (milliseconds * frequency() as f64 / 1000.0) as i64
}

pub fn elapsed_ms(start: i64, end: i64) -> f64 {
    ticks_to_ms(end - start)
}

/// RAII guard raising the multimedia timer resolution for one click session.
pub struct ScopedTimerResolution {
    period: u32,
    active: bool,
}

impl ScopedTimerResolution {
    pub fn new(period_ms: u32) -> Self {
        // SAFETY: calling with a validated period of 1 ms; the pairing
        // timeEndPeriod call runs in `drop`.
        let result = unsafe { timeBeginPeriod(period_ms) };
        Self {
            period: period_ms,
            active: result == 0, // TIMERR_NOERROR
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for ScopedTimerResolution {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: matches the successful timeBeginPeriod call in `new`.
            unsafe {
                timeEndPeriod(self.period);
            }
            self.active = false;
        }
    }
}

/// Spins until `deadline_tick` while sampling both stop flags every iteration.
/// Deliberately contains no sleep; `spin_loop` emits the platform PAUSE hint.
pub fn spin_until_deadline(
    deadline_tick: i64,
    stop_requested: &AtomicBool,
    panic_requested: &AtomicBool,
) -> SpinResult {
    loop {
        if panic_requested.load(Ordering::Acquire) {
            return SpinResult::PanicRequested;
        }
        if stop_requested.load(Ordering::Acquire) {
            return SpinResult::StopRequested;
        }
        if qpc_now() >= deadline_tick {
            return SpinResult::DeadlineReached;
        }
        std::hint::spin_loop();
    }
}
