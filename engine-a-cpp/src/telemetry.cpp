#include "telemetry.h"

#include <algorithm>
#include <cmath>

namespace op41 {

void TelemetryStore::record_click(const ClickRecord& record) {
    ring_.push(record);
    total_clicks_.fetch_add(1, std::memory_order_relaxed);
    if (record.missed_deadline) {
        missed_deadlines_.fetch_add(1, std::memory_order_relaxed);
    }
}

std::size_t TelemetryStore::drain_into(std::vector<ClickRecord>& out_records) {
    ClickRecord record;
    std::size_t drained = 0;
    while (ring_.pop(record)) {
        out_records.push_back(record);
        ++drained;
    }
    return drained;
}

void TelemetryStore::reset() {
    ring_.clear();
    total_clicks_.store(0, std::memory_order_relaxed);
    missed_deadlines_.store(0, std::memory_order_relaxed);
}

TelemetryStats compute_stats(const std::vector<ClickRecord>& window,
                             std::uint64_t total_clicks,
                             std::uint64_t missed_deadlines,
                             double elapsed_ms) {
    TelemetryStats stats;
    stats.total_clicks = total_clicks;
    stats.missed_deadlines = missed_deadlines;
    stats.elapsed_ms = elapsed_ms;
    stats.sample_count = window.size();

    if (window.empty() || elapsed_ms <= 0.0) {
        return stats;
    }

    stats.actual_cps = static_cast<double>(total_clicks) / (elapsed_ms / 1000.0);

    double sum = 0.0;
    stats.min_interval_ms = window.front().interval_ms;
    stats.max_interval_ms = window.front().interval_ms;
    for (const ClickRecord& r : window) {
        sum += r.interval_ms;
        stats.min_interval_ms = std::min(stats.min_interval_ms, r.interval_ms);
        stats.max_interval_ms = std::max(stats.max_interval_ms, r.interval_ms);
    }
    stats.mean_interval_ms = sum / static_cast<double>(window.size());

    double squared = 0.0;
    std::uint64_t within_band = 0;
    const double low_band = stats.mean_interval_ms * 0.9;
    const double high_band = stats.mean_interval_ms * 1.1;
    for (const ClickRecord& r : window) {
        const double delta = r.interval_ms - stats.mean_interval_ms;
        squared += delta * delta;
        if (r.interval_ms >= low_band && r.interval_ms <= high_band) {
            ++within_band;
        }
    }
    if (window.size() > 1) {
        stats.sigma_interval_ms =
            std::sqrt(squared / static_cast<double>(window.size() - 1));
    }
    stats.consistency_pct =
        100.0 * static_cast<double>(within_band) / static_cast<double>(window.size());

    stats.histogram_low_ms = stats.min_interval_ms;
    stats.histogram_high_ms = stats.max_interval_ms;
    const double span = stats.histogram_high_ms - stats.histogram_low_ms;
    if (span <= 0.0) {
        stats.histogram.fill(0);
        stats.histogram[0] = static_cast<std::uint32_t>(window.size());
        return stats;
    }
    const double bin_width = span / static_cast<double>(kHistogramBins);
    for (const ClickRecord& r : window) {
        std::size_t bin = static_cast<std::size_t>(
            (r.interval_ms - stats.histogram_low_ms) / bin_width);
        if (bin >= kHistogramBins) {
            bin = kHistogramBins - 1;
        }
        ++stats.histogram[bin];
    }
    return stats;
}

} // namespace op41
