#![allow(dead_code)]

#[path = "../src/texture.rs"]
mod texture;

#[test]
fn material_texture_is_deterministic_and_strictly_bounded() {
    let first = texture::material_alpha_tile();
    let second = texture::material_alpha_tile();
    assert_eq!(first, second);
    assert_eq!(first.len(), texture::TILE_SIDE * texture::TILE_SIDE);
    let (min, max) = first
        .iter()
        .fold((u8::MAX, u8::MIN), |(low, high), &value| {
            (low.min(value), high.max(value))
        });
    assert!(max - min > 80, "grain must contain multiple tonal values");
}

#[test]
fn material_texture_has_no_empty_rows_or_columns() {
    let tile = texture::material_alpha_tile();
    let side = texture::TILE_SIDE;
    for index in 0..side {
        assert!(tile[index * side..(index + 1) * side]
            .iter()
            .any(|&value| value > 0));
        assert!((0..side).any(|row| tile[row * side + index] > 0));
    }
}
