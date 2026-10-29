#[path = "../src/coordinates.rs"]
mod coordinates;

use coordinates::{make_lparam, normalize_axis};

#[test]
fn normalized_coordinates_round_trip_every_pixel_for_representative_desktops() {
    // Windows' 65,536-based mapping, modelled independently of the inverse.
    for length in (1..=1_024).chain([1_920, 2_560, 3_840, 7_680, 15_360, 32_767, 65_535, 65_536]) {
        for origin in [0, -1_920, i32::MIN, i32::MAX] {
            for offset in 0..length {
                let position = i64::from(origin) + i64::from(offset);
                let normalized = normalize_axis(position, origin, length).expect("valid pixel");
                assert!((0..=65_535).contains(&normalized));
                let mapped = i64::from(normalized) * i64::from(length) / 65_536;
                assert_eq!(
                    mapped,
                    i64::from(offset),
                    "length={length}, origin={origin}, offset={offset}, normalized={normalized}"
                );
            }
        }
    }
}

#[test]
fn normalized_coordinates_cover_edges_and_middle_at_every_supported_extent() {
    for length in 1..=65_536 {
        for offset in [
            0,
            1.min(length - 1),
            length / 2,
            (length - 2).max(0),
            length - 1,
        ] {
            let normalized = normalize_axis(i64::from(offset), 0, length).expect("valid pixel");
            assert_eq!(
                i64::from(normalized) * i64::from(length) / 65_536,
                i64::from(offset),
                "length={length}, offset={offset}"
            );
        }
    }
}

#[test]
fn rejects_bad_metrics_out_of_bounds_and_extreme_coordinates_without_overflow() {
    for length in [i32::MIN, -1, 0, 65_537, i32::MAX] {
        assert_eq!(normalize_axis(0, 0, length), None);
    }
    for position in [i64::MIN, -1_921, 1_920, i64::MAX] {
        assert_eq!(normalize_axis(position, -1_920, 3_840), None);
    }
    assert!(normalize_axis(-1_920, -1_920, 3_840).is_some());
    assert!(normalize_axis(1_919, -1_920, 3_840).is_some());
    assert!(normalize_axis(i64::from(i32::MAX) + 1, i32::MAX, 2).is_some());
}

#[test]
fn client_lparam_keeps_x_and_y_as_independent_signed_words() {
    for (x, y) in [
        (-1, 2),
        (-32_768, 32_767),
        (32_767, -32_768),
        (0, -1),
        (65_536, 12_345),
        (i32::MIN, i32::MAX),
    ] {
        let packed = make_lparam(x, y) as u32;
        assert_eq!((packed & 0xffff) as u16, x as u16, "x={x}, y={y}");
        assert_eq!((packed >> 16) as u16, y as u16, "x={x}, y={y}");
    }
}
