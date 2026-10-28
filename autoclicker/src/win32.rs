//! Narrow Win32 FFI wrappers.
//!
//! Every `unsafe` block carries a local `SAFETY` comment describing why the
//! call is sound. The legacy `mouse_event` and `keybd_event` APIs are never
//! referenced. Message and flag constants are declared locally so the wrapper
//! surface stays independent of internal metadata re-exports.

use std::ffi::c_void;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, BOOL, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_FAILED,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts, INTERNET_DEFAULT_HTTPS_PORT,
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_LOCATION, WINHTTP_QUERY_STATUS_CODE,
};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, GetCurrentThread, ResetEvent, SetEvent, SetThreadPriority,
    WaitForSingleObject, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, MOUSEINPUT,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, PostMessageW,
    SendMessageTimeoutW, SW_SHOWNORMAL, WNDENUMPROC,
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

/// Virtual-key codes for the physical mouse buttons, used by the
/// screen-point capture flow ("Pick location") and the macro recorder.
pub const VK_LBUTTON: i32 = 0x01;
pub const VK_RBUTTON: i32 = 0x02;
pub const VK_MBUTTON: i32 = 0x04;

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

/// True when the foreground window belongs to this process. The macro
/// recorder uses it so clicks on MacroClicker's own windows (for example
/// the "Stop recording" button) are never captured into the macro.
pub fn foreground_is_own_process() -> bool {
    // SAFETY: both calls only read global window state; the PID out
    // pointer is a valid local for the duration of the call.
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == 0 {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(foreground, &mut pid);
        pid == GetCurrentProcessId()
    }
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

// On the real windows-sys 0.52 target `HWND` is a raw pointer, so the
// `hwnd as isize` conversions below are required. The offline type-check
// harness aliases `HWND` to `isize`, where the same casts look redundant to
// clippy; silence that shim-only divergence explicitly rather than weakening
// the pointer-to-integer conversion on the shipping Windows target.
#[allow(clippy::unnecessary_cast)]
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

// --------------------------------------------------------------- networking

/// Null-terminated UTF-16 copy of `text`, the shape every wide Win32
/// string parameter expects.
fn wide_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// RAII owner for a WinHTTP handle, so every early return in the
/// request sequence still releases the OS resources.
struct HttpHandle(*mut c_void);

impl Drop for HttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from a WinHttpOpen-family call
            // and is closed exactly once, here.
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

/// Largest body `https_get_text` will buffer. The only consumer reads
/// a few KB of release metadata; the cap turns a hostile or broken
/// endpoint into a clean error instead of an unbounded allocation.
const HTTPS_BODY_LIMIT: usize = 256 * 1024;

/// Hard ceiling on the whole body-read phase. The per-operation
/// WinHTTP timeouts bound each individual call; this bounds a
/// hostile or merely glacial endpoint that would otherwise keep a
/// background worker (and a "Checking…" UI) alive indefinitely by
/// trickling bytes forever.
const HTTPS_READ_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// One opened HTTPS GET whose response headers have arrived: the
/// three handles that must outlive the body read (kept in RAII
/// owners so every path closes them) plus the HTTP status.
struct HttpsRequest {
    _session: HttpHandle,
    _connect: HttpHandle,
    request: HttpHandle,
    status: u32,
}

/// Opens an HTTPS GET via WinHTTP — the OS HTTP stack, so system
/// proxy settings, TLS and certificate validation all come from
/// Windows — sends it and waits for the response headers.
/// `request_headers` is a caller-owned CRLF header block (empty
/// for none), so this layer stays a transport and never hardcodes
/// one service's protocol. WinHTTP follows redirects itself
/// (refusing HTTPS→HTTP downgrades); callers that also handle a
/// surfaced 3xx do so as a second line of defence.
fn open_https_request(
    host: &str,
    path: &str,
    request_headers: &str,
    receive_timeout_ms: i32,
) -> io::Result<HttpsRequest> {
    let agent = wide_null(&format!("MacroClicker/{}", env!("CARGO_PKG_VERSION")));
    // SAFETY: `agent` is a live null-terminated wide string for the
    // call; automatic proxy resolution honors the system proxy
    // settings, and null proxy name/bypass are its documented pair.
    let session = HttpHandle(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(last_os_error("WinHttpOpen failed"));
    }
    // SAFETY: `session` is a valid WinHTTP session handle. Timeouts
    // are best-effort tuning, so the result is intentionally ignored.
    let _timeouts_set =
        unsafe { WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, receive_timeout_ms) };

    let host_wide = wide_null(host);
    // SAFETY: session handle plus a live null-terminated host name;
    // the reserved parameter is zero as documented.
    let connect = HttpHandle(unsafe {
        WinHttpConnect(
            session.0,
            host_wide.as_ptr(),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    });
    if connect.0.is_null() {
        return Err(last_os_error("WinHttpConnect failed"));
    }

    let verb = wide_null("GET");
    let path_wide = wide_null(path);
    // SAFETY: connect handle plus live null-terminated verb/path.
    // Null version/referrer/accept-types select HTTP/1.1 defaults;
    // WINHTTP_FLAG_SECURE makes the request HTTPS.
    let request = HttpHandle(unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            path_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(last_os_error("WinHttpOpenRequest failed"));
    }

    if !request_headers.is_empty() {
        let headers = wide_null(request_headers);
        // SAFETY: request handle plus a live null-terminated header
        // block; the u32::MAX length tells WinHTTP the string is
        // null-terminated, per the API contract.
        let added = unsafe {
            WinHttpAddRequestHeaders(
                request.0,
                headers.as_ptr(),
                u32::MAX,
                WINHTTP_ADDREQ_FLAG_ADD,
            )
        };
        if added == 0 {
            return Err(last_os_error("WinHttpAddRequestHeaders failed"));
        }
    }

    // SAFETY: this GET sends no headers of its own and no body.
    let sent =
        unsafe { WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) };
    if sent == 0 {
        return Err(last_os_error("WinHttpSendRequest failed"));
    }

    // SAFETY: the reserved parameter must be null.
    let received = unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) };
    if received == 0 {
        return Err(last_os_error("WinHttpReceiveResponse failed"));
    }

    let mut status: u32 = 0;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    // SAFETY: with QUERY_FLAG_NUMBER the buffer is exactly one u32,
    // and `status_len` in/out describes it.
    let queried = unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_len,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(last_os_error("WinHttpQueryHeaders failed"));
    }

    Ok(HttpsRequest {
        _session: session,
        _connect: connect,
        request,
        status,
    })
}

/// Performs one bounded HTTPS GET and returns the body decoded as
/// text (10 s per-operation timeouts, [`HTTPS_BODY_LIMIT`],
/// [`HTTPS_READ_DEADLINE`]): callers run it on a worker thread.
/// Every failure names its stage.
pub fn https_get_text(host: &str, path: &str, request_headers: &str) -> io::Result<String> {
    let request = open_https_request(host, path, request_headers, 10_000)?;
    if request.status != 200 {
        return Err(io::Error::other(format!(
            "the update server answered HTTP {}",
            request.status
        )));
    }

    // Release metadata is a few KB; reserving it up front makes
    // the common case a single allocation. The deadline starts
    // once headers are in, so it bounds only the transfer itself.
    let mut body: Vec<u8> = Vec::with_capacity(4096);
    let read_started = std::time::Instant::now();
    loop {
        if read_started.elapsed() > HTTPS_READ_DEADLINE {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the update server timed out while sending its reply",
            ));
        }
        let mut available: u32 = 0;
        // SAFETY: single u32 out-parameter on a live request handle.
        let ok = unsafe { WinHttpQueryDataAvailable(request.request.0, &mut available) };
        if ok == 0 {
            return Err(last_os_error("WinHttpQueryDataAvailable failed"));
        }
        if available == 0 {
            break;
        }
        let start = body.len();
        let Some(new_len) = start.checked_add(available as usize) else {
            return Err(io::Error::other(
                "the update server sent an oversized reply",
            ));
        };
        if new_len > HTTPS_BODY_LIMIT {
            return Err(io::Error::other(
                "the update server sent an oversized reply",
            ));
        }
        body.resize(new_len, 0);
        let mut read: u32 = 0;
        // SAFETY: the tail slice is writable for `available` bytes
        // and WinHTTP writes at most that many, reporting the true
        // count in `read`.
        let ok = unsafe {
            WinHttpReadData(
                request.request.0,
                body[start..].as_mut_ptr() as *mut c_void,
                available,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(last_os_error("WinHttpReadData failed"));
        }
        body.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }

    String::from_utf8(body).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "the update server sent a non-text reply",
        )
    })
}

/// Reads one response header as text. The two-call pattern is the
/// documented contract: the first call fails while reporting the
/// byte length needed, the second fills a buffer of that size.
/// Absurd lengths are refused rather than allocated.
fn query_header_string(request: *mut c_void, query_flag: u32) -> Option<String> {
    let mut byte_len: u32 = 0;
    // SAFETY: null buffer with a zero length is the documented
    // sizing call; `byte_len` receives the required byte count.
    let _sizing = unsafe {
        WinHttpQueryHeaders(
            request,
            query_flag,
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut byte_len,
            std::ptr::null_mut(),
        )
    };
    if byte_len == 0 || byte_len > 64 * 1024 {
        return None;
    }
    let mut buffer = vec![0_u16; byte_len as usize / 2 + 1];
    // SAFETY: the buffer holds at least `byte_len` bytes and the
    // length in/out parameter describes it in bytes, per contract.
    let queried = unsafe {
        WinHttpQueryHeaders(
            request,
            query_flag,
            std::ptr::null(),
            buffer.as_mut_ptr() as *mut c_void,
            &mut byte_len,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return None;
    }
    buffer.truncate(byte_len as usize / 2);
    Some(
        String::from_utf16_lossy(&buffer)
            .trim_end_matches('\0')
            .trim()
            .to_string(),
    )
}

/// Outcome of one HTTP hop of a file download.
enum DownloadHop {
    Saved(u64),
    Redirect(String),
}

/// Streams one HTTPS GET into `dest` (created/truncated only once
/// a 200 is in hand), or reports the redirect target of a 3xx.
/// `downloaded` tracks bytes written for the UI's progress row;
/// `cancel` is honored between chunks. A transfer that stalls —
/// per-read WinHTTP receive timeout, with the no-progress check
/// as a backstop — or outgrows `max_bytes` fails cleanly.
fn download_hop(
    host: &str,
    path: &str,
    dest: &Path,
    max_bytes: u64,
    downloaded: &AtomicU64,
    cancel: &AtomicBool,
) -> io::Result<DownloadHop> {
    let request = open_https_request(host, path, "", 30_000)?;
    if (300..400).contains(&request.status) {
        if let Some(location) = query_header_string(request.request.0, WINHTTP_QUERY_LOCATION) {
            return Ok(DownloadHop::Redirect(location));
        }
    }
    if request.status != 200 {
        return Err(io::Error::other(format!(
            "the download server answered HTTP {}",
            request.status
        )));
    }

    let mut file = std::fs::File::create(dest).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("the update file couldn't be created next to MacroClicker: {error}"),
        )
    })?;
    let mut total: u64 = 0;
    downloaded.store(0, Ordering::Relaxed);
    let mut last_progress = std::time::Instant::now();
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "download cancelled",
            ));
        }
        if last_progress.elapsed() > std::time::Duration::from_secs(45) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the download timed out — no data arrived for 45 seconds",
            ));
        }
        let mut read: u32 = 0;
        // SAFETY: the chunk buffer is writable for its full length
        // and WinHTTP writes at most that many bytes, reporting
        // the true count in `read` (0 at end of body).
        let ok = unsafe {
            WinHttpReadData(
                request.request.0,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(last_os_error("WinHttpReadData failed"));
        }
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > max_bytes {
            return Err(io::Error::other(
                "the download was larger than GitHub said it would be",
            ));
        }
        file.write_all(&chunk[..read as usize])?;
        downloaded.store(total, Ordering::Relaxed);
        last_progress = std::time::Instant::now();
    }
    file.flush()?;
    file.sync_all()?;
    Ok(DownloadHop::Saved(total))
}

/// Downloads `https://host/path` into `dest`, exactly what the
/// verified self-update needs: bounded size, cancellable,
/// progress-reporting, redirect-hardened. WinHTTP normally
/// follows GitHub's redirect to its file CDN invisibly; if a 3xx
/// ever surfaces instead, each hop's `Location` must pass
/// [`crate::update::parse_https_url`] (HTTPS only, sane host, no
/// credentials or odd ports) and the hop count is capped, so the
/// downloader can never be walked somewhere unexpected. Returns
/// the byte count written; on any error the caller deletes the
/// partial file.
pub fn https_download_to_file(
    host: &str,
    path: &str,
    dest: &Path,
    max_bytes: u64,
    downloaded: &AtomicU64,
    cancel: &AtomicBool,
) -> io::Result<u64> {
    const MAX_REDIRECT_HOPS: u32 = 5;
    let mut host = host.to_string();
    let mut path = path.to_string();
    for _hop in 0..=MAX_REDIRECT_HOPS {
        match download_hop(&host, &path, dest, max_bytes, downloaded, cancel)? {
            DownloadHop::Saved(total) => return Ok(total),
            DownloadHop::Redirect(location) => {
                let (next_host, next_path) =
                    crate::update::parse_https_url(&location).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "the download server sent an unusable redirect",
                        )
                    })?;
                host = next_host;
                path = next_path;
                downloaded.store(0, Ordering::Relaxed);
            }
        }
    }
    Err(io::Error::other(
        "the download server sent too many redirects",
    ))
}

/// Opens a URL in the user's default handler (for an https link,
/// the default browser) via the shell. Fire-and-forget from the
/// caller's perspective; returns false only if the shell refused
/// outright (return values up to 32 are the documented failure
/// codes).
pub fn open_url(url: &str) -> bool {
    let operation = wide_null("open");
    let target = wide_null(url);
    // SAFETY: both strings are live and null-terminated for the
    // call; null owner/parameters/directory are the defaults.
    let result = unsafe {
        ShellExecuteW(
            0,
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    result > 32
}
