#pragma once

#include <cmath>
#include <cstdint>
#include <optional>
#include <string>

namespace op41 {

enum class InputMode : int {
    GlobalSendInput = 0,
    BackgroundHwnd = 1
};

enum class CoordinateMode : int {
    CurrentCursor = 0,
    FixedScreen = 1,
    WindowClient = 2
};

// Immutable configuration snapshot. The UI thread builds one of these and the
// worker thread receives it through an atomic shared_ptr swap. The header is
// free of Windows.h so the validation logic remains unit-testable on any
// compiler/standard library.
struct EngineConfig {
    // Interval scheduling (milliseconds).
    double base_interval_ms = 100.0;
    double sigma_ms = 5.0;
    double sigma_percent = 0.0;       // additional relative Gaussian sigma
    double min_interval_ms = 1.0;
    double max_interval_ms = 10'000.0;

    // Button hold duration (milliseconds).
    double hold_mean_ms = 40.0;
    double hold_sigma_ms = 8.0;
    double hold_min_ms = 1.0;
    double hold_max_ms = 500.0;

    // Dispatch behaviour.
    InputMode input_mode = InputMode::GlobalSendInput;
    CoordinateMode coordinate_mode = CoordinateMode::CurrentCursor;
    long fixed_x = 0;                 // absolute screen pixels
    long fixed_y = 0;
    long client_x = 100;              // target window client pixels
    long client_y = 100;
    void* target_hwnd = nullptr;      // HWND, kept as void* for portability
    bool background_synchronous = false; // SendMessageTimeoutW instead of PostMessageW

    // Miscellaneous.
    bool humanize_enabled = true;
    std::uint64_t max_clicks = 0;     // 0 means unlimited

    [[nodiscard]] double effective_interval_sigma_ms() const {
        return sigma_ms + (sigma_percent * 0.01) * base_interval_ms;
    }

    [[nodiscard]] double target_cps() const {
        return base_interval_ms > 0.0 ? 1000.0 / base_interval_ms : 0.0;
    }

    // Returns a human readable error message, or an empty optional when valid.
    [[nodiscard]] std::optional<std::string> validate() const {
        if (!std::isfinite(base_interval_ms) || base_interval_ms <= 0.0) {
            return std::string{"Base interval must be a finite positive millisecond value."};
        }
        if (!std::isfinite(sigma_ms) || sigma_ms < 0.0) {
            return std::string{"Interval sigma (ms) must be finite and non-negative."};
        }
        if (!std::isfinite(sigma_percent) || sigma_percent < 0.0) {
            return std::string{"Interval sigma (%) must be finite and non-negative."};
        }
        if (!std::isfinite(min_interval_ms) || min_interval_ms <= 0.0) {
            return std::string{"Minimum interval must be a finite positive millisecond value."};
        }
        if (!std::isfinite(max_interval_ms) || max_interval_ms < min_interval_ms) {
            return std::string{"Maximum interval must be finite and not below the minimum."};
        }
        if (base_interval_ms < min_interval_ms || base_interval_ms > max_interval_ms) {
            return std::string{"Base interval must lie inside the min/max clamp range."};
        }

        if (!std::isfinite(hold_mean_ms) || hold_mean_ms < 0.0) {
            return std::string{"Hold mean must be a finite non-negative millisecond value."};
        }
        if (!std::isfinite(hold_sigma_ms) || hold_sigma_ms < 0.0) {
            return std::string{"Hold sigma must be finite and non-negative."};
        }
        if (!std::isfinite(hold_min_ms) || hold_min_ms < 0.0) {
            return std::string{"Hold minimum must be finite and non-negative."};
        }
        if (!std::isfinite(hold_max_ms) || hold_max_ms < hold_min_ms) {
            return std::string{"Hold maximum must be finite and not below the hold minimum."};
        }
        if (hold_mean_ms < hold_min_ms || hold_mean_ms > hold_max_ms) {
            return std::string{"Hold mean must lie inside the hold min/max clamp range."};
        }

        if (client_x < 0 || client_y < 0) {
            return std::string{"Window client coordinates must be non-negative."};
        }
        if (input_mode == InputMode::BackgroundHwnd && target_hwnd == nullptr) {
            return std::string{"Background mode requires a selected target window."};
        }
        return std::nullopt;
    }
};

} // namespace op41
