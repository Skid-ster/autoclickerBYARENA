# OP 4.1 Killer — Engine B (Rust / eframe egui / windows-sys)

Windows x64 auto-clicker written in stable Rust with an egui dashboard, a
dedicated QPC spin-scheduled worker thread, Gaussian humanization, global
`SendInput` injection and background HWND message delivery.

## Requirements

- Windows 10 21H2 or Windows 11, x64
- Rust stable toolchain (edition 2021), 1.75 or newer recommended
- Network access on first build to fetch the pinned crates from crates.io

Dependencies are pinned exactly in `Cargo.toml`: `eframe = "=0.27.2"`,
`egui = "=0.27.2"` and `windows-sys = "=0.52.0"` with the
`Win32_Foundation`, `Win32_UI_WindowsAndMessaging`,
`Win32_UI_Input_KeyboardAndMouse`, `Win32_System_Threading`,
`Win32_System_Performance`, `Win32_Media` and `Win32_Graphics_Gdi` features.

## Build commands

```
cargo build --release
```

The release profile is fixed to `opt-level = 3`, `lto = true`,
`codegen-units = 1` and `panic = "abort"`. The binary is
`target/release/autoclicker_engine_b.exe`.

## Run

```
cargo run --release
```

## Test commands

```
cargo test --release
```

Three integration test binaries run platform-independent logic:

- `humanize_tests` — Box-Muller determinism, seeded RNG reproducibility,
  Gaussian mean/sigma over 50,000 samples, zero-sigma exactness and clamp
  guarantees.
- `telemetry_tests` — SPSC ring FIFO, slot wrapping, oldest-overwrite and
  dropped counters, telemetry store counters, statistics moments and
  histogram totals.
- `config_tests` — invalid intervals, negative sigma, inverted clamps, hold
  ranges, background HWND requirement, sub-millisecond acceptance and the
  atomic configuration snapshot round trip.

The tests include `config.rs`, `humanize.rs` and `telemetry.rs` by path, so
they compile and pass on any platform even though the full binary is
Windows-only.

## Architecture

| File | Responsibility |
| --- | --- |
| `src/main.rs` | `eframe::run_native` entry point, viewport setup, platform gating |
| `src/app.rs` | egui cyberpunk dashboard, target picker, controls, painter-drawn CPS graph and histogram, F8 polling |
| `src/engine.rs` | Dedicated worker thread, atomic config snapshot, time-critical priority, RAII timer resolution, QPC spin loop, panic checks |
| `src/timing.rs` | QPC wrappers, `timeBeginPeriod(1)` RAII guard, `spin_loop` spin wait (no sleep) |
| `src/win32.rs` | Narrow FFI wrappers for `SendInput`, `PostMessageW`, `SendMessageTimeoutW`, `EnumWindows`, `IsWindow`, `GetClientRect`, `ClientToScreen`, thread priority, events and QPC; each unsafe block has a SAFETY comment |
| `src/humanize.rs` | Thread-local SplitMix64 PRNG (OS/QPC entropy), Box-Muller Gaussian samples with clamping |
| `src/telemetry.rs` | Lock-free SPSC ring buffer over `Cell` slots, atomic counters, sliding-window statistics |
| `src/config.rs` | Configuration struct, validation and the all-atomic runtime snapshot |

### Timing and threading rules

- The egui update/render thread never runs the timing loop. The worker parks
  on a manual-reset Win32 event (`WaitForSingleObject`) while idle, so idle
  CPU footprint stays near zero and no continuous repaint is requested.
- Active scheduling is a rate-based QPC deadline loop; the spin body is
  `std::hint::spin_loop` and there is no `std::thread::sleep` in the active
  click path.
- `timeBeginPeriod(1)` is acquired by an RAII guard per session and paired
  with `timeEndPeriod(1)` on drop.
- `SetThreadPriority(THREAD_PRIORITY_TIME_CRITICAL)` is verified; denial is
  surfaced on the dashboard and in the log.
- Configuration crosses threads through `AtomicConfigSnapshot` (one atomic per
  field); telemetry uses a lock-free single-producer/single-consumer ring.
  No mutex is taken on the per-click hot path.
- F8 (or the panic button) sets an atomic flag sampled before every click and
  on every spin iteration, including the hold-delay spin. The mouse button is
  always released before the worker leaves a panicked hold.

### Input dispatch

- Global mode submits arrayed `INPUT` structures to `SendInput`, including a
  two-element move-plus-down array for fixed coordinates using
  `MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE` with virtual-screen normalized
  0..65535 mapping. The injected count is checked against the request.
- Background mode validates the HWND, caches the client rectangle in atomics
  for minimized targets, clamps client coordinates, then delivers
  `WM_MOUSEMOVE`, `WM_LBUTTONDOWN` (`MK_LBUTTON`) and `WM_LBUTTONUP` with
  packed `MAKELPARAM` coordinates through `PostMessageW` (or
  `SendMessageTimeoutW` when configured).
- `mouse_event` and `keybd_event` are never used.

## Manual acceptance checklist

1. Run `cargo run --release`; the cyberpunk dashboard appears and stays
   responsive while idle without continuous repainting.
2. Set 10 CPS, global mode, current cursor, Start: clicks register in a text
   editor; actual CPS, mean, sigma and the live graph update.
3. Increase sigma while running: the histogram widens immediately because the
   atomic config snapshot is reloaded each click.
4. Enter an invalid value such as a zero interval: Start is rejected with the
   validation message.
5. Switch to background mode, Refresh, select an unfocused Notepad window:
   client-coordinate clicks land while Notepad remains unfocused.
6. Minimize the target and run again: clicks continue via the cached client
   rectangle; an invalidated HWND produces a visible target-loss error.
7. Press F8 mid-run: the session stops within one interval, the state banner
   turns red and no mouse button remains stuck.
8. Configure a sub-millisecond interval: missed deadlines are counted
   honestly when the machine cannot keep up.
9. Close the window: the worker is signaled, joined and its event handle
   closed cleanly.
