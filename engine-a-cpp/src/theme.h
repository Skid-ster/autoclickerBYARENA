#pragma once

#include "imgui.h"

namespace op41 {

// Cyberpunk dark palette: deep navy/black surfaces with cyan primary, magenta
// secondary and green status accents. Applied programmatically so the binary
// needs no external texture or font assets.
namespace theme {

inline constexpr ImU32 kBackground = IM_COL32(10, 12, 24, 255);
inline constexpr ImU32 kPanel = IM_COL32(18, 22, 40, 255);
inline constexpr ImU32 kPanelBorder = IM_COL32(0, 229, 255, 90);
inline constexpr ImU32 kCyan = IM_COL32(0, 229, 255, 255);
inline constexpr ImU32 kMagenta = IM_COL32(255, 43, 214, 255);
inline constexpr ImU32 kGreen = IM_COL32(57, 255, 136, 255);
inline constexpr ImU32 kAmber = IM_COL32(255, 191, 0, 255);
inline constexpr ImU32 kRed = IM_COL32(255, 70, 90, 255);
inline constexpr ImU32 kText = IM_COL32(224, 230, 245, 255);
inline constexpr ImU32 kTextDim = IM_COL32(130, 140, 165, 255);

void apply_cyberpunk_style();

} // namespace theme
} // namespace op41
