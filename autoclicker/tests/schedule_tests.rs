// The path-included module forms a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/schedule.rs"]
mod schedule;

use schedule::{next_target_tick, CATCHUP_SLACK_MS};

const INTERVAL: i64 = 100_000; // e.g. 100 ms at a 1 MHz tick rate
const SLACK: i64 = 2_000; // 2 ms at the same rate

#[test]
fn on_time_dispatch_keeps_exact_absolute_cadence() {
    let mut anchor: i64 = 1_000_000;
    for step in 1..=50 {
        // Worker wakes exactly on (or slightly before) each deadline.
        let now = anchor + 1;
        let target = next_target_tick(anchor, INTERVAL, now, SLACK);
        assert_eq!(target, 1_000_000 + step * INTERVAL);
        anchor = target;
    }
}

#[test]
fn jitter_within_slack_never_drifts_the_schedule() {
    // Dispatch lateness up to the slack must not re-anchor: the absolute
    // schedule absorbs the jitter so the average rate stays exact.
    let mut anchor: i64 = 500_000;
    let lateness = [0, 700, 1_999, 2_000, 1, 1_500];
    for (step, late) in lateness.iter().enumerate() {
        let target_expected = 500_000 + (step as i64 + 1) * INTERVAL;
        let now = target_expected + late;
        let target = next_target_tick(anchor, INTERVAL, now, SLACK);
        assert_eq!(target, target_expected, "lateness {late} must be absorbed");
        anchor = target;
    }
}

#[test]
fn stall_beyond_slack_reanchors_instead_of_bursting() {
    let anchor: i64 = 2_000_000;
    // The worker was stalled for 3.5 intervals past the deadline.
    let now = anchor + INTERVAL + (INTERVAL * 7 / 2);
    let target = next_target_tick(anchor, INTERVAL, now, SLACK);
    // Naive absolute scheduling would return anchor + INTERVAL (deep in the
    // past) and then fire several instant make-up clicks. Bounded catch-up
    // re-anchors to now, so the stall costs exactly the clicks it covered
    // and nothing fires early afterwards.
    assert_eq!(target, now);
    // The following click resumes normal cadence from the new anchor.
    let following = next_target_tick(target, INTERVAL, target + 10, SLACK);
    assert_eq!(following, target + INTERVAL);
}

#[test]
fn reanchored_target_is_never_in_the_past() {
    for late in [SLACK + 1, INTERVAL, 10 * INTERVAL, i64::MAX / 4] {
        let anchor = 1_000;
        let now = anchor + INTERVAL + late;
        let target = next_target_tick(anchor, INTERVAL, now, SLACK);
        assert!(target >= now - SLACK, "target must stay near or after now");
        assert!(
            target >= anchor + INTERVAL,
            "target must never move backwards"
        );
    }
}

#[test]
fn consecutive_targets_are_strictly_monotonic() {
    let mut anchor: i64 = 0;
    let mut previous = i64::MIN;
    let nows = [50, 99_000, 100_500, 305_000, 410_000, 410_001];
    let mut now_index = 0;
    while now_index < nows.len() {
        let target = next_target_tick(anchor, INTERVAL, nows[now_index], SLACK);
        assert!(target > previous, "targets must strictly increase");
        previous = target;
        anchor = target;
        now_index += 1;
    }
}

#[test]
fn degenerate_inputs_are_clamped_not_undefined() {
    // Negative interval/slack are clamped to zero: the result is still a
    // well-defined deadline at or after the anchor instead of UB or panic.
    assert_eq!(next_target_tick(1_000, -5, 900, -7), 1_000);
    // Saturating arithmetic near the representable extremes must not wrap.
    let near_max = i64::MAX - 10;
    let target = next_target_tick(near_max, INTERVAL, near_max, SLACK);
    assert_eq!(target, i64::MAX);
}

#[test]
fn catchup_slack_constant_behaves_at_its_boundary() {
    // Large enough to absorb real dispatch jitter, small enough that a
    // visible stall always re-anchors rather than bursting. Exercised
    // through the scheduler at a 1 MHz tick rate: 1 tick = 1 microsecond.
    let slack_ticks = (CATCHUP_SLACK_MS * 1_000.0) as i64;
    assert_eq!(slack_ticks, 2_000);
    let anchor = 10_000_000;
    let deadline = anchor + INTERVAL;
    // Exactly at the slack boundary: absorbed, absolute cadence kept.
    let at_boundary = next_target_tick(anchor, INTERVAL, deadline + slack_ticks, slack_ticks);
    assert_eq!(at_boundary, deadline);
    // One tick past the boundary: re-anchored, no catch-up burst.
    let past_boundary = next_target_tick(anchor, INTERVAL, deadline + slack_ticks + 1, slack_ticks);
    assert_eq!(past_boundary, deadline + slack_ticks + 1);
}
