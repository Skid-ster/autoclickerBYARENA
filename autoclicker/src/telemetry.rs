//! Lock-free telemetry pipeline.
//!
//! The click worker is the sole producer and the UI thread the sole consumer.
//! Head/tail indices live on separate cache lines and are coordinated with
//! acquire/release atomics. Every slot is itself atomic, so the whole buffer
//! is `Send + Sync` without any `unsafe` code or mutexes: the single-producer
//! / single-consumer protocol guarantees no two threads ever touch the same
//! slot at the same time. A full buffer drops the newest record and
//! increments the dropped counter instead of ever blocking the producer,
//! keeping `popped + dropped == pushed` exact under every interleaving.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};

pub const TELEMETRY_RING_SIZE: usize = 4096;
pub const HISTOGRAM_BINS: usize = 24;
pub const GRAPH_HISTORY_SIZE: usize = 240;
pub const SLIDING_WINDOW_LIMIT: usize = 2000;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClickRecord {
    pub seq: u64,
    pub interval_ms: f64,
    pub hold_ms: f64,
    pub deadline_error_ms: f64,
    pub missed_deadline: bool,
}

#[repr(align(64))]
struct CacheLine<T>(T);

/// Atomic storage for one ring slot. Implementors are inherently
/// `Send + Sync`, which is what lets the whole SPSC buffer cross threads
/// safely without any `unsafe` synchronization wrapper.
pub trait Atom: Send + Sync + Default {
    type Value: Copy + Default + Send;

    fn write(&self, value: Self::Value);
    fn read(&self) -> Self::Value;
}

impl Atom for AtomicI32 {
    type Value = i32;

    fn write(&self, value: i32) {
        self.store(value, Ordering::Relaxed);
    }

    fn read(&self) -> i32 {
        self.load(Ordering::Relaxed)
    }
}

/// Atomic representation of a [`ClickRecord`]. Floating-point fields travel
/// as their bit patterns, which round-trips exactly.
#[derive(Default)]
pub struct ClickRecordAtom {
    seq: AtomicU64,
    interval_ms: AtomicU64,
    hold_ms: AtomicU64,
    deadline_error_ms: AtomicU64,
    missed_deadline: AtomicBool,
}

impl Atom for ClickRecordAtom {
    type Value = ClickRecord;

    fn write(&self, value: ClickRecord) {
        self.seq.store(value.seq, Ordering::Relaxed);
        self.interval_ms
            .store(value.interval_ms.to_bits(), Ordering::Relaxed);
        self.hold_ms
            .store(value.hold_ms.to_bits(), Ordering::Relaxed);
        self.deadline_error_ms
            .store(value.deadline_error_ms.to_bits(), Ordering::Relaxed);
        self.missed_deadline
            .store(value.missed_deadline, Ordering::Relaxed);
    }

    fn read(&self) -> ClickRecord {
        ClickRecord {
            seq: self.seq.load(Ordering::Relaxed),
            interval_ms: f64::from_bits(self.interval_ms.load(Ordering::Relaxed)),
            hold_ms: f64::from_bits(self.hold_ms.load(Ordering::Relaxed)),
            deadline_error_ms: f64::from_bits(self.deadline_error_ms.load(Ordering::Relaxed)),
            missed_deadline: self.missed_deadline.load(Ordering::Relaxed),
        }
    }
}

pub struct SpscRingBuffer<A: Atom, const N: usize> {
    head: CacheLine<AtomicUsize>,
    tail: CacheLine<AtomicUsize>,
    dropped: CacheLine<AtomicU64>,
    slots: Box<[A; N]>,
}

impl<A: Atom, const N: usize> SpscRingBuffer<A, N> {
    pub fn new() -> Self {
        assert!(
            N >= 2 && N.is_power_of_two(),
            "N must be a power of two >= 2"
        );
        let mut slots: Vec<A> = Vec::with_capacity(N);
        for _ in 0..N {
            slots.push(A::default());
        }
        let boxed: Box<[A]> = slots.into_boxed_slice();
        let slots: Box<[A; N]> = match boxed.try_into() {
            Ok(array) => array,
            Err(_) => unreachable!("allocated exactly N slots"),
        };
        Self {
            head: CacheLine(AtomicUsize::new(0)),
            tail: CacheLine(AtomicUsize::new(0)),
            dropped: CacheLine(AtomicU64::new(0)),
            slots,
        }
    }

    /// Producer only. Returns `false` when the buffer is full; the newest
    /// sample is then dropped and the dropped-sample counter increments.
    ///
    /// The producer only ever writes `head`, never `tail`, so no index is
    /// mutated by both threads and `popped + dropped == pushed` stays exact
    /// under any interleaving. This drop-newest policy is the race-free SPSC
    /// design and still guarantees the click worker never blocks on the UI
    /// consumer.
    pub fn push(&self, value: A::Value) -> bool {
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Acquire);
        if head - tail >= N {
            self.dropped.0.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.slots[head & (N - 1)].write(value);
        self.head.0.store(head + 1, Ordering::Release);
        true
    }

    /// Producer only, back-pressured variant of [`push`]: returns `false`
    /// without counting a dropped sample, so callers can retry after the
    /// consumer drains a slot.
    pub fn try_push(&self, value: A::Value) -> bool {
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Acquire);
        if head - tail >= N {
            return false;
        }
        self.slots[head & (N - 1)].write(value);
        self.head.0.store(head + 1, Ordering::Release);
        true
    }

    /// Consumer only.
    pub fn pop(&self) -> Option<A::Value> {
        let tail = self.tail.0.load(Ordering::Relaxed);
        let head = self.head.0.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let value = self.slots[tail & (N - 1)].read();
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

impl<A: Atom, const N: usize> Default for SpscRingBuffer<A, N> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TelemetryStore {
    ring: SpscRingBuffer<ClickRecordAtom, TELEMETRY_RING_SIZE>,
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
        let mut bin = ((record.interval_ms - stats.histogram_low_ms) / bin_width) as usize;
        if bin >= HISTOGRAM_BINS {
            bin = HISTOGRAM_BINS - 1;
        }
        stats.histogram[bin] += 1;
    }
    stats
}
