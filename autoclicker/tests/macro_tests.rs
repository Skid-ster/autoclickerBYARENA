// The path-included modules form a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/config.rs"]
mod config;
#[path = "../src/macro_rec.rs"]
mod macro_rec;

use config::MouseButton;
use macro_rec::{
    clamp_delay, clicks_for_loops, step_index, MacroRecorder, MAX_STEPS, MAX_STEP_DELAY_MS,
    MIN_STEP_DELAY_MS,
};

#[test]
fn first_step_keeps_the_lead_in_delay_from_record_start() {
    let mut recorder = MacroRecorder::new();
    assert!(recorder.record_click(850.0, 10, 20, MouseButton::Left));
    assert_eq!(recorder.len(), 1);
    let step = recorder.steps()[0];
    assert!((step.delay_ms - 850.0).abs() < 1e-9);
    assert_eq!((step.x, step.y), (10, 20));
    assert_eq!(step.button, MouseButton::Left);
}

#[test]
fn subsequent_steps_store_gaps_not_absolute_times() {
    let mut recorder = MacroRecorder::new();
    recorder.record_click(100.0, 0, 0, MouseButton::Left);
    recorder.record_click(350.0, 5, 6, MouseButton::Right);
    recorder.record_click(351.5, 7, 8, MouseButton::Middle);
    let steps = recorder.steps();
    assert!((steps[1].delay_ms - 250.0).abs() < 1e-9);
    assert_eq!(steps[1].button, MouseButton::Right);
    assert!((steps[2].delay_ms - 1.5).abs() < 1e-9);
    assert_eq!(steps[2].button, MouseButton::Middle);
}

#[test]
fn zero_and_negative_gaps_clamp_to_the_minimum_delay() {
    let mut recorder = MacroRecorder::new();
    recorder.record_click(500.0, 0, 0, MouseButton::Left);
    // Same poll tick: zero gap must clamp, never schedule a 0 ms click.
    recorder.record_click(500.0, 1, 1, MouseButton::Left);
    // A non-monotonic timestamp must not produce a negative gap either.
    recorder.record_click(400.0, 2, 2, MouseButton::Left);
    assert!((recorder.steps()[1].delay_ms - MIN_STEP_DELAY_MS).abs() < 1e-9);
    assert!((recorder.steps()[2].delay_ms - MIN_STEP_DELAY_MS).abs() < 1e-9);
}

#[test]
fn recorder_refuses_clicks_beyond_the_step_cap() {
    let mut recorder = MacroRecorder::new();
    for i in 0..MAX_STEPS {
        assert!(recorder.record_click(i as f64 * 10.0, 0, 0, MouseButton::Left));
    }
    assert!(recorder.is_full());
    assert!(!recorder.record_click(1e9, 0, 0, MouseButton::Left));
    assert_eq!(recorder.len(), MAX_STEPS);
}

#[test]
fn clear_resets_steps_and_the_gap_baseline() {
    let mut recorder = MacroRecorder::new();
    recorder.record_click(5_000.0, 0, 0, MouseButton::Left);
    recorder.clear();
    assert!(recorder.is_empty());
    assert!((recorder.total_duration_ms() - 0.0).abs() < 1e-9);
    // After a clear the next recording starts a fresh timeline.
    recorder.record_click(120.0, 0, 0, MouseButton::Left);
    assert!((recorder.steps()[0].delay_ms - 120.0).abs() < 1e-9);
}

#[test]
fn total_duration_is_the_sum_of_replayed_gaps() {
    let mut recorder = MacroRecorder::new();
    recorder.record_click(100.0, 0, 0, MouseButton::Left);
    recorder.record_click(300.0, 0, 0, MouseButton::Left);
    recorder.record_click(600.0, 0, 0, MouseButton::Left);
    assert!((recorder.total_duration_ms() - 600.0).abs() < 1e-9);
}

#[test]
fn clamp_delay_bounds_and_rejects_non_finite_values() {
    assert!((clamp_delay(0.0) - MIN_STEP_DELAY_MS).abs() < 1e-9);
    assert!((clamp_delay(-50.0) - MIN_STEP_DELAY_MS).abs() < 1e-9);
    assert!((clamp_delay(250.0) - 250.0).abs() < 1e-9);
    assert!((clamp_delay(1e12) - MAX_STEP_DELAY_MS).abs() < 1e-9);
    // Non-finite inputs (NaN, infinities) all fall back to the safe
    // minimum rather than scheduling an hour-long wait.
    assert!((clamp_delay(f64::NAN) - MIN_STEP_DELAY_MS).abs() < 1e-9);
    assert!((clamp_delay(f64::INFINITY) - MIN_STEP_DELAY_MS).abs() < 1e-9);
}

#[test]
fn step_index_wraps_through_the_loop() {
    assert_eq!(step_index(0, 3), 0);
    assert_eq!(step_index(2, 3), 2);
    assert_eq!(step_index(3, 3), 0);
    assert_eq!(step_index(7, 3), 1);
    assert_eq!(step_index(u64::MAX, 1), 0);
}

#[test]
fn clicks_for_loops_budget_is_exact_and_saturating() {
    // Zero loops = play until stopped = unlimited budget.
    assert_eq!(clicks_for_loops(5, 0), 0);
    assert_eq!(clicks_for_loops(3, 2), 6);
    assert_eq!(clicks_for_loops(MAX_STEPS, 1), MAX_STEPS as u64);
    assert_eq!(clicks_for_loops(2, u64::MAX), u64::MAX);
}
