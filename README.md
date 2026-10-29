# MacroClicker

[![CI](https://github.com/Skid-ster/autoclickerBYARENA/actions/workflows/ci.yml/badge.svg)](https://github.com/Skid-ster/autoclickerBYARENA/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/Skid-ster/autoclickerBYARENA?label=latest%20release)](https://github.com/Skid-ster/autoclickerBYARENA/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2021H2%2B%20%7C%20Windows%2011%20x64-blue)](https://github.com/Skid-ster/autoclickerBYARENA/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-stable%20%7C%20edition%202021-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

**MacroClicker** is a Windows x64 auto-clicker written in stable Rust and
egui. Choose the familiar **Classic**, redesigned **Modern**, or customizable
**Custom** appearance, each in **Light** or **Dark**. An opt-in real-click
stability test reports timing for a selected configuration. The resizable
dashboard keeps the OP Auto Clicker 4.x controls visible: click
interval and random offset, mouse and repeat options, cursor picking, and
real **Record & Playback**.
A dedicated worker drives a precision QPC schedule while the UI stays
responsive; appearance and window size are remembered.

## MacroClicker vs OP Auto Clicker 4.1

A comparison of the main controls and capabilities:

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
| Classic and Modern, each Light or Dark + automatic scaling | No | Yes |
| Two-color custom gradient and real-click timing test | No | Yes |
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
clippy with warnings denied, standalone tests plus Windows app and worker
tests, release build, and embedded version/GUI-subsystem verification).
Releases also include `SHA256SUMS.txt` for offline verification. CI refuses a version tag
unless the tag, Cargo package version, and EXE FileVersion agree. Changes in
the [Unreleased changelog](CHANGELOG.md#unreleased) are source-only until a
new release is published.

MacroClicker updates itself after one explicit action. When an update is
confirmed, the footer **Update vX.Y.Z** button shows a small red **1**;
it is absent when no update is confirmed or an installation is in progress.
The button starts the complete sequence when no click or macro session is
active: download, exact size and SHA-256 verification, rollback-safe
replacement, and restart. The Updates window shows the
installed and latest versions, installation type, download size,
progress, and release notes. A recognized source checkout is handled
explicitly: its clean `main` branch is fast-forwarded to the exact
release tag before the verified release build is installed. No update
installs silently in the background.

No installer is required. The EXE is not code-signed, so on first launch
Windows SmartScreen may ask for confirmation. Verify the published
SHA-256 if the file was obtained from anywhere other than this
repository's GitHub Releases.

## Hardware and performance

MacroClicker targets **Windows 10 21H2+ / Windows 11 x64**. A reference
configuration for this work is an **Intel Core i5-13400, GeForce RTX 5060 Ti
8 GB and 16 GB RAM**, as reported by the project user. This configuration
has **not been benchmarked or tested natively** here; the software is not
locked to those components and results will vary on other systems too.

Click timing depends on the CPU, Windows scheduling, load, power settings,
privileges and the target application. The GPU may render the interface; it
does not accelerate the QPC click scheduler. On the i5-13400's hybrid cores,
Windows chooses where to run the worker: MacroClicker does not force P-core or
E-core affinity. During a session the worker requests time-critical priority
and a 1 ms timer period, and busy-spins; it can use a logical CPU and extra
power even at long intervals. Stop it when not needed, especially during
other demanding workloads. A denied priority/timer request is reported.

For a useful comparison on the reference PC, try **single clicks at 100 ms
with random offset off** on a safe disposable target, first with the machine
mostly idle and then under your normal workload. The 20-second timing test
can compare those runs. This is a starting point, not a claimed
optimum or guarantee; adjust the interval for your actual target. Windows,
drivers, display configuration and target behavior have not been measured
on the reported PC.

## Quick start

To build from source, use Windows 10 21H2+ or Windows 11 x64, a current
stable Rust toolchain (edition 2021), and the MSVC C++ build tools and Windows
SDK required by Rust's MSVC target. The release EXE does not need Rust or a
compiler installed.

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

- **Familiar controls, responsive layouts** — a naturally resizable window
  with the classic clicker controls visible and no dashboard scrolling:
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
  of action buttons. Tall windows use progressively narrower compositions:
  interval fields reflow, the middle groups stack, and the controls grow
  instead of leaving large gaps between sections or above the footer.
  Remaining height is balanced within the cards. At very large sizes the
  280 % zoom cap still applies. Secondary windows temporarily use a
  fitting zoom when needed, then restore the dashboard zoom on close;
  unusually long messages scroll or reveal their full text on hover.
  The minimum window size is refreshed across different display DPI
  settings without requesting a resize. A first run (no settings file yet) on a display larger than
  the 1080p reference opens proportionally larger once; every saved
  size is respected exactly. A Settings toggle turns the adaptation
  off. The size you choose is remembered across restarts.
  Click interval with the 4.1 Random offset row, side-by-side Click options
  and Click repeat, Cursor position with a real Pick location button
  (click the button, then click anywhere on screen; Esc cancels), and the
  4.1 button rows — Start/Stop, then hotkey settings and Record & Playback.
  Start refuses invalid configurations and Stop stays disabled until a
  session is running. Classic group titles break the border like the familiar
  dialog; Modern uses inset headings and its own typeface and controls.
- **Record & Playback that actually replays** — press Record and use the
  mouse anywhere on screen: every left/right/middle click is captured with
  its exact position and the measured gap since the previous click (clicks
  on MacroClicker's own window are ignored, so stopping the recording never
  pollutes the macro; the hotkey and F8 also stop it). Playback replays the
  sequence through the same QPC-scheduled worker as ordinary sessions —
  exact buttons, positions and delays, drift-free, loop N times or until
  stopped, with F8 panic-stop support during playback. Up to 2048 steps
  per macro. In the current source, playback stops with an error rather than
  clicking a different position if a recorded point is no longer on a display.
- **Appearance + automatic scaling** — open **Settings… → Appearance**
  and choose **Classic** or **Modern**, then **Light** or **Dark**. Modern
  has midnight-slate or cool porcelain surfaces, softly layered cards,
  teal-accented vector headings and Outfit typography with a distinct
  medium-weight emphasis. Checkboxes and radio choices have different shapes
  and clear selected and keyboard-focus states. A mint-to-blue Start action
  and original vector action icons clarify the main controls; icons give way
  to their labels when space is tight. Classic retains the familiar light and
  dark looks. An existing theme is never changed on upgrade:
  Midnight OLED, Terminal green and Aurora remain
  available under **Previous themes**. Classic Light is the fresh default.
  The choice is saved with your settings. Automatic adaptation can still be
  switched off, and saved scale values from older releases are preserved.
  The native Windows title bar follows Windows' app mode, independently of
  the in-app appearance.
- **Custom gradient** — choose **Custom** in Appearance,
  select Light or Dark, then expand **Customize gradient**. Pick two RGB
  colors, rotate the gradient from 0–359°, adjust its strength and the
  procedurally generated matte texture, or reset it. The dashboard, cards,
  dialogs and primary action use the colors; the surface gradient follows a
  shared window-wide direction rather than restarting in each card. Surfaces
  are softened and accents adapted to keep text readable. The texture is generated locally,
  cached, and can be turned off; no third-party image download is needed.
  The choice persists independently of the Classic and Modern appearances.
- **Real-click stability test** — open **Settings… →
  Open stability test…**, choose 10–60 seconds and explicitly start the test.
  Progress, exact cycle counts, late percentage, average cadence and the worst
  observed deadline are shown, with an optional timing-only copy report. This
  sends real mouse input; read the safety instructions below before use.
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
  to `%APPDATA%\op-auto-clicker\settings.conf` and reload on launch. The
  current source preserves an unreadable or oversized file and explains how
  to repair it in Settings instead of overwriting it with defaults.
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
- 153 standalone tests over platform-independent modules in the current
  source, plus app and worker tests and a Windows CI workflow (format check,
  Clippy with warnings denied, tests, release build and binary artifact), and
  a generated crosshair application icon.

The GUI, input injection and worker threads are Windows-specific and need
hands-on Windows acceptance; see [`autoclicker/README.md`](autoclicker/README.md)
for the full manual checklist and
[`docs/VERIFICATION.md`](docs/VERIFICATION.md) for the verification record.

## Using the real-click stability test

**Warning: this test sends real clicks.** They can activate or change whatever
is at your current cursor position or the configured fixed point, including in
other applications. Use a disposable, safe target and never test over
important controls or data.

1. Set the interval, button, click type, offset and cursor/fixed position on
   the dashboard. Open **Settings… → Open stability test…**.
2. Choose **10–60 seconds**, then press **Start real-click test**. A
   three-second countdown gives you time to move the cursor. For a fixed
   position, the saved point is used regardless of cursor movement.
3. The test snapshots the current click settings and ignores a finite repeat
   limit for this run only; your saved settings are not changed. **Stop**, an
   enabled start/stop hotkey, **F8**, or closing the test window cancels it.
   Hotkeys are polled by the UI and depend on it being responsive. The worker
   independently limits its scheduled waits to the test deadline and attempts
   to release a pressed button; an OS call can still delay completion.
4. A **good timing** result requires the full duration, at least 20 completed
   cycles, no engine warning, at most 5% late cycles, no observed deadline
   more than 250 ms late, and no more than 250 ms end overrun. A cycle is
   late when already overdue before scheduling or more than 1 ms late at
   dispatch. Double/triple bursts count as one cycle. Cancellation, failure,
   warnings or too few cycles never pass; long intervals can leave too few
   samples even when the clicker works normally.
5. Compare completed cycles, the rounded late percentage, average cycles per
   second and worst deadline lateness. **Copy timing report** is optional and
   copies the timing summary only—not cursor coordinates, machine identifiers,
   click settings other than duration, or raw error details. The test only
   requests a clipboard copy when you choose that action.

The result uses exact worker counters and an independent maximum even if the
bounded telemetry ring drops records. Worst deadline lateness is measured
against the original deadline before any catch-up re-anchoring, including a
deadline missed as the test ends. The result describes this run and reports
input-dispatch errors; it **cannot prove that another application received
the clicks**, detect every kind of stall, or guarantee future stability.

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

MacroClicker is open source under the [MIT License](LICENSE). The bundled
Outfit font is separately licensed under the SIL Open Font License;
see [font attribution and license](autoclicker/assets/fonts/README.md), or
open its full license offline from Modern Settings.
