#![allow(dead_code)]

#[path = "../src/icon.rs"]
mod icon;

use icon::{app_icon_rgba, mog_icon_rgba, ICON_SIZE};

#[test]
fn dimensions_and_buffer_length_are_correct() {
    let (rgba, width, height) = app_icon_rgba();
    assert_eq!(width, ICON_SIZE);
    assert_eq!(height, ICON_SIZE);
    assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
}

#[test]
fn corners_are_transparent_and_center_is_accent() {
    let (rgba, width, _) = app_icon_rgba();
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        [
            rgba[index],
            rgba[index + 1],
            rgba[index + 2],
            rgba[index + 3],
        ]
    };

    // The four outer corners lie outside the rounded square.
    for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
        assert_eq!(pixel(x, y)[3], 0, "corner ({x}, {y}) must be transparent");
    }

    // The exact center is the solid accent dot.
    let center = pixel(16, 16);
    assert_eq!(center[3], 255);
    assert!(center[2] > center[0], "center dot should read blue-accent");
}

#[test]
fn icon_is_symmetric_on_both_axes() {
    let (rgba, width, height) = app_icon_rgba();
    let alpha = |x: u32, y: u32| rgba[((y * width + x) * 4 + 3) as usize];
    for y in 0..height {
        for x in 0..width / 2 {
            assert_eq!(
                alpha(x, y),
                alpha(width - 1 - x, y),
                "horizontal symmetry at {x},{y}"
            );
        }
    }
    for y in 0..height / 2 {
        for x in 0..width {
            assert_eq!(
                alpha(x, y),
                alpha(x, height - 1 - y),
                "vertical symmetry at {x},{y}"
            );
        }
    }
}

#[test]
fn accent_pixels_and_background_pixels_are_both_present() {
    let (rgba, _, _) = app_icon_rgba();
    let mut accent_pixels = 0usize;
    let mut dark_background_pixels = 0usize;
    for chunk in rgba.chunks_exact(4) {
        let [r, g, b, a] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        if a == 0 {
            continue;
        }
        if b > 180 && r < 120 {
            accent_pixels += 1;
        }
        if r <= 20 && g <= 24 && b <= 28 {
            dark_background_pixels += 1;
        }
    }
    assert!(
        accent_pixels >= 40,
        "expected a visible crosshair, got {accent_pixels}"
    );
    assert!(
        dark_background_pixels >= 100,
        "expected a dark tile background"
    );
}

#[test]
fn mog_icon_shares_geometry_but_reads_hot_pink() {
    let (default_rgba, dw, dh) = app_icon_rgba();
    let (mog_rgba, mw, mh) = mog_icon_rgba();
    assert_eq!((mw, mh), (dw, dh));
    assert_eq!(mog_rgba.len(), default_rgba.len());

    // Identical geometry: the alpha channel (shape + anti-aliasing) must be
    // byte-identical between the two color ways.
    for (i, (d, m)) in default_rgba
        .chunks_exact(4)
        .zip(mog_rgba.chunks_exact(4))
        .enumerate()
    {
        assert_eq!(d[3], m[3], "alpha diverged at pixel {i}");
    }

    // The center dot is the solid Mog accent: hot pink, not blue.
    let index = ((16 * mw + 16) * 4) as usize;
    let center = [
        mog_rgba[index],
        mog_rgba[index + 1],
        mog_rgba[index + 2],
        mog_rgba[index + 3],
    ];
    assert_eq!(center[3], 255);
    assert_eq!(
        (center[0], center[1], center[2]),
        (255, 46, 136),
        "center dot must be the exact Mog accent"
    );
}

#[test]
fn mog_icon_is_visibly_different_from_the_default_icon() {
    let (default_rgba, _, _) = app_icon_rgba();
    let (mog_rgba, _, _) = mog_icon_rgba();
    let mut differing = 0usize;
    for (d, m) in default_rgba.chunks_exact(4).zip(mog_rgba.chunks_exact(4)) {
        if d[..3] != m[..3] {
            differing += 1;
        }
    }
    assert!(
        differing >= 200,
        "expected a clearly recolored icon, only {differing} pixels differ"
    );
}
