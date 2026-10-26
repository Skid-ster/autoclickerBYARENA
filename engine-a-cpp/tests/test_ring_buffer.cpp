#include <cmath>
#include <atomic>
#include <cstdio>
#include <memory>
#include <thread>

#include "telemetry.h"

namespace {

int g_failures = 0;

#define CHECK(condition)                                                      \
    do {                                                                      \
        if (!(condition)) {                                                   \
            std::printf("  FAIL: %s (line %d)\n", #condition, __LINE__);      \
            ++g_failures;                                                     \
        }                                                                     \
    } while (0)

using TestBuffer = op41::SpscRingBuffer<int, 8>;

void test_fifo_ordering() {
    TestBuffer buffer;
    CHECK(buffer.empty());
    CHECK(buffer.size() == 0);
    CHECK(buffer.capacity() == 8);

    for (int i = 1; i <= 5; ++i) {
        CHECK(buffer.push(i));
    }
    CHECK(buffer.size() == 5);
    CHECK(!buffer.empty());

    for (int expected = 1; expected <= 5; ++expected) {
        int value = 0;
        CHECK(buffer.pop(value));
        CHECK(value == expected);
    }
    CHECK(buffer.empty());
    int unused = 0;
    CHECK(!buffer.pop(unused));
}

void test_power_of_two_wrapping() {
    TestBuffer buffer;
    for (int round = 0; round < 4; ++round) {
        for (int i = 0; i < 6; ++i) {
            CHECK(buffer.push(round * 100 + i));
        }
        for (int i = 0; i < 6; ++i) {
            int value = -1;
            CHECK(buffer.pop(value));
            CHECK(value == round * 100 + i);
        }
        CHECK(buffer.empty());
    }
}

void test_full_buffer_drops_newest() {
    TestBuffer buffer;
    for (int i = 0; i < 8; ++i) {
        CHECK(buffer.push(i + 1)); // fills exactly to capacity
    }
    CHECK(buffer.size() == 8);
    CHECK(buffer.dropped() == 0);

    CHECK(!buffer.push(99)); // newest sample is dropped, oldest kept
    CHECK(buffer.dropped() == 1);
    CHECK(buffer.size() == 8);

    int value = 0;
    CHECK(buffer.pop(value));
    CHECK(value == 1); // oldest element is preserved

    for (int expected = 2; expected <= 8; ++expected) {
        CHECK(buffer.pop(value));
        CHECK(value == expected);
    }
    CHECK(!buffer.pop(value));
    CHECK(buffer.empty());

    for (int i = 0; i < 8; ++i) {
        CHECK(buffer.push(1000 + i)); // refill the drained buffer
    }
    CHECK(!buffer.push(200)); // two more rejected samples on a full buffer
    CHECK(!buffer.push(201));
    CHECK(buffer.dropped() == 3);
}

void test_try_push_applies_back_pressure() {
    TestBuffer buffer;
    for (int i = 0; i < 8; ++i) {
        CHECK(buffer.try_push(i + 1));
    }
    CHECK(!buffer.try_push(99)); // no mutation, no dropped counter change
    CHECK(buffer.dropped() == 0);
    CHECK(buffer.size() == 8);

    int value = 0;
    CHECK(buffer.pop(value));
    CHECK(value == 1);
    CHECK(buffer.try_push(99)); // a freed slot accepts the retry
    CHECK(buffer.size() == 8);
    for (int expected = 2; expected <= 8; ++expected) {
        CHECK(buffer.pop(value));
        CHECK(value == expected);
    }
    CHECK(buffer.pop(value));
    CHECK(value == 99);
}

void test_clear_resets_state() {
    TestBuffer buffer;
    for (int i = 0; i < 10; ++i) {
        buffer.push(i);
    }
    CHECK(buffer.dropped() == 2);
    buffer.clear();
    CHECK(buffer.empty());
    CHECK(buffer.size() == 0);
    CHECK(buffer.dropped() == 0);

    int value = 7;
    CHECK(!buffer.pop(value));
    CHECK(value == 7); // untouched on failed pop
}

void test_ring_across_real_threads() {
    // Real single-producer/single-consumer traffic. Back pressure via
    // try_push makes the run deterministic: no sample may be lost and every
    // value must arrive exactly once and in production order.
    constexpr int kTotal = 10'000;
    auto buffer = std::make_shared<TestBuffer>();
    std::atomic<bool> producer_done{false};

    std::thread producer([buffer, &producer_done] {
        for (int value = 0; value < kTotal; ++value) {
            while (!buffer->try_push(value)) {
                std::this_thread::yield();
            }
        }
        producer_done.store(true, std::memory_order_release);
    });

    int expected = 0;
    while (expected < kTotal) {
        int value = 0;
        if (buffer->pop(value)) {
            CHECK(value == expected);
            ++expected;
        } else {
            std::this_thread::yield();
        }
    }
    producer.join();
    CHECK(expected == kTotal);
    CHECK(buffer->dropped() == 0);
    CHECK(buffer->empty());
    CHECK(producer_done.load(std::memory_order_acquire));

    // Drop-newest accounting across threads: pushes minus drops equals pops,
    // and observed values remain a monotonic subsequence.
    constexpr int kPushes = 50'000;
    auto store = std::make_shared<op41::TelemetryStore>();
    std::thread hot_producer([store] {
        for (std::uint64_t seq = 1; seq <= kPushes; ++seq) {
            op41::ClickRecord record;
            record.seq = seq;
            record.interval_ms = 10.0 + static_cast<double>(seq % 7);
            record.hold_ms = 4.0;
            record.deadline_error_ms = 0.2;
            record.missed_deadline = (seq % 50 == 0);
            store->record_click(record);
        }
    });

    std::vector<op41::ClickRecord> received;
    while (store->total_clicks() < static_cast<std::uint64_t>(kPushes)) {
        store->drain_into(received);
        std::this_thread::yield();
    }
    hot_producer.join();
    store->drain_into(received); // final drain after publication completes

    CHECK(store->total_clicks() == kPushes);
    CHECK(store->missed_deadlines() == kPushes / 50);
    CHECK(static_cast<std::uint64_t>(received.size()) +
                  store->dropped_records() ==
              static_cast<std::uint64_t>(kPushes));
    std::uint64_t previous = 0;
    for (const op41::ClickRecord& record : received) {
        CHECK(record.seq > previous);
        CHECK(record.seq <= static_cast<std::uint64_t>(kPushes));
        previous = record.seq;
    }
}

void test_telemetry_store_counters() {
    op41::TelemetryStore store;
    for (std::uint64_t i = 1; i <= 12; ++i) {
        op41::ClickRecord record;
        record.seq = i;
        record.interval_ms = 10.0 + static_cast<double>(i % 3);
        record.hold_ms = 4.0;
        record.deadline_error_ms = 0.1;
        record.missed_deadline = (i % 4 == 0);
        store.record_click(record);
    }
    CHECK(store.total_clicks() == 12);
    CHECK(store.missed_deadlines() == 3);

    std::vector<op41::ClickRecord> drained;
    const std::size_t count = store.drain_into(drained);
    CHECK(count == 12);
    CHECK(drained.size() == 12);
    CHECK(drained.front().seq == 1);
    CHECK(drained.back().seq == 12);

    store.reset();
    CHECK(store.total_clicks() == 0);
    CHECK(store.missed_deadlines() == 0);
}

void test_compute_stats_basic_moments() {
    std::vector<op41::ClickRecord> records;
    for (int i = 0; i < 100; ++i) {
        op41::ClickRecord record;
        record.seq = static_cast<std::uint64_t>(i + 1);
        record.interval_ms = (i % 2 == 0) ? 90.0 : 110.0;
        record.hold_ms = 5.0;
        record.deadline_error_ms = 0.0;
        record.missed_deadline = false;
        records.push_back(record);
    }
    const op41::TelemetryStats stats =
        op41::compute_stats(records, 100, 0, 10'000.0);
    CHECK(stats.sample_count == 100);
    CHECK(std::fabs(stats.mean_interval_ms - 100.0) < 1e-9);
    CHECK(stats.actual_cps == 10.0);
    CHECK(stats.sigma_interval_ms > 9.0);
    CHECK(stats.consistency_pct >= 0.0);
    CHECK(stats.consistency_pct <= 100.0);
}

} // namespace

int main() {
    std::printf("== test_ring_buffer ==\n");
    test_fifo_ordering();
    test_power_of_two_wrapping();
    test_full_buffer_drops_newest();
    test_try_push_applies_back_pressure();
    test_ring_across_real_threads();
    test_clear_resets_state();
    test_telemetry_store_counters();
    test_compute_stats_basic_moments();
    if (g_failures == 0) {
        std::printf("test_ring_buffer: ALL CHECKS PASSED\n");
        return 0;
    }
    std::printf("test_ring_buffer: %d CHECK(S) FAILED\n", g_failures);
    return 1;
}
