#include "theme.h"

namespace op41::theme {

void apply_cyberpunk_style() {
    ImGuiStyle& style = ImGui::GetStyle();
    style.WindowPadding = ImVec2(14.0f, 14.0f);
    style.FramePadding = ImVec2(10.0f, 6.0f);
    style.CellPadding = ImVec2(8.0f, 6.0f);
    style.ItemSpacing = ImVec2(10.0f, 10.0f);
    style.ItemInnerSpacing = ImVec2(8.0f, 6.0f);
    style.TouchExtraPadding = ImVec2(0.0f, 0.0f);
    style.IndentSpacing = 22.0f;
    style.ScrollbarSize = 14.0f;
    style.GrabMinSize = 12.0f;

    style.WindowBorderSize = 1.0f;
    style.ChildBorderSize = 1.0f;
    style.PopupBorderSize = 1.0f;
    style.FrameBorderSize = 0.0f;
    style.TabBorderSize = 0.0f;

    style.WindowRounding = 10.0f;
    style.ChildRounding = 8.0f;
    style.FrameRounding = 6.0f;
    style.PopupRounding = 8.0f;
    style.ScrollbarRounding = 8.0f;
    style.GrabRounding = 6.0f;
    style.TabRounding = 6.0f;

    ImVec4* colors = style.Colors;
    const ImVec4 bg(0.039f, 0.047f, 0.094f, 1.0f);
    const ImVec4 panel(0.071f, 0.086f, 0.157f, 1.0f);
    const ImVec4 panel_hover(0.110f, 0.130f, 0.220f, 1.0f);
    const ImVec4 cyan(0.000f, 0.898f, 1.000f, 1.0f);
    const ImVec4 magenta(1.000f, 0.169f, 0.839f, 1.0f);
    const ImVec4 green(0.224f, 1.000f, 0.533f, 1.0f);
    const ImVec4 amber(1.000f, 0.749f, 0.000f, 1.0f);
    const ImVec4 red(1.000f, 0.275f, 0.353f, 1.0f);
    const ImVec4 text(0.878f, 0.902f, 0.961f, 1.0f);
    const ImVec4 text_dim(0.510f, 0.549f, 0.647f, 1.0f);

    colors[ImGuiCol_Text] = text;
    colors[ImGuiCol_TextDisabled] = text_dim;
    colors[ImGuiCol_WindowBg] = bg;
    colors[ImGuiCol_ChildBg] = panel;
    colors[ImGuiCol_PopupBg] = ImVec4(0.055f, 0.067f, 0.125f, 0.98f);
    colors[ImGuiCol_Border] = ImVec4(cyan.x, cyan.y, cyan.z, 0.35f);
    colors[ImGuiCol_BorderShadow] = ImVec4(0.0f, 0.0f, 0.0f, 0.0f);
    colors[ImGuiCol_FrameBg] = ImVec4(0.102f, 0.125f, 0.220f, 1.0f);
    colors[ImGuiCol_FrameBgHovered] = panel_hover;
    colors[ImGuiCol_FrameBgActive] = ImVec4(0.145f, 0.170f, 0.280f, 1.0f);
    colors[ImGuiCol_TitleBg] = panel;
    colors[ImGuiCol_TitleBgActive] = ImVec4(0.090f, 0.110f, 0.200f, 1.0f);
    colors[ImGuiCol_TitleBgCollapsed] = panel;
    colors[ImGuiCol_MenuBarBg] = panel;
    colors[ImGuiCol_ScrollbarBg] = ImVec4(0.0f, 0.0f, 0.0f, 0.0f);
    colors[ImGuiCol_ScrollbarGrab] = ImVec4(cyan.x, cyan.y, cyan.z, 0.35f);
    colors[ImGuiCol_ScrollbarGrabHovered] = ImVec4(cyan.x, cyan.y, cyan.z, 0.55f);
    colors[ImGuiCol_ScrollbarGrabActive] = cyan;
    colors[ImGuiCol_CheckMark] = cyan;
    colors[ImGuiCol_SliderGrab] = magenta;
    colors[ImGuiCol_SliderGrabActive] = ImVec4(magenta.x, magenta.y, magenta.z, 0.85f);
    colors[ImGuiCol_Button] = ImVec4(cyan.x, cyan.y, cyan.z, 0.18f);
    colors[ImGuiCol_ButtonHovered] = ImVec4(cyan.x, cyan.y, cyan.z, 0.35f);
    colors[ImGuiCol_ButtonActive] = ImVec4(cyan.x, cyan.y, cyan.z, 0.55f);
    colors[ImGuiCol_Header] = ImVec4(magenta.x, magenta.y, magenta.z, 0.20f);
    colors[ImGuiCol_HeaderHovered] = ImVec4(magenta.x, magenta.y, magenta.z, 0.35f);
    colors[ImGuiCol_HeaderActive] = ImVec4(magenta.x, magenta.y, magenta.z, 0.50f);
    colors[ImGuiCol_Separator] = ImVec4(cyan.x, cyan.y, cyan.z, 0.25f);
    colors[ImGuiCol_SeparatorHovered] = cyan;
    colors[ImGuiCol_SeparatorActive] = cyan;
    colors[ImGuiCol_Tab] = panel;
    colors[ImGuiCol_TabHovered] = panel_hover;
    colors[ImGuiCol_TabActive] = ImVec4(cyan.x, cyan.y, cyan.z, 0.22f);
    colors[ImGuiCol_TabUnfocused] = panel;
    colors[ImGuiCol_TabUnfocusedActive] = panel_hover;
    colors[ImGuiCol_PlotLines] = cyan;
    colors[ImGuiCol_PlotLinesHovered] = cyan;
    colors[ImGuiCol_PlotHistogram] = magenta;
    colors[ImGuiCol_PlotHistogramHovered] = magenta;
    colors[ImGuiCol_TableHeaderBg] = ImVec4(cyan.x, cyan.y, cyan.z, 0.16f);
    colors[ImGuiCol_TableBorderStrong] = ImVec4(cyan.x, cyan.y, cyan.z, 0.35f);
    colors[ImGuiCol_TableBorderLight] = ImVec4(cyan.x, cyan.y, cyan.z, 0.18f);
    colors[ImGuiCol_TableRowBg] = ImVec4(0.0f, 0.0f, 0.0f, 0.0f);
    colors[ImGuiCol_TableRowBgAlt] = ImVec4(1.0f, 1.0f, 1.0f, 0.04f);
    colors[ImGuiCol_TextSelectedBg] = ImVec4(magenta.x, magenta.y, magenta.z, 0.45f);
    colors[ImGuiCol_DragDropTarget] = amber;
    colors[ImGuiCol_NavHighlight] = cyan;
    colors[ImGuiCol_NavWindowingHighlight] = cyan;
    colors[ImGuiCol_NavWindowingDimBg] = ImVec4(0.10f, 0.12f, 0.20f, 0.7f);
    colors[ImGuiCol_ModalWindowDimBg] = ImVec4(0.02f, 0.03f, 0.06f, 0.65f);
}

} // namespace op41::theme
