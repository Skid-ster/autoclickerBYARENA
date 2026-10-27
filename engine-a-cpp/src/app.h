#pragma once

#if defined(_WIN32)

#include <array>
#include <string>
#include <vector>

#include "click_engine.h"
#include "config.h"
#include "telemetry.h"
#include "theme.h"
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
    enum class Tab { Clicker, Target, Analytics, Activity };

    void poll_panic_hotkey();
    void refresh_target_list();
    void handle_start();
    void drain_and_aggregate();
    void add_log(const std::string& line);
    void surface_engine_errors();

    void draw_header();
    void draw_tab_bar();
    void draw_clicker_tab();
    void draw_target_tab();
    void draw_analytics_tab();
    void draw_activity_tab();
    void draw_footer();
    void draw_cps_graph();
    void draw_interval_histogram();
    void draw_validation_line();
    void draw_status_pill(const char* text, ImU32 color, ImU32 soft_color);

    [[nodiscard]] static std::string narrow(const std::wstring& wide);

    ClickEngine engine_;
    EngineConfig config_;

    Tab tab_ = Tab::Clicker;
    bool repeat_limited_ = false;

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
