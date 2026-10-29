//! Dedicated click worker thread.
//!
//! The egui render thread never executes timing code. It swaps the atomic
//! configuration snapshot and reads the lock-free telemetry store; the worker
//! parks on a manual-reset event while idle and runs a QPC spin schedule while
//! active. No sleep call exists anywhere in the active click path.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT;

use crate::config::{AtomicConfigSnapshot, CoordinateMode, EngineConfig, InputMode, MouseButton};
use crate::humanize;
use crate::macro_rec::{self, MacroStep};
use crate::schedule;
use crate::telemetry::{ClickRecord, TelemetryStore};
use crate::timing::{self, ScopedTimerResolution, SpinResult};
use crate::win32::{
    self, make_lparam, make_mouse_input, MK_LBUTTON, MK_MBUTTON, MK_RBUTTON, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN,
    WM_RBUTTONUP,
};

pub const STATE_IDLE: i32 = 0;
pub const STATE_RUNNING: i32 = 1;
pub const STATE_STOPPING: i32 = 2;
pub const STATE_PANIC: i32 = 3;

/// The terminal result is separate from warnings such as a rejected timer
/// resolution or priority request. It is published before the worker's idle
/// transition so the UI never has to infer success from a status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionOutcome {
    Pending = 0,
    Completed = 1,
    Stopped = 2,
    Panicked = 3,
    DispatchFailed = 4,
    WorkerLost = 5,
}

impl SessionOutcome {
    fn at_session_end(dispatch_failed: bool, panicked: bool, stopped: bool) -> Self {
        if dispatch_failed {
            // A failed release is important even when F8 was pressed too.
            Self::DispatchFailed
        } else if panicked {
            Self::Panicked
        } else if stopped {
            Self::Stopped
        } else {
            Self::Completed
        }
    }

    fn from_atomic(value: u8) -> Self {
        match value {
            1 => Self::Completed,
            2 => Self::Stopped,
            3 => Self::Panicked,
            4 => Self::DispatchFailed,
            5 => Self::WorkerLost,
            _ => Self::Pending,
        }
    }
}

/// Gap between the down/up bursts of a double or triple click. Spun on the
/// QPC deadline (never a sleep) so stop and panic flags stay responsive.
const INTER_CLICK_GAP_MS: f64 = 50.0;
/// Idle waits periodically recheck termination even if signalling fails.
/// This is outside the click path; active timing remains QPC-spun.
const IDLE_WAIT_MS: u32 = 250;

struct RectCache {
    left: AtomicI32,
    top: AtomicI32,
    right: AtomicI32,
    bottom: AtomicI32,
}

impl RectCache {
    fn new() -> Self {
        Self {
            left: AtomicI32::new(0),
            top: AtomicI32::new(0),
            right: AtomicI32::new(0),
            bottom: AtomicI32::new(0),
        }
    }

    fn store(&self, rect: (i32, i32, i32, i32)) {
        self.left.store(rect.0, Ordering::Relaxed);
        self.top.store(rect.1, Ordering::Relaxed);
        self.right.store(rect.2, Ordering::Relaxed);
        self.bottom.store(rect.3, Ordering::Relaxed);
    }

    fn load(&self) -> (i32, i32, i32, i32) {
        (
            self.left.load(Ordering::Relaxed),
            self.top.load(Ordering::Relaxed),
            self.right.load(Ordering::Relaxed),
            self.bottom.load(Ordering::Relaxed),
        )
    }
}

struct Shared {
    wake_event: i64,
    terminate: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    panic: Arc<AtomicBool>,
    state: Arc<AtomicI32>,
    outcome: AtomicU8,
    /// Serializes starts, stop requests, and the worker's final state
    /// transition. Never locked inside the active click loop.
    session_gate: Mutex<()>,
    config: Arc<AtomicConfigSnapshot>,
    /// The first valid configuration is captured at start, before signalling.
    /// The worker clones it once at session entry, never inside the click loop.
    session_config: Mutex<EngineConfig>,
    telemetry: Arc<TelemetryStore>,
    priority_denied: Arc<AtomicBool>,
    start_tick: Arc<AtomicI64>,
    last_elapsed: Arc<AtomicI64>,
    error: Arc<Mutex<Option<String>>>,
    rect_cache: Arc<RectCache>,
    /// Steps replayed by a Record & Playback session; empty for ordinary
    /// sessions. Locked only at session start (store) and session entry
    /// (one clone), never inside the click loop.
    macro_steps: Mutex<Arc<[MacroStep]>>,
}

pub struct ClickEngine {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    wake_handle: isize,
}

impl Default for ClickEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickEngine {
    pub fn new() -> Self {
        let (wake_handle, initial_error) = match win32::create_manual_reset_event() {
            Ok(handle) => (handle, None),
            Err(error) => (
                0,
                Some(format!("Could not create click-worker event: {error}")),
            ),
        };
        let shared = Arc::new(Shared {
            wake_event: wake_handle as i64,
            terminate: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            panic: Arc::new(AtomicBool::new(false)),
            state: Arc::new(AtomicI32::new(STATE_IDLE)),
            outcome: AtomicU8::new(SessionOutcome::Pending as u8),
            session_gate: Mutex::new(()),
            config: Arc::new(AtomicConfigSnapshot::default()),
            session_config: Mutex::new(EngineConfig::default()),
            telemetry: Arc::new(TelemetryStore::new()),
            priority_denied: Arc::new(AtomicBool::new(false)),
            start_tick: Arc::new(AtomicI64::new(0)),
            last_elapsed: Arc::new(AtomicI64::new(0)),
            error: Arc::new(Mutex::new(initial_error)),
            rect_cache: Arc::new(RectCache::new()),
            macro_steps: Mutex::new(Arc::from(Vec::new())),
        });
        let worker = if wake_handle == 0 {
            None
        } else {
            let worker_shared = Arc::clone(&shared);
            match thread::Builder::new()
                .name("macroclicker-click-worker".to_string())
                .spawn(move || worker_loop(worker_shared))
            {
                Ok(handle) => Some(handle),
                Err(error) => {
                    publish_error(&shared, format!("Could not start click worker: {error}"));
                    None
                }
            }
        };
        Self {
            shared,
            worker,
            wake_handle,
        }
    }

    pub fn start(&self, config: &EngineConfig) -> Result<(), String> {
        self.start_with_macro(config, Arc::from(Vec::new()))
    }

    /// Starts a session that replays recorded `steps` in order, looping
    /// until the configured click budget (or a stop/panic) ends it. An
    /// empty slice degenerates to an ordinary single-point session.
    pub fn start_with_macro(
        &self,
        config: &EngineConfig,
        steps: Arc<[MacroStep]>,
    ) -> Result<(), String> {
        if !steps.is_empty() && config.input_mode != InputMode::GlobalSendInput {
            return Err("Macro playback requires global SendInput delivery.".to_string());
        }
        config.validate()?;
        let _gate = lock_recovering(&self.shared.session_gate);
        if self.is_running() {
            return Err("The previous click session is still running or stopping.".to_string());
        }
        if !self.worker_alive() {
            return Err(
                "Click worker is unavailable. Restart the app before clicking.".to_string(),
            );
        }
        let previous_state = self.state();
        let previous_outcome = self.session_outcome();
        *lock_recovering(&self.shared.macro_steps) = steps;
        self.shared.telemetry.reset();
        self.shared.start_tick.store(0, Ordering::Release);
        self.shared.last_elapsed.store(0, Ordering::Release);
        self.shared.priority_denied.store(false, Ordering::Release);
        *lock_recovering(&self.shared.error) = None;
        self.shared.stop.store(false, Ordering::Release);
        self.shared.panic.store(false, Ordering::Release);
        *lock_recovering(&self.shared.session_config) = config.clone();
        self.shared.config.store(config);
        self.shared
            .outcome
            .store(SessionOutcome::Pending as u8, Ordering::Release);
        self.shared.state.store(STATE_RUNNING, Ordering::Release);
        if let Err(error) = win32::set_event(self.wake_handle) {
            self.shared.stop.store(true, Ordering::Release);
            self.shared
                .outcome
                .store(previous_outcome as u8, Ordering::Release);
            self.shared.state.store(previous_state, Ordering::Release);
            return Err(format!("Could not wake click worker: {error}"));
        }
        Ok(())
    }

    /// Atomically replaces the running configuration so humanization and
    /// dispatch changes take effect on the next click iteration without
    /// restarting the worker.
    pub fn update_config(&self, config: &EngineConfig) -> Result<(), String> {
        config.validate()?;
        self.shared.config.store(config);
        Ok(())
    }

    pub fn request_stop(&self) {
        let _gate = lock_recovering(&self.shared.session_gate);
        if self.is_running() {
            self.shared.stop.store(true, Ordering::Release);
            self.shared.state.store(STATE_STOPPING, Ordering::Release);
        }
    }

    pub fn panic_stop(&self) {
        let _gate = lock_recovering(&self.shared.session_gate);
        if self.is_running() {
            self.shared.panic.store(true, Ordering::Release);
            self.shared.stop.store(true, Ordering::Release);
            // PANIC is terminal only after the worker has fully drained.
            self.shared.state.store(STATE_STOPPING, Ordering::Release);
        }
    }

    /// Includes the drain period: another session must not clear the stop
    /// flag, reset telemetry, or begin an update until the worker is idle.
    pub fn is_running(&self) -> bool {
        matches!(self.state(), STATE_RUNNING | STATE_STOPPING)
    }

    pub fn state(&self) -> i32 {
        self.shared.state.load(Ordering::Acquire)
    }

    pub fn session_outcome(&self) -> SessionOutcome {
        SessionOutcome::from_atomic(self.shared.outcome.load(Ordering::Acquire))
    }

    pub fn click_count(&self) -> u64 {
        self.shared.telemetry.total_clicks()
    }

    #[cfg(test)]
    pub(crate) fn session_config_for_test(&self) -> EngineConfig {
        lock_recovering(&self.shared.session_config).clone()
    }

    pub fn missed_count(&self) -> u64 {
        self.shared.telemetry.missed_deadlines()
    }

    pub fn elapsed_ms(&self) -> f64 {
        let start = self.shared.start_tick.load(Ordering::Acquire);
        if self.is_running() && start != 0 {
            timing::elapsed_ms(start, timing::qpc_now())
        } else {
            timing::ticks_to_ms(self.shared.last_elapsed.load(Ordering::Acquire))
        }
    }

    pub fn priority_denied(&self) -> bool {
        self.shared.priority_denied.load(Ordering::Acquire)
    }

    /// True while the dedicated click worker thread is alive. The worker
    /// is spawned once and parks between sessions, so `false` means the
    /// thread died (a defensive check — the worker contains no panicking
    /// paths by design) and the engine can no longer deliver clicks.
    pub fn worker_alive(&self) -> bool {
        self.worker
            .as_ref()
            .map(|handle| !handle.is_finished())
            .unwrap_or(false)
    }

    /// Forces the engine back to the idle state after the UI detects a
    /// dead worker mid-session, so Start/Stop enablement and the status
    /// strip recover instead of reporting a running session forever.
    pub fn force_idle(&self) {
        let _gate = lock_recovering(&self.shared.session_gate);
        self.shared.stop.store(true, Ordering::Release);
        self.shared
            .outcome
            .store(SessionOutcome::WorkerLost as u8, Ordering::Release);
        self.shared.state.store(STATE_IDLE, Ordering::Release);
    }

    pub fn last_error(&self) -> Option<String> {
        lock_recovering(&self.shared.error).clone()
    }

    pub fn telemetry(&self) -> Arc<TelemetryStore> {
        Arc::clone(&self.shared.telemetry)
    }
}

impl Drop for ClickEngine {
    fn drop(&mut self) {
        self.shared.terminate.store(true, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
        self.shared.panic.store(true, Ordering::Release);
        if self.wake_handle != 0 {
            let _ = win32::set_event(self.wake_handle);
        }
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        win32::close_handle(self.wake_handle);
    }
}

/// Locks a mutex and recovers the inner data if the lock is poisoned.
/// A panic on the other side of these locks must never cascade a second
/// panic into this thread: the protected values (macro sequence, error
/// string) are plain data that stays valid even if the writer died
/// mid-update, so recovering is strictly safer than crashing the UI.
fn lock_recovering<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn publish_error(shared: &Shared, message: impl Into<String>) {
    *lock_recovering(&shared.error) = Some(message.into());
}

fn worker_loop(shared: Arc<Shared>) {
    loop {
        if shared.terminate.load(Ordering::Acquire) {
            return;
        }
        let wake = win32::wait_for_event(shared.wake_event as isize, IDLE_WAIT_MS);
        if wake == win32::WAIT_TIMED_OUT {
            continue;
        }
        if wake != win32::WAIT_OBJECT {
            publish_error(&shared, "Idle event wait failed; worker exited");
            return;
        }
        if shared.terminate.load(Ordering::Acquire) {
            return;
        }
        // Reset before the session, not after it. A new start can signal
        // as soon as the old session becomes idle; resetting afterwards
        // could erase that signal and strand the new session forever.
        if let Err(error) = win32::reset_event(shared.wake_event as isize) {
            publish_error(
                &shared,
                format!("Could not reset click-worker event: {error}"),
            );
            return;
        }
        run_click_session(&shared);
    }
}

fn clamp_client(value: i64, low: i32, high: i32) -> i32 {
    value.clamp(low as i64, (high - 1) as i64) as i32
}

fn dispatch_background(shared: &Shared, config: &EngineConfig, phase: Phase) -> Result<(), String> {
    let hwnd = config.target_hwnd;
    if !win32::is_valid_window(hwnd) {
        return Err("Target window was lost before the click burst".to_string());
    }
    let rect = match win32::get_client_rect(hwnd) {
        Some(r) => {
            let tuple = (r.left, r.top, r.right, r.bottom);
            if r.right > r.left && r.bottom > r.top {
                shared.rect_cache.store(tuple);
            }
            tuple
        }
        None => shared.rect_cache.load(),
    };
    let (mut x, mut y) = (config.client_x, config.client_y);
    if rect.2 > rect.0 && rect.3 > rect.1 {
        x = clamp_client(x, rect.0, rect.2) as i64;
        y = clamp_client(y, rect.1, rect.3) as i64;
    }
    let lparam = make_lparam(x as i32, y as i32);
    let (down_message, up_message, button_wparam) = match config.mouse_button {
        MouseButton::Left => (WM_LBUTTONDOWN, WM_LBUTTONUP, MK_LBUTTON),
        MouseButton::Right => (WM_RBUTTONDOWN, WM_RBUTTONUP, MK_RBUTTON),
        MouseButton::Middle => (WM_MBUTTONDOWN, WM_MBUTTONUP, MK_MBUTTON),
    };
    let deliver = |message: u32, wparam: usize| -> Result<(), String> {
        if config.background_synchronous {
            win32::send_message_timeout(hwnd, message, wparam, lparam).map_err(|e| e.to_string())
        } else {
            win32::post_message(hwnd, message, wparam, lparam).map_err(|e| e.to_string())
        }
    };
    match phase {
        Phase::Move => deliver(WM_MOUSEMOVE, 0),
        Phase::Down => deliver(down_message, button_wparam),
        Phase::Up => deliver(up_message, 0),
    }
}

enum Phase {
    Move,
    Down,
    Up,
}

/// Build the whole input burst before sending any part of it. In particular,
/// an invalid fixed point must never send the button-down at the old cursor.
fn global_inputs(
    config: &EngineConfig,
    phase: Phase,
    normalize: impl FnOnce(i64, i64) -> Result<(i32, i32), String>,
) -> Result<Vec<INPUT>, String> {
    let (down_flag, up_flag) = match config.mouse_button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let inputs: Vec<INPUT> = match phase {
        Phase::Move => Vec::new(), // moves are folded into the down burst
        Phase::Down => {
            if config.coordinate_mode == CoordinateMode::FixedScreen {
                let (x, y) = normalize(config.fixed_x, config.fixed_y)?;
                vec![
                    make_mouse_input(
                        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                        x,
                        y,
                    ),
                    make_mouse_input(down_flag, 0, 0),
                ]
            } else {
                vec![make_mouse_input(down_flag, 0, 0)]
            }
        }
        Phase::Up => vec![make_mouse_input(up_flag, 0, 0)],
    };
    Ok(inputs)
}

fn dispatch_global(config: &EngineConfig, phase: Phase) -> Result<(), String> {
    let inputs = global_inputs(config, phase, win32::normalize_virtual_point)?;
    if inputs.is_empty() {
        return Ok(());
    }
    win32::send_inputs(&inputs)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn run_click_session(shared: &Shared) {
    let resolution = ScopedTimerResolution::new(1);
    if !resolution.is_active() {
        publish_error(
            shared,
            "timeBeginPeriod(1) was rejected; timing precision degraded",
        );
    }
    if let Err(error) = win32::set_current_thread_time_critical() {
        shared.priority_denied.store(true, Ordering::Release);
        publish_error(shared, error.to_string());
    }

    let session_start = timing::qpc_now();
    shared.start_tick.store(session_start, Ordering::Release);
    humanize::reseed_thread_humanizer(session_start as u64);
    // One clone of the macro sequence at session entry; the click loop
    // itself never touches the lock.
    let macro_steps = lock_recovering(&shared.macro_steps).clone();
    let mut next_deadline = session_start;
    let mut previous_completion: Option<i64> = None;
    let mut sequence: u64 = 0;
    let mut dispatch_failed = false;
    let mut last_config = lock_recovering(&shared.session_config).clone();

    while !shared.stop.load(Ordering::Acquire) && !shared.panic.load(Ordering::Acquire) {
        if let Some(next) = shared.config.try_load() {
            last_config = next;
        }
        // When a writer is in flight, keep the last complete configuration
        // for this click instead of spinning, blocking, or mixing fields.
        let mut config = last_config.clone();
        if config.max_clicks != 0 && sequence >= config.max_clicks {
            break;
        }

        // Macro playback: the current step dictates position, button and
        // gap, replayed exactly as recorded through the same scheduler.
        let playback_step = if macro_steps.is_empty() {
            None
        } else {
            Some(macro_steps[macro_rec::step_index(sequence, macro_steps.len())])
        };
        if let Some(step) = playback_step {
            config.coordinate_mode = CoordinateMode::FixedScreen;
            config.fixed_x = step.x;
            config.fixed_y = step.y;
            config.mouse_button = step.button;
        }

        // A positive 4.1-style random offset selects uniform jitter around the
        // base interval; otherwise the Gaussian humanizer rules apply.
        let interval_ms = match playback_step {
            Some(step) => macro_rec::clamp_delay(step.delay_ms),
            None => humanize::with_thread_humanizer(|h| {
                if config.random_offset_ms > 0.0 {
                    h.next_offset_interval_ms(&config)
                } else {
                    h.next_interval_ms(&config)
                }
            }),
        };
        let hold_ms = humanize::with_thread_humanizer(|h| h.next_hold_ms(&config));
        // Absolute schedule with bounded catch-up: a stall longer than the
        // slack re-anchors to "now" instead of firing instant make-up
        // clicks, so one late click can never become a fast burst.
        let interval_ticks = timing::ms_to_ticks(interval_ms);
        let now_tick = timing::qpc_now();
        let already_late = now_tick > next_deadline.saturating_add(interval_ticks);
        let target_tick = schedule::next_target_tick(
            next_deadline,
            interval_ticks,
            now_tick,
            timing::ms_to_ticks(schedule::CATCHUP_SLACK_MS),
        );

        let wait = timing::spin_until_deadline(target_tick, &shared.stop, &shared.panic);
        if wait != SpinResult::DeadlineReached {
            break;
        }
        // Lateness is judged at dispatch time, BEFORE the burst: the burst
        // legitimately contains the full button-hold duration, which must
        // never be counted as scheduler error.
        let dispatch_tick = timing::qpc_now();

        let background = config.input_mode == InputMode::BackgroundHwnd;
        let mut burst_failed: Option<String> = None;
        let mut interrupted = false;

        // Single, double and triple clicks share one loop: one down/hold/up
        // burst per repetition, separated by a short QPC-spun gap. The button
        // is always released, even when stop or panic fires during the hold.
        for burst_index in 0..config.click_type.burst_count() {
            let down_result = if background {
                dispatch_background(shared, &config, Phase::Move)
                    .and_then(|_| dispatch_background(shared, &config, Phase::Down))
            } else {
                dispatch_global(&config, Phase::Down)
            };
            if let Err(error) = down_result {
                burst_failed = Some(error);
                break;
            }

            let hold_deadline = timing::qpc_now() + timing::ms_to_ticks(hold_ms);
            let hold_wait = timing::spin_until_deadline(hold_deadline, &shared.stop, &shared.panic);

            // The button must always be released, even on panic/stop.
            let up_result = if background {
                dispatch_background(shared, &config, Phase::Up)
            } else {
                dispatch_global(&config, Phase::Up)
            };
            if let Err(error) = up_result {
                burst_failed = Some(error);
                break;
            }
            if hold_wait != SpinResult::DeadlineReached {
                interrupted = true;
                break;
            }

            if burst_index + 1 < config.click_type.burst_count() {
                let gap_deadline = timing::qpc_now() + timing::ms_to_ticks(INTER_CLICK_GAP_MS);
                let gap_wait =
                    timing::spin_until_deadline(gap_deadline, &shared.stop, &shared.panic);
                if gap_wait != SpinResult::DeadlineReached {
                    interrupted = true;
                    break;
                }
            }
        }

        if let Some(error) = burst_failed {
            publish_error(shared, error);
            dispatch_failed = true;
            break;
        }
        if interrupted {
            break;
        }

        let completion = timing::qpc_now();
        let deadline_error_ms = timing::elapsed_ms(target_tick, dispatch_tick);
        // The real measured gap between consecutive click completions: this is
        // what the Real-time CPS chart plots, so it reflects physical click
        // spacing instead of a UI-frame sampling artifact.
        let actual_gap_ms = match previous_completion {
            Some(previous) => timing::elapsed_ms(previous, completion),
            None => interval_ms,
        };
        previous_completion = Some(completion);
        shared.telemetry.record_click(ClickRecord {
            seq: sequence + 1,
            interval_ms,
            hold_ms,
            deadline_error_ms,
            actual_gap_ms,
            missed_deadline: already_late || deadline_error_ms > 1.0,
        });

        sequence += 1;
        next_deadline = target_tick;
        shared
            .last_elapsed
            .store(completion - session_start, Ordering::Release);
    }

    let _gate = lock_recovering(&shared.session_gate);
    let panicked = shared.panic.load(Ordering::Acquire);
    let stopped = shared.stop.load(Ordering::Acquire);
    let outcome = SessionOutcome::at_session_end(dispatch_failed, panicked, stopped);
    shared.outcome.store(outcome as u8, Ordering::Release);
    shared.state.store(
        if panicked { STATE_PANIC } else { STATE_IDLE },
        Ordering::Release,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // The first deadline is ten minutes away: these tests exercise the
    // worker handoff without sending a single mouse input on the runner.
    fn safe_config() -> EngineConfig {
        EngineConfig {
            base_interval_ms: 600_000.0,
            max_interval_ms: 600_000.0,
            ..EngineConfig::default()
        }
    }

    fn wait_until(mut predicate: impl FnMut() -> bool, message: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(Instant::now() < deadline, "{message}");
            std::thread::yield_now();
        }
    }

    #[test]
    fn terminal_outcome_prioritizes_failed_release_and_panic() {
        assert_eq!(
            SessionOutcome::at_session_end(false, false, false),
            SessionOutcome::Completed
        );
        assert_eq!(
            SessionOutcome::at_session_end(false, false, true),
            SessionOutcome::Stopped
        );
        assert_eq!(
            SessionOutcome::at_session_end(false, true, true),
            SessionOutcome::Panicked
        );
        assert_eq!(
            SessionOutcome::at_session_end(true, false, false),
            SessionOutcome::DispatchFailed
        );
        assert_eq!(
            SessionOutcome::at_session_end(true, true, true),
            SessionOutcome::DispatchFailed
        );
    }

    #[test]
    fn fixed_screen_input_is_batched_for_the_virtual_desktop() {
        for (button, down_flag, up_flag) in [
            (MouseButton::Left, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            (
                MouseButton::Right,
                MOUSEEVENTF_RIGHTDOWN,
                MOUSEEVENTF_RIGHTUP,
            ),
            (
                MouseButton::Middle,
                MOUSEEVENTF_MIDDLEDOWN,
                MOUSEEVENTF_MIDDLEUP,
            ),
        ] {
            let config = EngineConfig {
                coordinate_mode: CoordinateMode::FixedScreen,
                fixed_x: -1_234,
                fixed_y: 567,
                mouse_button: button,
                ..EngineConfig::default()
            };
            let inputs = global_inputs(&config, Phase::Down, |x, y| {
                assert_eq!((x, y), (-1_234, 567));
                Ok((12_345, 23_456))
            })
            .expect("valid point");
            assert_eq!(inputs.len(), 2);
            // SAFETY: make_mouse_input initializes this union with the mi member.
            let movement = unsafe { inputs[0].Anonymous.mi };
            // SAFETY: make_mouse_input initializes this union with the mi member.
            let down = unsafe { inputs[1].Anonymous.mi };
            assert_eq!(
                movement.dwFlags,
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK
            );
            assert_eq!((movement.dx, movement.dy), (12_345, 23_456));
            assert_eq!(down.dwFlags, down_flag);
            let up = global_inputs(&config, Phase::Up, |_, _| panic!("release must not move"))
                .expect("release");
            // SAFETY: make_mouse_input initializes this union with the mi member.
            assert_eq!(unsafe { up[0].Anonymous.mi }.dwFlags, up_flag);
            assert!(
                global_inputs(&config, Phase::Down, |_, _| Err("off-screen".to_string())).is_err()
            );
        }
    }

    #[test]
    fn current_cursor_input_does_not_require_a_desktop_point() {
        let config = EngineConfig::default();
        let down = global_inputs(&config, Phase::Down, |_, _| {
            panic!("cursor mode must not normalize")
        })
        .expect("current-cursor down");
        assert_eq!(down.len(), 1);
        // SAFETY: make_mouse_input initializes this union with the mi member.
        assert_eq!(
            unsafe { down[0].Anonymous.mi }.dwFlags,
            MOUSEEVENTF_LEFTDOWN
        );
        let movement = global_inputs(&config, Phase::Move, |_, _| {
            panic!("move must not normalize")
        })
        .expect("empty move");
        assert!(movement.is_empty());
    }

    #[test]
    fn stopping_is_busy_and_idle_stop_does_not_strand_the_engine() {
        let engine = ClickEngine::new();
        engine.request_stop();
        assert_eq!(engine.state(), STATE_IDLE);
        engine.shared.state.store(STATE_STOPPING, Ordering::Release);
        assert!(engine.is_running());
        assert!(engine.start(&safe_config()).is_err());
        engine.shared.state.store(STATE_IDLE, Ordering::Release);
        assert!(engine.start(&safe_config()).is_ok());
        engine.request_stop();
        wait_until(|| !engine.is_running(), "stopped worker never became idle");
        assert_eq!(engine.session_outcome(), SessionOutcome::Stopped);
    }

    #[test]
    fn failed_signal_rolls_back_running_state() {
        let mut engine = ClickEngine::new();
        assert!(engine.worker_alive());
        let original_handle = engine.wake_handle;
        engine.wake_handle = 0; // invalid for SetEvent; worker still owns the real event.
        let result = engine.start(&safe_config());
        engine.wake_handle = original_handle;
        assert!(result.is_err());
        assert_eq!(engine.state(), STATE_IDLE);
        assert!(!engine.is_running());
        assert!(engine.start(&safe_config()).is_ok());
        engine.request_stop();
        wait_until(
            || !engine.is_running(),
            "worker did not recover from failed wake",
        );
    }

    #[test]
    fn rapid_stop_and_restart_never_loses_a_wake() {
        let engine = ClickEngine::new();
        let config = safe_config();
        for _ in 0..200 {
            assert!(engine.start(&config).is_ok());
            wait_until(
                || engine.shared.start_tick.load(Ordering::Acquire) != 0,
                "new session never reached the worker",
            );
            engine.request_stop();
            wait_until(|| !engine.is_running(), "worker failed to drain after stop");
            assert_eq!(engine.click_count(), 0);
        }
    }

    #[test]
    fn panic_is_terminal_only_after_drain_and_allows_a_later_start() {
        let engine = ClickEngine::new();
        let config = safe_config();
        assert!(engine.start(&config).is_ok());
        wait_until(
            || engine.shared.start_tick.load(Ordering::Acquire) != 0,
            "panic test never reached the worker",
        );
        engine.panic_stop();
        wait_until(|| !engine.is_running(), "panic did not drain");
        assert_eq!(engine.state(), STATE_PANIC);
        assert_eq!(engine.session_outcome(), SessionOutcome::Panicked);
        assert!(engine.start(&config).is_ok());
        assert_eq!(engine.session_outcome(), SessionOutcome::Pending);
        engine.request_stop();
        wait_until(|| !engine.is_running(), "restart after panic did not stop");
        assert_eq!(engine.state(), STATE_IDLE);
        assert_eq!(engine.session_outcome(), SessionOutcome::Stopped);
        assert_eq!(engine.click_count(), 0);
    }
}
