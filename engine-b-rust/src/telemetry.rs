//! Lock-free telemetry pipeline.
//!
//! The click worker is the sole producer and the UI thread the sole consumer.
//! Head/tail indices live on separate cache lines and are coordinated with
//! acquire/release atomics; the slots use [`Cell`] because records are `Copy`,
//! which keeps the whole data path free of unsafe code and mutexes. A full
//! buffer overwrites the oldest record and increments the dropped counter
//! instead of ever blocking the producer.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub const TELEMETRY_RING_SIZE: usize = 4096;
pub const HISTOGRAM_BINS: usize = 24;
pub const GRAPH_HISTORY_SIZE: usize = 240;
pub const SLIDING_WINDOW_LIMIT: usize = 2000;

#[derive(Debug, Clone, Copy, Default)]
pub struct ClickRecord {
    pub seq: u64,
    pub interval_ms: f64,
    pub hold_ms: f64,
    pub deadline_error_ms: f64,
    pub missed_deadline: bool,
}

#[repr(align(64))]
struct CacheLine<T>(T);

pub struct SpscRingBuffer<T: Copy + Default, const N: usize> {
    head: CacheLine<AtomicUsize>,
    tail: CacheLine<AtomicUsize>,
    dropped: CacheLine<AtomicU64>,
    slots: Vec<Cell<T>>,
}

impl<T: Copy + Default, const N: usize> SpscRingBuffer<T, N> {
    pub fn new() -> Self {
        assert!(N >= 2 && N.is_power_of_two(), "N must be a power of two >= 2");
        let mut slots = Vec::with_capacity(N);
        for _ in 0..N {
            slots.push(Cell::new(T::default()));
        }
        Self {
            head: CacheLine(AtomicUsize::new(0)),
            tail: CacheLine(AtomicUsize::new(0)),
            dropped: CacheLine(AtomicU64::new(0)),
            slots,
        }
    }

    /// Producer only. Returns `false` when the buffer was full and the oldest
    /// value was overwritten.
    pub fn push(&self, value: T) -> bool {
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Acquire);
        let full = head - tail >= N;
        self.slots[head & (N - 1)].set(value);
        self.head.0.store(head + 1, Ordering::Release);
        if full {
            self.dropped.0.fetch_add(1, Ordering::Relaxed);
            self.tail.0.store(tail + 1, Ordering::Release);
            false
        } else {
            true
        }
    }

    /// Consumer only.
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.0.load(Ordering::Relaxed);
        let head = self.head.0.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let value = self.slots[tail & (N - 1)].get();
        self.tail.0.store(tail + 1, Ordering::Release);
        Some(value)
    }

    pub fn len(&self) -> usize {
        let head = self.head.0.load(Ordering::Acquire);
        let tail = self.tail.0.load(Ordering::Acquire);
        head - tail
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.0.load(Ordering::Relaxed)
    }

    pub fn clear(&self) {
        self.head.0.store(0, Ordering::Release);
        self.tail.0.store(0, Ordering::Release);
        self.dropped.0.store(0, Ordering::Release);
    }
}

impl<T: Copy + Default, const N: usize> Default for SpscRingBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TelemetryStore {
    ring: SpscRingBuffer<ClickRecord, TELEMETRY_RING_SIZE>,
    total_clicks: AtomicU64,
    missed_deadlines: AtomicU64,
}

impl Default for TelemetryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryStore {
    pub fn new() -> Self {
        Self {
            ring: SpscRingBuffer::new(),
            total_clicks: AtomicU64::new(0),
            missed_deadlines: AtomicU64::new(0),
        }
    }

    pub fn record_click(&self, record: ClickRecord) {
        self.ring.push(record);
        self.total_clicks.fetch_add(1, Ordering::Relaxed);
        if record.missed_deadline {
            self.missed_deadlines.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn drain_into(&self, out: &mut Vec<ClickRecord>) -> usize {
        let mut drained = 0;
        while let Some(record) = self.ring.pop() {
            out.push(record);
            drained += 1;
        }
        drained
    }

    pub fn total_clicks(&self) -> u64 {
        self.total_clicks.load(Ordering::Relaxed)
    }

    pub fn missed_deadlines(&self) -> u64 {
        self.missed_deadlines.load(Ordering::Relaxed)
    }

    pub fn dropped_records(&self) -> u64 {
        self.ring.dropped()
    }

    pub fn reset(&self) {
        self.ring.clear();
        self.total_clicks.store(0, Ordering::Relaxed);
        self.missed_deadlines.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryStats {
    pub sample_count: u64,
    pub total_clicks: u64,
    pub missed_deadlines: u64,
    pub elapsed_ms: f64,
    pub actual_cps: f64,
    pub mean_interval_ms: f64,
    pub sigma_interval_ms: f64,
    pub consistency_pct: f64,
    pub min_interval_ms: f64,
    pub max_interval_ms: f64,
    pub histogram: [u32; HISTOGRAM_BINS],
    pub histogram_low_ms: f64,
    pub histogram_high_ms: f64,
}

/// Pure aggregation over a drained window of records. No OS calls so it can be
/// unit tested deterministically.
pub fn compute_stats(
    window: &[ClickRecord],
    total_clicks: u64,
    missed_deadlines: u64,
    elapsed_ms: f64,
) -> TelemetryStats {
    let mut stats = TelemetryStats {
        sample_count: window.len() as u64,
        total_clicks,
        missed_deadlines,
        elapsed_ms,
        histogram: [0; HISTOGRAM_BINS],
        ..Default::default()
    };

    if window.is_empty() || elapsed_ms <= 0.0 {
        return stats;
    }

    stats.actual_cps = total_clicks as f64 / (elapsed_ms / 1000.0);

    let sum: f64 = window.iter().map(|r| r.interval_ms).sum();
    stats.mean_interval_ms = sum / window.len() as f64;
    stats.min_interval_ms = window
        .iter()
        .map(|r| r.interval_ms)
        .fold(f64::INFINITY, f64::min);
    stats.max_interval_ms = window
        .iter()
        .map(|r| r.interval_ms)
        .fold(f64::NEG_INFINITY, f64::max);

    let squared: f64 = window
        .iter()
        .map(|r| {
            let delta = r.interval_ms - stats.mean_interval_ms;
            delta * delta
        })
        .sum();
    if window.len() > 1 {
        stats.sigma_interval_ms = (squared / (window.len() - 1) as f64).sqrt();
    }

    let low_band = stats.mean_interval_ms * 0.9;
    let high_band = stats.mean_interval_ms * 1.1;
    let within_band = window
        .iter()
        .filter(|r| r.interval_ms >= low_band && r.interval_ms <= high_band)
        .count();
    stats.consistency_pct = 100.0 * within_band as f64 / window.len() as f64;

    stats.histogram_low_ms = stats.min_interval_ms;
    stats.histogram_high_ms = stats.max_interval_ms;
    let span = stats.histogram_high_ms - stats.histogram_low_ms;
    if span <= 0.0 {
        stats.histogram[0] = window.len() as u32;
        return stats;
    }
    let bin_width = span / HISTOGRAM_BINS as f64;
    for record in window {
        let mut bin =
            ((record.interval_ms - stats.histogram_low_ms) / bin_width) as usize;
        if bin >= HISTOGRAM_BINS {
            bin = HISTOGRAM_BINS - 1;
        }
        stats.histogram[bin] += 1;
    }
    stats
}
