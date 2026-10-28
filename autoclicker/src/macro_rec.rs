//! Pure macro record & playback bookkeeping (no OS calls), unit-tested on
//! every platform.
//!
//! Recording captures real user clicks as a sequence of steps, each holding
//! the screen position, the mouse button and the measured delay since the
//! previous step (the first step keeps the lead-in delay from the moment
//! recording started). Playback hands the exact sequence to the click
//! engine, which replays positions, buttons and gaps through the same
//! QPC-scheduled worker that drives ordinary sessions — so playback gets
//! drift-free timing, bounded catch-up, the F8 panic path and telemetry for
//! free, which is what puts this Record & Playback ahead of OP Auto
//! Clicker 4.1's.

use crate::config::MouseButton;

/// Hard cap on recorded steps; recording stops accepting clicks beyond it.
pub const MAX_STEPS: usize = 2048;
/// Replayed gaps are clamped below by this (a 0 ms gap cannot be scheduled).
pub const MIN_STEP_DELAY_MS: f64 = 1.0;
/// And above by one hour, matching the interval editor's ceiling spirit.
pub const MAX_STEP_DELAY_MS: f64 = 3_600_000.0;

/// One recorded click: where, which button, and how long after the
/// previous step it happened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroStep {
    pub delay_ms: f64,
    pub x: i64,
    pub y: i64,
    pub button: MouseButton,
}

/// Accumulates clicks while a recording is armed. `elapsed_ms` values are
/// measured from the moment recording started and must be non-decreasing;
/// the recorder turns them into per-step gaps.
#[derive(Debug, Default)]
pub struct MacroRecorder {
    steps: Vec<MacroStep>,
    last_event_ms: f64,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one click. Returns `false` (and drops the click) once the
    /// step cap is reached, so a runaway recording cannot grow unbounded.
    pub fn record_click(&mut self, elapsed_ms: f64, x: i64, y: i64, button: MouseButton) -> bool {
        if self.steps.len() >= MAX_STEPS {
            return false;
        }
        let raw_gap = elapsed_ms - self.last_event_ms;
        self.steps.push(MacroStep {
            delay_ms: clamp_delay(raw_gap),
            x,
            y,
            button,
        });
        self.last_event_ms = self.last_event_ms.max(elapsed_ms);
        true
    }

    pub fn steps(&self) -> &[MacroStep] {
        &self.steps
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.steps.len() >= MAX_STEPS
    }

    pub fn clear(&mut self) {
        self.steps.clear();
        self.last_event_ms = 0.0;
    }

    /// Sum of every replayed gap — the duration of one playback loop.
    pub fn total_duration_ms(&self) -> f64 {
        self.steps.iter().map(|s| s.delay_ms).sum()
    }
}

/// Clamps a recorded or hand-edited gap into the replayable range.
pub fn clamp_delay(delay_ms: f64) -> f64 {
    if !delay_ms.is_finite() {
        return MIN_STEP_DELAY_MS;
    }
    delay_ms.clamp(MIN_STEP_DELAY_MS, MAX_STEP_DELAY_MS)
}

/// Which step the `sequence`-th click of a looping playback executes.
pub fn step_index(sequence: u64, len: usize) -> usize {
    debug_assert!(len > 0, "step_index requires a non-empty macro");
    (sequence % len.max(1) as u64) as usize
}

/// Total click budget for `loops` full passes over `steps` steps.
/// Zero loops means "play until stopped" and yields an unlimited budget.
pub fn clicks_for_loops(steps: usize, loops: u64) -> u64 {
    if loops == 0 {
        return 0;
    }
    (steps as u64).saturating_mul(loops)
}
