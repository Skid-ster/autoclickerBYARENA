#pragma once

#if defined(_WIN32)

#include <windows.h>

#include <atomic>
#include <cstdint>

namespace op41 {

// Thin QueryPerformanceCounter wrapper. All values are integer QPC ticks;
// conversion to milliseconds happens through the captured frequency.
class QpcClock {
public:
    [[nodiscard]] static std::int64_t frequency();
    [[nodiscard]] static std::int64_t now();
    [[nodiscard]] static double ticks_to_ms(std::int64_t ticks);
    [[nodiscard]] static double elapsed_ms(std::int64_t start_tick,
                                           std::int64_t end_tick);
    [[nodiscard]] static std::int64_t ms_to_ticks(double milliseconds);
};

// RAII guard around timeBeginPeriod(1)/timeEndPeriod(1). The guard is created
// immediately before the clicking worker enters its active loop and destroyed
// when that loop exits, so the system timer resolution is raised only while
// clicking is actually in progress.
class ScopedTimerResolution {
public:
    explicit ScopedTimerResolution(unsigned period_ms = 1);
    ~ScopedTimerResolution();
    ScopedTimerResolution(const ScopedTimerResolution&) = delete;
    ScopedTimerResolution& operator=(const ScopedTimerResolution&) = delete;

    [[nodiscard]] bool active() const { return active_; }
    [[nodiscard]] unsigned period_ms() const { return period_ms_; }

private:
    unsigned period_ms_;
    bool active_;
};

enum class SpinResult {
    DeadlineReached,
    StopRequested,
    PanicRequested
};

// Busy-waits until deadline_tick using QueryPerformanceCounter and _mm_pause.
// There is deliberately no Sleep() call anywhere in this path. Both flags are
// sampled on every iteration so panic stop latency stays below one interval.
[[nodiscard]] SpinResult spin_until_deadline(
    std::int64_t deadline_tick,
    const std::atomic<bool>& stop_requested,
    const std::atomic<bool>& panic_requested);

} // namespace op41

#endif // _WIN32
