#include "humanize.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <limits>
#include <random>

#if defined(_WIN32)
#include <windows.h>
#endif

namespace op41 {
namespace {

// SplitMix64 mixing step used to fold several weak entropy sources into a
// single high-quality 64-bit seed.
[[nodiscard]] std::uint64_t splitmix64(std::uint64_t x) {
    x += 0x9e3779b97f4a7c15ULL;
    x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL;
    x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL;
    return x ^ (x >> 31);
}

struct ThreadRng {
    std::mt19937_64 engine;
    bool has_spare = false;
    double spare_normal = 0.0;

    ThreadRng() {
        std::random_device rd;
        std::uint64_t seed = 0x243f6a8885a308d3ULL;
        seed ^= splitmix64((static_cast<std::uint64_t>(rd()) << 32U) ^
                           static_cast<std::uint64_t>(rd()));
        seed ^= splitmix64((static_cast<std::uint64_t>(rd()) << 32U) ^
                           static_cast<std::uint64_t>(rd()));
        const auto now = std::chrono::steady_clock::now().time_since_epoch();
        seed ^= splitmix64(static_cast<std::uint64_t>(
            std::chrono::duration_cast<std::chrono::nanoseconds>(now).count()));
#if defined(_WIN32)
        LARGE_INTEGER qpc{};
        if (QueryPerformanceCounter(&qpc) != 0) {
            seed ^= splitmix64(static_cast<std::uint64_t>(qpc.QuadPart));
        }
#endif
        engine.seed(seed);
    }
};

[[nodiscard]] ThreadRng& local_rng() {
    thread_local ThreadRng rng;
    return rng;
}

[[nodiscard]] double clamp_finite(double value, double low, double high) {
    if (!std::isfinite(value)) {
        return low;
    }
    return std::clamp(value, low, high);
}

} // namespace

void GaussianHumanizer::reseed(std::uint64_t extra_entropy) {
    ThreadRng& rng = local_rng();
    const std::uint64_t mixed = splitmix64(extra_entropy ^ rng.engine());
    rng.engine.seed(mixed);
    rng.has_spare = false;
}

double GaussianHumanizer::box_muller_standard(double u1, double u2) {
    const double safe_u1 = (u1 <= 0.0 || !std::isfinite(u1))
                               ? std::numeric_limits<double>::min()
                               : (u1 > 1.0 ? 1.0 : u1);
    const double angle = (std::isfinite(u2) ? std::clamp(u2, 0.0, 1.0) : 0.5) *
                         2.0 * 3.14159265358979323846;
    return std::sqrt(-2.0 * std::log(safe_u1)) * std::cos(angle);
}

double GaussianHumanizer::next_gaussian(double mean, double sigma) {
    if (sigma <= 0.0) {
        return mean;
    }
    ThreadRng& rng = local_rng();
    if (rng.has_spare) {
        rng.has_spare = false;
        return mean + sigma * rng.spare_normal;
    }

    std::uniform_real_distribution<double> unit(0.0, 1.0);
    double u1 = 0.0;
    while (u1 <= 0.0) {
        u1 = unit(rng.engine); // log(0) is undefined, discard exact zeros
    }
    const double u2 = unit(rng.engine);
    const double radius = std::sqrt(-2.0 * std::log(u1));
    const double angle = 2.0 * 3.14159265358979323846 * u2;
    rng.spare_normal = radius * std::sin(angle);
    rng.has_spare = true;
    return mean + sigma * radius * std::cos(angle);
}

double GaussianHumanizer::next_interval_ms(const EngineConfig& config) {
    if (!config.humanize_enabled) {
        return clamp_finite(config.base_interval_ms, config.min_interval_ms,
                            config.max_interval_ms);
    }
    const double raw = next_gaussian(config.base_interval_ms,
                                     config.effective_interval_sigma_ms());
    return clamp_finite(raw, config.min_interval_ms, config.max_interval_ms);
}

double GaussianHumanizer::next_hold_ms(const EngineConfig& config) {
    const double raw = config.hold_sigma_ms <= 0.0
                           ? config.hold_mean_ms
                           : next_gaussian(config.hold_mean_ms, config.hold_sigma_ms);
    return clamp_finite(raw, config.hold_min_ms, config.hold_max_ms);
}

} // namespace op41
