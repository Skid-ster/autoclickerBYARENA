# OP Auto Clicker

A Windows x64 auto-clicker written in stable Rust with an egui dashboard that
follows the OP Auto Clicker 4.1 layout: Hours/Mins/Secs/Milliseconds interval,
random offset, mouse button and click-type pickers, repeat rules and cursor
position. A dedicated worker thread drives a QPC spin schedule while the UI
stays fully responsive.

## Quick start

Requirements: Windows 10 21H2+ or Windows 11 x64 and a Rust stable
toolchain (edition 2021). The Rust MSVC target uses the Visual Studio Build
Tools linker, which `rustup` sets up automatically on a standard install.

```
cd autoclicker
cargo build --release
cargo test --release
cargo run --release
```

The executable is `autoclicker/target/release/op_auto_clicker.exe`.

## Features

- **OP Auto Clicker 4.1-style screen** — four familiar cards (Click Interval,
  Click Options, Click Repeat, Cursor Position) in one compact dark window,
  with a live Valid/Invalid indicator and a Start button that refuses invalid
  configurations.
- Left, right and middle mouse buttons; single, double and triple click
  types (multi-clicks use QPC-spun gaps, never sleeps).
- Hours/Mins/Secs/Milliseconds interval fields plus 4.1's uniform
  **Random offset ± ms**, with optional expert Gaussian humanization behind
  the Advanced window.
- Remappable global start/stop hotkey (default **F6**) with an assignment
  dialog; **F8** is a permanently reserved panic stop.
- Pick location captures the live cursor (`GetCursorPos`); settings persist
  to `%APPDATA%\op-auto-clicker\settings.conf` and reload on launch.
- Dedicated click worker thread — the render thread never runs timing code;
  QPC spin scheduling with no sleep calls, idle parking on a Win32
  manual-reset event, RAII `timeBeginPeriod(1)` and time-critical thread
  priority with verified failure reporting.
- Global arrayed `SendInput` mode and background HWND message mode
  (button-specific `WM_*BUTTON*` messages, `MK_*` flags, `MAKELPARAM`)
  supporting unfocused and minimized targets.
- Lock-free single-producer/single-consumer telemetry ring, live CPS graph,
  interval histogram, consistency percentage and missed-deadline count in
  the Advanced window.
- 44 unit tests over platform-independent modules, plus a Windows CI
  workflow (format check, clippy with warnings denied, tests, release build
  and binary artifact), and a generated crosshair application icon.

The GUI, input injection and worker threads are Windows-specific and are
acceptance-tested on Windows; see [`autoclicker/README.md`](autoclicker/README.md)
for the full manual checklist and
[`docs/VERIFICATION.md`](docs/VERIFICATION.md) for the verification record.

## Repository layout

| Path | Purpose |
| --- | --- |
| `autoclicker/` | The Rust/egui application (the single shipped engine) |
| `.github/workflows/ci.yml` | Windows MSVC build, test and artifact pipeline |
| `docs/VERIFICATION.md` | Toolchains, test evidence, defect history and limitations |
