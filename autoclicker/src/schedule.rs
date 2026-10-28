//! Pure click-schedule arithmetic (no OS calls), unit-tested on every
//! platform.
//!
//! The worker runs an absolute schedule: each click's target tick is the
//! previous target plus the intended interval, so dispatch jitter never
//! accumulates into rate drift. The one failure mode of a naive absolute
//! schedule is *catch-up bursts*: if the OS stalls the worker (or a
//! background `SendMessageTimeout` blocks) long enough to slip past a
//! deadline, every missed deadline would fire instantly back-to-back,
//! producing a burst of fast clicks and a visible CPS spike. The helper
//! here bounds that debt: slipping further than the slack re-anchors the
//! schedule to "now", so one late click stays one late click.

/// How far (in milliseconds) the schedule may slip before it re-anchors
/// instead of catching up. Small enough that normal sub-millisecond
/// dispatch jitter never re-anchors (which would let rate drift creep in),
/// large enough that only a real stall triggers it.
pub const CATCHUP_SLACK_MS: f64 = 2.0;

/// Next absolute deadline for a click.
///
/// * On time (or within `slack_ticks` of it): the exact absolute target
///   `anchor_tick + interval_ticks`, preserving drift-free cadence.
/// * Slipped beyond the slack: `now_tick`, dropping the accumulated debt
///   so the worker never fires a burst of instant make-up clicks.
pub fn next_target_tick(
    anchor_tick: i64,
    interval_ticks: i64,
    now_tick: i64,
    slack_ticks: i64,
) -> i64 {
    let target = anchor_tick.saturating_add(interval_ticks.max(0));
    if now_tick > target.saturating_add(slack_ticks.max(0)) {
        now_tick
    } else {
        target
    }
}
