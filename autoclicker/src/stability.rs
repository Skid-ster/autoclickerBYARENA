//! Honest, platform-independent interpretation of a real-click timing test.
//! The worker and SendInput report what they observed; this does not prove
//! that the target application accepted the clicks or predict future uptime.

use crate::hotkey::{MAX_STABILITY_SECONDS, MIN_STABILITY_SECONDS};

/// A short countdown gives the user time to move the cursor to a safe target.
pub const COUNTDOWN_SECONDS: u64 = 3;
/// Fewer completed cycles cannot support a useful timing verdict.
pub const MIN_COMPLETED_CYCLES: u64 = 20;
/// The test calls timing "good" only when at most 5% of completed cycles
/// were marked late by the existing worker telemetry (already overdue
/// before scheduling, or more than 1 ms late at dispatch).
pub const MAX_LATE_PERCENT: u64 = 5;
/// A test that substantially overruns its deadline may have been suspended
/// or delayed by the OS. Do not describe that run as good, even if no late
/// click was recorded while the worker was unable to run.
pub const MAX_END_OVERRUN_MS: f64 = 250.0;
/// Even one very large scheduling slip is worth reporting, rather than
/// letting it disappear inside an otherwise low percentage of late cycles.
pub const MAX_DEADLINE_LATENESS_MS: f64 = 250.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Finished,
    Stopped,
    Panicked,
    DispatchFailed,
    WorkerLost,
    StartRejected,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurements {
    pub requested_seconds: u32,
    pub elapsed_ms: f64,
    pub completed_cycles: u64,
    pub late_cycles: u64,
    /// Largest observed delay against an original pre-reanchor deadline,
    /// including an overdue deadline when the test ended before dispatch.
    pub max_deadline_lateness_ms: Option<f64>,
    pub warning: bool,
}

impl Measurements {
    /// Rounded presentation is separate from the exact integer verdict.
    pub fn late_percent(self) -> Option<f64> {
        if self.completed_cycles == 0 || self.late_cycles > self.completed_cycles {
            return None;
        }
        Some(100.0 * self.late_cycles as f64 / self.completed_cycles as f64)
    }

    /// Completed logical cycles per elapsed worker second, not physical
    /// down/up events or clicks acknowledged by the target application.
    pub fn cycles_per_second(self) -> Option<f64> {
        if !self.elapsed_ms.is_finite() || self.elapsed_ms <= 0.0 {
            return None;
        }
        let rate = self.completed_cycles as f64 / (self.elapsed_ms / 1000.0);
        rate.is_finite().then_some(rate)
    }

    /// Delay beyond the requested deadline, including any OS suspension or
    /// time spent inside an input call. Only meaningful after completion.
    pub fn end_overrun_ms(self) -> Option<f64> {
        if !(MIN_STABILITY_SECONDS..=MAX_STABILITY_SECONDS).contains(&self.requested_seconds)
            || !self.elapsed_ms.is_finite()
            || self.elapsed_ms < 0.0
        {
            return None;
        }
        Some((self.elapsed_ms - f64::from(self.requested_seconds) * 1000.0).max(0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Good,
    TimingIssues,
    Warning,
    InsufficientData,
    Interrupted,
    Failed,
}

impl Verdict {
    pub fn title(self) -> &'static str {
        match self {
            Self::Good => "Good timing in this run",
            Self::TimingIssues => "Timing issues observed",
            Self::Warning => "Engine warning — no pass issued",
            Self::InsufficientData => "Not enough data for a verdict",
            Self::Interrupted => "Test interrupted — no pass issued",
            Self::Failed => "Test failed — no pass issued",
        }
    }

    pub fn guidance(self) -> &'static str {
        match self {
            Self::Good => "This run met the timing thresholds; repeat under your usual workload.",
            Self::TimingIssues => {
                "Try a longer interval or a lighter workload, then retest safely."
            }
            Self::Warning => "Read the engine warning before interpreting the timing result.",
            Self::InsufficientData => {
                "Run for the full duration with an interval short enough to collect 20 cycles."
            }
            Self::Interrupted => "Check the target is safe before starting a new test.",
            Self::Failed => "Check the error details and correct the problem before retrying.",
        }
    }
}

/// A readable rounded percentage; the verdict always uses exact counters.
pub fn format_late_percent(sample: Measurements) -> String {
    match sample.late_percent() {
        Some(percent) if percent > 0.0 && percent < 0.01 => "<0.01%".to_string(),
        Some(percent) => format!("{percent:.2}%"),
        None => "n/a".to_string(),
    }
}

/// Build an explicitly user-copied, bounded summary. It intentionally omits
/// the click position, raw error details, machine identifiers and click
/// settings other than the requested duration.
pub fn format_report(reason: EndReason, sample: Measurements, dropped_records: u64) -> String {
    let outcome = match reason {
        EndReason::Finished => "Finished",
        EndReason::Stopped => "Stopped",
        EndReason::Panicked => "Panic stopped",
        EndReason::DispatchFailed => "Input dispatch failed",
        EndReason::WorkerLost => "Worker unavailable",
        EndReason::StartRejected => "Start rejected",
    };
    let elapsed = if sample.elapsed_ms.is_finite() && sample.elapsed_ms >= 0.0 {
        format!("{:.3} s", sample.elapsed_ms / 1000.0)
    } else {
        "n/a".to_string()
    };
    let cadence = sample
        .cycles_per_second()
        .map_or_else(|| "n/a".to_string(), |rate| format!("{rate:.2} cycles/s"));
    let worst_lateness = sample
        .max_deadline_lateness_ms
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(|| "n/a".to_string(), |value| format!("{value:.1} ms"));
    let overrun = if reason == EndReason::Finished {
        sample
            .end_overrun_ms()
            .map_or_else(|| "n/a".to_string(), |ms| format!("{ms:.1} ms"))
    } else {
        "n/a".to_string()
    };
    format!(
        "MacroClicker real-click timing report\nVerdict: {}\nOutcome: {}\nElapsed: {} / {} s\nCompleted cycles: {}\nLate cycles: {} ({}; rounded)\nAverage cadence: {}\nWorst deadline lateness: {}\nDeadline overrun: {}\nEngine warning: {}\nDropped detail records: {}\nThis measures worker timing, not target receipt or future stability.",
        assess(reason, sample).title(), outcome, elapsed, sample.requested_seconds,
        sample.completed_cycles, sample.late_cycles, format_late_percent(sample), cadence,
        worst_lateness, overrun, if sample.warning { "yes" } else { "no" }, dropped_records
    )
}

/// Assess only the measured run. A warning or failure cannot become a pass,
/// nor can a stopped, short, invalid or statistically tiny run.
pub fn assess(reason: EndReason, sample: Measurements) -> Verdict {
    match reason {
        EndReason::DispatchFailed | EndReason::WorkerLost | EndReason::StartRejected => {
            return Verdict::Failed
        }
        EndReason::Stopped | EndReason::Panicked => return Verdict::Interrupted,
        EndReason::Finished => {}
    }
    if !(MIN_STABILITY_SECONDS..=MAX_STABILITY_SECONDS).contains(&sample.requested_seconds)
        || !sample.elapsed_ms.is_finite()
        || sample.elapsed_ms < f64::from(sample.requested_seconds) * 1000.0
        || sample.late_cycles > sample.completed_cycles
        || sample
            .max_deadline_lateness_ms
            .is_none_or(|ms| !ms.is_finite() || ms < 0.0)
        || sample.completed_cycles < MIN_COMPLETED_CYCLES
    {
        return Verdict::InsufficientData;
    }
    if sample.warning {
        return Verdict::Warning;
    }
    if sample.elapsed_ms > f64::from(sample.requested_seconds) * 1000.0 + MAX_END_OVERRUN_MS
        || sample
            .max_deadline_lateness_ms
            .is_some_and(|ms| ms > MAX_DEADLINE_LATENESS_MS)
        || u128::from(sample.late_cycles) * 100
            > u128::from(sample.completed_cycles) * u128::from(MAX_LATE_PERCENT)
    {
        Verdict::TimingIssues
    } else {
        Verdict::Good
    }
}
