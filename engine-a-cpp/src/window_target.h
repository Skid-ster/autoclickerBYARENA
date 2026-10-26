#pragma once

#if defined(_WIN32)

#include <windows.h>

#include <string>
#include <unordered_map>
#include <vector>

namespace op41 {

struct WindowInfo {
    HWND hwnd = nullptr;
    std::wstring title;
    DWORD process_id = 0;
    RECT client_rect{};
    bool visible = false;
    bool minimized = false;
};

// Enumerates top-level windows and tracks last-known client geometry so that
// background clicks can still be clamped correctly when a target is minimized.
class WindowTargetService {
public:
    // Returns visible, top-level application windows with usable titles.
    [[nodiscard]] static std::vector<WindowInfo> enumerate_windows();

    [[nodiscard]] static bool is_valid(HWND hwnd);
    [[nodiscard]] static std::wstring window_title(HWND hwnd);
    [[nodiscard]] static bool get_client_rect(HWND hwnd, RECT& out_rect);

    // Refreshes and returns the cached client rectangle for hwnd. When the
    // window is minimized the previous cached value is retained.
    RECT refresh_cached_rect(HWND hwnd);
    [[nodiscard]] RECT last_known_rect(HWND hwnd) const;
    void forget(HWND hwnd);

private:
    mutable std::unordered_map<HWND, RECT> cache_;
};

} // namespace op41

#endif // _WIN32
