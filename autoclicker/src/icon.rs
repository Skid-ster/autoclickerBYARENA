//! Procedurally generated application icon.
//!
//! The icon is drawn at runtime from plain pixel math, so the repository
//! needs no binary asset and every pixel is deterministic and testable. It
//! depicts a crosshair target: two concentric accent rings, a solid center
//! dot and four cardinal ticks on a dark rounded square. The module stays
//! platform-independent (it returns raw RGBA bytes); the Windows entry point
//! wraps those bytes in an `egui::IconData`.

/// Edge length of the square icon in pixels.
pub const ICON_SIZE: u32 = 32;

const BG_R: u8 = 14;
const BG_G: u8 = 18;
const BG_B: u8 = 22;
const ACCENT_R: u8 = 76;
const ACCENT_G: u8 = 141;
const ACCENT_B: u8 = 255;
// MogClicker-theme icon colors: the hot-pink accent on the deep-purple
// face used by the in-app MOG palette, so the taskbar mogs along.
const MOG_ACCENT_R: u8 = 255;
const MOG_ACCENT_G: u8 = 46;
const MOG_ACCENT_B: u8 = 136;
const MOG_BG_R: u8 = 24;
const MOG_BG_G: u8 = 16;
const MOG_BG_B: u8 = 32;
const CORNER_RADIUS: f64 = 7.0;
const OUTER_RING_RADIUS: f64 = 12.0;
const INNER_RING_RADIUS: f64 = 7.0;
const CENTER_DOT_RADIUS: f64 = 2.3;
const RING_HALF_WIDTH: f64 = 1.1;
const TICK_HALF_WIDTH: f64 = 1.0;
const TICK_INNER: f64 = 14.2;
const TICK_OUTER: f64 = 16.0;

fn rounded_square_coverage(x: f64, y: f64) -> f64 {
    // Signed-distance field for a rounded square centered on the icon.
    let half = ICON_SIZE as f64 / 2.0;
    let qx = (x - half).abs() - (half - CORNER_RADIUS);
    let qy = (y - half).abs() - (half - CORNER_RADIUS);
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    let inside = qx.max(qy).min(0.0);
    let distance = outside + inside - CORNER_RADIUS;
    (0.5 - distance).clamp(0.0, 1.0)
}

fn ring_coverage(distance: f64, radius: f64) -> f64 {
    let error = (distance - radius).abs();
    (RING_HALF_WIDTH + 0.5 - error).clamp(0.0, 1.0)
}

fn dot_coverage(distance: f64) -> f64 {
    (CENTER_DOT_RADIUS + 0.5 - distance).clamp(0.0, 1.0)
}

fn tick_coverage(x: f64, y: f64) -> f64 {
    let dx = x - ICON_SIZE as f64 / 2.0;
    let dy = y - ICON_SIZE as f64 / 2.0;
    let along = dx.abs().max(dy.abs());
    let across = dx.abs().min(dy.abs());
    let radial = (TICK_HALF_WIDTH + 0.5 - across).clamp(0.0, 1.0);
    let length = if (TICK_INNER..=TICK_OUTER).contains(&along) {
        1.0
    } else if along < TICK_INNER {
        (along - (TICK_INNER - 0.5)).clamp(0.0, 1.0)
    } else {
        (TICK_OUTER + 0.5 - along).clamp(0.0, 1.0)
    };
    radial * length
}

/// Builds the 32x32 icon as unmultiplied RGBA bytes (4 bytes per pixel,
/// row-major from the top-left corner) in the default blue-on-dark colors.
pub fn app_icon_rgba() -> (Vec<u8>, u32, u32) {
    themed_icon_rgba([ACCENT_R, ACCENT_G, ACCENT_B], [BG_R, BG_G, BG_B])
}

/// The same crosshair icon in MogClicker colors (hot pink on deep purple),
/// swapped in at runtime while the meme theme is active.
pub fn mog_icon_rgba() -> (Vec<u8>, u32, u32) {
    themed_icon_rgba(
        [MOG_ACCENT_R, MOG_ACCENT_G, MOG_ACCENT_B],
        [MOG_BG_R, MOG_BG_G, MOG_BG_B],
    )
}

/// Shared pixel math: identical geometry for every color way, so all icon
/// invariants (shape, symmetry, transparency) hold for each variant.
fn themed_icon_rgba(accent_rgb: [u8; 3], bg_rgb: [u8; 3]) -> (Vec<u8>, u32, u32) {
    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    let center = (ICON_SIZE as f64 - 1.0) / 2.0;

    for row in 0..ICON_SIZE {
        for col in 0..ICON_SIZE {
            // Sample pixel centers rather than integer grid corners.
            let x = col as f64 + 0.5;
            let y = row as f64 + 0.5;
            let distance = (x - center).hypot(y - center);

            let background = rounded_square_coverage(x, y);
            let accent = ring_coverage(distance, OUTER_RING_RADIUS)
                .max(ring_coverage(distance, INNER_RING_RADIUS))
                .max(dot_coverage(distance))
                .max(tick_coverage(x, y))
                .min(1.0);

            // Composite the accent over the dark background inside the
            // rounded square; pixels outside the square stay transparent.
            let alpha = background.max(accent);
            let mix_r = accent_rgb[0] as f64 * accent + bg_rgb[0] as f64 * (1.0 - accent);
            let mix_g = accent_rgb[1] as f64 * accent + bg_rgb[1] as f64 * (1.0 - accent);
            let mix_b = accent_rgb[2] as f64 * accent + bg_rgb[2] as f64 * (1.0 - accent);

            rgba.push((mix_r * alpha).round() as u8);
            rgba.push((mix_g * alpha).round() as u8);
            rgba.push((mix_b * alpha).round() as u8);
            rgba.push((alpha * 255.0).round() as u8);
        }
    }

    (rgba, ICON_SIZE, ICON_SIZE)
}
