# OP 4.1 Killer — Engine A (C++20 / Dear ImGui / Win32 / DirectX 11)

Native Windows x64 auto-clicker with a QPC spin-scheduled worker thread,
Gaussian humanization, global `SendInput` injection and background HWND
message delivery.

## Requirements

- Windows 10 21H2 or Windows 11, x64
- Visual Studio 2022 (MSVC v143, C++20) with the Windows 10/11 SDK
- CMake 3.24 or newer (network access on first configure to fetch Dear ImGui
  `v1.90.4` from GitHub)

## Build commands

Open "x64 Native Tools Command Prompt for VS 2022" and run:

```
cmake -S . -B build -G "Visual Studio 17 2022" -A x64
cmake --build build --config Release --parallel
```

The executable is `build/Release/AutoClickerEngineA.exe`.

For a debug build replace `Release` with `Debug`. The MSVC options are fixed
in `CMakeLists.txt`: `/W4 /permissive- /EHsc /std:c++20 /utf-8`.

## Run

```
build\Release\AutoClickerEngineA.exe
```

## Test commands

```
cmake --build build --config Release
ctest --test-dir build --build-config Release --output-on-failure
```

Three CTest entries are registered:

- `test_humanize` — Box-Muller Gaussian bounds, clamped ranges, measured
  mean/sigma over 50,000 samples, zero-sigma determinism.
- `test_ring_buffer` — SPSC ring FIFO, power-of-two wrap, drop-newest back
  pressure and `try_push`, dropped counter, telemetry counters, statistics
  moments.
- `test_config` — invalid intervals, negative sigma, inverted clamps, hold
  ranges, background HWND requirement, sub-millisecond acceptance.

The test translation units are pure standard C++20 and also compile on Linux
with `g++ -std=c++20 -Wall -Wextra -Wpedantic -Werror`.

## Architecture

| File | Responsibility |
| --- | --- |
| `src/main.cpp` | WinMain, window class, D3D11 device/swap chain, ImGui bootstrap, message loop, resize and cleanup |
| `src/app.cpp` | Dashboard UI, target picker, commands, telemetry drain, CPS graph and histogram rendering, F8 polling |
| `src/click_engine.cpp` | Dedicated worker, atomic config snapshot, time-critical priority, RAII timer resolution, QPC spin loop, panic checks |
| `src/input_dispatcher.cpp` | Arrayed `SendInput` global mode and `PostMessageW` / `SendMessageTimeoutW` background mode |
| `src/humanize.cpp` | Thread-local MT19937_64 seeded from `random_device`, steady clock and QPC; Box-Muller Gaussian samples with clamping |
| `src/telemetry.cpp` | Lock-free SPSC ring buffer, atomic counters, sliding-window statistics and histogram |
| `src/timing.cpp` | QPC wrappers, `timeBeginPeriod(1)` RAII guard, `_mm_pause` spin wait (no `Sleep`) |
| `src/window_target.cpp` | `EnumWindows` enumeration, `IsWindow` validation, cached client geometry for minimized targets |
| `src/theme.cpp` | Programmatic cyberpunk ImGui style |
| `src/config.h` | Configuration snapshot and validation |

### Timing and threading rules

- The worker thread parks on a manual-reset event (`WaitForSingleObject`)
  while idle; it never busy-spins and the UI thread never blocks on it.
- Active scheduling is a rate-based deadline loop driven by
  `QueryPerformanceCounter`. The spin body uses `_mm_pause`; there is no
  `Sleep` or equivalent anywhere in the click path.
- `timeBeginPeriod(1)` is acquired through an RAII guard for the active
  session and released with `timeEndPeriod(1)` when the session ends.
- `SetThreadPriority(THREAD_PRIORITY_TIME_CRITICAL)` is verified; denial is
  surfaced as a visible dashboard warning and a log entry.
- Config updates use `std::atomic<std::shared_ptr<const EngineConfig>>`;
  telemetry uses a single-producer/single-consumer lock-free ring buffer.
- F8 (or the panic button) sets an atomic flag sampled before every click and
  on every spin iteration, including the hold-delay spin. The button is always
  released before the worker exits a panicked hold.

### Input dispatch

- Global mode builds `INPUT` arrays and calls `SendInput`, checking the
  returned injected count. Fixed coordinates use
  `MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE` with virtual-screen normalized
  0..65535 mapping; current-cursor mode never moves the pointer.
- Background mode validates the HWND, caches the client rectangle (reused and
  clamped while minimized), then posts `WM_MOUSEMOVE`, `WM_LBUTTONDOWN`
  (`MK_LBUTTON`) and `WM_LBUTTONUP` with `MAKELPARAM` client coordinates.
- The legacy `mouse_event` / `keybd_event` APIs are never used.

## Manual acceptance checklist

1. Launch the executable; the cyberpunk dashboard renders at 60 FPS with no
   console window.
2. Set 10 CPS, global mode, current cursor, Start: clicks register in a text
   editor; actual CPS, mean, sigma and the graph update live.
3. Drag the sigma field to a large value while running: the histogram widens
   immediately and the measured sigma tracks the configured sigma.
4. Set an invalid value (for example interval 0): the dashboard shows the
   validation error and Start is rejected.
5. Switch to background mode, Refresh, pick an unfocused Notepad window:
   client-coordinate clicks land while Notepad remains unfocused.
6. Minimize the target and run again: clicks continue using the cached client
   rectangle; invalidating the HWND produces a visible target-loss error.
7. Press F8 mid-run: the session stops inside one interval, the state banner
   turns red, and no mouse button remains stuck down.
8. Configure a sub-millisecond interval: missed-deadline counting increases
   honestly when the machine cannot keep up.
9. Close the window: the worker is signaled and joined cleanly with no
   process hang or leaked handles.
