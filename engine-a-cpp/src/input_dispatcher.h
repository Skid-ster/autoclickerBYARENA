#pragma once

#if defined(_WIN32)

#include <windows.h>

#include <cstdint>
#include <string>

namespace op41 {

struct DispatchResult {
    bool ok = true;
    std::uint32_t events_injected = 0;
    std::string message;
};

// Dual-mode Win32 input injection.
//
// Global mode uses SendInput with arrayed INPUT structures only. Background
// mode posts WM_MOUSEMOVE/WM_LBUTTONDOWN/WM_LBUTTONUP messages directly to a
// target HWND so clicks land even when the window is unfocused or minimized.
// The legacy mouse_event/keybd_event APIs are never used.
class InputDispatcher {
public:
    // Moves the physical cursor to an absolute screen position using
    // MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE and virtual-screen normalized
    // 0..65535 coordinates.
    DispatchResult global_move_absolute(long screen_x, long screen_y);

    // Left button down/up through arrayed SendInput calls.
    DispatchResult global_button_down();
    DispatchResult global_button_up();

    // Background message delivery. Coordinates are client-space pixels and are
    // clamped into clamp_rect when it describes a non-empty rectangle (the
    // cached client rect used while the target is minimized).
    DispatchResult background_move(HWND target, long x, long y,
                                   const RECT& clamp_rect);
    DispatchResult background_button_down(HWND target, long x, long y,
                                          const RECT& clamp_rect,
                                          bool synchronous);
    DispatchResult background_button_up(HWND target, long x, long y,
                                        const RECT& clamp_rect,
                                        bool synchronous);

private:
    DispatchResult send_array(INPUT* inputs, std::uint32_t count);
    DispatchResult post_or_send(HWND target, UINT message, WPARAM wparam,
                                LPARAM lparam, bool synchronous);
};

} // namespace op41

#endif // _WIN32
