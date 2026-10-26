#if defined(_WIN32)

#include "window_target.h"

#include <algorithm>
#include <iterator>
#include <utility>

namespace op41 {
namespace {

struct EnumContext {
    std::vector<WindowInfo> windows;
};

BOOL CALLBACK enum_windows_proc(HWND hwnd, LPARAM lparam) {
    auto* context = reinterpret_cast<EnumContext*>(lparam);
    if (!IsWindow(hwnd) || !IsWindowVisible(hwnd)) {
        return TRUE; // keep enumerating
    }

    wchar_t buffer[512] = {0};
    const int length = GetWindowTextW(hwnd, buffer,
                                      static_cast<int>(std::size(buffer)));
    if (length <= 0) {
        return TRUE; // skip untitled helper/tool windows
    }

    WindowInfo info;
    info.hwnd = hwnd;
    info.title.assign(buffer, static_cast<std::size_t>(length));
    info.visible = true;
    info.minimized = IsIconic(hwnd) != 0;
    GetWindowThreadProcessId(hwnd, &info.process_id);
    GetClientRect(hwnd, &info.client_rect);
    context->windows.push_back(std::move(info));
    return TRUE;
}

} // namespace

std::vector<WindowInfo> WindowTargetService::enumerate_windows() {
    EnumContext context;
    EnumWindows(&enum_windows_proc, reinterpret_cast<LPARAM>(&context));
    std::sort(context.windows.begin(), context.windows.end(),
              [](const WindowInfo& a, const WindowInfo& b) {
                  return a.title < b.title;
              });
    return context.windows;
}

bool WindowTargetService::is_valid(HWND hwnd) {
    return hwnd != nullptr && IsWindow(hwnd) != 0;
}

std::wstring WindowTargetService::window_title(HWND hwnd) {
    if (!is_valid(hwnd)) {
        return L"";
    }
    wchar_t buffer[512] = {0};
    const int length = GetWindowTextW(hwnd, buffer,
                                      static_cast<int>(std::size(buffer)));
    return std::wstring(buffer, static_cast<std::size_t>(std::max(length, 0)));
}

bool WindowTargetService::get_client_rect(HWND hwnd, RECT& out_rect) {
    if (!is_valid(hwnd)) {
        return false;
    }
    return GetClientRect(hwnd, &out_rect) != 0;
}

RECT WindowTargetService::refresh_cached_rect(HWND hwnd) {
    RECT rect{};
    if (get_client_rect(hwnd, rect) &&
        (rect.right - rect.left) > 0 && (rect.bottom - rect.top) > 0) {
        cache_[hwnd] = rect;
        return rect;
    }
    const auto found = cache_.find(hwnd);
    if (found != cache_.end()) {
        return found->second; // minimized/invalid: reuse last known geometry
    }
    return rect;
}

RECT WindowTargetService::last_known_rect(HWND hwnd) const {
    const auto found = cache_.find(hwnd);
    if (found != cache_.end()) {
        return found->second;
    }
    return RECT{};
}

void WindowTargetService::forget(HWND hwnd) {
    cache_.erase(hwnd);
}

} // namespace op41

#endif // _WIN32
