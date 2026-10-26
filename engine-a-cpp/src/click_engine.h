#pragma once

#if defined(_WIN32)

#include <windows.h>

#include <atomic>
#include <memory>
#include <mutex>
#include <string>
#include <thread>

#include "config.h"
#include "input_dispatcher.h"
#include "telemetry.h"
#include "window_target.h"

namespace op41 {

enum class EngineState : int {
    Idle = 0,
    Running = 1,
    Stopping = 2,
    Panic = 3
};

// Dedicated click worker. The UI thread never executes any timing code: it
// only swaps an atomic shared_ptr configuration snapshot and reads telemetry.
// The worker parks on a Windows event while idle and uses QPC spin scheduling
// while active. No Sleep() call exists anywhere in the active click path.
class ClickEngine {
public:
    ClickEngine();
    ~ClickEngine();
    ClickEngine(const ClickEngine&) = delete;
    ClickEngine& operator=(const ClickEngine&) = delete;

    // Launches a run with the supplied configuration. Returns false (with an
    // error message) without blocking when validation fails.
    bool start(const EngineConfig& config, std::string& error_out);
    void request_stop();   // graceful stop after the current click
    void panic_stop();     // emergency stop, checked inside every spin

    [[nodiscard]] bool is_running() const;
    [[nodiscard]] EngineState state() const;
    [[nodiscard]] std::uint64_t click_count() const;
    [[nodiscard]] std::uint64_t missed_count() const;
    [[nodiscard]] double elapsed_ms() const;
    [[nodiscard]] bool priority_denied() const;
    [[nodiscard]] std::string last_error();

    TelemetryStore& telemetry() { return telemetry_; }
    WindowTargetService& targets() { return target_service_; }

private:
    void worker_main();
    void run_click_session();
    void publish_error(const std::string& message);

    std::thread worker_;
    HANDLE wake_event_ = nullptr;
    std::atomic<bool> terminate_{false};
    std::atomic<bool> stop_requested_{false};
    std::atomic<bool> panic_requested_{false};
    std::atomic<bool> priority_denied_{false};
    std::atomic<int> state_{static_cast<int>(EngineState::Idle)};
    std::atomic<std::int64_t> start_tick_{0};
    std::atomic<std::int64_t> last_elapsed_ticks_{0};

    std::atomic<std::shared_ptr<const EngineConfig>> config_{
        std::shared_ptr<const EngineConfig>{}};

    TelemetryStore telemetry_;
    InputDispatcher dispatcher_;
    WindowTargetService target_service_;

    mutable std::mutex error_mutex_;
    std::string last_error_;
};

} // namespace op41

#endif // _WIN32
