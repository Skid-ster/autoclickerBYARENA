#![allow(dead_code)]

#[path = "../src/hotkey.rs"]
mod hotkey;
#[path = "../src/stability.rs"]
mod stability;

use stability::{assess, format_late_percent, format_report, EndReason, Measurements, Verdict};

fn sample() -> Measurements {
    Measurements {
        requested_seconds: 10,
        elapsed_ms: 10_000.0,
        completed_cycles: 100,
        late_cycles: 5,
        max_deadline_lateness_ms: Some(3.5),
        warning: false,
    }
}

#[test]
fn timing_threshold_is_explicit_and_inclusive() {
    assert_eq!(assess(EndReason::Finished, sample()), Verdict::Good);
    assert_eq!(
        assess(
            EndReason::Finished,
            Measurements {
                late_cycles: 6,
                ..sample()
            }
        ),
        Verdict::TimingIssues
    );
    // Compare without overflowing at the largest representable counters.
    assert_eq!(
        assess(
            EndReason::Finished,
            Measurements {
                completed_cycles: u64::MAX,
                late_cycles: u64::MAX,
                ..sample()
            }
        ),
        Verdict::TimingIssues
    );
}

#[test]
fn deadline_overrun_cannot_be_misreported_as_good() {
    let on_limit = Measurements {
        elapsed_ms: 10_000.0 + stability::MAX_END_OVERRUN_MS,
        ..sample()
    };
    assert_eq!(assess(EndReason::Finished, on_limit), Verdict::Good);
    let late_end = Measurements {
        elapsed_ms: on_limit.elapsed_ms + 0.001,
        ..on_limit
    };
    assert_eq!(assess(EndReason::Finished, late_end), Verdict::TimingIssues);
    let resumed_after_sleep = Measurements {
        elapsed_ms: 3_600_000.0,
        ..sample()
    };
    assert_eq!(
        assess(EndReason::Finished, resumed_after_sleep),
        Verdict::TimingIssues
    );
    assert_eq!(
        assess(
            EndReason::Finished,
            Measurements {
                warning: true,
                ..late_end
            }
        ),
        Verdict::Warning,
        "a warning must not be hidden by a late deadline"
    );
    assert_eq!(assess(EndReason::Stopped, late_end), Verdict::Interrupted);
}

#[test]
fn a_single_major_dispatch_stall_cannot_hide_in_a_low_late_percentage() {
    let on_limit = Measurements {
        completed_cycles: 10_000,
        late_cycles: 1,
        max_deadline_lateness_ms: Some(stability::MAX_DEADLINE_LATENESS_MS),
        ..sample()
    };
    assert_eq!(assess(EndReason::Finished, on_limit), Verdict::Good);
    assert_eq!(
        assess(
            EndReason::Finished,
            Measurements {
                max_deadline_lateness_ms: Some(stability::MAX_DEADLINE_LATENESS_MS + 0.001),
                ..on_limit
            }
        ),
        Verdict::TimingIssues
    );
    for invalid in [None, Some(-1.0), Some(f64::NAN), Some(f64::INFINITY)] {
        assert_eq!(
            assess(
                EndReason::Finished,
                Measurements {
                    max_deadline_lateness_ms: invalid,
                    ..on_limit
                }
            ),
            Verdict::InsufficientData
        );
    }
}

#[test]
fn display_metrics_handle_zero_invalid_and_extreme_measurements() {
    assert_eq!(sample().late_percent(), Some(5.0));
    assert_eq!(sample().cycles_per_second(), Some(10.0));
    assert_eq!(sample().end_overrun_ms(), Some(0.0));
    assert_eq!(format_late_percent(sample()), "5.00%");
    assert_eq!(
        format_late_percent(Measurements {
            completed_cycles: 1_000_000,
            late_cycles: 1,
            ..sample()
        }),
        "<0.01%"
    );
    let empty = Measurements {
        completed_cycles: 0,
        late_cycles: 0,
        elapsed_ms: 0.0,
        ..sample()
    };
    assert_eq!(empty.late_percent(), None);
    assert_eq!(empty.cycles_per_second(), None);
    assert_eq!(format_late_percent(empty), "n/a");
    let invalid = Measurements {
        completed_cycles: 1,
        late_cycles: 2,
        elapsed_ms: f64::NAN,
        ..sample()
    };
    assert_eq!(invalid.late_percent(), None);
    assert_eq!(invalid.cycles_per_second(), None);
    assert_eq!(invalid.end_overrun_ms(), None);
    assert_eq!(
        Measurements {
            elapsed_ms: f64::MIN_POSITIVE,
            completed_cycles: u64::MAX,
            ..sample()
        }
        .cycles_per_second(),
        None
    );
    assert_eq!(
        Measurements {
            requested_seconds: 9,
            ..sample()
        }
        .end_overrun_ms(),
        None
    );
}

#[test]
fn copied_report_is_clear_and_excludes_unrequested_private_details() {
    let report = format_report(EndReason::Finished, sample(), 31);
    for text in [
        "Good timing in this run",
        "Elapsed: 10.000 s / 10 s",
        "Completed cycles: 100",
        "Late cycles: 5 (5.00%; rounded)",
        "Average cadence: 10.00 cycles/s",
        "Worst deadline lateness: 3.5 ms",
        "Deadline overrun: 0.0 ms",
        "Engine warning: no",
        "Dropped detail records: 31",
        "not target receipt or future stability",
    ] {
        assert!(report.contains(text), "missing {text:?}: {report}");
    }
    assert!(!report.contains("coordinate"));
    assert!(!report.contains("hardware"));
    let interrupted = format_report(EndReason::Stopped, sample(), 0);
    assert!(interrupted.contains("Test interrupted — no pass issued"));
    assert!(interrupted.contains("Deadline overrun: n/a"));
    let invalid = format_report(
        EndReason::Finished,
        Measurements {
            elapsed_ms: f64::NAN,
            completed_cycles: 0,
            late_cycles: 0,
            max_deadline_lateness_ms: None,
            ..sample()
        },
        0,
    );
    assert!(invalid.contains("Worst deadline lateness: n/a"));
    assert!(invalid.contains("Elapsed: n/a / 10 s"));
    assert!(invalid.contains("Average cadence: n/a"));
    assert!(!invalid.contains("NaN"));
}

#[test]
fn short_invalid_and_tiny_runs_are_never_reported_as_good() {
    for measurement in [
        Measurements {
            elapsed_ms: 1_000.0,
            ..sample()
        },
        Measurements {
            elapsed_ms: 9_999.999,
            ..sample()
        },
        Measurements {
            elapsed_ms: f64::NAN,
            ..sample()
        },
        Measurements {
            elapsed_ms: f64::INFINITY,
            ..sample()
        },
        Measurements {
            requested_seconds: 9,
            ..sample()
        },
        Measurements {
            requested_seconds: 61,
            ..sample()
        },
        Measurements {
            completed_cycles: 19,
            late_cycles: 0,
            ..sample()
        },
        Measurements {
            completed_cycles: 1,
            late_cycles: 2,
            ..sample()
        },
    ] {
        assert_eq!(
            assess(EndReason::Finished, measurement),
            Verdict::InsufficientData
        );
    }
}

#[test]
fn warnings_failures_and_interruptions_cannot_become_a_pass() {
    assert_eq!(
        assess(
            EndReason::Finished,
            Measurements {
                warning: true,
                ..sample()
            }
        ),
        Verdict::Warning
    );
    assert_eq!(assess(EndReason::Stopped, sample()), Verdict::Interrupted);
    assert_eq!(assess(EndReason::Panicked, sample()), Verdict::Interrupted);
    assert_eq!(assess(EndReason::DispatchFailed, sample()), Verdict::Failed);
    assert_eq!(assess(EndReason::WorkerLost, sample()), Verdict::Failed);
    assert_eq!(assess(EndReason::StartRejected, sample()), Verdict::Failed);
}
