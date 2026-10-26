#include <cmath>
#include <cstdio>
#include <limits>

#include "humanize.h"

namespace {

int g_failures = 0;

#define CHECK(condition)                                                      \
    do {                                                                      \
        if (!(condition)) {                                                   \
            std::printf("  FAIL: %s (line %d)\n", #condition, __LINE__);      \
            ++g_failures;                                                     \
        }                                                                     \
    } while (0)

void test_box_muller_determinism() {
    const double a = op41::GaussianHumanizer::box_muller_standard(0.5, 0.25);
    const double b = op41::GaussianHumanizer::box_muller_standard(0.5, 0.25);
    CHECK(std::isfinite(a));
    CHECK(std::isfinite(b));
    CHECK(std::fabs(a - b) < 1e-12);

    // Defensive handling of the log(0) singularity.
    const double guarded =
        op41::GaussianHumanizer::box_muller_standard(0.0, 0.5);
    CHECK(std::isfinite(guarded));
}

void test_interval_distribution() {
    op41::EngineConfig config;
    config.base_interval_ms = 100.0;
    config.sigma_ms = 5.0;
    config.sigma_percent = 0.0;
    config.min_interval_ms = 1.0;
    config.max_interval_ms = 1000.0;
    config.humanize_enabled = true;

    op41::GaussianHumanizer humanizer;
    constexpr int kSamples = 50'000;
    double sum = 0.0;
    double squared = 0.0;
    for (int i = 0; i < kSamples; ++i) {
        const double sample = humanizer.next_interval_ms(config);
        CHECK(std::isfinite(sample));
        CHECK(sample >= config.min_interval_ms);
        CHECK(sample <= config.max_interval_ms);
        CHECK(sample >= 0.0);
        sum += sample;
        squared += sample * sample;
    }
    const double mean = sum / kSamples;
    const double variance =
        squared / kSamples - mean * mean; // population variance
    const double sigma = std::sqrt(std::max(variance, 0.0));
    std::printf("  interval mean=%.4f sigma=%.4f (target mean=100 sigma=5)\n",
                mean, sigma);
    CHECK(std::fabs(mean - 100.0) < 0.25);
    CHECK(std::fabs(sigma - 5.0) < 0.5);
}

void test_hold_distribution_and_bounds() {
    op41::EngineConfig config;
    config.hold_mean_ms = 40.0;
    config.hold_sigma_ms = 6.0;
    config.hold_min_ms = 10.0;
    config.hold_max_ms = 90.0;

    op41::GaussianHumanizer humanizer;
    double sum = 0.0;
    for (int i = 0; i < 20'000; ++i) {
        const double hold = humanizer.next_hold_ms(config);
        CHECK(std::isfinite(hold));
        CHECK(hold >= config.hold_min_ms);
        CHECK(hold <= config.hold_max_ms);
        sum += hold;
    }
    const double mean = sum / 20'000.0;
    std::printf("  hold mean=%.4f (target 40)\n", mean);
    CHECK(std::fabs(mean - 40.0) < 0.3);
}

void test_zero_sigma_and_disabled_humanization() {
    op41::EngineConfig config;
    config.base_interval_ms = 75.0;
    config.sigma_ms = 0.0;
    config.sigma_percent = 0.0;
    config.humanize_enabled = true;

    op41::GaussianHumanizer humanizer;
    for (int i = 0; i < 100; ++i) {
        CHECK(std::fabs(humanizer.next_interval_ms(config) - 75.0) < 1e-12);
    }

    config.humanize_enabled = false;
    config.sigma_ms = 9.0;
    for (int i = 0; i < 100; ++i) {
        CHECK(std::fabs(humanizer.next_interval_ms(config) - 75.0) < 1e-12);
    }
}

void test_aggressive_clamp_never_negative() {
    op41::EngineConfig config;
    config.base_interval_ms = 5.0;
    config.sigma_ms = 50.0; // huge spread relative to the mean
    config.min_interval_ms = 2.0;
    config.max_interval_ms = 8.0;

    op41::GaussianHumanizer humanizer;
    for (int i = 0; i < 30'000; ++i) {
        const double sample = humanizer.next_interval_ms(config);
        CHECK(sample >= 2.0);
        CHECK(sample <= 8.0);
    }
}

void test_non_finite_inputs_are_trapped() {
    op41::GaussianHumanizer humanizer;
    op41::EngineConfig config;
    config.base_interval_ms =
        std::numeric_limits<double>::quiet_NaN(); // clamps to min
    const double sample = humanizer.next_interval_ms(config);
    CHECK(std::isfinite(sample));
    CHECK(sample >= config.min_interval_ms);
}

} // namespace

int main() {
    std::printf("== test_humanize ==\n");
    test_box_muller_determinism();
    test_interval_distribution();
    test_hold_distribution_and_bounds();
    test_zero_sigma_and_disabled_humanization();
    test_aggressive_clamp_never_negative();
    test_non_finite_inputs_are_trapped();
    if (g_failures == 0) {
        std::printf("test_humanize: ALL CHECKS PASSED\n");
        return 0;
    }
    std::printf("test_humanize: %d CHECK(S) FAILED\n", g_failures);
    return 1;
}
