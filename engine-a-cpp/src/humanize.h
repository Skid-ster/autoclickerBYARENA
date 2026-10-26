#pragma once

#include <cstdint>

#include "config.h"

namespace op41 {

// Gaussian humanization engine.
//
// Delays are produced with a Box-Muller transform driven by a thread-local
// std::mt19937_64 PRNG. The thread-local seed mixes std::random_device output,
// the steady clock and, on Windows, QueryPerformanceCounter entropy. Every
// produced delay is clamped into the configured legal range so a negative or
// runaway value can never reach the click scheduler.
class GaussianHumanizer {
public:
    GaussianHumanizer() = default;

    // Re-seed the calling thread's PRNG with additional external entropy.
    static void reseed(std::uint64_t extra_entropy);

    // Gaussian sample around the configured base interval, clamped to
    // [min_interval_ms, max_interval_ms]. Returns the raw base value when
    // humanization is disabled.
    [[nodiscard]] double next_interval_ms(const EngineConfig& config);

    // Gaussian sample around the configured hold mean, clamped to
    // [hold_min_ms, hold_max_ms]. Never returns a negative duration.
    [[nodiscard]] double next_hold_ms(const EngineConfig& config);

    // Pure Box-Muller transform exposed for deterministic unit testing.
    // Given two uniform samples in (0, 1] it returns a standard normal sample.
    [[nodiscard]] static double box_muller_standard(double u1, double u2);

private:
    [[nodiscard]] double next_gaussian(double mean, double sigma);
};

} // namespace op41
