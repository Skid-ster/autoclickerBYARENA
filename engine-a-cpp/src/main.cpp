#if defined(_WIN32)

#include <d3d11.h>
#include <windows.h>

#include <cstddef>
#include <iterator>

#include "imgui.h"
#include "imgui_impl_dx11.h"
#include "imgui_impl_win32.h"

#include "app.h"
#include "theme.h"

// Dear ImGui keeps this declaration inside an `#if 0` block in
// imgui_impl_win32.h so the helper header does not drag <windows.h> into
// every consumer. The upstream header instructs applications to copy the
// forward declaration into their own translation unit (required since the
// 1.90 backend series); the definition lives in imgui_impl_win32.cpp.
extern IMGUI_IMPL_API LRESULT ImGui_ImplWin32_WndProcHandler(
    HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam);

namespace {

ID3D11Device* g_device = nullptr;
ID3D11DeviceContext* g_device_context = nullptr;
IDXGISwapChain* g_swap_chain = nullptr;
ID3D11RenderTargetView* g_render_target_view = nullptr;
UINT g_resize_width = 0;
UINT g_resize_height = 0;

void create_render_target() {
    ID3D11Texture2D* back_buffer = nullptr;
    g_swap_chain->GetBuffer(0, IID_PPV_ARGS(&back_buffer));
    if (back_buffer != nullptr) {
        g_device->CreateRenderTargetView(back_buffer, nullptr,
                                         &g_render_target_view);
        back_buffer->Release();
    }
}

void release_render_target() {
    if (g_render_target_view != nullptr) {
        g_render_target_view->Release();
        g_render_target_view = nullptr;
    }
}

bool create_device(HWND hwnd) {
    DXGI_SWAP_CHAIN_DESC desc{};
    desc.BufferCount = 2;
    desc.BufferDesc.Width = 0;
    desc.BufferDesc.Height = 0;
    desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.BufferDesc.RefreshRate.Numerator = 60;
    desc.BufferDesc.RefreshRate.Denominator = 1;
    desc.Flags = DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.OutputWindow = hwnd;
    desc.SampleDesc.Count = 1;
    desc.SampleDesc.Quality = 0;
    desc.Windowed = TRUE;
    desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;

    UINT create_flags = 0;
    D3D_FEATURE_LEVEL feature_level;
    const D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_11_0,
                                        D3D_FEATURE_LEVEL_10_0};
    const HRESULT hr = D3D11CreateDeviceAndSwapChain(
        nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, create_flags, levels,
        static_cast<UINT>(std::size(levels)), D3D11_SDK_VERSION, &desc,
        &g_swap_chain, &g_device, &feature_level, &g_device_context);
    if (FAILED(hr)) {
        return false;
    }
    create_render_target();
    return true;
}

void cleanup_device() {
    release_render_target();
    if (g_swap_chain != nullptr) {
        g_swap_chain->Release();
        g_swap_chain = nullptr;
    }
    if (g_device_context != nullptr) {
        g_device_context->Release();
        g_device_context = nullptr;
    }
    if (g_device != nullptr) {
        g_device->Release();
        g_device = nullptr;
    }
}

LRESULT WINAPI wnd_proc(HWND hwnd, UINT msg, WPARAM wparam, LPARAM lparam) {
    if (ImGui_ImplWin32_WndProcHandler(hwnd, msg, wparam, lparam)) {
        return TRUE;
    }
    switch (msg) {
    case WM_SIZE:
        if (wparam == SIZE_MINIMIZED) {
            return 0;
        }
        g_resize_width = LOWORD(lparam);
        g_resize_height = HIWORD(lparam);
        return 0;
    case WM_SYSCOMMAND:
        if ((wparam & 0xfff0) == SC_KEYMENU) {
            return 0; // block ALT menu focus steal
        }
        break;
    case WM_DESTROY:
        PostQuitMessage(0);
        return 0;
    default:
        break;
    }
    return DefWindowProc(hwnd, msg, wparam, lparam);
}

} // namespace

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE, LPWSTR, int show_command) {
    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(WNDCLASSEXW);
    wc.style = CS_CLASSDC;
    wc.lpfnWndProc = wnd_proc;
    wc.hInstance = instance;
    wc.lpszClassName = L"Op41KillerEngineA";
    wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
    RegisterClassExW(&wc);

    HWND hwnd = CreateWindowExW(
        0, wc.lpszClassName, L"OP Auto Clicker - Engine A (C++20)",
        WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 1080, 880,
        nullptr, nullptr, instance, nullptr);
    if (hwnd == nullptr) {
        UnregisterClassW(wc.lpszClassName, instance);
        return 1;
    }
    ShowWindow(hwnd, show_command);
    UpdateWindow(hwnd);

    if (!create_device(hwnd)) {
        cleanup_device();
        DestroyWindow(hwnd);
        UnregisterClassW(wc.lpszClassName, instance);
        MessageBoxW(nullptr,
                    L"Direct3D 11 device creation failed. A D3D11-capable GPU "
                    L"and driver are required.",
                    L"OP 4.1 Killer", MB_ICONERROR);
        return 1;
    }

    IMGUI_CHECKVERSION();
    ImGui::CreateContext();
    ImGuiIO& io = ImGui::GetIO();
    io.ConfigFlags |= ImGuiConfigFlags_NavEnableKeyboard;
    op41::theme::apply_modern_style();
    ImGui_ImplWin32_Init(hwnd);
    ImGui_ImplDX11_Init(g_device, g_device_context);

    op41::App app;
    bool done = false;
    while (!done) {
        MSG msg;
        while (PeekMessage(&msg, nullptr, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessage(&msg);
            if (msg.message == WM_QUIT) {
                done = true;
            }
        }
        if (done) {
            break;
        }

        if (g_resize_width != 0 && g_resize_height != 0) {
            release_render_target();
            g_swap_chain->ResizeBuffers(0, g_resize_width, g_resize_height,
                                        DXGI_FORMAT_UNKNOWN, 0);
            g_resize_width = 0;
            g_resize_height = 0;
            create_render_target();
        }

        ImGui_ImplDX11_NewFrame();
        ImGui_ImplWin32_NewFrame();
        ImGui::NewFrame();
        app.render();
        ImGui::Render();

        // #0D1117, matching the modern dark theme background.
        const float clear_color[4] = {0.051f, 0.067f, 0.090f, 1.0f};
        g_device_context->OMSetRenderTargets(1, &g_render_target_view, nullptr);
        g_device_context->ClearRenderTargetView(g_render_target_view,
                                                clear_color);
        ImGui_ImplDX11_RenderDrawData(ImGui::GetDrawData());
        g_swap_chain->Present(1, 0);
    }

    ImGui_ImplDX11_Shutdown();
    ImGui_ImplWin32_Shutdown();
    ImGui::DestroyContext();
    cleanup_device();
    DestroyWindow(hwnd);
    UnregisterClassW(wc.lpszClassName, instance);
    return 0;
}

#endif // _WIN32
