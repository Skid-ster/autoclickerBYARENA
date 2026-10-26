#pragma once

#if defined(_WIN32)

#include <array>
#include <string>
#include <vector>

#include "click_engine.h"
#include "config.h"
#include "telemetry.h"
#include "window_target.h"

namespace op41 {

// Owns all UI state and the single ClickEngine instance. Every frame it polls
// the engine and drains the lock-free telemetry ring; it never blocks on the
// worker and never joins it while the window is alive.
class App {
public:
    App();
    void render();

private:
    void poll_panic_hotkey();
    void refresh_target_list();
    void handle_start();
    void drain_and_aggregate();
    void add_log(const std::string& line);
    void surface_engine_errors();
    void draw_header();
    void draw_timing_panel();
    void draw_input_panel();
    void draw_target_panel();
    void draw_control_panel();
    void draw_analytics_panel();
    void draw_log_panel();
    void draw_cps_graph();
    void draw_interval_histogram();
    [[nodiscard]] static std::string narrow(const std::wstring& wide);

    ClickEngine engine_;
    EngineConfig config_;

    std::vector<WindowInfo> windows_;
    int selected_window_ = -1;

    std::vector<std::string> logs_;
    std::string last_seen_error_;

    std::vector<ClickRecord> sliding_window_;
    std::array<double, kGraphHistorySize> cps_history_{};
    std::size_t cps_head_ = 0;
    std::size_t cps_count_ = 0;
    TelemetryStats stats_;

    bool f8_was_down_ = false;
    double previous_elapsed_ms_ = 0.0;
    std::uint64_t previous_clicks_ = 0;
};

} // namespace op41

#endif // _WIN32
