#if defined(_WIN32)

#include "click_engine.h"

#include <utility>

#include "humanize.h"
#include "timing.h"

namespace op41 {

ClickEngine::ClickEngine() {
    wake_event_ = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (wake_event_ == nullptr) {
        publish_error("CreateEventW failed: the click engine cannot park idle");
    }
    worker_ = std::thread([this] { worker_main(); });
}

ClickEngine::~ClickEngine() {
    terminate_.store(true, std::memory_order_release);
    stop_requested_.store(true, std::memory_order_release);
    panic_requested_.store(true, std::memory_order_release);
    if (wake_event_ != nullptr) {
        SetEvent(wake_event_);
    }
    if (worker_.joinable()) {
        worker_.join();
    }
    if (wake_event_ != nullptr) {
        CloseHandle(wake_event_);
        wake_event_ = nullptr;
    }
}

bool ClickEngine::start(const EngineConfig& config, std::string& error_out) {
    if (is_running()) {
        error_out = "A click session is already running.";
        return false;
    }
    if (auto validation_error = config.validate()) {
        error_out = *validation_error;
        return false;
    }
    if (wake_event_ == nullptr) {
        error_out = "Worker idle event was never created.";
        return false;
    }

    telemetry_.reset();
    last_elapsed_ticks_.store(0, std::memory_order_release);
    priority_denied_.store(false, std::memory_order_release);
    {
        std::lock_guard<std::mutex> lock(error_mutex_);
        last_error_.clear();
    }
    stop_requested_.store(false, std::memory_order_release);
    panic_requested_.store(false, std::memory_order_release);
    config_.store(std::make_shared<EngineConfig>(config),
                  std::memory_order_release);
    state_.store(static_cast<int>(EngineState::Running),
                 std::memory_order_release);
    SetEvent(wake_event_);
    return true;
}

void ClickEngine::request_stop() {
    stop_requested_.store(true, std::memory_order_release);
    state_.store(static_cast<int>(EngineState::Stopping),
                 std::memory_order_release);
}

void ClickEngine::panic_stop() {
    panic_requested_.store(true, std::memory_order_release);
    stop_requested_.store(true, std::memory_order_release);
    state_.store(static_cast<int>(EngineState::Panic),
                 std::memory_order_release);
}

bool ClickEngine::is_running() const {
    return state_.load(std::memory_order_acquire) ==
           static_cast<int>(EngineState::Running);
}

EngineState ClickEngine::state() const {
    return static_cast<EngineState>(state_.load(std::memory_order_acquire));
}

std::uint64_t ClickEngine::click_count() const {
    return telemetry_.total_clicks();
}

std::uint64_t ClickEngine::missed_count() const {
    return telemetry_.missed_deadlines();
}

double ClickEngine::elapsed_ms() const {
    const std::int64_t start = start_tick_.load(std::memory_order_acquire);
    if (is_running() && start != 0) {
        return QpcClock::elapsed_ms(start, QpcClock::now());
    }
    return QpcClock::ticks_to_ms(
        last_elapsed_ticks_.load(std::memory_order_acquire));
}

bool ClickEngine::priority_denied() const {
    return priority_denied_.load(std::memory_order_acquire);
}

std::string ClickEngine::last_error() {
    std::lock_guard<std::mutex> lock(error_mutex_);
    return last_error_;
}

void ClickEngine::publish_error(const std::string& message) {
    std::lock_guard<std::mutex> lock(error_mutex_);
    last_error_ = message;
}

void ClickEngine::worker_main() {
    for (;;) {
        // Idle park: blocks on a kernel event instead of busy-spinning.
        const DWORD wait = WaitForSingleObject(wake_event_, INFINITE);
        if (wait != WAIT_OBJECT_0 || terminate_.load(std::memory_order_acquire)) {
            return;
        }
        run_click_session();
        ResetEvent(wake_event_);
    }
}

void ClickEngine::run_click_session() {
    // Raise system timer resolution for the active session only (RAII).
    ScopedTimerResolution resolution(1);
    if (!resolution.active()) {
        publish_error("timeBeginPeriod(1) was rejected; timing precision degraded");
    }
    if (SetThreadPriority(GetCurrentThread(),
                          THREAD_PRIORITY_TIME_CRITICAL) == 0) {
        priority_denied_.store(true, std::memory_order_release);
        publish_error("SetThreadPriority(TIME_CRITICAL) denied by the OS");
    }

    GaussianHumanizer humanizer;
    std::uint64_t sequence = 0;
    const std::int64_t session_start = QpcClock::now();
    start_tick_.store(session_start, std::memory_order_release);
    std::int64_t next_deadline = session_start;

    while (!stop_requested_.load(std::memory_order_acquire) &&
           !panic_requested_.load(std::memory_order_acquire)) {
        const std::shared_ptr<const EngineConfig> cfg =
            config_.load(std::memory_order_acquire);
        if (!cfg) {
            publish_error("Configuration snapshot missing");
            break;
        }
        if (cfg->max_clicks != 0 && sequence >= cfg->max_clicks) {
            break;
        }

        const double interval_ms = humanizer.next_interval_ms(*cfg);
        const double hold_ms = humanizer.next_hold_ms(*cfg);
        const std::int64_t target_tick =
            next_deadline + QpcClock::ms_to_ticks(interval_ms);
        const bool already_late =
            QpcClock::now() > target_tick;

        const SpinResult wait = spin_until_deadline(
            target_tick, stop_requested_, panic_requested_);
        if (wait != SpinResult::DeadlineReached) {
            break;
        }

        const bool background =
            cfg->input_mode == InputMode::BackgroundHwnd;
        RECT client_rect{};
        DispatchResult down_result;

        if (background) {
            HWND target = static_cast<HWND>(cfg->target_hwnd);
            if (!WindowTargetService::is_valid(target)) {
                publish_error("Target window was lost before the click burst");
                break;
            }
            client_rect = target_service_.refresh_cached_rect(target);
            const DispatchResult move_result = dispatcher_.background_move(
                target, cfg->client_x, cfg->client_y, client_rect);
            if (!move_result.ok) {
                publish_error(move_result.message);
                break;
            }
            down_result = dispatcher_.background_button_down(
                target, cfg->client_x, cfg->client_y, client_rect,
                cfg->background_synchronous);
        } else {
            if (cfg->coordinate_mode == CoordinateMode::FixedScreen) {
                const DispatchResult move_result =
                    dispatcher_.global_move_absolute(cfg->fixed_x,
                                                     cfg->fixed_y);
                if (!move_result.ok) {
                    publish_error(move_result.message);
                    break;
                }
            }
            down_result = dispatcher_.global_button_down();
        }
        if (!down_result.ok) {
            publish_error(down_result.message);
            break;
        }

        const std::int64_t hold_deadline =
            QpcClock::now() + QpcClock::ms_to_ticks(hold_ms);
        const SpinResult hold_wait = spin_until_deadline(
            hold_deadline, stop_requested_, panic_requested_);

        // The button must always be released, even on panic/stop.
        DispatchResult up_result;
        if (background) {
            HWND target = static_cast<HWND>(cfg->target_hwnd);
            up_result = dispatcher_.background_button_up(
                target, cfg->client_x, cfg->client_y, client_rect,
                cfg->background_synchronous);
        } else {
            up_result = dispatcher_.global_button_up();
        }
        if (!up_result.ok) {
            publish_error(up_result.message);
        }
        if (hold_wait != SpinResult::DeadlineReached) {
            break;
        }

        const std::int64_t completion_tick = QpcClock::now();
        const double deadline_error_ms =
            QpcClock::elapsed_ms(target_tick, completion_tick);
        ClickRecord record;
        record.seq = sequence + 1;
        record.interval_ms = interval_ms;
        record.hold_ms = hold_ms;
        record.deadline_error_ms = deadline_error_ms;
        record.missed_deadline = already_late || deadline_error_ms > 1.0;
        telemetry_.record_click(record);

        ++sequence;
        next_deadline = target_tick;
        last_elapsed_ticks_.store(completion_tick - session_start,
                                  std::memory_order_release);
    }

    const bool panicked =
        panic_requested_.load(std::memory_order_acquire);
    state_.store(
        static_cast<int>(panicked ? EngineState::Panic : EngineState::Idle),
        std::memory_order_release);
}

} // namespace op41

#endif // _WIN32
