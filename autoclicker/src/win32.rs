//! Narrow Win32 FFI wrappers.
//!
//! Every `unsafe` block carries a local `SAFETY` comment describing why the
//! call is sound. The legacy `mouse_event` and `keybd_event` APIs are never
//! referenced. Message and flag constants are declared locally so the wrapper
//! surface stays independent of internal metadata re-exports.

use std::io;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, BOOL, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_FAILED,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, ResetEvent, SetEvent, SetThreadPriority, WaitForSingleObject,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, MOUSEINPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetCursorPos, GetSystemMetrics, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, PostMessageW,
    SendMessageTimeoutW, WNDENUMPROC,
};

use crate::hotkey::PANIC_VK;

pub const INPUT_MOUSE: u32 = 0;
pub const MOUSEEVENTF_MOVE: u32 = 0x0001;
pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
pub const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
pub const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
pub const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;

pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_RBUTTONDOWN: u32 = 0x0204;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_MBUTTONDOWN: u32 = 0x0207;
pub const WM_MBUTTONUP: u32 = 0x0208;
pub const MK_LBUTTON: usize = 0x0001;
pub const MK_RBUTTON: usize = 0x0002;
pub const MK_MBUTTON: usize = 0x0010;

pub const SMTO_ABORTIFHUNG: u32 = 0x0002;
pub const SM_XVIRTUALSCREEN: i32 = 76;
pub const SM_YVIRTUALSCREEN: i32 = 77;
pub const SM_CXVIRTUALSCREEN: i32 = 78;
pub const SM_CYVIRTUALSCREEN: i32 = 79;

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub process_id: u32,
    pub client_width: i32,
    pub client_height: i32,
    pub minimized: bool,
}

fn last_os_error(context: &str) -> io::Error {
    // SAFETY: GetLastError is always safe to call and simply reads the
    // thread-local last-error value.
    let code = unsafe { GetLastError() };
    io::Error::new(
        io::Error::from_raw_os_error(code as i32).kind(),
        format!("{context} (GetLastError={code})"),
    )
}

pub fn query_performance_counter() -> i64 {
    let mut counter = 0i64;
    // SAFETY: `counter` is a valid local i64 for the duration of the call.
    let ok = unsafe { QueryPerformanceCounter(&mut counter) };
    debug_assert_ne!(ok, 0);
    counter
}

pub fn query_performance_frequency() -> i64 {
    let mut freq = 0i64;
    // SAFETY: `freq` is a valid local i64 for the duration of the call.
    let ok = unsafe { QueryPerformanceFrequency(&mut freq) };
    if ok == 0 || freq <= 0 {
        10_000_000
    } else {
        freq
    }
}

pub fn create_manual_reset_event() -> io::Result<isize> {
    // SAFETY: null security attributes, manual reset, initially non-signaled,
    // unnamed event. The returned handle is closed by `close_handle`.
    let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if handle == 0 {
        return Err(last_os_error("CreateEventW failed"));
    }
    Ok(handle)
}

pub fn set_event(handle: isize) -> io::Result<()> {
    // SAFETY: handle is a valid event created by `create_manual_reset_event`.
    if unsafe { SetEvent(handle as HANDLE) } == 0 {
        return Err(last_os_error("SetEvent failed"));
    }
    Ok(())
}

pub fn reset_event(handle: isize) -> io::Result<()> {
    // SAFETY: handle is a valid manual-reset event handle.
    if unsafe { ResetEvent(handle as HANDLE) } == 0 {
        return Err(last_os_error("ResetEvent failed"));
    }
    Ok(())
}

pub fn close_handle(handle: isize) {
    if handle != 0 {
        // SAFETY: handle is owned by the caller and is not used afterwards.
        unsafe {
            CloseHandle(handle as HANDLE);
        }
    }
}

pub fn wait_for_event(handle: isize, timeout_ms: u32) -> u32 {
    // SAFETY: handle is a valid event; an infinite wait parks the worker
    // without consuming CPU until `set_event` releases it.
    unsafe { WaitForSingleObject(handle as HANDLE, timeout_ms) }
}

pub const WAIT_OBJECT: u32 = WAIT_OBJECT_0;
pub const WAIT_ERROR: u32 = WAIT_FAILED;

pub fn set_current_thread_time_critical() -> io::Result<()> {
    // SAFETY: GetCurrentThread returns a pseudo-handle that never needs
    // closing; SetThreadPriority is safe with a valid thread handle.
    let thread = unsafe { GetCurrentThread() };
    if unsafe { SetThreadPriority(thread, THREAD_PRIORITY_TIME_CRITICAL) } == 0 {
        return Err(last_os_error("SetThreadPriority(TIME_CRITICAL) denied"));
    }
    Ok(())
}

/// Polls the physical state of any virtual-key code via GetAsyncKeyState,
/// which reports presses even when the window is unfocused or minimized.
pub fn is_vk_down(vk: i32) -> bool {
    // SAFETY: GetAsyncKeyState only reads global key state; no pointers.
    (unsafe { GetAsyncKeyState(vk) } as u16) & 0x8000 != 0
}

pub fn is_f8_down() -> bool {
    is_vk_down(PANIC_VK)
}

/// Returns the current physical cursor position in virtual-screen pixels.
pub fn get_cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: `point` is a valid local POINT for the duration of the call.
    let ok = unsafe { GetCursorPos(&mut point) };
    if ok == 0 {
        None
    } else {
        Some((point.x, point.y))
    }
}

pub fn is_valid_window(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    // SAFETY: IsWindow is a pure predicate over an opaque HWND value.
    unsafe { IsWindow(hwnd as HWND) != 0 }
}

pub fn get_window_text(hwnd: isize) -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: buffer is a valid 512-element UTF-16 array for the call.
    let length = unsafe { GetWindowTextW(hwnd as HWND, buffer.as_mut_ptr(), buffer.len() as i32) };
    if length <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..length as usize])
}

pub fn get_client_rect(hwnd: isize) -> Option<RECT> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `rect` is a valid local RECT for the duration of the call.
    let ok = unsafe { GetClientRect(hwnd as HWND, &mut rect) };
    if ok == 0 {
        None
    } else {
        Some(rect)
    }
}

pub fn client_to_screen(hwnd: isize, x: i32, y: i32) -> Option<(i32, i32)> {
    let mut point = POINT { x, y };
    // SAFETY: `point` is a valid local POINT for the duration of the call.
    let ok = unsafe { ClientToScreen(hwnd as HWND, &mut point) };
    if ok == 0 {
        None
    } else {
        Some((point.x, point.y))
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the &mut Vec created in `enumerate_windows` and stays
    // valid for the entire synchronous EnumWindows call on this one thread.
    let collected = &mut *(lparam as *mut Vec<WindowInfo>);

    // SAFETY: both calls are pure predicates over an opaque HWND value.
    if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let title = get_window_text(hwnd as isize);
    if title.is_empty() {
        return 1;
    }
    let mut process_id = 0u32;
    // SAFETY: process_id is a valid local u32 for the duration of the call.
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut process_id);
    }
    let (client_width, client_height) = match get_client_rect(hwnd as isize) {
        Some(rect) => (rect.right - rect.left, rect.bottom - rect.top),
        None => (0, 0),
    };
    // SAFETY: IsIconic is a pure predicate over an opaque HWND value.
    let minimized = unsafe { IsIconic(hwnd) != 0 };
    collected.push(WindowInfo {
        hwnd: hwnd as isize,
        title,
        process_id,
        client_width,
        client_height,
        minimized,
    });
    1
}

pub fn enumerate_windows() -> Vec<WindowInfo> {
    let mut collected = Vec::<WindowInfo>::new();
    let lparam = &mut collected as *mut Vec<WindowInfo> as LPARAM;
    // SAFETY: the callback and lparam are valid for the synchronous call, and
    // only this thread touches `collected` while it runs.
    let callback: WNDENUMPROC = Some(enum_windows_proc);
    unsafe {
        EnumWindows(callback, lparam);
    }
    collected.sort_by(|a, b| a.title.cmp(&b.title));
    collected
}

pub fn make_mouse_input(flags: u32, dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn send_inputs(inputs: &[INPUT]) -> io::Result<u32> {
    // SAFETY: the slice pointer is valid for `len` elements and SendInput
    // reads it synchronously before returning.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        return Err(last_os_error(
            "SendInput injected fewer events than requested",
        ));
    }
    Ok(sent)
}

pub fn system_metrics(index: i32) -> i32 {
    // SAFETY: GetSystemMetrics takes only an integer index and returns i32.
    unsafe { GetSystemMetrics(index) }
}

pub fn normalize_virtual_x(screen_x: i64) -> i32 {
    let width = system_metrics(SM_CXVIRTUALSCREEN);
    let origin = system_metrics(SM_XVIRTUALSCREEN);
    if width <= 1 {
        return 0;
    }
    let relative = screen_x - origin as i64;
    (relative as f64 * 65535.0 / (width - 1) as f64) as i32
}

pub fn normalize_virtual_y(screen_y: i64) -> i32 {
    let height = system_metrics(SM_CYVIRTUALSCREEN);
    let origin = system_metrics(SM_YVIRTUALSCREEN);
    if height <= 1 {
        return 0;
    }
    let relative = screen_y - origin as i64;
    (relative as f64 * 65535.0 / (height - 1) as f64) as i32
}

pub fn post_message(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> io::Result<()> {
    if !is_valid_window(hwnd) {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "Target window handle is no longer valid",
        ));
    }
    // SAFETY: hwnd has just been validated and the message parameters are
    // plain integer values packed per the WM_MOUSE* contract.
    let ok = unsafe { PostMessageW(hwnd as HWND, message, wparam, lparam as LPARAM) };
    if ok == 0 {
        return Err(last_os_error("PostMessageW failed"));
    }
    Ok(())
}

pub fn send_message_timeout(
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> io::Result<()> {
    if !is_valid_window(hwnd) {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "Target window handle is no longer valid",
        ));
    }
    let mut reply: usize = 0;
    // SAFETY: hwnd is validated and reply is a valid local usize for the call.
    let result: LRESULT = unsafe {
        SendMessageTimeoutW(
            hwnd as HWND,
            message,
            wparam,
            lparam as LPARAM,
            SMTO_ABORTIFHUNG,
            50,
            &mut reply,
        )
    };
    if result == 0 {
        return Err(last_os_error("SendMessageTimeoutW failed"));
    }
    Ok(())
}

pub fn make_lparam(x: i32, y: i32) -> isize {
    let packed = (x as u32) | ((y as u32) << 16);
    packed as i32 as isize
}
