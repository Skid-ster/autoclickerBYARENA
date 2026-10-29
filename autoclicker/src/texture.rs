//! Small deterministic, tileable material grain for the Custom appearance.
//! This is generated locally; it needs no downloaded assets or animation.

pub const TILE_SIDE: usize = 128;

fn hash(x: u32, y: u32, seed: u32) -> u8 {
    let mut value = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B) ^ seed;
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    (value ^ (value >> 16)) as u8
}

fn smooth_noise(x: usize, y: usize, cell: usize, seed: u32) -> f32 {
    let grid = TILE_SIDE / cell;
    let x0 = x / cell;
    let y0 = y / cell;
    let x1 = (x0 + 1) % grid;
    let y1 = (y0 + 1) % grid;
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let tx = smooth((x % cell) as f32 / cell as f32);
    let ty = smooth((y % cell) as f32 / cell as f32);
    let value = |a: usize, b: usize| f32::from(hash(a as u32, b as u32, seed));
    let top = value(x0, y0) * (1.0 - tx) + value(x1, y0) * tx;
    let bottom = value(x0, y1) * (1.0 - tx) + value(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// Returns one bounded alpha tile. Fine stochastic grain and two smoothly
/// varying, periodic scales give the surface a quiet paper/slate character;
/// the palette controls the extremely low final opacity separately.
pub fn material_alpha_tile() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(TILE_SIDE * TILE_SIDE);
    for y in 0..TILE_SIDE {
        for x in 0..TILE_SIDE {
            let grain = f32::from(hash(x as u32, y as u32, 0x719A_3B5D));
            let medium = smooth_noise(x, y, 8, 0x0C47_1245);
            let broad = smooth_noise(x, y, 32, 0x612F_58B3);
            pixels.push((grain * 0.55 + medium * 0.30 + broad * 0.15).round() as u8);
        }
    }
    pixels
}
