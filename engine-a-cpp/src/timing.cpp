#if defined(_WIN32)

#include "timing.h"

#include <timeapi.h>
#include <intrin.h>

#pragma comment(lib, "winmm.lib")

namespace op41 {

std::int64_t QpcClock::frequency() {
    static const std::int64_t cached_frequency = [] {
        LARGE_INTEGER freq{};
        QueryPerformanceFrequency(&freq);
        return freq.QuadPart;
    }();
    return cached_frequency;
}

std::int64_t QpcClock::now() {
    LARGE_INTEGER counter{};
    QueryPerformanceCounter(&counter);
    return counter.QuadPart;
}

double QpcClock::ticks_to_ms(std::int64_t ticks) {
    const std::int64_t freq = frequency();
    if (freq <= 0) {
        return 0.0;
    }
    return static_cast<double>(ticks) * 1000.0 / static_cast<double>(freq);
}

double QpcClock::elapsed_ms(std::int64_t start_tick, std::int64_t end_tick) {
    return ticks_to_ms(end_tick - start_tick);
}

std::int64_t QpcClock::ms_to_ticks(double milliseconds) {
    const std::int64_t freq = frequency();
    return static_cast<std::int64_t>(milliseconds * static_cast<double>(freq) /
                                     1000.0);
}

ScopedTimerResolution::ScopedTimerResolution(unsigned period_ms)
    : period_ms_(period_ms), active_(false) {
    // TIMERR_NOERROR is zero. Any other result means the resolution request
    // was rejected and the caller must surface that honestly.
    if (timeBeginPeriod(period_ms_) == TIMERR_NOERROR) {
        active_ = true;
    }
}

ScopedTimerResolution::~ScopedTimerResolution() {
    if (active_) {
        timeEndPeriod(period_ms_);
        active_ = false;
    }
}

SpinResult spin_until_deadline(std::int64_t deadline_tick,
                               const std::atomic<bool>& stop_requested,
                               const std::atomic<bool>& panic_requested) {
    for (;;) {
        if (panic_requested.load(std::memory_order_acquire)) {
            return SpinResult::PanicRequested;
        }
        if (stop_requested.load(std::memory_order_acquire)) {
            return SpinResult::StopRequested;
        }
        if (QpcClock::now() >= deadline_tick) {
            return SpinResult::DeadlineReached;
        }
        _mm_pause(); // reduce power/spin contention without sleeping
    }
}

} // namespace op41

#endif // _WIN32
