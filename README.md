# OP Auto Clicker

A Windows x64 auto-clicker written in stable Rust with an egui interface that
reproduces the authentic classic OP Auto Clicker 4.x Win32 dialog — a fixed,
non-resizable light-gray window (with six persisted themes — including a
MogClicker meme theme — and a 75–200 % window scale) with
group boxes, sunken numeric fields and classic push buttons, nothing
scrolled: Hours/Mins/Secs/Milliseconds interval with the 4.1 random-offset
row, mouse button and click-type pickers, repeat rules and cursor position
with on-screen point picking. A dedicated worker
thread drives a QPC spin schedule while the UI stays fully responsive.

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

- **Authentic classic OP Auto Clicker 4.x screen** — a fixed-size,
  non-resizable light-gray Win32-style dialog (group boxes, white numeric
  fields, gray 3D buttons) with every control visible and no scrolling:
  Click interval with the 4.1 Random offset row, side-by-side Click options
  and Click repeat, Cursor position with a real Pick location button
  (click the button, then click anywhere on screen; Esc cancels), and a
  2x2 Start/Stop/Hotkey setting/Advanced button grid. Start refuses invalid
  configurations and Stop stays greyed until a session is running. The
  560x448 viewport hugs the content; the group-box titles sit on the face
  color and break the top border like the native control, and every numeric
  field and dropdown keeps readable contrast in all states.
- **Six themes + window scaling** — a **Settings…** button in the
  bottom-right corner opens a Settings window with a theme picker (Classic
  light, Dark, Midnight OLED, Terminal green, Vaporwave, and the
  **MogClicker** meme theme, which retitles the window "MogClicker", swaps
  the window/taskbar icon to a hot-pink edition, paints the group captions
  hot pink and re-skins the labels: the Start button becomes **Mog**, Stop
  becomes **It's over**, the groups become "Mogging interval" / "Mog
  options" / "Mog repeat" / "Aura position", and the status lines speak
  fluent meme — "+1000 aura per click", "Aura cooling down…", "everyone
  brutally frame-mogged") and a window-scale picker (75–200 %):
  the whole UI zooms and the window resizes itself so every control stays
  visible. Both choices restyle every window (main screen, Hotkey dialog,
  Advanced window and its charts), are remembered across restarts, and all
  themes share the exact same layout, so switching never moves a control.
  (The native OS title bar follows the Windows app mode — black when
  Windows is in dark app mode; eframe/egui 0.27 cannot recolor native
  window chrome, so picking a dark-family theme is the way to make the
  window match a black title bar.)
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
