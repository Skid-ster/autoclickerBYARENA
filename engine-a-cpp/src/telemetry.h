#pragma once

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <vector>

namespace op41 {

inline constexpr std::size_t kTelemetryRingSize = 4096; // power of two
inline constexpr std::size_t kHistogramBins = 24;
inline constexpr std::size_t kGraphHistorySize = 240;

struct alignas(16) ClickRecord {
    std::uint64_t seq = 0;
    double interval_ms = 0.0;     // scheduled interval actually used
    double hold_ms = 0.0;         // button hold duration
    double deadline_error_ms = 0.0; // positive => late relative to deadline
    bool missed_deadline = false;
};

struct TelemetryStats {
    std::uint64_t sample_count = 0;
    std::uint64_t total_clicks = 0;
    std::uint64_t missed_deadlines = 0;
    double elapsed_ms = 0.0;
    double actual_cps = 0.0;
    double mean_interval_ms = 0.0;
    double sigma_interval_ms = 0.0;
    double consistency_pct = 0.0; // share of intervals within +/-10 % of mean
    double min_interval_ms = 0.0;
    double max_interval_ms = 0.0;
    std::array<std::uint32_t, kHistogramBins> histogram{};
    double histogram_low_ms = 0.0;
    double histogram_high_ms = 0.0;
};

// Lock-free single-producer / single-consumer ring buffer.
//
// The click worker is the only producer (push) and the UI thread is the only
// consumer (pop). head_ is written by the producer and tail_ by the consumer;
// the opposite side only reads with acquire ordering. Cache lines are padded
// so the two indices never false-share. When full, push drops the newest
// record and bumps an internal dropped-sample counter instead of blocking:
// the producer never mutates the consumer-owned tail index, so accounting
// (popped + dropped == pushed) stays exact under every interleaving and no
// slot can be touched by both threads at once.
template <typename T, std::size_t N>
class SpscRingBuffer {
    static_assert(N >= 2 && (N & (N - 1)) == 0, "N must be a power of two >= 2");

public:
    SpscRingBuffer() = default;
    SpscRingBuffer(const SpscRingBuffer&) = delete;
    SpscRingBuffer& operator=(const SpscRingBuffer&) = delete;

    // Producer only. Returns false when the buffer was full; the newest
    // element is then dropped and the dropped-sample counter increments.
    bool push(const T& value) {
        const std::size_t head = head_.load(std::memory_order_relaxed);
        const std::size_t tail = tail_.load(std::memory_order_acquire);
        if ((head - tail) >= N) {
            dropped_.fetch_add(1, std::memory_order_relaxed);
            return false;
        }
        slots_[head & kMask] = value;
        head_.store(head + 1, std::memory_order_release);
        return true;
    }

    // Producer only, back-pressured variant of push: returns false without
    // counting a dropped sample so the caller can retry after a pop.
    bool try_push(const T& value) {
        const std::size_t head = head_.load(std::memory_order_relaxed);
        const std::size_t tail = tail_.load(std::memory_order_acquire);
        if ((head - tail) >= N) {
            return false;
        }
        slots_[head & kMask] = value;
        head_.store(head + 1, std::memory_order_release);
        return true;
    }

    // Consumer only.
    bool pop(T& out) {
        const std::size_t tail = tail_.load(std::memory_order_relaxed);
        const std::size_t head = head_.load(std::memory_order_acquire);
        if (head == tail) {
            return false;
        }
        out = slots_[tail & kMask];
        tail_.store(tail + 1, std::memory_order_release);
        return true;
    }

    [[nodiscard]] std::size_t size() const {
        const std::size_t head = head_.load(std::memory_order_acquire);
        const std::size_t tail = tail_.load(std::memory_order_acquire);
        return head - tail;
    }

    [[nodiscard]] bool empty() const { return size() == 0; }
    [[nodiscard]] static constexpr std::size_t capacity() { return N; }
    [[nodiscard]] std::uint64_t dropped() const {
        return dropped_.load(std::memory_order_relaxed);
    }

    void clear() {
        head_.store(0, std::memory_order_release);
        tail_.store(0, std::memory_order_release);
        dropped_.store(0, std::memory_order_relaxed);
    }

private:
    static constexpr std::size_t kMask = N - 1;

    alignas(64) std::atomic<std::size_t> head_{0};
    alignas(64) std::atomic<std::size_t> tail_{0};
    alignas(64) std::atomic<std::uint64_t> dropped_{0};
    alignas(64) std::array<T, N> slots_{};
};

// Aggregate telemetry facade. All counters are atomic; the per-click hot path
// only touches the ring buffer and two relaxed atomics.
class TelemetryStore {
public:
    TelemetryStore() = default;

    // Producer (click worker).
    void record_click(const ClickRecord& record);

    // Consumer (UI thread). Drains every published record into out_records.
    std::size_t drain_into(std::vector<ClickRecord>& out_records);

    [[nodiscard]] std::uint64_t total_clicks() const {
        return total_clicks_.load(std::memory_order_relaxed);
    }
    [[nodiscard]] std::uint64_t missed_deadlines() const {
        return missed_deadlines_.load(std::memory_order_relaxed);
    }
    [[nodiscard]] std::uint64_t dropped_records() const {
        return ring_.dropped();
    }

    void reset();

private:
    SpscRingBuffer<ClickRecord, kTelemetryRingSize> ring_;
    std::atomic<std::uint64_t> total_clicks_{0};
    std::atomic<std::uint64_t> missed_deadlines_{0};
};

// Pure statistics computation over a drained window of records. Kept free of
// OS calls so it can be unit tested deterministically.
[[nodiscard]] TelemetryStats compute_stats(const std::vector<ClickRecord>& window,
                                           std::uint64_t total_clicks,
                                           std::uint64_t missed_deadlines,
                                           double elapsed_ms);

} // namespace op41
