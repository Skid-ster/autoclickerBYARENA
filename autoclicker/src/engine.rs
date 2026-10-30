//! Dedicated click worker thread.
//!
//! The egui render thread never executes timing code. It swaps the atomic
//! configuration snapshot and reads the lock-free telemetry store; the worker
//! parks on a manual-reset event while idle and runs a QPC schedule while
//! active. No sleep call exists anywhere in the active click path.
//!
//! Two events drive the worker. `wake_event` starts a session; `abort_event`
//! ends one. Waits inside a session are QPC-spun for the last
//! [`SPIN_TAIL_MS`], and — only when the wait is long enough that parking
//! cannot move the dispatch instant — the portion before that tail is parked
//! on `abort_event` in slices of at most [`MAX_PARK_MS`]. Fast clicking keeps
//! the pure spin it has always had; long intervals no longer hold a whole
//! logical CPU at 100 %, which is itself a timing risk (heat, throttling and
//! battery). A stop, a panic or a session deadline always lands inside one
//! slice, so responsiveness is unchanged.
//!
//! Failure handling is deliberate: a panic inside the click loop is caught and
//! reported instead of taking the worker thread with it, and a worker that is
//! lost for any reason is replaced by [`ClickEngine::ensure_worker`] (bounded
//! by [`MAX_WORKER_RESTARTS`]). A worker that is alive but blocked inside an
//! input call cannot be interrupted, so it is *observed* instead: the worker
//! publishes the dispatch it owes ([`ClickEngine::dispatch_overdue_ms`]) and
//! the UI reports lateness rather than claiming a healthy session.

use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicI64, AtomicU32, AtomicU64, AtomicU8, Ordering,
};
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

/// Long waits park on a kernel event for the part that cannot affect click
/// precision and spin the tail, so an hour-long interval no longer holds a
/// logical CPU at 100 % (heat and throttling are themselves timing risks).
/// The last [`SPIN_TAIL_MS`] before a deadline is always QPC-spun, and waits
/// shorter than [`MIN_PARK_MS`] below the tail never park at all: fast
/// clicking keeps the pure spin schedule it has always had.
const SPIN_TAIL_MS: f64 = 12.0;
/// Smallest park worth handing to the OS. Below this the kernel wait costs
/// more than it saves and could return late relative to a 1 ms timer.
const MIN_PARK_MS: f64 = 4.0;
/// Long parks are issued in slices so a stop, a panic or a session deadline
/// is always honoured inside one slice, whatever the interval.
const MAX_PARK_MS: u32 = 250;
/// A replacement worker is spawned at most this many times per engine. Past
/// it the engine reports itself permanently unavailable instead of spawning
/// in a loop, which is the honest outcome for a process that keeps losing
/// its worker thread.
pub const MAX_WORKER_RESTARTS: u32 = 8;

struct Shared {
    wake_event: i64,
    /// Signalled by every stop and panic request. Long waits park on it, so
    /// an hour-long interval is still interrupted in well under a slice.
    abort_event: i64,
    terminate: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    panic: Arc<AtomicBool>,
    state: Arc<AtomicI32>,
    outcome: AtomicU8,
    /// QPC tick the worker currently owes a dispatch at, or 0 when it is not
    /// inside a scheduled cycle. Published by the worker, read by the UI's
    /// stall detector; it never changes scheduling, only observation.
    waiting_deadline: AtomicI64,
    /// How many replacement workers have been spawned since construction.
    worker_restarts: AtomicU32,
    /// Serializes starts, stop requests, and the worker's final state
    /// transition. Never locked inside the active click loop.
    session_gate: Mutex<()>,
    config: Arc<AtomicConfigSnapshot>,
    /// The first valid configuration is captured at start, before signalling.
    /// The worker clones it once at session entry, never inside the click loop.
    session_config: Mutex<EngineConfig>,
    /// Zero for ordinary sessions; a timed self-test carries a bounded
    /// duration independently of the UI heartbeat.
    session_duration_ms: AtomicI64,
    /// Maximum timed-test lateness against an original, un-reanchored
    /// deadline. f64 bits are published by the single worker; u64::MAX means
    /// no deadline was observed. Reset at each start.
    test_max_lateness_bits: AtomicU64,
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
    /// Interior mutability so a lost worker can be replaced through `&self`
    /// from the UI's watchdog without making the engine `&mut` everywhere.
    worker: Mutex<Option<JoinHandle<()>>>,
    wake_handle: isize,
    abort_handle: isize,
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
        // A second event is dedicated to cancellation. Sharing the wake
        // event would make a start signal indistinguishable from a stop.
        let (abort_handle, initial_error) = match win32::create_manual_reset_event() {
            Ok(handle) => (handle, initial_error),
            Err(error) => (
                0,
                Some(initial_error.unwrap_or_else(|| {
                    format!("Could not create click-worker stop event: {error}")
                })),
            ),
        };
        let shared = Arc::new(Shared {
            wake_event: wake_handle as i64,
            abort_event: abort_handle as i64,
            terminate: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            panic: Arc::new(AtomicBool::new(false)),
            state: Arc::new(AtomicI32::new(STATE_IDLE)),
            outcome: AtomicU8::new(SessionOutcome::Pending as u8),
            waiting_deadline: AtomicI64::new(0),
            worker_restarts: AtomicU32::new(0),
            session_gate: Mutex::new(()),
            config: Arc::new(AtomicConfigSnapshot::default()),
            session_config: Mutex::new(EngineConfig::default()),
            session_duration_ms: AtomicI64::new(0),
            test_max_lateness_bits: AtomicU64::new(u64::MAX),
            telemetry: Arc::new(TelemetryStore::new()),
            priority_denied: Arc::new(AtomicBool::new(false)),
            start_tick: Arc::new(AtomicI64::new(0)),
            last_elapsed: Arc::new(AtomicI64::new(0)),
            error: Arc::new(Mutex::new(initial_error)),
            rect_cache: Arc::new(RectCache::new()),
            macro_steps: Mutex::new(Arc::from(Vec::new())),
        });
        let worker = if wake_handle == 0 || abort_handle == 0 {
            None
        } else {
            match spawn_worker(&shared) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    publish_error(&shared, error);
                    None
                }
            }
        };
        Self {
            shared,
            worker: Mutex::new(worker),
            wake_handle,
            abort_handle,
        }
    }

    /// Publishes a dispatch deadline the worker is already late for, so the
    /// UI's stall detector can be exercised without a real blocked input
    /// call. Test-only.
    #[cfg(test)]
    pub(crate) fn publish_dispatch_deadline_for_test(&self, tick: i64) {
        self.shared.waiting_deadline.store(tick, Ordering::Release);
    }

    /// Clears the published dispatch deadline. Test-only counterpart of
    /// [`ClickEngine::publish_dispatch_deadline_for_test`].
    #[cfg(test)]
    pub(crate) fn clear_dispatch_deadline_for_test(&self) {
        self.shared.waiting_deadline.store(0, Ordering::Release);
    }

    /// Ends the worker thread the way an unexpected exit would, leaving the
    /// engine holding a finished handle. Test-only: production code never
    /// kills the worker, it replaces it through [`ClickEngine::ensure_worker`].
    #[cfg(test)]
    pub(crate) fn simulate_worker_loss_for_test(&self) {
        self.shared.terminate.store(true, Ordering::Release);
        if self.wake_handle != 0 {
            let _ = win32::set_event(self.wake_handle);
        }
        if self.abort_handle != 0 {
            let _ = win32::set_event(self.abort_handle);
        }
        if let Some(handle) = lock_recovering(&self.worker).take() {
            let _ = handle.join();
        }
        self.shared.terminate.store(false, Ordering::Release);
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
        self.start_session(config, steps, 0)
    }

    /// Run the actual global click path for a bounded duration. The test
    /// takes a snapshot, ignores the ordinary repeat limit, and never sends
    /// background-window input. The caller must warn and obtain confirmation
    /// because this sends real mouse input.
    pub fn start_stability_test(&self, config: &EngineConfig, seconds: u32) -> Result<(), String> {
        if !(crate::hotkey::MIN_STABILITY_SECONDS..=crate::hotkey::MAX_STABILITY_SECONDS)
            .contains(&seconds)
        {
            return Err("Stability tests must run for 10–60 seconds.".to_string());
        }
        if config.input_mode != InputMode::GlobalSendInput {
            return Err("Stability tests require global SendInput delivery.".to_string());
        }
        let mut snapshot = config.clone();
        snapshot.max_clicks = 0;
        self.start_session(&snapshot, Arc::from(Vec::new()), i64::from(seconds) * 1000)
    }

    fn start_session(
        &self,
        config: &EngineConfig,
        steps: Arc<[MacroStep]>,
        duration_ms: i64,
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
        self.shared
            .test_max_lateness_bits
            .store(u64::MAX, Ordering::Relaxed);
        self.shared.priority_denied.store(false, Ordering::Release);
        *lock_recovering(&self.shared.error) = None;
        self.shared.stop.store(false, Ordering::Release);
        self.shared.panic.store(false, Ordering::Release);
        self.shared.waiting_deadline.store(0, Ordering::Release);
        // Clear any stop signal left by the previous session before the new
        // one parks; otherwise its first long wait would return immediately.
        if self.abort_handle != 0 {
            let _ = win32::reset_event(self.abort_handle);
        }
        *lock_recovering(&self.shared.session_config) = config.clone();
        self.shared
            .session_duration_ms
            .store(duration_ms.max(0), Ordering::Release);
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
            self.signal_abort();
        }
    }

    pub fn panic_stop(&self) {
        let _gate = lock_recovering(&self.shared.session_gate);
        if self.is_running() {
            self.shared.panic.store(true, Ordering::Release);
            self.shared.stop.store(true, Ordering::Release);
            // PANIC is terminal only after the worker has fully drained.
            self.shared.state.store(STATE_STOPPING, Ordering::Release);
            self.signal_abort();
        }
    }

    /// Wakes a worker that is parked for a long interval. The stop flags are
    /// set first, so a park that begins after this call still sees them.
    fn signal_abort(&self) {
        if self.abort_handle != 0 {
            let _ = win32::set_event(self.abort_handle);
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

    #[cfg(test)]
    pub(crate) fn live_config_for_test(&self) -> Option<EngineConfig> {
        self.shared.config.try_load()
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

    /// Maximum observed lateness of a timed-test deadline, including a
    /// deadline missed as the test ended. Independent of the telemetry ring.
    pub fn test_max_lateness_ms(&self) -> Option<f64> {
        let bits = self.shared.test_max_lateness_bits.load(Ordering::Relaxed);
        (bits != u64::MAX).then(|| f64::from_bits(bits))
    }

    pub fn priority_denied(&self) -> bool {
        self.shared.priority_denied.load(Ordering::Acquire)
    }

    /// True while the dedicated click worker thread is alive. The worker
    /// is spawned once and parks between sessions, so `false` means the
    /// thread died (a defensive check — the worker contains no panicking
    /// paths by design) and the engine can no longer deliver clicks.
    pub fn worker_alive(&self) -> bool {
        lock_recovering(&self.worker)
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    /// True when a worker exists at all (alive or not) and a replacement can
    /// legally be spawned. `false` means the engine was never able to start
    /// one, or it has exhausted its restart budget.
    pub fn worker_restartable(&self) -> bool {
        self.wake_handle != 0
            && self.abort_handle != 0
            && self.shared.worker_restarts.load(Ordering::Relaxed) < MAX_WORKER_RESTARTS
    }

    /// How many replacement workers have been spawned since construction.
    pub fn worker_restarts(&self) -> u32 {
        self.shared.worker_restarts.load(Ordering::Relaxed)
    }

    /// Replaces a worker thread that ended, so one lost thread costs a single
    /// aborted session instead of the whole application. Returns `Ok(true)`
    /// when a replacement was actually spawned, `Ok(false)` when the current
    /// worker is already alive, and `Err` with an honest reason when no
    /// replacement is possible (no kernel handles, or the restart budget is
    /// spent, so a process that keeps losing its worker is reported rather
    /// than respawned forever).
    pub fn ensure_worker(&self) -> Result<bool, String> {
        let mut worker = lock_recovering(&self.worker);
        let alive = worker.as_ref().is_some_and(|handle| !handle.is_finished());
        if alive {
            return Ok(false);
        }
        if !self.worker_restartable() {
            return Err(if self.wake_handle == 0 || self.abort_handle == 0 {
                "Click worker is unavailable: its start and stop events could not be created. \
                 Restart MacroClicker."
                    .to_string()
            } else {
                format!(
                    "Click worker stopped {MAX_WORKER_RESTARTS} times; it will not be restarted \
                     again in this session. Restart MacroClicker before clicking again."
                )
            });
        }
        // The old handle is finished by definition, so joining it only
        // reclaims the thread; it can never block on live click code.
        if let Some(handle) = worker.take() {
            let _ = handle.join();
        }
        // Any session the dead worker was driving is over and its terminal
        // transition never ran, so publish the outcome here instead of
        // leaving the UI reporting a running session nobody is driving.
        recover_after_worker_failure(
            &self.shared,
            "Click worker stopped unexpectedly; the session was aborted and the worker was \
             replaced."
                .to_string(),
        );
        self.shared.worker_restarts.fetch_add(1, Ordering::Relaxed);
        let replacement = spawn_worker(&self.shared).map_err(|error| {
            // The budget was spent on this attempt; do not let a transient
            // spawn failure consume one of the remaining restarts.
            self.shared.worker_restarts.fetch_sub(1, Ordering::Relaxed);
            format!("{error} Restart MacroClicker before clicking again.")
        })?;
        *worker = Some(replacement);
        // Clear the stop flags the recovery raised, so the fresh worker can
        // accept the next start instead of draining a session it never ran.
        self.shared.stop.store(false, Ordering::Release);
        self.shared.panic.store(false, Ordering::Release);
        self.shared.state.store(STATE_IDLE, Ordering::Release);
        Ok(true)
    }

    /// How long the worker has been overdue for the dispatch it currently
    /// owes, or `None` when it is idle, between cycles, or on time. The UI
    /// uses this to admit a stalled session instead of reporting a healthy
    /// one: a worker blocked inside an input call is the one failure the
    /// stop flags cannot interrupt.
    pub fn dispatch_overdue_ms(&self) -> Option<f64> {
        if !self.is_running() {
            return None;
        }
        let deadline = self.shared.waiting_deadline.load(Ordering::Acquire);
        if deadline == 0 {
            return None;
        }
        let late = timing::ticks_to_ms(timing::qpc_now().saturating_sub(deadline));
        (late > 0.0).then_some(late)
    }

    /// Forces the engine back to the idle state after the UI detects a
    /// dead worker mid-session, so Start/Stop enablement and the status
    /// strip recover instead of reporting a running session forever.
    pub fn force_idle(&self) {
        let _gate = lock_recovering(&self.shared.session_gate);
        self.shared.stop.store(true, Ordering::Release);
        self.shared.waiting_deadline.store(0, Ordering::Relaxed);
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
        // Wake any parked long-interval wait first: without this a worker
        // waiting out a one-hour interval would only notice termination when
        // it chose to stop parking, which is never how a shutdown must read.
        if self.abort_handle != 0 {
            let _ = win32::set_event(self.abort_handle);
        }
        if let Some(handle) = lock_recovering(&self.worker).take() {
            let _ = handle.join();
        }
        win32::close_handle(self.wake_handle);
        win32::close_handle(self.abort_handle);
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

/// Spawns the dedicated click worker. Called once at construction and again
/// by [`ClickEngine::ensure_worker`] if the thread is ever lost, so a single
/// worker failure degrades to one aborted session instead of an app restart.
fn spawn_worker(shared: &Arc<Shared>) -> Result<JoinHandle<()>, String> {
    let worker_shared = Arc::clone(shared);
    let mut builder = thread::Builder::new();
    let generation = shared.worker_restarts.load(Ordering::Relaxed);
    if generation > 0 {
        // The name carries the generation so a debugger or a dump can tell
        // the original worker from a replacement.
        builder = builder.name(format!("macroclicker-click-worker-{generation}"));
    } else {
        builder = builder.name("macroclicker-click-worker".to_string());
    }
    builder
        .spawn(move || worker_loop(worker_shared))
        .map_err(|error| format!("Could not start click worker: {error}"))
}

/// Pulls the payload of a caught panic into a readable, bounded string. A
/// panic message is untrusted input as far as the status strip is concerned:
/// it is truncated so one pathological message cannot bloat the UI state.
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    const MAX_PANIC_CHARS: usize = 200;
    let text = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "an unexpected worker error".to_string()
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "an unexpected worker error".to_string();
    }
    let mut bounded = String::with_capacity(MAX_PANIC_CHARS.min(trimmed.len()));
    for (index, character) in trimmed.chars().enumerate() {
        if index >= MAX_PANIC_CHARS {
            bounded.push('…');
            break;
        }
        bounded.push(character);
    }
    bounded
}

/// Ends the session after the worker thread failed in a way that left the
/// shared state mid-session: an unwound panic or a thread that ended without
/// its own terminal transition. The next start is always possible, and the
/// outcome is reported as [`SessionOutcome::WorkerLost`] so the UI never
/// claims a clean finish.
fn recover_after_worker_failure(shared: &Shared, error: String) {
    let _gate = lock_recovering(&shared.session_gate);
    shared.stop.store(true, Ordering::Release);
    shared.waiting_deadline.store(0, Ordering::Relaxed);
    shared
        .outcome
        .store(SessionOutcome::WorkerLost as u8, Ordering::Release);
    shared.state.store(STATE_IDLE, Ordering::Release);
    publish_error(shared, error);
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
        // A panic inside the click loop must never take the worker thread
        // with it: an unwound session is contained here, reported as a lost
        // worker, and the thread goes back to parking so the next Start
        // still works (see `ClickEngine::ensure_worker`). This is why the
        // release profile unwinds instead of aborting.
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_click_session(&shared)));
        if let Err(payload) = outcome {
            recover_after_worker_failure(
                &shared,
                format!(
                    "Click worker stopped after an unexpected error ({}) — the session was \
                     aborted.",
                    panic_message(payload)
                ),
            );
        }
    }
}

/// Waits for `deadline_tick` without holding a CPU for the whole interval.
///
/// The portion of the wait that cannot affect click precision is parked on
/// the abort event — a kernel wait, never a sleep, and never a busy loop —
/// issued in slices of at most [`MAX_PARK_MS`] so a stop, a panic or a
/// session deadline always lands inside one slice. The final
/// [`SPIN_TAIL_MS`] is spun on QPC exactly as before, so the dispatch
/// instant is unchanged and intervals below the park threshold keep the
/// pure spin schedule they have always had.
fn wait_for_deadline(shared: &Shared, deadline_tick: i64) -> SpinResult {
    loop {
        if shared.panic.load(Ordering::Acquire) {
            return SpinResult::PanicRequested;
        }
        if shared.stop.load(Ordering::Acquire) {
            return SpinResult::StopRequested;
        }
        let remaining_ms = timing::ticks_to_ms(deadline_tick.saturating_sub(timing::qpc_now()));
        let park_ms = remaining_ms - SPIN_TAIL_MS;
        if park_ms < MIN_PARK_MS {
            break;
        }
        // Reset, then re-check the flags, then wait: a stop that lands
        // between the check and the wait has signalled the event, and one
        // that landed before the reset is caught by the check. Neither can
        // be lost.
        if win32::reset_event(shared.abort_event as isize).is_err() {
            break; // Fall back to spinning rather than failing to wait.
        }
        if shared.panic.load(Ordering::Acquire) {
            return SpinResult::PanicRequested;
        }
        if shared.stop.load(Ordering::Acquire) {
            return SpinResult::StopRequested;
        }
        let slice = park_ms.min(f64::from(MAX_PARK_MS)).max(1.0) as u32;
        let _ = win32::wait_for_event(shared.abort_event as isize, slice);
    }
    timing::spin_until_deadline(deadline_tick, &shared.stop, &shared.panic)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

fn bounded_deadline(deadline: i64, session_end: Option<i64>) -> i64 {
    session_end.map_or(deadline, |end| deadline.min(end))
}

fn timed_session_expired(session_end: Option<i64>) -> bool {
    session_end.is_some_and(|end| timing::qpc_now() >= end)
}

/// Track against the original deadline, even if catch-up later re-anchors
/// or the test ends before a delayed burst can start. Only the worker writes;
/// the UI reads the atomic snapshot, independently of the telemetry ring.
fn observe_test_lateness(shared: &Shared, maximum: &mut Option<f64>, original: i64, observed: i64) {
    let late_ticks = observed.saturating_sub(original).max(0);
    let lateness_ms = timing::ticks_to_ms(late_ticks);
    if maximum.is_none_or(|max| lateness_ms > max) {
        *maximum = Some(lateness_ms);
        shared
            .test_max_lateness_bits
            .store(lateness_ms.to_bits(), Ordering::Relaxed);
    }
}

fn run_click_session(shared: &Shared) {
    run_click_session_with_dispatch(shared, |shared, config, phase| {
        if config.input_mode == InputMode::BackgroundHwnd {
            dispatch_background(shared, config, phase)
        } else {
            dispatch_global(config, phase)
        }
    });
}

/// The production closure above is monomorphized into the worker. Tests can
/// supply a recorder instead of injecting any real desktop input.
fn run_click_session_with_dispatch(
    shared: &Shared,
    mut dispatch: impl FnMut(&Shared, &EngineConfig, Phase) -> Result<(), String>,
) {
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
    let duration_ms = shared.session_duration_ms.load(Ordering::Acquire);
    let session_end = (duration_ms > 0)
        .then(|| session_start.saturating_add(timing::ms_to_ticks(duration_ms as f64)));
    shared.start_tick.store(session_start, Ordering::Release);
    humanize::reseed_thread_humanizer(session_start as u64);
    // One clone of the macro sequence at session entry; the click loop
    // itself never touches the lock.
    let macro_steps = lock_recovering(&shared.macro_steps).clone();
    let mut next_deadline = session_start;
    let mut previous_completion: Option<i64> = None;
    let mut sequence: u64 = 0;
    let mut test_max_lateness_ms: Option<f64> = None;
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
        let original_deadline = next_deadline.saturating_add(interval_ticks.max(0));
        let now_tick = timing::qpc_now();
        if let Some(end) = session_end.filter(|end| now_tick >= *end) {
            if original_deadline < end {
                observe_test_lateness(
                    shared,
                    &mut test_max_lateness_ms,
                    original_deadline,
                    now_tick,
                );
            }
            break;
        }
        let already_late = now_tick > original_deadline;
        let target_tick = schedule::next_target_tick(
            next_deadline,
            interval_ticks,
            now_tick,
            timing::ms_to_ticks(schedule::CATCHUP_SLACK_MS),
        );

        // Publish the tick this cycle owes a dispatch at, so the UI can tell
        // a worker that is merely waiting from one that has stopped making
        // progress (blocked inside an input call). Cleared when the cycle
        // completes, and at every exit below.
        shared
            .waiting_deadline
            .store(target_tick, Ordering::Release);
        let wait = wait_for_deadline(shared, bounded_deadline(target_tick, session_end));
        if wait != SpinResult::DeadlineReached {
            break;
        }
        // Lateness is judged at dispatch time, BEFORE the burst: the burst
        // legitimately contains the full button-hold duration, which must
        // never be counted as scheduler error. A timed test never starts a
        // new burst on or after its deadline, even if the UI is unresponsive.
        let dispatch_tick = timing::qpc_now();
        if let Some(end) = session_end.filter(|end| dispatch_tick >= *end) {
            if original_deadline < end {
                observe_test_lateness(
                    shared,
                    &mut test_max_lateness_ms,
                    original_deadline,
                    dispatch_tick,
                );
            }
            break;
        }
        if session_end.is_some() {
            // `target_tick` may have been re-anchored after a stall. Keep the
            // original deadline for this diagnostic without changing the
            // schedule; ordinary sessions do not collect the measurement.
            observe_test_lateness(
                shared,
                &mut test_max_lateness_ms,
                original_deadline,
                dispatch_tick,
            );
        }

        let background = config.input_mode == InputMode::BackgroundHwnd;
        let mut burst_failed: Option<String> = None;
        let mut interrupted = false;

        // Single, double and triple clicks share one loop: one down/hold/up
        // burst per repetition, separated by a short QPC-spun gap. The button
        // is always released, even when stop or panic fires during the hold.
        for burst_index in 0..config.click_type.burst_count() {
            if timed_session_expired(session_end) {
                interrupted = true;
                break;
            }
            let down_result = if background {
                dispatch(shared, &config, Phase::Move)
                    .and_then(|_| dispatch(shared, &config, Phase::Down))
            } else {
                dispatch(shared, &config, Phase::Down)
            };
            if let Err(error) = down_result {
                burst_failed = Some(error);
                break;
            }

            let hold_deadline = timing::qpc_now().saturating_add(timing::ms_to_ticks(hold_ms));
            let hold_wait = timing::spin_until_deadline(
                bounded_deadline(hold_deadline, session_end),
                &shared.stop,
                &shared.panic,
            );

            // The button must always be released, even on panic/stop.
            let up_result = dispatch(shared, &config, Phase::Up);
            if let Err(error) = up_result {
                burst_failed = Some(error);
                break;
            }
            if hold_wait != SpinResult::DeadlineReached || timed_session_expired(session_end) {
                interrupted = true;
                break;
            }

            if burst_index + 1 < config.click_type.burst_count() {
                let gap_deadline =
                    timing::qpc_now().saturating_add(timing::ms_to_ticks(INTER_CLICK_GAP_MS));
                let gap_wait = timing::spin_until_deadline(
                    bounded_deadline(gap_deadline, session_end),
                    &shared.stop,
                    &shared.panic,
                );
                if gap_wait != SpinResult::DeadlineReached || timed_session_expired(session_end) {
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
        // This cycle is complete: the worker no longer owes a dispatch.
        shared.waiting_deadline.store(0, Ordering::Release);
        shared
            .last_elapsed
            .store(completion - session_start, Ordering::Release);
    }

    if session_end.is_some() {
        shared.last_elapsed.store(
            timing::qpc_now().saturating_sub(session_start),
            Ordering::Release,
        );
    }
    // The session no longer owes a dispatch on any exit path.
    shared.waiting_deadline.store(0, Ordering::Release);
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
    use crate::config::ClickType;
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
    fn timed_test_bounds_and_session_deadline_do_not_require_ui_repaints() {
        let engine = ClickEngine::new();
        let config = safe_config();
        assert!(engine.start_stability_test(&config, 9).is_err());
        assert!(engine.start_stability_test(&config, 61).is_err());
        assert!(!engine.is_running());
        assert_eq!(bounded_deadline(100, None), 100);
        assert_eq!(bounded_deadline(100, Some(50)), 50);
        assert_eq!(bounded_deadline(20, Some(50)), 20);

        // The private helper accepts a short deadline to keep the automated
        // test quick. Its first real click would be ten minutes away, so no
        // input is sent to the runner. No UI code polls or stops this worker.
        assert!(engine
            .start_session(&config, Arc::from(Vec::new()), 50)
            .is_ok());
        wait_until(
            || !engine.is_running(),
            "timed worker did not finish itself",
        );
        assert_eq!(engine.session_outcome(), SessionOutcome::Completed);
        assert_eq!(engine.click_count(), 0);
        assert!(engine.elapsed_ms() >= 45.0);
        assert!(engine.elapsed_ms() < 5_000.0);

        assert!(engine.start_stability_test(&config, 10).is_ok());
        assert_eq!(engine.session_config_for_test().max_clicks, 0);
        engine.request_stop();
        wait_until(
            || !engine.is_running(),
            "timed test did not stop on request",
        );
        assert_eq!(engine.session_outcome(), SessionOutcome::Stopped);
        assert!(engine.start(&config).is_ok());
        assert_eq!(engine.shared.session_duration_ms.load(Ordering::Acquire), 0);
        engine.request_stop();
        wait_until(|| !engine.is_running(), "ordinary session did not stop");
    }

    #[test]
    fn timed_worker_caps_hold_and_burst_gap_and_always_attempts_release() {
        for (hold_ms, duration_ms, fail_release) in [
            (1_000.0, 200, false),
            (0.0, 200, false),
            (1_000.0, 200, true),
        ] {
            let config = EngineConfig {
                base_interval_ms: 1.0,
                sigma_ms: 0.0,
                hold_mean_ms: hold_ms,
                hold_sigma_ms: 0.0,
                hold_min_ms: hold_ms,
                hold_max_ms: hold_ms,
                click_type: ClickType::Triple,
                ..EngineConfig::default()
            };
            assert!(config.validate().is_ok());
            let engine = ClickEngine::new();
            let shared = Arc::clone(&engine.shared);
            *lock_recovering(&shared.session_config) = config.clone();
            shared.config.store(&config);
            shared
                .session_duration_ms
                .store(duration_ms, Ordering::Release);
            shared.state.store(STATE_RUNNING, Ordering::Release);
            let trace = Arc::new(Mutex::new(Vec::new()));
            let worker_shared = Arc::clone(&shared);
            let worker_trace = Arc::clone(&trace);
            let worker = std::thread::spawn(move || {
                run_click_session_with_dispatch(&worker_shared, |shared, _, phase| {
                    lock_recovering(&worker_trace).push(phase);
                    if hold_ms == 0.0 && phase == Phase::Up {
                        // Leave less than one 50 ms burst gap at the deadline,
                        // even on a slow test runner; this is simulated input.
                        let target = shared.start_tick.load(Ordering::Acquire)
                            + timing::ms_to_ticks(duration_ms as f64 - 20.0);
                        while timing::qpc_now() < target {
                            std::thread::yield_now();
                        }
                    }
                    if fail_release && phase == Phase::Up {
                        Err("simulated release failure".to_string())
                    } else {
                        Ok(())
                    }
                });
            });
            assert!(worker.join().is_ok());
            assert_eq!(*lock_recovering(&trace), vec![Phase::Down, Phase::Up]);
            assert_eq!(
                shared.telemetry.total_clicks(),
                0,
                "a partially completed burst is not a complete cycle"
            );
            assert!(
                timing::ticks_to_ms(shared.last_elapsed.load(Ordering::Acquire))
                    >= duration_ms as f64 - 5.0
            );
            assert!(timing::ticks_to_ms(shared.last_elapsed.load(Ordering::Acquire)) < 5_000.0);
            let expected = if fail_release {
                SessionOutcome::DispatchFailed
            } else {
                SessionOutcome::Completed
            };
            assert_eq!(shared.outcome.load(Ordering::Acquire), expected as u8);
        }
    }

    #[test]
    fn observed_test_deadline_maximum_is_monotonic_and_does_not_need_ring_records() {
        let engine = ClickEngine::new();
        let shared = Arc::clone(&engine.shared);
        let mut maximum = None;
        assert_eq!(engine.test_max_lateness_ms(), None);
        let base = timing::ms_to_ticks(1_000.0);
        observe_test_lateness(&shared, &mut maximum, base, base - 1);
        assert_eq!(engine.test_max_lateness_ms(), Some(0.0));
        observe_test_lateness(
            &shared,
            &mut maximum,
            base,
            base + timing::ms_to_ticks(400.0),
        );
        observe_test_lateness(
            &shared,
            &mut maximum,
            base,
            base + timing::ms_to_ticks(30.0),
        );
        let observed = engine.test_max_lateness_ms().expect("observed deadline");
        assert!((observed - 400.0).abs() < 0.01, "{observed}");
        assert_eq!(maximum, Some(observed));
        assert_eq!(shared.telemetry.total_clicks(), 0);
        for seq in 0..5_000 {
            shared.telemetry.record_click(ClickRecord {
                seq,
                ..ClickRecord::default()
            });
        }
        assert!(shared.telemetry.dropped_records() > 0);
        observe_test_lateness(
            &shared,
            &mut maximum,
            base,
            base + timing::ms_to_ticks(600.0),
        );
        assert!(
            (engine
                .test_max_lateness_ms()
                .expect("maximum after overflow")
                - 600.0)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn timed_test_keeps_the_original_deadline_when_a_stall_reanchors() {
        let config = EngineConfig {
            base_interval_ms: 10.0,
            min_interval_ms: 10.0,
            max_interval_ms: 10.0,
            hold_mean_ms: 0.0,
            hold_sigma_ms: 0.0,
            hold_min_ms: 0.0,
            hold_max_ms: 0.0,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_ok());
        let engine = ClickEngine::new();
        let shared = Arc::clone(&engine.shared);
        *lock_recovering(&shared.session_config) = config.clone();
        shared.config.store(&config);
        shared.session_duration_ms.store(2_000, Ordering::Release);
        shared.state.store(STATE_RUNNING, Ordering::Release);
        // Use a separate thread so a successful time-critical priority
        // request cannot leave the test harness thread at elevated priority.
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::spawn(move || {
            let mut downs = 0;
            run_click_session_with_dispatch(&worker_shared, |shared, _, phase| {
                if phase == Phase::Down {
                    downs += 1;
                    if downs == 2 {
                        shared.stop.store(true, Ordering::Release);
                    }
                } else if phase == Phase::Up && downs == 1 {
                    // Simulate a slow OS call without sending input; the next
                    // scheduler target will re-anchor past its old deadline.
                    let until = timing::qpc_now().saturating_add(timing::ms_to_ticks(150.0));
                    while timing::qpc_now() < until {
                        std::thread::yield_now();
                    }
                }
                Ok(())
            });
            downs
        });
        assert_eq!(worker.join().expect("simulated worker"), 2);
        assert_eq!(shared.telemetry.total_clicks(), 1);
        assert!(engine.test_max_lateness_ms().is_some_and(|ms| ms >= 100.0));
        // A subsequent ordinary start clears the test-only maximum and does
        // not collect this measurement in the ordinary dispatch loop.
        assert!(engine.start(&safe_config()).is_ok());
        assert_eq!(engine.test_max_lateness_ms(), None);
        engine.request_stop();
        wait_until(|| !engine.is_running(), "ordinary worker did not stop");
        assert_eq!(engine.test_max_lateness_ms(), None);
    }

    #[test]
    fn panic_during_a_simulated_hold_releases_before_reporting_the_outcome() {
        let config = EngineConfig {
            base_interval_ms: 1.0,
            sigma_ms: 0.0,
            hold_mean_ms: 1_000.0,
            hold_sigma_ms: 0.0,
            hold_min_ms: 1_000.0,
            hold_max_ms: 1_000.0,
            ..EngineConfig::default()
        };
        let engine = ClickEngine::new();
        let shared = Arc::clone(&engine.shared);
        *lock_recovering(&shared.session_config) = config.clone();
        shared.config.store(&config);
        shared.session_duration_ms.store(10_000, Ordering::Release);
        shared.state.store(STATE_RUNNING, Ordering::Release);
        let trace = Arc::new(Mutex::new(Vec::new()));
        let worker_shared = Arc::clone(&shared);
        let worker_trace = Arc::clone(&trace);
        let worker = std::thread::spawn(move || {
            run_click_session_with_dispatch(&worker_shared, |_, _, phase| {
                lock_recovering(&worker_trace).push(phase);
                Ok(())
            });
        });
        wait_until(
            || !lock_recovering(&trace).is_empty(),
            "simulated down did not occur",
        );
        shared.panic.store(true, Ordering::Release);
        assert!(worker.join().is_ok());
        assert_eq!(*lock_recovering(&trace), vec![Phase::Down, Phase::Up]);
        assert_eq!(
            shared.outcome.load(Ordering::Acquire),
            SessionOutcome::Panicked as u8
        );
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

    /// Ends the real worker thread the way an unexpected exit would, leaving
    /// the engine holding a finished handle and no live worker.
    fn kill_worker(engine: &ClickEngine) {
        engine.shared.terminate.store(true, Ordering::Release);
        let _ = win32::set_event(engine.wake_handle);
        let _ = win32::set_event(engine.abort_handle);
        if let Some(handle) = lock_recovering(&engine.worker).take() {
            let _ = handle.join();
        }
        // Clear the flag again: the simulated loss is a thread that ended,
        // not a request for every future worker to exit.
        engine.shared.terminate.store(false, Ordering::Release);
    }

    #[test]
    fn a_lost_worker_is_replaced_and_the_next_session_still_runs() {
        let engine = ClickEngine::new();
        assert!(engine.worker_alive());
        assert!(engine.worker_restartable());
        kill_worker(&engine);
        assert!(
            !engine.worker_alive(),
            "the simulated worker loss must be visible"
        );
        assert!(matches!(engine.ensure_worker(), Ok(true)));
        assert!(engine.worker_alive(), "the replacement must really run");
        assert_eq!(engine.worker_restarts(), 1);
        // The session the dead worker was driving is reported, not hidden.
        assert_eq!(engine.state(), STATE_IDLE);
        assert_eq!(engine.session_outcome(), SessionOutcome::WorkerLost);
        assert!(engine
            .last_error()
            .is_some_and(|error| error.contains("stopped unexpectedly")));
        // And the replacement accepts work instead of draining a stop flag.
        assert!(engine.start(&safe_config()).is_ok());
        wait_until(
            || engine.shared.start_tick.load(Ordering::Acquire) != 0,
            "replacement worker never reached the session",
        );
        engine.request_stop();
        wait_until(|| !engine.is_running(), "replacement worker never stopped");
        assert_eq!(engine.session_outcome(), SessionOutcome::Stopped);
        assert!(matches!(engine.ensure_worker(), Ok(false)));
    }

    #[test]
    fn the_restart_budget_is_bounded_and_reported() {
        let engine = ClickEngine::new();
        kill_worker(&engine);
        engine
            .shared
            .worker_restarts
            .store(MAX_WORKER_RESTARTS, Ordering::Release);
        assert!(!engine.worker_restartable());
        let error = engine
            .ensure_worker()
            .expect_err("a spent budget must be refused");
        assert!(
            error.contains("will not be restarted again"),
            "unexpected message: {error}"
        );
        assert!(!engine.worker_alive());
        let missing_handles = ClickEngine {
            shared: Arc::clone(&engine.shared),
            worker: Mutex::new(None),
            wake_handle: 0,
            abort_handle: 0,
        };
        assert!(!missing_handles.worker_restartable());
        assert!(missing_handles
            .ensure_worker()
            .expect_err("no handles")
            .contains("could not be created"));
    }

    #[test]
    fn a_parked_long_interval_wakes_the_moment_a_stop_is_requested() {
        let engine = ClickEngine::new();
        // Ten minutes: without the abort event this wait would spin until
        // the interval expired, and a stop could not be observed at all.
        assert!(engine.start(&safe_config()).is_ok());
        wait_until(
            || engine.shared.start_tick.load(Ordering::Acquire) != 0,
            "session never reached the worker",
        );
        engine.request_stop();
        wait_until(
            || !engine.is_running(),
            "a parked worker ignored the stop request",
        );
        assert_eq!(engine.session_outcome(), SessionOutcome::Stopped);
        assert_eq!(engine.dispatch_overdue_ms(), None);
    }

    #[test]
    fn overdue_dispatch_is_reported_only_while_a_cycle_is_owed() {
        let engine = ClickEngine::new();
        let shared = Arc::clone(&engine.shared);
        assert_eq!(engine.dispatch_overdue_ms(), None);
        shared.state.store(STATE_RUNNING, Ordering::Release);
        // Not waiting for anything: between cycles is not lateness.
        shared.waiting_deadline.store(0, Ordering::Release);
        assert_eq!(engine.dispatch_overdue_ms(), None);
        // A deadline in the future is simply not overdue yet.
        let future = timing::qpc_now().saturating_add(timing::ms_to_ticks(5_000.0));
        shared.waiting_deadline.store(future, Ordering::Release);
        assert_eq!(engine.dispatch_overdue_ms(), None);
        // A deadline already passed is reported, in milliseconds.
        let past = timing::qpc_now().saturating_sub(timing::ms_to_ticks(250.0));
        shared.waiting_deadline.store(past, Ordering::Release);
        let overdue = engine
            .dispatch_overdue_ms()
            .expect("a missed deadline must be reported");
        assert!((250.0..5_000.0).contains(&overdue), "{overdue}");
        // An idle engine never reports lateness.
        shared.state.store(STATE_IDLE, Ordering::Release);
        assert_eq!(engine.dispatch_overdue_ms(), None);
    }

    #[test]
    fn worker_failure_recovery_ends_the_session_as_lost_and_reports_it() {
        let engine = ClickEngine::new();
        let shared = Arc::clone(&engine.shared);
        shared.state.store(STATE_RUNNING, Ordering::Release);
        shared
            .waiting_deadline
            .store(timing::qpc_now(), Ordering::Release);
        recover_after_worker_failure(&shared, "simulated worker failure".to_string());
        assert_eq!(shared.state.load(Ordering::Acquire), STATE_IDLE);
        assert_eq!(
            shared.outcome.load(Ordering::Acquire),
            SessionOutcome::WorkerLost as u8
        );
        assert!(shared.stop.load(Ordering::Acquire));
        assert_eq!(shared.waiting_deadline.load(Ordering::Acquire), 0);
        assert_eq!(
            lock_recovering(&shared.error).clone(),
            Some("simulated worker failure".to_string())
        );
    }

    #[test]
    fn panic_messages_are_readable_and_bounded() {
        assert_eq!(panic_message(Box::new("boom".to_string())), "boom");
        assert_eq!(panic_message(Box::new("static str")), "static str");
        assert_eq!(
            panic_message(Box::new(42_u32)),
            "an unexpected worker error"
        );
        assert_eq!(
            panic_message(Box::new("   ".to_string())),
            "an unexpected worker error"
        );
        let long = "x".repeat(10_000);
        let bounded = panic_message(Box::new(long));
        assert!(
            bounded.chars().count() <= 201,
            "{}",
            bounded.chars().count()
        );
        assert!(bounded.ends_with('…'));
    }

    #[test]
    fn long_waits_park_in_slices_and_always_spin_the_tail() {
        // A park that would overrun the deadline must never be issued: the
        // last SPIN_TAIL_MS is always spun, and no slice exceeds the cap.
        let engine = ClickEngine::new();
        let shared = Arc::clone(&engine.shared);
        // Just past the threshold: one park, then the spun tail.
        let near = timing::qpc_now().saturating_add(timing::ms_to_ticks(40.0));
        assert_eq!(
            wait_for_deadline(&shared, near),
            SpinResult::DeadlineReached
        );
        // Already due: no park at all, the spin returns immediately.
        assert_eq!(
            wait_for_deadline(&shared, timing::qpc_now()),
            SpinResult::DeadlineReached
        );
        // A stop raised before the wait is honoured without waiting.
        shared.stop.store(true, Ordering::Release);
        let far = timing::qpc_now().saturating_add(timing::ms_to_ticks(60_000.0));
        assert_eq!(wait_for_deadline(&shared, far), SpinResult::StopRequested);
        shared.stop.store(false, Ordering::Release);
        shared.panic.store(true, Ordering::Release);
        assert_eq!(wait_for_deadline(&shared, far), SpinResult::PanicRequested);
        shared.panic.store(false, Ordering::Release);
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
