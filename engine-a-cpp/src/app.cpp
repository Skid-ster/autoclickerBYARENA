#if defined(_WIN32)

#include "app.h"

#include <algorithm>
#include <cmath>
#include <cstdio>

#include "humanize.h"
#include "theme.h"

namespace op41 {
namespace {

constexpr std::size_t kSlidingWindowLimit = 2000;
constexpr std::size_t kLogLimit = 200;

std::string format_double(double value, const char* format) {
    char buffer[64];
    std::snprintf(buffer, sizeof(buffer), format, value);
    return std::string(buffer);
}

} // namespace

App::App() {
    add_log("OP 4.1 Killer Engine A initialized. F8 is the panic stop key.");
}

std::string App::narrow(const std::wstring& wide) {
    if (wide.empty()) {
        return std::string{};
    }
    const int needed = WideCharToMultiByte(
        CP_UTF8, 0, wide.c_str(), static_cast<int>(wide.size()),
        nullptr, 0, nullptr, nullptr);
    std::string out(static_cast<std::size_t>(needed), '\0');
    WideCharToMultiByte(CP_UTF8, 0, wide.c_str(),
                        static_cast<int>(wide.size()), out.data(), needed,
                        nullptr, nullptr);
    return out;
}

void App::add_log(const std::string& line) {
    logs_.push_back(line);
    if (logs_.size() > kLogLimit) {
        logs_.erase(logs_.begin(),
                    logs_.begin() + (logs_.size() - kLogLimit));
    }
}

void App::poll_panic_hotkey() {
    const bool down = (GetAsyncKeyState(VK_F8) & 0x8000) != 0;
    if (down && !f8_was_down_ && engine_.is_running()) {
        engine_.panic_stop();
        add_log("PANIC STOP triggered by F8.");
    }
    f8_was_down_ = down;
}

void App::refresh_target_list() {
    windows_ = WindowTargetService::enumerate_windows();
    if (selected_window_ >= static_cast<int>(windows_.size())) {
        selected_window_ = -1;
        config_.target_hwnd = nullptr;
    }
    add_log("Target window list refreshed: " +
            std::to_string(windows_.size()) + " candidates.");
}

void App::handle_start() {
    if (config_.input_mode == InputMode::BackgroundHwnd) {
        if (selected_window_ < 0 ||
            selected_window_ >= static_cast<int>(windows_.size())) {
            add_log("Start rejected: select a target window first.");
            return;
        }
        HWND hwnd = windows_[static_cast<std::size_t>(selected_window_)].hwnd;
        if (!WindowTargetService::is_valid(hwnd)) {
            add_log("Start rejected: selected window handle is invalid.");
            return;
        }
        config_.target_hwnd = static_cast<void*>(hwnd);
        engine_.targets().refresh_cached_rect(hwnd);
    } else {
        config_.target_hwnd = nullptr;
    }

    std::string error;
    if (engine_.start(config_, error)) {
        cps_head_ = 0;
        cps_count_ = 0;
        cps_history_.fill(0.0);
        sliding_window_.clear();
        previous_elapsed_ms_ = 0.0;
        previous_clicks_ = 0;
        add_log("Click session started at " +
                format_double(config_.target_cps(), "%.2f") + " CPS.");
    } else {
        add_log("Start rejected: " + error);
    }
}

void App::surface_engine_errors() {
    const std::string current = engine_.last_error();
    if (!current.empty() && current != last_seen_error_) {
        add_log("Engine error: " + current);
        last_seen_error_ = current;
    }
}

void App::drain_and_aggregate() {
    const std::size_t before = sliding_window_.size();
    engine_.telemetry().drain_into(sliding_window_);
    if (sliding_window_.size() > kSlidingWindowLimit) {
        sliding_window_.erase(
            sliding_window_.begin(),
            sliding_window_.begin() +
                (sliding_window_.size() - kSlidingWindowLimit));
    }

    const double elapsed = engine_.elapsed_ms();
    stats_ = compute_stats(sliding_window_, engine_.click_count(),
                           engine_.missed_count(), elapsed);

    const double delta_ms = elapsed - previous_elapsed_ms_;
    const std::uint64_t delta_clicks =
        engine_.click_count() - previous_clicks_;
    double instant_cps = 0.0;
    if (delta_ms > 0.0 && delta_clicks > 0) {
        instant_cps = static_cast<double>(delta_clicks) * 1000.0 / delta_ms;
    }
    previous_elapsed_ms_ = elapsed;
    previous_clicks_ = engine_.click_count();
    if (before > 0 || engine_.is_running()) {
        cps_history_[cps_head_] = instant_cps;
        cps_head_ = (cps_head_ + 1) % cps_history_.size();
        cps_count_ = std::min(cps_count_ + 1, cps_history_.size());
    }
}

void App::draw_header() {
    const EngineState state = engine_.state();
    const char* state_text = "IDLE";
    ImU32 state_color = theme::kCyan;
    if (state == EngineState::Running) {
        state_text = "RUNNING";
        state_color = theme::kGreen;
    } else if (state == EngineState::Stopping) {
        state_text = "STOPPING";
        state_color = theme::kAmber;
    } else if (state == EngineState::Panic) {
        state_text = "PANIC STOP";
        state_color = theme::kRed;
    }

    ImGui::PushStyleColor(ImGuiCol_Text, ImVec4(0.0f, 0.898f, 1.0f, 1.0f));
    ImGui::Text("OP 4.1 KILLER  -  ENGINE A (C++20 / Dear ImGui / DX11)");
    ImGui::PopStyleColor();
    ImGui::SameLine(ImGui::GetWindowWidth() - 180.0f);
    ImGui::PushStyleColor(ImGuiCol_Text, ImGui::ColorConvertU32ToFloat4(state_color));
    ImGui::Text("STATE: %s", state_text);
    ImGui::PopStyleColor();
    ImGui::Separator();
    ImGui::Text("Clicks: %llu   Elapsed: %.2f s   Actual CPS: %.2f   Missed: %llu",
                static_cast<unsigned long long>(engine_.click_count()),
                engine_.elapsed_ms() / 1000.0,
                stats_.actual_cps,
                static_cast<unsigned long long>(engine_.missed_count()));
    if (engine_.priority_denied()) {
        ImGui::PushStyleColor(ImGuiCol_Text, ImGui::ColorConvertU32ToFloat4(theme::kRed));
        ImGui::Text("WARNING: THREAD_PRIORITY_TIME_CRITICAL was denied by the OS.");
        ImGui::PopStyleColor();
    }
}

void App::draw_timing_panel() {
    if (ImGui::BeginChild("timing_panel", ImVec2(0, 250), true)) {
        ImGui::TextDisabled("TIMING AND GAUSSIAN HUMANIZATION");
        ImGui::Separator();

        double cps = config_.target_cps();
        if (ImGui::InputDouble("Target CPS", &cps, 0.5, 2.0, "%.3f")) {
            config_.base_interval_ms = cps > 0.0 ? 1000.0 / cps : 100.0;
        }
        ImGui::InputDouble("Base interval (ms)", &config_.base_interval_ms,
                           0.5, 5.0, "%.4f");
        ImGui::InputDouble("Interval sigma (ms)", &config_.sigma_ms, 0.5, 2.0,
                           "%.4f");
        ImGui::InputDouble("Interval sigma (%)", &config_.sigma_percent, 1.0,
                           5.0, "%.2f");
        ImGui::InputDouble("Min interval (ms)", &config_.min_interval_ms, 0.5,
                           1.0, "%.4f");
        ImGui::InputDouble("Max interval (ms)", &config_.max_interval_ms, 10.0,
                           50.0, "%.4f");
        ImGui::Checkbox("Enable Gaussian humanization",
                        &config_.humanize_enabled);

        ImGui::Separator();
        ImGui::InputDouble("Hold mean (ms)", &config_.hold_mean_ms, 1.0, 5.0,
                           "%.3f");
        ImGui::InputDouble("Hold sigma (ms)", &config_.hold_sigma_ms, 0.5, 2.0,
                           "%.3f");
        ImGui::InputDouble("Hold min (ms)", &config_.hold_min_ms, 0.5, 1.0,
                           "%.3f");
        ImGui::InputDouble("Hold max (ms)", &config_.hold_max_ms, 5.0, 20.0,
                           "%.3f");

        if (auto error = config_.validate()) {
            ImGui::PushStyleColor(ImGuiCol_Text,
                                  ImGui::ColorConvertU32ToFloat4(theme::kAmber));
            ImGui::TextWrapped("Config: %s", error->c_str());
            ImGui::PopStyleColor();
        } else {
            ImGui::PushStyleColor(ImGuiCol_Text,
                                  ImGui::ColorConvertU32ToFloat4(theme::kGreen));
            ImGui::Text("Config valid. Effective interval sigma: %.3f ms",
                        config_.effective_interval_sigma_ms());
            ImGui::PopStyleColor();
        }
    }
    ImGui::EndChild();
}

void App::draw_input_panel() {
    if (ImGui::BeginChild("input_panel", ImVec2(0, 190), true)) {
        ImGui::TextDisabled("INPUT DISPATCH");
        ImGui::Separator();

        const char* mode_items[] = {"Global SendInput", "Background HWND messages"};
        int mode_index = static_cast<int>(config_.input_mode);
        if (ImGui::Combo("Input mode", &mode_index, mode_items,
                         static_cast<int>(std::size(mode_items)))) {
            config_.input_mode = static_cast<InputMode>(mode_index);
        }

        const char* coord_items[] = {"Current cursor position",
                                     "Fixed screen coordinates",
                                     "Target window client coordinates"};
        int coord_index = static_cast<int>(config_.coordinate_mode);
        if (ImGui::Combo("Coordinate mode", &coord_index, coord_items,
                         static_cast<int>(std::size(coord_items)))) {
            config_.coordinate_mode =
                static_cast<CoordinateMode>(coord_index);
        }

        ImGui::InputScalar("Fixed screen X", ImGuiDataType_S32, &config_.fixed_x);
        ImGui::InputScalar("Fixed screen Y", ImGuiDataType_S32, &config_.fixed_y);
        ImGui::InputScalar("Client X", ImGuiDataType_S32, &config_.client_x);
        ImGui::InputScalar("Client Y", ImGuiDataType_S32, &config_.client_y);
        ImGui::Checkbox("Use SendMessageTimeoutW for background delivery",
                        &config_.background_synchronous);
        ImGui::TextDisabled("Global mode uses arrayed SendInput; background mode "
                            "posts WM_MOUSEMOVE/WM_LBUTTONDOWN/WM_LBUTTONUP.");
    }
    ImGui::EndChild();
}

void App::draw_target_panel() {
    if (ImGui::BeginChild("target_panel", ImVec2(0, 200), true)) {
        ImGui::TextDisabled("BACKGROUND TARGET WINDOW");
        ImGui::Separator();
        if (ImGui::Button("Refresh window list")) {
            refresh_target_list();
        }
        ImGui::SameLine();
        if (ImGui::Button("Validate selected HWND")) {
            if (selected_window_ >= 0 &&
                selected_window_ < static_cast<int>(windows_.size())) {
                HWND hwnd =
                    windows_[static_cast<std::size_t>(selected_window_)].hwnd;
                add_log(WindowTargetService::is_valid(hwnd)
                            ? "Selected HWND is valid."
                            : "Selected HWND is invalid.");
            }
        }

        if (windows_.empty()) {
            ImGui::TextDisabled("No windows enumerated yet.");
        } else if (ImGui::BeginListBox("##target_list",
                                       ImVec2(-1, 110))) {
            for (int i = 0; i < static_cast<int>(windows_.size()); ++i) {
                const WindowInfo& info =
                    windows_[static_cast<std::size_t>(i)];
                const std::string label =
                    narrow(info.title) + "  [PID " +
                    std::to_string(info.process_id) + "]" +
                    (info.minimized ? "  (minimized)" : "");
                const bool selected = selected_window_ == i;
                if (ImGui::Selectable(label.c_str(), selected)) {
                    selected_window_ = i;
                    config_.target_hwnd =
                        static_cast<void*>(info.hwnd);
                }
                if (selected) {
                    ImGui::SetItemDefaultFocus();
                }
            }
            ImGui::EndListBox();
        }
    }
    ImGui::EndChild();
}

void App::draw_control_panel() {
    const bool running = engine_.is_running();
    if (!running) {
        ImGui::PushStyleColor(ImGuiCol_Button,
                              ImVec4(0.224f, 1.0f, 0.533f, 0.35f));
        if (ImGui::Button("START", ImVec2(140, 40))) {
            handle_start();
        }
        ImGui::PopStyleColor();
    } else {
        ImGui::BeginDisabled();
        ImGui::Button("START", ImVec2(140, 40));
        ImGui::EndDisabled();
    }
    ImGui::SameLine();
    ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(1.0f, 0.749f, 0.0f, 0.35f));
    if (ImGui::Button("STOP", ImVec2(140, 40))) {
        engine_.request_stop();
        add_log("Graceful stop requested.");
    }
    ImGui::PopStyleColor();
    ImGui::SameLine();
    ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(1.0f, 0.275f, 0.353f, 0.45f));
    if (ImGui::Button("PANIC STOP (F8)", ImVec2(180, 40))) {
        engine_.panic_stop();
        add_log("PANIC STOP triggered from dashboard.");
    }
    ImGui::PopStyleColor();
}

void App::draw_cps_graph() {
    const ImVec2 p0 = ImGui::GetCursorScreenPos();
    const ImVec2 size(ImGui::GetContentRegionAvail().x, 130);
    ImDrawList* draw = ImGui::GetWindowDrawList();
    draw->AddRectFilled(p0, ImVec2(p0.x + size.x, p0.y + size.y),
                        theme::kPanel, 8.0f);
    draw->PushClipRect(p0, ImVec2(p0.x + size.x, p0.y + size.y), true);

    for (int i = 1; i < 4; ++i) {
        const float y = p0.y + size.y * static_cast<float>(i) / 4.0f;
        draw->AddLine(ImVec2(p0.x + 4, y), ImVec2(p0.x + size.x - 4, y),
                      IM_COL32(0, 229, 255, 28));
    }

    double max_cps = std::max(config_.target_cps() * 1.25, 1.0);
    for (double sample : cps_history_) {
        max_cps = std::max(max_cps, sample);
    }

    if (cps_count_ > 1) {
        ImVec2 previous;
        for (std::size_t i = 0; i < cps_count_; ++i) {
            const std::size_t index =
                (cps_head_ + cps_history_.size() - cps_count_ + i) %
                cps_history_.size();
            const float x = p0.x + size.x * static_cast<float>(i) /
                                    static_cast<float>(cps_history_.size() - 1);
            const float normalized =
                static_cast<float>(cps_history_[index] / max_cps);
            const float y = p0.y + size.y -
                            std::clamp(normalized, 0.0f, 1.0f) *
                                (size.y - 10.0f) -
                            5.0f;
            const ImVec2 current(x, y);
            if (i > 0) {
                draw->AddLine(previous, current, theme::kCyan, 2.0f);
            }
            previous = current;
        }
    }
    draw->PopClipRect();
    draw->AddText(ImVec2(p0.x + 8, p0.y + 4), theme::kTextDim,
                  "REAL-TIME CPS");
    ImGui::Dummy(size);
}

void App::draw_interval_histogram() {
    const ImVec2 p0 = ImGui::GetCursorScreenPos();
    const ImVec2 size(ImGui::GetContentRegionAvail().x, 130);
    ImDrawList* draw = ImGui::GetWindowDrawList();
    draw->AddRectFilled(p0, ImVec2(p0.x + size.x, p0.y + size.y),
                        theme::kPanel, 8.0f);
    draw->PushClipRect(p0, ImVec2(p0.x + size.x, p0.y + size.y), true);

    std::uint32_t max_bin = 1;
    for (std::uint32_t count : stats_.histogram) {
        max_bin = std::max(max_bin, count);
    }
    const float bar_gap = 2.0f;
    const float bar_width =
        size.x / static_cast<float>(kHistogramBins) - bar_gap;
    for (std::size_t bin = 0; bin < kHistogramBins; ++bin) {
        const float height = size.y *
                             static_cast<float>(stats_.histogram[bin]) /
                             static_cast<float>(max_bin) * 0.82f;
        const float x = p0.x + 4.0f +
                        static_cast<float>(bin) * (bar_width + bar_gap);
        draw->AddRectFilled(
            ImVec2(x, p0.y + size.y - height - 6.0f),
            ImVec2(x + bar_width, p0.y + size.y - 6.0f),
            bin % 2 == 0 ? theme::kMagenta : theme::kCyan, 3.0f);
    }
    draw->PopClipRect();
    draw->AddText(ImVec2(p0.x + 8, p0.y + 4), theme::kTextDim,
                  "INTERVAL HISTOGRAM (ms)");
    ImGui::Dummy(size);
}

void App::draw_analytics_panel() {
    if (ImGui::BeginChild("analytics_panel", ImVec2(0, 520), true)) {
        ImGui::TextDisabled("LIVE PERFORMANCE ANALYTICS");
        ImGui::Separator();
        ImGui::Columns(2, "stat_columns", false);
        ImGui::Text("Actual CPS");
        ImGui::NextColumn();
        ImGui::Text("%.3f", stats_.actual_cps);
        ImGui::NextColumn();
        ImGui::Text("Interval mean");
        ImGui::NextColumn();
        ImGui::Text("%.4f ms", stats_.mean_interval_ms);
        ImGui::NextColumn();
        ImGui::Text("Interval sigma");
        ImGui::NextColumn();
        ImGui::Text("%.4f ms", stats_.sigma_interval_ms);
        ImGui::NextColumn();
        ImGui::Text("Consistency (+/-10 % band)");
        ImGui::NextColumn();
        ImGui::Text("%.2f %%", stats_.consistency_pct);
        ImGui::NextColumn();
        ImGui::Text("Observed interval range");
        ImGui::NextColumn();
        ImGui::Text("%.4f .. %.4f ms", stats_.min_interval_ms,
                    stats_.max_interval_ms);
        ImGui::NextColumn();
        ImGui::Text("Missed deadlines");
        ImGui::NextColumn();
        ImGui::Text("%llu",
                    static_cast<unsigned long long>(stats_.missed_deadlines));
        ImGui::NextColumn();
        ImGui::Text("Ring buffer dropped");
        ImGui::NextColumn();
        ImGui::Text("%llu", static_cast<unsigned long long>(
                                engine_.telemetry().dropped_records()));
        ImGui::Columns(1);
        ImGui::Separator();
        draw_cps_graph();
        ImGui::Spacing();
        draw_interval_histogram();
    }
    ImGui::EndChild();
}

void App::draw_log_panel() {
    if (ImGui::BeginChild("log_panel", ImVec2(0, 140), true)) {
        ImGui::TextDisabled("STATUS AND ERROR LOG");
        ImGui::Separator();
        for (const std::string& line : logs_) {
            const bool is_warning =
                line.find("PANIC") != std::string::npos ||
                line.find("rejected") != std::string::npos ||
                line.find("error") != std::string::npos ||
                line.find("WARNING") != std::string::npos;
            if (is_warning) {
                ImGui::PushStyleColor(
                    ImGuiCol_Text,
                    ImGui::ColorConvertU32ToFloat4(theme::kAmber));
            }
            ImGui::TextWrapped("%s", line.c_str());
            if (is_warning) {
                ImGui::PopStyleColor();
            }
        }
        if (ImGui::GetScrollY() >= ImGui::GetScrollMaxY() - 4.0f) {
            ImGui::SetScrollHereY(1.0f);
        }
    }
    ImGui::EndChild();
}

void App::render() {
    poll_panic_hotkey();
    surface_engine_errors();
    drain_and_aggregate();

    ImGui::SetNextWindowPos(ImVec2(0, 0), ImGuiCond_Once);
    ImGui::SetNextWindowSize(ImVec2(1180, 900), ImGuiCond_Once);
    ImGui::Begin("##main_window", nullptr,
                 ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize |
                     ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoCollapse);

    draw_header();
    ImGui::Separator();
    if (ImGui::BeginTable("dashboard", 2,
                          ImGuiTableFlags_Resizable |
                              ImGuiTableFlags_BordersInnerV)) {
        ImGui::TableNextRow();
        ImGui::TableSetColumnIndex(0);
        draw_timing_panel();
        draw_input_panel();
        draw_target_panel();
        draw_control_panel();

        ImGui::TableSetColumnIndex(1);
        draw_analytics_panel();
        ImGui::EndTable();
    }
    ImGui::Separator();
    draw_log_panel();
    ImGui::End();
}

} // namespace op41

#endif // _WIN32
