# OP Auto Clicker (Rust / eframe egui / windows-sys)

Windows x64 auto-clicker written in stable Rust with an egui dashboard that
follows the OP Auto Clicker 4.1 layout, backed by a dedicated QPC
spin-scheduled worker thread, uniform/Gaussian timing jitter, global
`SendInput` injection and background HWND message delivery.

## Requirements

- Windows 10 21H2 or Windows 11, x64
- Rust stable toolchain (edition 2021), 1.75 or newer recommended
- Network access on first build to fetch the pinned crates from crates.io

Dependencies are pinned exactly in `Cargo.toml`: `eframe = "=0.27.2"`,
`egui = "=0.27.2"` and `windows-sys = "=0.52.0"`.

## Build commands

```
cargo build --release
```

The release profile is fixed to `opt-level = 3`, `lto = true`,
`codegen-units = 1`, `incremental = false`, `debug = false` and
`panic = "abort"`. The binary is
`target/release/op_auto_clicker.exe`.

## Run

```
cargo run --release
```

## Test commands

```
cargo test --release
```

Five integration test binaries run platform-independent logic (40 tests):

- `config_tests` — invalid intervals, negative sigma, inverted clamps, hold
  ranges, background HWND requirement, sub-millisecond acceptance, the atomic
  snapshot round trip, h/m/s/ms interval helpers, random-offset validation,
  and the 4.1 baseline defaults.
- `humanize_tests` — Box-Muller determinism, seeded RNG reproducibility,
  Gaussian mean/sigma over large samples, zero-sigma exactness, clamp
  guarantees, and the uniform +/- random-offset bounds and distribution.
- `telemetry_tests` — SPSC ring FIFO, slot wrapping, drop-newest back
  pressure, dropped counters, real two-thread producer/consumer traffic,
  statistics moments and histogram totals.
- `hotkey_tests` — supported virtual-key ranges, F8/Esc reservation, display
  names and the default F6 preference.
- `settings_tests` — full serialization round trip, tolerance of unknown
  keys, sections and comments, and per-field recovery from malformed values.

The tests include the platform-independent sources by path, so they compile
and pass on any platform even though the full binary is Windows-only.

## Interface

The main window mirrors the four OP Auto Clicker 4.1 cards in one compact
dark screen:

- **Click Interval** — Hours / Mins / Secs / Milliseconds numeric fields plus
  the 4.1 **Random offset ± ms** uniform jitter. A live `ms · CPS` readout and
  a Valid/Invalid indicator (clicker-rs style) make every edit checkable; an
  invalid configuration disables Start.
- **Click Options** — Mouse button (Left / Right / Middle) and Click type
  (Single / Double / Triple). Double and triple emit two or three down/up
  bursts separated by a short QPC-spun gap — never a sleep.
- **Click Repeat** — Repeat until stopped, or Repeat N times (the worker
  auto-stops at the count).
- **Cursor Position** — Current location, or Pick location with X/Y fields
  and a Pick button that captures the live cursor via `GetCursorPos`.
- **Footer** — solid green **Start (F6)**, outlined **Stop (F6)**, red
  **Panic (F8)**, plus Hotkey setting, Reset and Advanced buttons.
- **Status line** — semantic colored state (Ready / Running / Stopping /
  Panic / Warning / Error) under the header, with click counters.

The **Hotkey setting** dialog remaps the start/stop key (F1–F24, A–Z, 0–9,
Space); F8 is permanently reserved for Panic Stop and cannot be reassigned.
Hotkeys use `GetAsyncKeyState`, so they work while the window is unfocused.

The **Advanced** window keeps the power-user features without disturbing the
4.1 layout:

- Global `SendInput` vs background window-message delivery, with a
  refreshable target-window picker, client coordinates and synchronous
  delivery option.
- Gaussian humanization with all nine expert fields and live validation.
- Live telemetry: metrics grid, real-time CPS sparkline, interval histogram
  and the recent-activity log.

Settings persist automatically to
`%APPDATA%\op-auto-clicker\settings.conf` (a hand-editable, tolerant
key=value document) and reload on launch. A stale window handle from a
previous session is never reused.

## Architecture

| File | Responsibility |
| --- | --- |
| `src/main.rs` | `eframe::run_native` entry point, viewport setup, platform gating |
| `src/app.rs` | 4.1-style egui dashboard, dialogs, hotkey polling, settings persistence, telemetry painting |
| `src/engine.rs` | Dedicated worker thread, atomic config snapshot, time-critical priority, RAII timer resolution, QPC spin loop, single/double/triple bursts, panic checks |
| `src/timing.rs` | QPC wrappers, `timeBeginPeriod(1)` RAII guard, `spin_loop` spin wait (no sleep) |
| `src/win32.rs` | Narrow FFI wrappers for `SendInput`, `PostMessageW`, `SendMessageTimeoutW`, `GetCursorPos`, `GetAsyncKeyState`, `EnumWindows`, events and QPC; each unsafe block has a SAFETY comment |
| `src/hotkey.rs` | Virtual-key constants, hotkey validation and key names (platform-independent, fully tested) |
| `src/humanize.rs` | Thread-local SplitMix64 PRNG (OS/QPC entropy), Box-Muller Gaussian and uniform +/- offset samples with clamping |
| `src/settings.rs` | Dependency-free key=value settings serialization and tolerant parsing (platform-independent, fully tested) |
| `src/telemetry.rs` | Lock-free SPSC ring buffer over atomic `Atom` slots (`Send + Sync`, no `unsafe`), drop-newest back pressure, sliding-window statistics |
| `src/config.rs` | Configuration struct, validation, h/m/s/ms helpers and the all-atomic runtime snapshot |

### Timing and threading rules

- The egui update/render thread never runs the timing loop. The worker parks
  on a manual-reset Win32 event (`WaitForSingleObject`) while idle, so idle
  CPU footprint stays near zero and no continuous repaint is requested.
- Active scheduling is a rate-based QPC deadline loop; the spin body is
  `std::hint::spin_loop` and there is no `std::thread::sleep` in the active
  click path — including inter-click gaps inside double/triple bursts.
- `timeBeginPeriod(1)` is acquired by an RAII guard per session and paired
  with `timeEndPeriod(1)` on drop.
- `SetThreadPriority(THREAD_PRIORITY_TIME_CRITICAL)` is verified; denial is
  surfaced on the dashboard and in the log.
- Configuration crosses threads through `AtomicConfigSnapshot` (one atomic
  per field); telemetry uses a lock-free single-producer/single-consumer
  ring. No mutex is taken on the per-click hot path.
- F8 (or the panic button) sets an atomic flag sampled before every click and
  on every spin iteration, including hold and inter-click spins. The mouse
  button is always released before the worker leaves a panicked hold.

### Input dispatch

- Global mode submits arrayed `INPUT` structures to `SendInput`, including a
  two-element move-plus-down array for fixed coordinates using
  `MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE` with virtual-screen normalized
  0..65535 mapping. Left, right and middle buttons use their respective
  down/up flags; the injected count is checked against the request.
- Background mode validates the HWND, caches the client rectangle in atomics
  for minimized targets, clamps client coordinates, then delivers
  `WM_MOUSEMOVE`, the button-specific down message (`WM_LBUTTONDOWN`,
  `WM_RBUTTONDOWN` or `WM_MBUTTONDOWN` with the matching `MK_*` flag) and its
  up message with packed `MAKELPARAM` coordinates through `PostMessageW` (or
  `SendMessageTimeoutW` when configured).
- `mouse_event` and `keybd_event` are never used.

## Manual acceptance checklist

1. Run `cargo run --release`; the compact 4.1-style window appears and stays
   responsive while idle.
2. Leave the defaults (100 ms, Left, Single, Repeat until stopped, Current
   location), press Start or F6: clicks register in a text editor at ~10 CPS;
   the counters and status line update.
3. Set Random offset to 40 ms: observed intervals vary uniformly between
   60 ms and 140 ms (visible in the Advanced histogram).
4. Choose Right button and Double click: each repetition sends two right
   down/up bursts; Triple sends three.
5. Pick location: press Pick while hovering a target point; X/Y populate and
   clicks land there even after you move the mouse away.
6. Set Repeat to 25 times: the session auto-stops at 25 and the status line
   reports completion.
7. Open Hotkey setting, assign F7, close the dialog: F7 now starts and stops
   the session; trying to assign F8 is rejected; Esc cancels capture.
8. Close and reopen the app: every setting (interval, offset, button, type,
   repeat, position, hotkey) is restored from `%APPDATA%`.
9. In Advanced, switch to background mode, Refresh, select an unfocused
   Notepad window: client-coordinate clicks land while Notepad stays
   unfocused; a dead HWND produces a visible target-loss error.
10. Press F8 mid-run: the session stops within one interval, the status turns
    red and no mouse button remains stuck.
11. Close the window: the worker is signaled, joined and its event handle
    closed cleanly.
