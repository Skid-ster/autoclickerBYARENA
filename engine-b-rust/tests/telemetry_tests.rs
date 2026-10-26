#[path = "../src/telemetry.rs"]
mod telemetry;

use telemetry::{
    compute_stats, ClickRecord, SpscRingBuffer, TelemetryStore, HISTOGRAM_BINS,
};

#[test]
fn fifo_ordering_and_capacity() {
    let buffer = SpscRingBuffer::<i32, 8>::new();
    assert!(buffer.is_empty());
    assert_eq!(buffer.len(), 0);
    assert_eq!(buffer.capacity(), 8);

    for value in 1..=5 {
        assert!(buffer.push(value));
    }
    assert_eq!(buffer.len(), 5);
    for expected in 1..=5 {
        assert_eq!(buffer.pop(), Some(expected));
    }
    assert!(buffer.is_empty());
    assert_eq!(buffer.pop(), None);
}

#[test]
fn wrapping_reuses_slots() {
    let buffer = SpscRingBuffer::<i32, 8>::new();
    for round in 0..4 {
        for i in 0..6 {
            assert!(buffer.push(round * 100 + i));
        }
        for i in 0..6 {
            assert_eq!(buffer.pop(), Some(round * 100 + i));
        }
        assert!(buffer.is_empty());
    }
}

#[test]
fn full_buffer_overwrites_oldest() {
    let buffer = SpscRingBuffer::<i32, 8>::new();
    for value in 1..=8 {
        assert!(buffer.push(value));
    }
    assert_eq!(buffer.dropped(), 0);
    assert!(!buffer.push(99));
    assert_eq!(buffer.dropped(), 1);
    assert_eq!(buffer.len(), 8);
    assert_eq!(buffer.pop(), Some(2));
    for expected in 3..=8 {
        assert_eq!(buffer.pop(), Some(expected));
    }
    assert_eq!(buffer.pop(), Some(99));
    assert!(buffer.is_empty());

    for value in 0..8 {
        assert!(buffer.push(1000 + value));
    }
    assert!(!buffer.push(200));
    assert!(!buffer.push(201));
    assert_eq!(buffer.dropped(), 3);
}

#[test]
fn clear_resets_indices_and_dropped() {
    let buffer = SpscRingBuffer::<i32, 4>::new();
    for value in 0..10 {
        buffer.push(value);
    }
    assert_eq!(buffer.dropped(), 6);
    buffer.clear();
    assert!(buffer.is_empty());
    assert_eq!(buffer.dropped(), 0);
    assert_eq!(buffer.pop(), None);
}

#[test]
fn telemetry_store_counters_and_drain() {
    let store = TelemetryStore::new();
    for seq in 1..=12u64 {
        store.record_click(ClickRecord {
            seq,
            interval_ms: 10.0 + (seq % 3) as f64,
            hold_ms: 4.0,
            deadline_error_ms: 0.1,
            missed_deadline: seq % 4 == 0,
        });
    }
    assert_eq!(store.total_clicks(), 12);
    assert_eq!(store.missed_deadlines(), 3);

    let mut drained = Vec::new();
    let count = store.drain_into(&mut drained);
    assert_eq!(count, 12);
    assert_eq!(drained.first().unwrap().seq, 1);
    assert_eq!(drained.last().unwrap().seq, 12);

    store.reset();
    assert_eq!(store.total_clicks(), 0);
    assert_eq!(store.missed_deadlines(), 0);
    assert_eq!(store.dropped_records(), 0);
}

#[test]
fn stats_moments_consistency_and_histogram() {
    let mut records = Vec::new();
    for i in 0..100 {
        records.push(ClickRecord {
            seq: i + 1,
            interval_ms: if i % 2 == 0 { 90.0 } else { 110.0 },
            hold_ms: 5.0,
            deadline_error_ms: 0.0,
            missed_deadline: false,
        });
    }
    let stats = compute_stats(&records, 100, 0, 10_000.0);
    assert_eq!(stats.sample_count, 100);
    assert!((stats.mean_interval_ms - 100.0).abs() < 1e-9);
    assert_eq!(stats.actual_cps, 10.0);
    assert!(stats.sigma_interval_ms > 9.0);
    assert!((0.0..=100.0).contains(&stats.consistency_pct));
    let histogram_total: u32 = stats.histogram.iter().sum();
    assert_eq!(histogram_total, 100);
    assert_eq!(stats.histogram.len(), HISTOGRAM_BINS);
}

#[test]
fn empty_window_yets_zeroed_stats() {
    let stats = compute_stats(&[], 0, 0, 0.0);
    assert_eq!(stats.sample_count, 0);
    assert_eq!(stats.actual_cps, 0.0);
}
