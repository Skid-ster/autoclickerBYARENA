#pragma once

#include "imgui.h"

namespace op41 {

// Modern dark design system shared by the whole dashboard. A single blue
// accent carries interaction; green/amber/red are reserved for status
// semantics. Applied programmatically so the binary needs no external font
// or texture assets.
namespace theme {

// Surfaces.
inline constexpr ImU32 kBackground = IM_COL32(13, 17, 23, 255);   // #0D1117
inline constexpr ImU32 kCard = IM_COL32(22, 27, 34, 255);         // #161B22
inline constexpr ImU32 kInput = IM_COL32(28, 35, 48, 255);        // #1C2330
inline constexpr ImU32 kBorder = IM_COL32(48, 58, 76, 255);       // #303A4C

// Single interaction accent.
inline constexpr ImU32 kAccent = IM_COL32(76, 141, 255, 255);     // #4C8DFF
inline constexpr ImU32 kAccentSoft = IM_COL32(76, 141, 255, 46);
inline constexpr ImU32 kAccentBorder = IM_COL32(76, 141, 255, 110);

// Semantic status colors.
inline constexpr ImU32 kSuccess = IM_COL32(63, 185, 80, 255);     // #3FB950
inline constexpr ImU32 kSuccessSoft = IM_COL32(63, 185, 80, 42);
inline constexpr ImU32 kDanger = IM_COL32(248, 81, 73, 255);      // #F85149
inline constexpr ImU32 kDangerSoft = IM_COL32(248, 81, 73, 42);
inline constexpr ImU32 kWarning = IM_COL32(210, 153, 34, 255);    // #D29922
inline constexpr ImU32 kWarningSoft = IM_COL32(210, 153, 34, 42);

// Text.
inline constexpr ImU32 kText = IM_COL32(230, 237, 243, 255);      // #E6EDF3
inline constexpr ImU32 kTextDim = IM_COL32(139, 148, 158, 255);   // #8B949E
inline constexpr ImU32 kTextFaint = IM_COL32(96, 105, 120, 255);

// Chart series (monochrome accent family instead of competing neons).
inline constexpr ImU32 kChartGrid = IM_COL32(48, 58, 76, 90);
inline constexpr ImU32 kChartBarAlt = IM_COL32(76, 141, 255, 110);

void apply_modern_style();

} // namespace theme
} // namespace op41
