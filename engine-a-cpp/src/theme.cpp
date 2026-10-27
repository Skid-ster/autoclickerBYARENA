#include "theme.h"

namespace op41::theme {

namespace {

ImVec4 rgb(int r, int g, int b, float a = 1.0f) {
    return ImVec4(r / 255.0f, g / 255.0f, b / 255.0f, a);
}

} // namespace

void apply_modern_style() {
    ImGuiStyle& style = ImGui::GetStyle();
    ImGuiIO& io = ImGui::GetIO();

    // Slightly larger default type for readability; no external font assets.
    io.FontGlobalScale = 1.12f;

    // Generous, calm spacing.
    style.WindowPadding = ImVec2(20.0f, 18.0f);
    style.FramePadding = ImVec2(12.0f, 8.0f);
    style.CellPadding = ImVec2(10.0f, 8.0f);
    style.ItemSpacing = ImVec2(10.0f, 11.0f);
    style.ItemInnerSpacing = ImVec2(8.0f, 6.0f);
    style.IndentSpacing = 22.0f;
    style.ScrollbarSize = 12.0f;
    style.GrabMinSize = 10.0f;

    // Soft rounded geometry everywhere.
    style.WindowRounding = 10.0f;
    style.ChildRounding = 10.0f;
    style.FrameRounding = 8.0f;
    style.PopupRounding = 8.0f;
    style.ScrollbarRounding = 8.0f;
    style.GrabRounding = 6.0f;
    style.TabRounding = 8.0f;
    style.WindowBorderSize = 1.0f;
    style.ChildBorderSize = 1.0f;
    style.FrameBorderSize = 0.0f;
    style.PopupBorderSize = 1.0f;

    const ImVec4 bg(0.051f, 0.067f, 0.090f, 1.0f);        // #0D1117
    const ImVec4 card(0.086f, 0.106f, 0.133f, 1.0f);      // #161B22
    const ImVec4 input(0.110f, 0.137f, 0.188f, 1.0f);     // #1C2330
    const ImVec4 border(0.188f, 0.227f, 0.298f, 1.0f);    // #303A4C
    const ImVec4 accent = rgb(76, 141, 255);              // #4C8DFF
    const ImVec4 accent_soft = rgb(76, 141, 255, 0.18f);
    const ImVec4 text(0.902f, 0.929f, 0.953f, 1.0f);      // #E6EDF3
    const ImVec4 text_dim(0.545f, 0.580f, 0.620f, 1.0f);  // #8B949E

    ImVec4* colors = style.Colors;
    colors[ImGuiCol_Text] = text;
    colors[ImGuiCol_TextDisabled] = text_dim;
    colors[ImGuiCol_WindowBg] = bg;
    colors[ImGuiCol_ChildBg] = card;
    colors[ImGuiCol_PopupBg] = rgb(22, 27, 34, 0.98f);
    colors[ImGuiCol_Border] = border;
    colors[ImGuiCol_BorderShadow] = ImVec4(0.0f, 0.0f, 0.0f, 0.0f);

    // Inputs and framed widgets.
    colors[ImGuiCol_FrameBg] = input;
    colors[ImGuiCol_FrameBgHovered] = rgb(38, 47, 64, 1.0f);
    colors[ImGuiCol_FrameBgActive] = rgb(46, 57, 78, 1.0f);

    // Titles and menus.
    colors[ImGuiCol_TitleBg] = card;
    colors[ImGuiCol_TitleBgActive] = card;
    colors[ImGuiCol_TitleBgCollapsed] = card;
    colors[ImGuiCol_MenuBarBg] = card;

    // Neutral, quiet scrollbars.
    colors[ImGuiCol_ScrollbarBg] = ImVec4(0.0f, 0.0f, 0.0f, 0.0f);
    colors[ImGuiCol_ScrollbarGrab] = border;
    colors[ImGuiCol_ScrollbarGrabHovered] = rgb(76, 141, 255, 0.65f);
    colors[ImGuiCol_ScrollbarGrabActive] = accent;

    // One accent for every interactive affordance.
    colors[ImGuiCol_CheckMark] = accent;
    colors[ImGuiCol_SliderGrab] = accent;
    colors[ImGuiCol_SliderGrabActive] = rgb(120, 170, 255, 1.0f);
    colors[ImGuiCol_Button] = accent_soft;
    colors[ImGuiCol_ButtonHovered] = rgb(76, 141, 255, 0.32f);
    colors[ImGuiCol_ButtonActive] = rgb(76, 141, 255, 0.50f);

    // Lists, trees and headers.
    colors[ImGuiCol_Header] = accent_soft;
    colors[ImGuiCol_HeaderHovered] = rgb(76, 141, 255, 0.30f);
    colors[ImGuiCol_HeaderActive] = rgb(76, 141, 255, 0.45f);

    // Quiet separators and dividers.
    colors[ImGuiCol_Separator] = border;
    colors[ImGuiCol_SeparatorHovered] = accent;
    colors[ImGuiCol_SeparatorActive] = accent;

    // Tabs.
    colors[ImGuiCol_Tab] = card;
    colors[ImGuiCol_TabHovered] = accent_soft;
    colors[ImGuiCol_TabActive] = accent_soft;
    colors[ImGuiCol_TabUnfocused] = card;
    colors[ImGuiCol_TabUnfocusedActive] = rgb(30, 38, 52, 1.0f);

    // Plots use the accent family only.
    colors[ImGuiCol_PlotLines] = accent;
    colors[ImGuiCol_PlotLinesHovered] = rgb(120, 170, 255, 1.0f);
    colors[ImGuiCol_PlotHistogram] = accent;
    colors[ImGuiCol_PlotHistogramHovered] = rgb(120, 170, 255, 1.0f);

    // Tables.
    colors[ImGuiCol_TableHeaderBg] = rgb(76, 141, 255, 0.14f);
    colors[ImGuiCol_TableBorderStrong] = border;
    colors[ImGuiCol_TableBorderLight] = rgb(48, 58, 76, 0.55f);
    colors[ImGuiCol_TableRowBg] = ImVec4(0.0f, 0.0f, 0.0f, 0.0f);
    colors[ImGuiCol_TableRowBgAlt] = ImVec4(1.0f, 1.0f, 1.0f, 0.03f);

    colors[ImGuiCol_TextSelectedBg] = rgb(76, 141, 255, 0.38f);
    colors[ImGuiCol_DragDropTarget] = rgb(210, 153, 34, 1.0f);
    colors[ImGuiCol_NavHighlight] = accent;
    colors[ImGuiCol_NavWindowingHighlight] = accent;
    colors[ImGuiCol_NavWindowingDimBg] = rgb(13, 17, 23, 0.7f);
    colors[ImGuiCol_ModalWindowDimBg] = rgb(5, 7, 11, 0.65f);
}

} // namespace op41::theme
