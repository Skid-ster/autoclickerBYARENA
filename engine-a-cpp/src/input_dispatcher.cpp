#if defined(_WIN32)

#include "input_dispatcher.h"

#include <algorithm>

namespace op41 {
namespace {

[[nodiscard]] LONG normalize_virtual_x(long screen_x) {
    const LONG virtual_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    const LONG virtual_origin_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    if (virtual_width <= 0) {
        return 0;
    }
    const long relative = screen_x - virtual_origin_x;
    return static_cast<LONG>(static_cast<double>(relative) * 65535.0 /
                             static_cast<double>(virtual_width - 1));
}

[[nodiscard]] LONG normalize_virtual_y(long screen_y) {
    const LONG virtual_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    const LONG virtual_origin_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    if (virtual_height <= 0) {
        return 0;
    }
    const long relative = screen_y - virtual_origin_y;
    return static_cast<LONG>(static_cast<double>(relative) * 65535.0 /
                             static_cast<double>(virtual_height - 1));
}

void clamp_client_point(long& x, long& y, const RECT& rect) {
    if (rect.right <= rect.left || rect.bottom <= rect.top) {
        return; // no usable cached geometry: post the raw coordinates
    }
    x = std::clamp(x, static_cast<long>(rect.left),
                   static_cast<long>(rect.right) - 1L);
    y = std::clamp(y, static_cast<long>(rect.top),
                   static_cast<long>(rect.bottom) - 1L);
}

} // namespace

DispatchResult InputDispatcher::send_array(INPUT* inputs, std::uint32_t count) {
    DispatchResult result;
    if (inputs == nullptr || count == 0) {
        result.ok = false;
        result.message = "SendInput called with an empty INPUT array";
        return result;
    }
    for (std::uint32_t i = 0; i < count; ++i) {
        inputs[i].type = INPUT_MOUSE;
        inputs[i].mi.dx = 0;
        inputs[i].mi.dy = 0;
        inputs[i].mi.mouseData = 0;
        inputs[i].mi.dwExtraInfo = 0;
        inputs[i].mi.time = 0;
    }
    const std::uint32_t sent = SendInput(count, inputs, sizeof(INPUT));
    result.events_injected = sent;
    if (sent != count) {
        result.ok = false;
        const DWORD error = GetLastError();
        result.message = "SendInput injected " + std::to_string(sent) + " of " +
                         std::to_string(count) + " events (GetLastError=" +
                         std::to_string(error) + ")";
    }
    return result;
}

DispatchResult InputDispatcher::global_move_absolute(long screen_x, long screen_y) {
    INPUT input{};
    input.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE;
    input.mi.dx = normalize_virtual_x(screen_x);
    input.mi.dy = normalize_virtual_y(screen_y);
    return send_array(&input, 1);
}

DispatchResult InputDispatcher::global_button_down() {
    INPUT input{};
    input.mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
    return send_array(&input, 1);
}

DispatchResult InputDispatcher::global_button_up() {
    INPUT input{};
    input.mi.dwFlags = MOUSEEVENTF_LEFTUP;
    return send_array(&input, 1);
}

DispatchResult InputDispatcher::post_or_send(HWND target, UINT message,
                                             WPARAM wparam, LPARAM lparam,
                                             bool synchronous) {
    DispatchResult result;
    if (!IsWindow(target)) {
        result.ok = false;
        result.message = "Target window handle is no longer valid";
        return result;
    }
    if (!synchronous) {
        if (PostMessageW(target, message, wparam, lparam) == 0) {
            result.ok = false;
            result.message = "PostMessageW failed for message 0x" +
                             std::to_string(message) + " (GetLastError=" +
                             std::to_string(GetLastError()) + ")";
        }
        return result;
    }
    DWORD_PTR reply = 0;
    LRESULT sent = SendMessageTimeoutW(target, message, wparam, lparam,
                                       SMTO_ABORTIFHUNG, 50, &reply);
    if (sent == 0) {
        result.ok = false;
        result.message = "SendMessageTimeoutW failed for message 0x" +
                         std::to_string(message) + " (GetLastError=" +
                         std::to_string(GetLastError()) + ")";
    }
    return result;
}

DispatchResult InputDispatcher::background_move(HWND target, long x, long y,
                                                const RECT& clamp_rect) {
    clamp_client_point(x, y, clamp_rect);
    return post_or_send(target, WM_MOUSEMOVE, 0, MAKELPARAM(x, y), false);
}

DispatchResult InputDispatcher::background_button_down(HWND target, long x,
                                                       long y,
                                                       const RECT& clamp_rect,
                                                       bool synchronous) {
    clamp_client_point(x, y, clamp_rect);
    return post_or_send(target, WM_LBUTTONDOWN,
                        static_cast<WPARAM>(MK_LBUTTON), MAKELPARAM(x, y),
                        synchronous);
}

DispatchResult InputDispatcher::background_button_up(HWND target, long x,
                                                     long y,
                                                     const RECT& clamp_rect,
                                                     bool synchronous) {
    clamp_client_point(x, y, clamp_rect);
    return post_or_send(target, WM_LBUTTONUP, 0, MAKELPARAM(x, y),
                        synchronous);
}

} // namespace op41

#endif // _WIN32
