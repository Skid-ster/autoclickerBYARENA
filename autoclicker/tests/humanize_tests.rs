// The path-included modules form a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/config.rs"]
mod config;
#[path = "../src/humanize.rs"]
mod humanize;

use config::EngineConfig;
use humanize::GaussianHumanizer;

#[test]
fn box_muller_is_deterministic_and_guarded() {
    let a = GaussianHumanizer::box_muller_standard(0.5, 0.25);
    let b = GaussianHumanizer::box_muller_standard(0.5, 0.25);
    assert!(a.is_finite());
    assert!((a - b).abs() < 1e-12);
    assert!(GaussianHumanizer::box_muller_standard(0.0, 0.5).is_finite());
}

#[test]
fn seeded_rng_is_reproducible() {
    let first = GaussianHumanizer::with_seed(0xdead_beef);
    let second = GaussianHumanizer::with_seed(0xdead_beef);
    let config = EngineConfig::default();
    for _ in 0..100 {
        let a = first.next_interval_ms(&config);
        let b = second.next_interval_ms(&config);
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

#[test]
fn interval_distribution_matches_moments() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 100.0;
    config.sigma_ms = 5.0;
    config.sigma_percent = 0.0;
    config.min_interval_ms = 1.0;
    config.max_interval_ms = 1000.0;
    config.humanize_enabled = true;

    let humanizer = GaussianHumanizer::with_seed(7);
    let samples = 50_000;
    let mut sum = 0.0;
    let mut squared = 0.0;
    for _ in 0..samples {
        let value = humanizer.next_interval_ms(&config);
        assert!(value.is_finite());
        assert!(value >= config.min_interval_ms && value <= config.max_interval_ms);
        sum += value;
        squared += value * value;
    }
    let mean = sum / samples as f64;
    let variance = squared / samples as f64 - mean * mean;
    let sigma = variance.max(0.0).sqrt();
    assert!((mean - 100.0).abs() < 0.25, "mean was {mean}");
    assert!((sigma - 5.0).abs() < 0.5, "sigma was {sigma}");
}

#[test]
fn hold_distribution_is_clamped() {
    let mut config = EngineConfig::default();
    config.hold_mean_ms = 40.0;
    config.hold_sigma_ms = 6.0;
    config.hold_min_ms = 10.0;
    config.hold_max_ms = 90.0;

    let humanizer = GaussianHumanizer::with_seed(11);
    let mut sum = 0.0;
    for _ in 0..20_000 {
        let hold = humanizer.next_hold_ms(&config);
        assert!(hold >= config.hold_min_ms && hold <= config.hold_max_ms);
        sum += hold;
    }
    let mean = sum / 20_000.0;
    assert!((mean - 40.0).abs() < 0.3, "hold mean was {mean}");
}

#[test]
fn zero_sigma_and_disabled_humanization_are_exact() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 75.0;
    config.sigma_ms = 0.0;
    config.sigma_percent = 0.0;
    config.humanize_enabled = true;
    let humanizer = GaussianHumanizer::with_seed(3);
    for _ in 0..100 {
        assert!((humanizer.next_interval_ms(&config) - 75.0).abs() < 1e-12);
    }

    config.humanize_enabled = false;
    config.sigma_ms = 9.0;
    for _ in 0..100 {
        assert!((humanizer.next_interval_ms(&config) - 75.0).abs() < 1e-12);
    }
}

#[test]
fn aggressive_clamp_never_negative() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 5.0;
    config.sigma_ms = 50.0;
    config.min_interval_ms = 2.0;
    config.max_interval_ms = 8.0;
    let humanizer = GaussianHumanizer::with_seed(99);
    for _ in 0..30_000 {
        let value = humanizer.next_interval_ms(&config);
        assert!((2.0..=8.0).contains(&value));
    }
}

#[test]
fn zero_offset_is_exact_base_interval() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 120.0;
    config.random_offset_ms = 0.0;
    let humanizer = GaussianHumanizer::with_seed(7);
    for _ in 0..100 {
        assert!((humanizer.next_offset_interval_ms(&config) - 120.0).abs() < 1e-12);
    }
}

#[test]
fn random_offset_stays_inside_bounds() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 200.0;
    config.random_offset_ms = 40.0;
    config.min_interval_ms = 1.0;
    config.max_interval_ms = 10_000.0;
    let humanizer = GaussianHumanizer::with_seed(11);
    for _ in 0..50_000 {
        let value = humanizer.next_offset_interval_ms(&config);
        assert!(
            (160.0..=240.0).contains(&value),
            "offset sample {value} escaped the +/-40 ms window"
        );
    }
}

#[test]
fn random_offset_uses_uniform_distribution() {
    let mut config = EngineConfig::default();
    config.base_interval_ms = 1_000.0;
    config.random_offset_ms = 100.0;
    let humanizer = GaussianHumanizer::with_seed(23);
    let samples: Vec<f64> = (0..20_000)
        .map(|_| humanizer.next_offset_interval_ms(&config))
        .collect();
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    // Uniform [900, 1100] has mean 1000; generous 10 ms tolerance.
    assert!((mean - 1_000.0).abs() < 10.0, "mean was {mean}");
    let below_mid = samples.iter().filter(|v| **v < 1_000.0).count();
    let ratio = below_mid as f64 / samples.len() as f64;
    assert!((0.45..=0.55).contains(&ratio), "split ratio was {ratio}");
}
