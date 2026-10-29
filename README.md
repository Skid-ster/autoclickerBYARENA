# MacroClicker

[![CI](https://github.com/Skid-ster/autoclickerBYARENA/actions/workflows/ci.yml/badge.svg)](https://github.com/Skid-ster/autoclickerBYARENA/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/Skid-ster/autoclickerBYARENA?label=latest%20release)](https://github.com/Skid-ster/autoclickerBYARENA/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2021H2%2B%20%7C%20Windows%2011%20x64-blue)](https://github.com/Skid-ster/autoclickerBYARENA/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-stable%20%7C%20edition%202021-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

**MacroClicker** is a Windows x64 auto-clicker written in stable Rust with an
egui interface that keeps the authentic classic OP Auto Clicker 4.x
dialog layout in a modern card surface (the visual language of its
Updates window: rounded elevated cards, an accent primary **Start**,
soft shadows) — a naturally resizable window whose classic
560x456 size is the enforced minimum (with five
persisted themes, a 75–200 % window scale and a remembered window size)
with group cards, numeric fields and push buttons, nothing
scrolled: Hours/Mins/Secs/Milliseconds interval with the 4.1 random-offset
row, mouse button and click-type pickers, repeat rules, cursor position
with on-screen point picking, and the 4.1 bottom-row **Record & Playback** —
here a real click-sequence recorder that replays through the precision
engine. A dedicated worker thread drives a QPC spin schedule while the UI
stays fully responsive.

## MacroClicker vs OP Auto Clicker 4.1

Everything 4.1 does, plus the parts it cannot:

| Capability | OP Auto Clicker 4.1 | MacroClicker |
| --- | --- | --- |
| Interval fields (h/m/s/ms) + Random offset ± ms | Yes | Yes |
| Left/right/middle, single/double/triple | Yes | Yes |
| Repeat N times / until stopped | Yes | Yes |
| Current location / Pick location X,Y | Yes | Yes |
| Remappable background hotkey (default F6) | Yes | Yes |
| Record & Playback | Yes | Yes — exact positions, buttons **and gaps**, loop counts, F8 panic mid-playback |
| Drift-free absolute QPC schedule with bounded catch-up | No | Yes |
| Dedicated panic key (F8) always reserved | No | Yes |
| Free resizing, maximize/snap and fit-to-window UI scaling | No* | Yes |
| Five themes + 75–200 % window scaling | No | Yes |
| Built-in updates against GitHub releases | No | Yes — one click downloads, SHA-256-verifies, installs and restarts |
| Open source with Windows CI and automated tests | No | Yes |

\* Resizing/scaling is the researched part: OP's public 4.1 feature
pages list no resize or UI-scale feature; the winget-catalogued 4.1
portable build (SHA-256 `1ce7da6f…a6ec1`) is publicly analysed as a
native AutoIt v3 program (AutoIt's default dialogs are non-sizable
and non-maximizable), and the inspected open-source Orphamiel line
is a fixed-size WinForms dialog. The exact internal GUI calls of
later closed builds are not public, so no stronger claim is made.

## Download

Download the ready-to-run **`macro_clicker.exe`** from the
[latest GitHub release](https://github.com/Skid-ster/autoclickerBYARENA/releases/latest).
Every version tag is compiled by the Windows CI workflow (format check,
clippy with warnings denied, 128 standalone tests plus Windows app and
worker tests, release build, and embedded version/GUI-subsystem verification).
Releases also include `SHA256SUMS.txt` for offline verification. CI refuses a version tag
unless the tag, Cargo package version, and EXE FileVersion agree.

MacroClicker updates itself after one explicit action. The footer
**Update vX.Y.Z** button starts the complete sequence when no click or
macro session is active: download, exact size and SHA-256 verification,
rollback-safe replacement, and restart. The Updates window shows the
installed and latest versions, installation type, download size,
progress, and release notes. A recognized source checkout is handled
explicitly: its clean `main` branch is fast-forwarded to the exact
release tag before the verified release build is installed. No update
installs silently in the background.

No installer is required. The EXE is not code-signed, so on first launch
Windows SmartScreen may ask for confirmation. Verify the published
SHA-256 if the file was obtained from anywhere other than this
repository's GitHub Releases.

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

The executable is `autoclicker/target/release/macro_clicker.exe`.
When that executable is launched from this repository, the Updates
window identifies it as a **Source checkout build**. Choosing an update
fast-forwards a clean `main` checkout to the release tag and restarts
into the matching verified release build; local tracked changes stop
the update before the program file is replaced.

## Features

- **Authentic classic OP Auto Clicker 4.x screen, modern surface** — the
  classic dialog's exact layout as rounded elevated cards with an
  accent-filled Start button (white numeric fields, flat buttons), in a
  naturally resizable window with every control visible and no scrolling:
  drag any edge or corner, maximize, or snap with Win+arrow / Snap
  Layouts, exactly like any normal Windows app. The classic 560x456 size
  is the OS-enforced minimum, so the window can never shrink far enough
  to hide a control; making it larger widens the group boxes and the
  button grid so every border stays flush with the content edges, and
  the whole interface scales continuously (in 1 % increments) to fit
  the window — wider, taller, snapped or maximized — measured
  zoom-independently so it can never feed back into itself. Growth
  eases smoothly (~180 ms) onto the exact fit once a drag settles;
  shrinkage requests a smaller fit without growth easing, and the physical
  minimum is compensated for the current fit so a large window can always be
  dragged back down without restarting. Total zoom is capped at
  280 %. Panoramic
  windows rearrange the middle groups three-across with a single row
  of action buttons; tall, narrow windows reflow interval fields into two
  rows and stack the middle groups so their controls can grow. The layout
  itself stretches to the window's full height, groups flowing from the top with the status and
  buttons riding at the bottom edge, so a big window is filled at
  any size or resolution, never a small dialog marooned in empty
  space. A first run (no settings file yet) on a display larger than
  the 1080p reference opens proportionally larger once; every saved
  size is respected exactly. A Settings toggle turns the adaptation
  off. The size you choose is remembered across restarts.
  Click interval with the 4.1 Random offset row, side-by-side Click options
  and Click repeat, Cursor position with a real Pick location button
  (click the button, then click anywhere on screen; Esc cancels), and the
  4.1 button rows — Start/Stop, then Hotkey setting/Record & Playback.
  Start refuses invalid configurations
  and Stop stays greyed until a session is running. The group-box titles
  sit on the face color and break
  the top border like the native control, and every numeric field and
  dropdown keeps readable contrast in all states.
- **Record & Playback that actually replays** — press Record and use the
  mouse anywhere on screen: every left/right/middle click is captured with
  its exact position and the measured gap since the previous click (clicks
  on MacroClicker's own window are ignored, so stopping the recording never
  pollutes the macro; the hotkey and F8 also stop it). Playback replays the
  sequence through the same QPC-scheduled worker as ordinary sessions —
  exact buttons, positions and delays, drift-free, loop N times or until
  stopped, and F8 panic-stops instantly mid-playback. Up to 2048 steps
  per macro.
- **Five themes + window scaling** — a **Settings…** button in the
  bottom-right corner opens a Settings window with a theme picker (Classic
  light, Dark, Midnight OLED, Terminal green and the modern Aurora — every one tuned to a
  WCAG 4.5:1 contrast floor for all text, status and accent colors) and a
  window-scale picker (75–200 %): the whole UI zooms and the window only
  ever grows — and only if the controls would otherwise no longer fit —
  so a size you chose by dragging is never forced back down. Both choices restyle
  every window (main screen, Hotkey dialog and Settings window), are
  remembered across restarts, and all themes share the exact same layout,
  so switching never moves a control. Every theme renders the groups as
  elevated cards (Classic's are white on the light-gray dialog, like the
  Updates window's plate) over their sunken fields and status plate, and
  the status dot gives one short pulse
  whenever the app changes state, so Start/Stop/pick transitions are
  visible without reading the text.
  (The native OS title bar follows the Windows app mode — black when
  Windows is in dark app mode; eframe/egui 0.27 cannot recolor native
  window chrome, so picking a dark-family theme is the way to make the
  window match a black title bar.)
- Left, right and middle mouse buttons; single, double and triple click
  types (multi-clicks use QPC-spun gaps, never sleeps).
- Hours/Mins/Secs/Milliseconds interval fields plus 4.1's uniform
  **Random offset ± ms** — every clicking control lives on the one main
  screen, with nothing hidden behind extra windows.
- Remappable global start/stop hotkey (default **F6**) that works from
  anywhere — the app keeps a light 20 Hz background heartbeat alive even
  while idle and unfocused, so pressing the hotkey inside a game or any
  other app starts and stops clicking (while running, the poll tightens
  to every 16 ms) — with an assignment
  dialog; **F8** is a permanently reserved panic stop.
- Pick location captures the live cursor (`GetCursorPos`); settings persist
  to `%APPDATA%\op-auto-clicker\settings.conf` and reload on launch.
- Dedicated click worker thread — the render thread never runs timing code;
  QPC spin scheduling with no sleep calls, idle parking on a Win32
  manual-reset event, RAII `timeBeginPeriod(1)` and time-critical thread
  priority with verified failure reporting. A UI-side watchdog detects a
  dead worker thread and recovers the controls instead of reporting a
  running session forever, engine locks recover from poisoning rather
  than cascading panics, and settings are written by atomic temp-file
  replacement, with failed saves retained for retry.
- A semantic status dot leads the status strip (shape and position carry
  the state, not color alone) and breathes gently while a session runs;
  every status change ends in an explicit confirmation line.
- Global arrayed `SendInput` click delivery driven by a QPC scheduler that
  keeps an absolute, drift-free cadence with bounded catch-up, so an OS
  stall is absorbed instead of being repaid as a burst of fast clicks.
- 128 standalone tests over platform-independent modules, plus Windows
  app and worker tests and a CI workflow (format check, clippy with warnings
  denied, tests, release build and binary artifact), and a generated crosshair application icon.

The GUI, input injection and worker threads are Windows-specific and need
hands-on Windows acceptance; see [`autoclicker/README.md`](autoclicker/README.md)
for the full manual checklist and
[`docs/VERIFICATION.md`](docs/VERIFICATION.md) for the verification record.

## Repository layout

| Path | Purpose |
| --- | --- |
| `autoclicker/` | The Rust/egui application (the single shipped engine) |
| `.github/workflows/ci.yml` | Windows MSVC build, test, release, and artifact pipeline |
| `CHANGELOG.md` | User-visible changes by release |
| `docs/releases/` | Checked-in public notes for each tagged release |
| `docs/RELEASING.md` | Versioning, release preparation, and publication checks |
| `docs/VERIFICATION.md` | Toolchains, test evidence, defect history and limitations |

## Project documents

- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [Changelog](CHANGELOG.md)
- [Release process](docs/RELEASING.md)
- [Verification record](docs/VERIFICATION.md)

## License

MacroClicker is open source under the [MIT License](LICENSE).
