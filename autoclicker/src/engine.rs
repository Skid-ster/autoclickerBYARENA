//! Dedicated click worker thread.
//!
//! The egui render thread never executes timing code. It swaps the atomic
//! configuration snapshot and reads the lock-free telemetry store; the worker
//! parks on a manual-reset event while idle and runs a QPC spin schedule while
//! active. No sleep call exists anywhere in the active click path.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use windows_sys::Win32::System::Threading::INFINITE;
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
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

pub const STATE_IDLE: i32 = 0;
pub const STATE_RUNNING: i32 = 1;
pub const STATE_STOPPING: i32 = 2;
pub const STATE_PANIC: i32 = 3;

/// Gap between the down/up bursts of a double or triple click. Spun on the
/// QPC deadline (never a sleep) so stop and panic flags stay responsive.
const INTER_CLICK_GAP_MS: f64 = 50.0;

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
    config: Arc<AtomicConfigSnapshot>,
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
        let wake_handle = win32::create_manual_reset_event().expect("event creation");
        let shared = Arc::new(Shared {
            wake_event: wake_handle as i64,
            terminate: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            panic: Arc::new(AtomicBool::new(false)),
            state: Arc::new(AtomicI32::new(STATE_IDLE)),
            config: Arc::new(AtomicConfigSnapshot::default()),
            telemetry: Arc::new(TelemetryStore::new()),
            priority_denied: Arc::new(AtomicBool::new(false)),
            start_tick: Arc::new(AtomicI64::new(0)),
            last_elapsed: Arc::new(AtomicI64::new(0)),
            error: Arc::new(Mutex::new(None)),
            rect_cache: Arc::new(RectCache::new()),
            macro_steps: Mutex::new(Arc::from(Vec::new())),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || worker_loop(worker_shared));
        Self {
            shared,
            worker: Some(worker),
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
        if self.is_running() {
            return Err("A click session is already running.".to_string());
        }
        if !steps.is_empty() && config.input_mode != InputMode::GlobalSendInput {
            return Err("Macro playback requires global SendInput delivery.".to_string());
        }
        config.validate()?;
        *self.shared.macro_steps.lock().expect("macro mutex") = steps;
        self.shared.telemetry.reset();
        self.shared.last_elapsed.store(0, Ordering::Release);
        self.shared.priority_denied.store(false, Ordering::Release);
        *self.shared.error.lock().expect("error mutex") = None;
        self.shared.stop.store(false, Ordering::Release);
        self.shared.panic.store(false, Ordering::Release);
        self.shared.config.store(config);
        self.shared.state.store(STATE_RUNNING, Ordering::Release);
        win32::set_event(self.wake_handle).map_err(|e| e.to_string())
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
        self.shared.stop.store(true, Ordering::Release);
        self.shared.state.store(STATE_STOPPING, Ordering::Release);
    }

    pub fn panic_stop(&self) {
        self.shared.panic.store(true, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
        self.shared.state.store(STATE_PANIC, Ordering::Release);
    }

    pub fn is_running(&self) -> bool {
        self.shared.state.load(Ordering::Acquire) == STATE_RUNNING
    }

    pub fn state(&self) -> i32 {
        self.shared.state.load(Ordering::Acquire)
    }

    pub fn click_count(&self) -> u64 {
        self.shared.telemetry.total_clicks()
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

    pub fn last_error(&self) -> Option<String> {
        self.shared.error.lock().expect("error mutex").clone()
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
        let _ = win32::set_event(self.wake_handle);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        win32::close_handle(self.wake_handle);
    }
}

fn publish_error(shared: &Shared, message: impl Into<String>) {
    *shared.error.lock().expect("error mutex") = Some(message.into());
}

fn worker_loop(shared: Arc<Shared>) {
    loop {
        if shared.terminate.load(Ordering::Acquire) {
            return;
        }
        let wake = win32::wait_for_event(shared.wake_event as isize, INFINITE);
        if wake == win32::WAIT_ERROR {
            publish_error(&shared, "Idle event wait failed; worker exited");
            return;
        }
        if shared.terminate.load(Ordering::Acquire) {
            return;
        }
        run_click_session(&shared);
        let _ = win32::reset_event(shared.wake_event as isize);
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

fn dispatch_global(config: &EngineConfig, phase: Phase) -> Result<(), String> {
    let (down_flag, up_flag) = match config.mouse_button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let inputs: Vec<INPUT> = match phase {
        Phase::Move => Vec::new(), // moves are folded into the down burst
        Phase::Down => {
            if config.coordinate_mode == CoordinateMode::FixedScreen {
                vec![
                    make_mouse_input(
                        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                        win32::normalize_virtual_x(config.fixed_x),
                        win32::normalize_virtual_y(config.fixed_y),
                    ),
                    make_mouse_input(down_flag, 0, 0),
                ]
            } else {
                vec![make_mouse_input(down_flag, 0, 0)]
            }
        }
        Phase::Up => vec![make_mouse_input(up_flag, 0, 0)],
    };
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
    let macro_steps = shared.macro_steps.lock().expect("macro mutex").clone();
    let mut next_deadline = session_start;
    let mut previous_completion: Option<i64> = None;
    let mut sequence: u64 = 0;

    while !shared.stop.load(Ordering::Acquire) && !shared.panic.load(Ordering::Acquire) {
        let mut config = shared.config.load();
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

    let panicked = shared.panic.load(Ordering::Acquire);
    shared.state.store(
        if panicked { STATE_PANIC } else { STATE_IDLE },
        Ordering::Release,
    );
}
