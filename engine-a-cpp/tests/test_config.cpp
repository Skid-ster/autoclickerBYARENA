#include <cmath>
#include <cstdio>
#include <limits>

#include "config.h"

namespace {

int g_failures = 0;

#define CHECK(condition)                                                      \
    do {                                                                      \
        if (!(condition)) {                                                   \
            std::printf("  FAIL: %s (line %d)\n", #condition, __LINE__);      \
            ++g_failures;                                                     \
        }                                                                     \
    } while (0)

void test_default_config_is_valid() {
    op41::EngineConfig config;
    const auto error = config.validate();
    CHECK(!error.has_value());
    CHECK(config.target_cps() == 10.0);
    CHECK(config.effective_interval_sigma_ms() == 5.0);
}

void test_invalid_intervals_rejected() {
    op41::EngineConfig config;

    config.base_interval_ms = 0.0;
    CHECK(config.validate().has_value());

    config.base_interval_ms = -1.0;
    CHECK(config.validate().has_value());

    config.base_interval_ms = std::numeric_limits<double>::quiet_NaN();
    CHECK(config.validate().has_value());

    config.base_interval_ms =
        std::numeric_limits<double>::infinity();
    CHECK(config.validate().has_value());
}

void test_negative_sigma_rejected() {
    op41::EngineConfig config;
    config.sigma_ms = -0.1;
    CHECK(config.validate().has_value());

    config.sigma_ms = 5.0;
    config.sigma_percent = -2.0;
    CHECK(config.validate().has_value());

    config.sigma_percent = 2.0;
    config.hold_sigma_ms = -1.0;
    CHECK(config.validate().has_value());
}

void test_inverted_clamps_rejected() {
    op41::EngineConfig config;
    config.min_interval_ms = 200.0;
    config.max_interval_ms = 50.0;
    CHECK(config.validate().has_value());

    config.min_interval_ms = 1.0;
    config.max_interval_ms = 1000.0;
    config.base_interval_ms = 5000.0;
    CHECK(config.validate().has_value()); // base outside clamp range

    config.base_interval_ms = 100.0;
    config.hold_min_ms = 200.0;
    config.hold_max_ms = 100.0;
    CHECK(config.validate().has_value());
}

void test_hold_range_validation() {
    op41::EngineConfig config;
    config.hold_min_ms = 10.0;
    config.hold_max_ms = 20.0;
    config.hold_mean_ms = 50.0; // mean outside clamp range
    CHECK(config.validate().has_value());

    config.hold_mean_ms = 15.0;
    CHECK(!config.validate().has_value());

    config.hold_min_ms = -1.0;
    CHECK(config.validate().has_value());
}

void test_background_requires_hwnd() {
    op41::EngineConfig config;
    config.input_mode = op41::InputMode::BackgroundHwnd;
    config.target_hwnd = nullptr;
    CHECK(config.validate().has_value());

    int sentinel = 0;
    config.target_hwnd = static_cast<void*>(&sentinel);
    CHECK(!config.validate().has_value());
}

void test_negative_client_coordinates_rejected() {
    op41::EngineConfig config;
    config.client_x = -5;
    CHECK(config.validate().has_value());

    config.client_x = 10;
    config.client_y = -1;
    CHECK(config.validate().has_value());
}

void test_sub_millisecond_interval_accepted() {
    op41::EngineConfig config;
    config.base_interval_ms = 0.25; // 4000 CPS target
    config.min_interval_ms = 0.05;
    config.max_interval_ms = 5.0;
    config.sigma_ms = 0.02;
    CHECK(!config.validate().has_value());
    CHECK(std::fabs(config.target_cps() - 4000.0) < 1e-9);
}

} // namespace

int main() {
    std::printf("== test_config ==\n");
    test_default_config_is_valid();
    test_invalid_intervals_rejected();
    test_negative_sigma_rejected();
    test_inverted_clamps_rejected();
    test_hold_range_validation();
    test_background_requires_hwnd();
    test_negative_client_coordinates_rejected();
    test_sub_millisecond_interval_accepted();
    if (g_failures == 0) {
        std::printf("test_config: ALL CHECKS PASSED\n");
        return 0;
    }
    std::printf("test_config: %d CHECK(S) FAILED\n", g_failures);
    return 1;
}
