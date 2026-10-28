// The path-included module forms a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/telemetry.rs"]
mod telemetry;

use std::sync::atomic::AtomicI32;

use telemetry::{
    compute_stats, ClickRecord, ClickRecordAtom, SpscRingBuffer, TelemetryStore, HISTOGRAM_BINS,
};

#[test]
fn fifo_ordering_and_capacity() {
    let buffer = SpscRingBuffer::<AtomicI32, 8>::new();
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
    let buffer = SpscRingBuffer::<AtomicI32, 8>::new();
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
fn full_buffer_drops_newest_and_keeps_oldest() {
    let buffer = SpscRingBuffer::<AtomicI32, 8>::new();
    for value in 1..=8 {
        assert!(buffer.push(value));
    }
    assert_eq!(buffer.dropped(), 0);
    // A full buffer rejects the newest sample instead of evicting the oldest,
    // so the producer never has to mutate the consumer-owned tail index.
    assert!(!buffer.push(99));
    assert_eq!(buffer.dropped(), 1);
    assert_eq!(buffer.len(), 8);
    assert_eq!(buffer.pop(), Some(1));
    for expected in 2..=8 {
        assert_eq!(buffer.pop(), Some(expected));
    }
    assert_eq!(buffer.pop(), None);
    assert!(buffer.is_empty());

    for value in 0..8 {
        assert!(buffer.push(1000 + value));
    }
    assert!(!buffer.push(200));
    assert!(!buffer.push(201));
    assert_eq!(buffer.dropped(), 3);
    assert_eq!(buffer.len(), 8);
    assert_eq!(buffer.pop(), Some(1000));
}

#[test]
fn clear_resets_indices_and_dropped() {
    let buffer = SpscRingBuffer::<AtomicI32, 4>::new();
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
            actual_gap_ms: 10.0 + (seq % 3) as f64,
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
            actual_gap_ms: if i % 2 == 0 { 90.0 } else { 110.0 },
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

#[test]
fn ring_buffer_is_send_sync_and_survives_real_threads() {
    // Compile-time guarantee: the telemetry pipeline must be shareable with
    // the click worker thread without any unsafe synchronization wrapper.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SpscRingBuffer<ClickRecordAtom, 64>>();
    assert_send_sync::<TelemetryStore>();

    use std::sync::Arc;
    use std::thread;

    // Deterministic SPSC traffic across real threads: the producer applies
    // back pressure when the eight-slot ring is full, so no sample is lost
    // and every value arrives exactly once and in order.
    let ring = Arc::new(SpscRingBuffer::<AtomicI32, 8>::new());
    let producer_ring = Arc::clone(&ring);
    let producer = thread::spawn(move || {
        for value in 0..10_000i32 {
            while !producer_ring.try_push(value) {
                std::hint::spin_loop();
            }
        }
    });

    let mut expected = 0i32;
    while expected < 10_000 {
        match ring.pop() {
            Some(value) => {
                assert_eq!(value, expected);
                expected += 1;
            }
            None => std::hint::spin_loop(),
        }
    }
    producer.join().expect("producer thread panicked");
    assert_eq!(expected, 10_000);
    assert_eq!(ring.dropped(), 0);
    assert!(ring.is_empty());

    // TelemetryStore across threads: atomic counters are exact even when the
    // bounded ring legitimately overwrites under scheduler jitter, and every
    // record the consumer observes must form a monotonic subsequence.
    const TOTAL: u64 = 50_000;
    let store = Arc::new(TelemetryStore::new());
    let producer_store = Arc::clone(&store);
    let producer = thread::spawn(move || {
        for seq in 1..=TOTAL {
            producer_store.record_click(ClickRecord {
                seq,
                interval_ms: 10.0 + (seq % 7) as f64,
                hold_ms: 4.0,
                deadline_error_ms: 0.2,
                actual_gap_ms: 10.0 + (seq % 7) as f64,
                missed_deadline: seq % 50 == 0,
            });
        }
    });

    let mut received = Vec::new();
    while !producer.is_finished() {
        store.drain_into(&mut received);
        std::hint::spin_loop();
    }
    producer.join().expect("producer thread panicked");
    // Final drain after the producer has fully published.
    store.drain_into(&mut received);

    assert!(!received.is_empty());
    assert_eq!(store.total_clicks(), TOTAL);
    assert_eq!(store.missed_deadlines(), TOTAL / 50);
    assert_eq!(received.len() as u64 + store.dropped_records(), TOTAL);
    let mut previous = 0u64;
    for record in &received {
        assert!(record.seq > previous);
        assert!(record.seq <= TOTAL);
        previous = record.seq;
    }
}
