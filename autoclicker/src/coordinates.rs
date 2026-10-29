//! Pure coordinate conversions used by the Win32 input wrappers.

/// Maximum number of distinct absolute SendInput coordinates on one axis.
pub const ABSOLUTE_COORDINATE_COUNT: i32 = 65_536;

/// Convert a screen pixel to a normalized absolute coordinate. The virtual
/// desktop origin may be negative. Reject invalid sizes and positions rather
/// than letting Windows clamp an unintended click to a different location.
///
/// Windows' absolute input uses a 16-bit coordinate space. For each pixel we
/// choose the middle of the integer coordinates that map back to that pixel
/// under the 65,536-based desktop scaling. This avoids the rounding-down
/// error of directly scaling by 65,535; it also works at the first and last
/// pixels and for desktop dimensions close to the 16-bit precision limit.
/// A larger dimension cannot represent every pixel and is rejected.
pub fn normalize_axis(position: i64, origin: i32, length: i32) -> Option<i32> {
    if !(1..=ABSOLUTE_COORDINATE_COUNT).contains(&length) {
        return None;
    }
    let offset = position.checked_sub(i64::from(origin))?;
    let length = i64::from(length);
    if !(0..length).contains(&offset) {
        return None;
    }
    let units = i64::from(ABSOLUTE_COORDINATE_COUNT);
    // [first, last] are the integer input values whose scaled pixel is offset.
    let first = (offset * units + length - 1) / length;
    let last = ((offset + 1) * units + length - 1) / length - 1;
    Some((first + (last - first) / 2) as i32)
}

/// Pack independent signed 16-bit client coordinates into a mouse LPARAM.
/// Mask both words before combining them so a negative x cannot corrupt y.
pub fn make_lparam(x: i32, y: i32) -> isize {
    let packed = u32::from(x as u16) | (u32::from(y as u16) << 16);
    packed as i32 as isize
}
