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
constexpr double kCpsMin = 0.1;
constexpr double kCpsMax = 50.0;

std::string format_double(double value, const char* format) {
    char buffer[64];
    std::snprintf(buffer, sizeof(buffer), format, value);
    return std::string(buffer);
}

ImVec4 as_vec4(ImU32 color) {
    return ImGui::ColorConvertU32ToFloat4(color);
}

// A bordered card that sizes itself to its content, so the page scrollbar is
// the only scrollbar in the window.
void begin_card(const char* id) {
    ImGui::BeginChild(id, ImVec2(-FLT_MIN, 0.0f),
                      ImGuiChildFlags_AutoResizeY | ImGuiChildFlags_Border);
}

void end_card() {
    ImGui::EndChild();
    ImGui::Spacing();
}

void card_title(const char* title) {
    ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kTextDim));
    ImGui::TextUnformatted(title);
    ImGui::PopStyleColor();
    ImGui::Spacing();
}

void hint_text(const char* text) {
    ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kTextDim));
    ImGui::TextWrapped("%s", text);
    ImGui::PopStyleColor();
}

void push_semantic_button(ImU32 soft, ImU32 full) {
    ImGui::PushStyleColor(ImGuiCol_Button, as_vec4(soft));
    ImGui::PushStyleColor(ImGuiCol_ButtonHovered,
                          ImVec4(as_vec4(full).x, as_vec4(full).y,
                                 as_vec4(full).z, 0.55f));
    ImGui::PushStyleColor(ImGuiCol_ButtonActive, as_vec4(full));
    ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kText));
}

void pop_semantic_button() {
    ImGui::PopStyleColor(4);
}

} // namespace

App::App() {
    add_log("OP Auto Clicker (Engine A) initialized. F8 is the panic stop key.");
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
    (void)before;

    const double elapsed = engine_.elapsed_ms();
    stats_ = compute_stats(sliding_window_, engine_.click_count(),
                           engine_.missed_count(), elapsed);

    const double delta_ms = elapsed - previous_elapsed_ms_;
    const std::uint64_t delta_clicks =
        engine_.click_count() - previous_clicks_;
    double instant_cps = 0.0;
    if (delta_ms > 0.0 && delta_clicks > 0) {
        instant_cps =
            static_cast<double>(delta_clicks) * 1000.0 / delta_ms;
    }
    previous_elapsed_ms_ = elapsed;
    previous_clicks_ = engine_.click_count();
    if (!sliding_window_.empty() || engine_.is_running()) {
        cps_history_[cps_head_] = instant_cps;
        cps_head_ = (cps_head_ + 1) % cps_history_.size();
        cps_count_ = std::min(cps_count_ + 1, cps_history_.size());
    }
}

void App::draw_status_pill(const char* text, ImU32 color, ImU32 soft_color) {
    const ImVec2 text_size = ImGui::CalcTextSize(text);
    const ImVec2 pill_size(text_size.x + 28.0f, text_size.y + 12.0f);
    const ImVec2 origin = ImGui::GetCursorScreenPos();
    ImDrawList* draw = ImGui::GetWindowDrawList();
    draw->AddRectFilled(origin,
                        ImVec2(origin.x + pill_size.x, origin.y + pill_size.y),
                        soft_color, 9.0f);
    draw->AddRect(origin,
                  ImVec2(origin.x + pill_size.x, origin.y + pill_size.y),
                  color, 9.0f, 0, 1.2f);
    draw->AddText(ImVec2(origin.x + 14.0f, origin.y + 6.0f), color, text);
    ImGui::Dummy(pill_size);
}

void App::draw_header() {
    const bool running = engine_.is_running();
    const int state = static_cast<int>(engine_.state());

    const char* state_text = "IDLE";
    ImU32 state_color = theme::kTextDim;
    ImU32 state_soft = theme::kInput;
    if (state == static_cast<int>(EngineState::Panic)) {
        state_text = "PANIC STOP";
        state_color = theme::kDanger;
        state_soft = theme::kDangerSoft;
    } else if (state == static_cast<int>(EngineState::Stopping)) {
        state_text = "STOPPING";
        state_color = theme::kWarning;
        state_soft = theme::kWarningSoft;
    } else if (running) {
        state_text = "RUNNING";
        state_color = theme::kSuccess;
        state_soft = theme::kSuccessSoft;
    }

    ImGui::SetWindowFontScale(1.18f);
    ImGui::TextUnformatted("OP Auto Clicker");
    ImGui::SetWindowFontScale(1.0f);

    const ImVec2 pill_size =
        ImVec2(ImGui::CalcTextSize(state_text).x + 28.0f,
               ImGui::CalcTextSize(state_text).y + 12.0f);
    ImGui::SameLine(ImGui::GetWindowWidth() - pill_size.x -
                    ImGui::GetStyle().WindowPadding.x);
    draw_status_pill(state_text, state_color, state_soft);

    ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kTextDim));
    ImGui::Text("Clicks: %llu",
                static_cast<unsigned long long>(engine_.click_count()));
    ImGui::SameLine(180.0f);
    ImGui::Text("Elapsed: %.2f s", engine_.elapsed_ms() / 1000.0);
    ImGui::SameLine(380.0f);
    ImGui::Text("Actual CPS: %.2f", stats_.actual_cps);
    ImGui::SameLine(600.0f);
    ImGui::Text("Missed: %llu",
                static_cast<unsigned long long>(engine_.missed_count()));
    ImGui::PopStyleColor();

    if (engine_.priority_denied()) {
        ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kWarning));
        ImGui::TextWrapped(
            "Warning: THREAD_PRIORITY_TIME_CRITICAL was denied by the OS.");
        ImGui::PopStyleColor();
    }
    ImGui::Spacing();
}

void App::draw_tab_bar() {
    if (ImGui::BeginTabBar("primary_tabs", ImGuiTabBarFlags_None)) {
        if (ImGui::BeginTabItem("Clicker")) {
            tab_ = Tab::Clicker;
            ImGui::EndTabItem();
        }
        if (ImGui::BeginTabItem("Target")) {
            tab_ = Tab::Target;
            ImGui::EndTabItem();
        }
        if (ImGui::BeginTabItem("Analytics")) {
            tab_ = Tab::Analytics;
            ImGui::EndTabItem();
        }
        if (ImGui::BeginTabItem("Activity")) {
            tab_ = Tab::Activity;
            ImGui::EndTabItem();
        }
        ImGui::EndTabBar();
    }
    ImGui::Spacing();
}

void App::draw_validation_line() {
    if (auto error = config_.validate()) {
        ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kWarning));
        ImGui::TextWrapped("Configuration issue: %s", error->c_str());
        ImGui::PopStyleColor();
    } else {
        ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kSuccess));
        ImGui::TextWrapped("Configuration valid. Effective interval sigma: "
                           "%.3f ms",
                           config_.effective_interval_sigma_ms());
        ImGui::PopStyleColor();
    }
}

void App::draw_clicker_tab() {
    // --- Click rate -------------------------------------------------------
    begin_card("rate_card");
    card_title("CLICK RATE");
    double cps = config_.target_cps();
    if (ImGui::SliderScalar("##cps_slider", ImGuiDataType_Double, &cps,
                            &kCpsMin, &kCpsMax, "%.2f CPS")) {
        config_.base_interval_ms = 1000.0 / cps;
    }
    ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kTextDim));
    ImGui::Text("One click every %.2f ms", config_.base_interval_ms);
    ImGui::PopStyleColor();
    end_card();

    // --- Repeat -----------------------------------------------------------
    begin_card("repeat_card");
    card_title("REPEAT");
    int repeat_mode = repeat_limited_ ? 1 : 0;
    if (ImGui::RadioButton("Run until stopped", &repeat_mode, 0)) {
        repeat_limited_ = false;
        config_.max_clicks = 0;
    }
    ImGui::SameLine(220.0f);
    if (ImGui::RadioButton("Stop after a number of clicks", &repeat_mode, 1)) {
        repeat_limited_ = true;
        if (config_.max_clicks == 0) {
            config_.max_clicks = 100;
        }
    }
    if (repeat_limited_) {
        std::uint64_t count = config_.max_clicks;
        ImGui::SetNextItemWidth(220.0f);
        if (ImGui::InputScalar("Click count", ImGuiDataType_U64, &count)) {
            config_.max_clicks = count == 0 ? 1 : count;
        }
    }
    end_card();

    // --- Humanization -----------------------------------------------------
    begin_card("humanize_card");
    card_title("HUMANIZATION");
    ImGui::Checkbox("Enable Gaussian humanization",
                    &config_.humanize_enabled);
    ImGui::SameLine();
    hint_text("adds natural timing variation");
    if (ImGui::CollapsingHeader("Advanced timing & humanization")) {
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Base interval (ms)", &config_.base_interval_ms,
                           0.5, 5.0, "%.4f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Interval sigma (ms)", &config_.sigma_ms, 0.5, 2.0,
                           "%.4f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Interval sigma (%)", &config_.sigma_percent, 1.0,
                           5.0, "%.2f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Min interval (ms)", &config_.min_interval_ms, 0.5,
                           1.0, "%.4f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Max interval (ms)", &config_.max_interval_ms, 10.0,
                           50.0, "%.4f");
        ImGui::Spacing();
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Hold mean (ms)", &config_.hold_mean_ms, 1.0, 5.0,
                           "%.3f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Hold sigma (ms)", &config_.hold_sigma_ms, 0.5, 2.0,
                           "%.3f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Hold min (ms)", &config_.hold_min_ms, 0.5, 1.0,
                           "%.3f");
        ImGui::SetNextItemWidth(240.0f);
        ImGui::InputDouble("Hold max (ms)", &config_.hold_max_ms, 5.0, 20.0,
                           "%.3f");
        ImGui::Spacing();
        draw_validation_line();
    }
    end_card();

    // --- Input dispatch ---------------------------------------------------
    begin_card("input_card");
    card_title("INPUT DISPATCH");
    int mode_index = static_cast<int>(config_.input_mode);
    ImGui::RadioButton("Global (SendInput)", &mode_index, 0);
    ImGui::SameLine(260.0f);
    ImGui::RadioButton("Background (window messages)", &mode_index, 1);
    config_.input_mode = static_cast<InputMode>(mode_index);
    ImGui::Spacing();

    if (config_.input_mode == InputMode::GlobalSendInput) {
        const char* coord_items[] = {"Current cursor position",
                                     "Fixed screen coordinates"};
        int coord_index =
            config_.coordinate_mode == CoordinateMode::FixedScreen ? 1 : 0;
        ImGui::SetNextItemWidth(320.0f);
        if (ImGui::Combo("##coord_mode", &coord_index, coord_items,
                         static_cast<int>(std::size(coord_items)))) {
            config_.coordinate_mode = coord_index == 1
                                          ? CoordinateMode::FixedScreen
                                          : CoordinateMode::CurrentCursor;
        }
        if (config_.coordinate_mode == CoordinateMode::FixedScreen) {
            ImGui::SetNextItemWidth(160.0f);
            ImGui::InputScalar("Fixed screen X", ImGuiDataType_S32,
                               &config_.fixed_x);
            ImGui::SetNextItemWidth(160.0f);
            ImGui::InputScalar("Fixed screen Y", ImGuiDataType_S32,
                               &config_.fixed_y);
        }
        hint_text("Global mode injects hardware-level input with arrayed "
                  "SendInput and works in any application.");
    } else {
        ImGui::SetNextItemWidth(160.0f);
        ImGui::InputScalar("Client X", ImGuiDataType_S32, &config_.client_x);
        ImGui::SetNextItemWidth(160.0f);
        ImGui::InputScalar("Client Y", ImGuiDataType_S32, &config_.client_y);
        ImGui::Checkbox("Use SendMessageTimeoutW (synchronous delivery)",
                        &config_.background_synchronous);
        if (selected_window_ < 0 ||
            selected_window_ >= static_cast<int>(windows_.size())) {
            ImGui::Spacing();
            push_semantic_button(theme::kAccentSoft, theme::kAccent);
            if (ImGui::Button("Pick a target window on the Target tab",
                              ImVec2(380.0f, 34.0f))) {
                tab_ = Tab::Target;
            }
            pop_semantic_button();
        }
        hint_text("Background mode posts WM_MOUSEMOVE/WM_LBUTTONDOWN/"
                  "WM_LBUTTONUP and can click unfocused or minimized windows.");
    }
    end_card();
}

void App::draw_target_tab() {
    begin_card("target_controls");
    card_title("BACKGROUND TARGET WINDOW");
    if (ImGui::Button("Refresh window list", ImVec2(190.0f, 34.0f))) {
        refresh_target_list();
    }
    ImGui::SameLine();
    if (ImGui::Button("Validate selected HWND", ImVec2(220.0f, 34.0f))) {
        if (selected_window_ >= 0 &&
            selected_window_ < static_cast<int>(windows_.size())) {
            HWND hwnd =
                windows_[static_cast<std::size_t>(selected_window_)].hwnd;
            add_log(WindowTargetService::is_valid(hwnd)
                        ? "Selected HWND is valid."
                        : "Selected HWND is invalid.");
        }
    }
    end_card();

    begin_card("target_list_card");
    if (windows_.empty()) {
        hint_text("No windows enumerated yet. Press \"Refresh window list\" "
                  "to list visible top-level windows.");
    } else if (ImGui::BeginListBox(
                   "##target_list",
                   ImVec2(-FLT_MIN,
                          std::max(160.0f,
                                   std::min(340.0f,
                                            ImGui::GetContentRegionAvail().y -
                                                12.0f))))) {
        for (int i = 0; i < static_cast<int>(windows_.size()); ++i) {
            const WindowInfo& info = windows_[static_cast<std::size_t>(i)];
            const std::string label =
                narrow(info.title) + "  [PID " +
                std::to_string(info.process_id) + "]" +
                (info.minimized ? "  (minimized)" : "");
            const bool selected = selected_window_ == i;
            if (ImGui::Selectable(label.c_str(), selected)) {
                selected_window_ = i;
                config_.target_hwnd = static_cast<void*>(info.hwnd);
            }
            if (selected) {
                ImGui::SetItemDefaultFocus();
            }
        }
        ImGui::EndListBox();
    }
    end_card();

    begin_card("target_hint_card");
    hint_text("Only needed for Background mode. The selected window's client "
              "rectangle is cached so clicks keep landing while the target is "
              "minimized. Global mode ignores this list.");
    end_card();
}

void App::draw_cps_graph() {
    const ImVec2 p0 = ImGui::GetCursorScreenPos();
    const ImVec2 size(ImGui::GetContentRegionAvail().x, 140.0f);
    ImDrawList* draw = ImGui::GetWindowDrawList();
    draw->AddRectFilled(p0, ImVec2(p0.x + size.x, p0.y + size.y),
                        theme::kInput, 8.0f);
    draw->PushClipRect(p0, ImVec2(p0.x + size.x, p0.y + size.y), true);

    for (int step = 1; step < 4; ++step) {
        const float y = p0.y + size.y * step / 4.0f;
        draw->AddLine(ImVec2(p0.x + 6.0f, y),
                      ImVec2(p0.x + size.x - 6.0f, y), theme::kChartGrid,
                      1.0f);
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
            const float x =
                p0.x + size.x * static_cast<float>(i) /
                          static_cast<float>(cps_history_.size() - 1);
            const double normalized =
                std::clamp(cps_history_[index] / max_cps, 0.0, 1.0);
            const float y = p0.y + size.y - 8.0f -
                            static_cast<float>(normalized) * (size.y - 18.0f);
            const ImVec2 current(x, y);
            if (i > 0) {
                draw->AddLine(previous, current, theme::kAccent, 2.0f);
            }
            previous = current;
        }
    }
    draw->PopClipRect();
    draw->AddText(ImVec2(p0.x + 10.0f, p0.y + 8.0f), theme::kTextDim,
                  "REAL-TIME CPS");
    ImGui::Dummy(size);
}

void App::draw_interval_histogram() {
    const ImVec2 p0 = ImGui::GetCursorScreenPos();
    const ImVec2 size(ImGui::GetContentRegionAvail().x, 140.0f);
    ImDrawList* draw = ImGui::GetWindowDrawList();
    draw->AddRectFilled(p0, ImVec2(p0.x + size.x, p0.y + size.y),
                        theme::kInput, 8.0f);
    draw->PushClipRect(p0, ImVec2(p0.x + size.x, p0.y + size.y), true);

    std::uint32_t max_bin = 1;
    for (std::uint32_t count : stats_.histogram) {
        max_bin = std::max(max_bin, count);
    }
    constexpr std::size_t kBins = kHistogramBins;
    const float bar_gap = 3.0f;
    const float bar_width =
        size.x / static_cast<float>(kBins) - bar_gap;
    for (std::size_t bin = 0; bin < kBins; ++bin) {
        const float height =
            size.y * static_cast<float>(stats_.histogram[bin]) /
            static_cast<float>(max_bin) * 0.82f;
        const float x = p0.x + 6.0f +
                        static_cast<float>(bin) * (bar_width + bar_gap);
        draw->AddRectFilled(
            ImVec2(x, p0.y + size.y - height - 8.0f),
            ImVec2(x + bar_width, p0.y + size.y - 8.0f),
            bin % 2 == 0 ? theme::kAccent : theme::kChartBarAlt, 4.0f);
    }
    draw->PopClipRect();
    draw->AddText(ImVec2(p0.x + 10.0f, p0.y + 8.0f), theme::kTextDim,
                  "INTERVAL HISTOGRAM (ms)");
    ImGui::Dummy(size);
}

void App::draw_analytics_tab() {
    begin_card("metrics_card");
    card_title("LIVE PERFORMANCE ANALYTICS");
    if (ImGui::BeginTable("metrics_table", 2, ImGuiTableFlags_None)) {
        const auto metric = [](const char* label, const std::string& value) {
            ImGui::TableNextRow();
            ImGui::TableNextColumn();
            ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kTextDim));
            ImGui::TextUnformatted(label);
            ImGui::PopStyleColor();
            ImGui::TableNextColumn();
            ImGui::TextUnformatted(value.c_str());
        };
        metric("Actual CPS", format_double(stats_.actual_cps, "%.3f"));
        metric("Interval mean",
               format_double(stats_.mean_interval_ms, "%.4f ms"));
        metric("Interval sigma",
               format_double(stats_.sigma_interval_ms, "%.4f ms"));
        metric("Consistency (+/-10%)",
               format_double(stats_.consistency_pct, "%.2f %%"));
        metric("Missed deadlines",
               std::to_string(stats_.missed_deadlines));
        metric("Ring buffer dropped",
               std::to_string(engine_.telemetry().dropped_records()));
        ImGui::EndTable();
    }
    ImGui::PushStyleColor(ImGuiCol_Text, as_vec4(theme::kTextDim));
    ImGui::Text("Observed interval range: %.4f .. %.4f ms",
                stats_.min_interval_ms, stats_.max_interval_ms);
    ImGui::PopStyleColor();
    end_card();

    begin_card("cps_graph_card");
    draw_cps_graph();
    end_card();

    begin_card("histogram_card");
    draw_interval_histogram();
    end_card();
}

void App::draw_activity_tab() {
    begin_card("log_card");
    card_title("STATUS AND ERROR LOG");
    const float height =
        std::max(220.0f, ImGui::GetContentRegionAvail().y - 12.0f);
    if (ImGui::BeginChild("log_scroll", ImVec2(-FLT_MIN, height),
                          ImGuiChildFlags_Border)) {
        for (const std::string& line : logs_) {
            const bool is_warning =
                line.find("PANIC") != std::string::npos ||
                line.find("rejected") != std::string::npos ||
                line.find("error") != std::string::npos ||
                line.find("Warning") != std::string::npos;
            if (is_warning) {
                ImGui::PushStyleColor(ImGuiCol_Text,
                                      as_vec4(theme::kWarning));
            } else {
                ImGui::PushStyleColor(ImGuiCol_Text,
                                      as_vec4(theme::kTextDim));
            }
            ImGui::TextWrapped("%s", line.c_str());
            ImGui::PopStyleColor();
        }
        if (ImGui::GetScrollY() >= ImGui::GetScrollMaxY() - 4.0f) {
            ImGui::SetScrollHereY(1.0f);
        }
    }
    ImGui::EndChild();
    end_card();
}

void App::draw_footer() {
    ImGui::Separator();
    const bool running = engine_.is_running();
    const float width = ImGui::GetContentRegionAvail().x;
    const float gap = ImGui::GetStyle().ItemSpacing.x;
    const float start_width = (width - 2.0f * gap) * 0.5f;
    const float small_width = (width - 2.0f * gap) * 0.25f;

    ImGui::BeginDisabled(running);
    push_semantic_button(theme::kSuccessSoft, theme::kSuccess);
    if (ImGui::Button("START", ImVec2(start_width, 46.0f))) {
        handle_start();
    }
    pop_semantic_button();
    ImGui::EndDisabled();

    ImGui::SameLine();
    ImGui::BeginDisabled(!running);
    push_semantic_button(theme::kWarningSoft, theme::kWarning);
    if (ImGui::Button("STOP", ImVec2(small_width, 46.0f))) {
        engine_.request_stop();
        add_log("Graceful stop requested.");
    }
    pop_semantic_button();
    ImGui::EndDisabled();

    ImGui::SameLine();
    push_semantic_button(theme::kDangerSoft, theme::kDanger);
    if (ImGui::Button("PANIC STOP (F8)", ImVec2(small_width, 46.0f))) {
        engine_.panic_stop();
        add_log("PANIC STOP triggered from dashboard.");
    }
    pop_semantic_button();
}

void App::render() {
    poll_panic_hotkey();
    surface_engine_errors();
    drain_and_aggregate();

    ImGui::SetNextWindowPos(ImVec2(0, 0), ImGuiCond_Once);
    ImGui::SetNextWindowSize(ImVec2(1040, 820), ImGuiCond_Once);
    ImGui::Begin("##main_window", nullptr,
                 ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoMove |
                     ImGuiWindowFlags_NoCollapse);

    draw_header();
    draw_tab_bar();

    // The tab bar records the user's selection into tab_ every frame; render
    // whichever tab is active so inactive widgets never consume layout space.
    switch (tab_) {
    case Tab::Clicker:
        draw_clicker_tab();
        break;
    case Tab::Target:
        draw_target_tab();
        break;
    case Tab::Analytics:
        draw_analytics_tab();
        break;
    case Tab::Activity:
        draw_activity_tab();
        break;
    }

    draw_footer();
    ImGui::End();
}

} // namespace op41

#endif // _WIN32
