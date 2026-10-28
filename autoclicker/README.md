# MacroClicker (Rust / eframe egui / windows-sys)

**MacroClicker** — a Windows x64 auto-clicker written in stable Rust with an
egui interface that reproduces the authentic classic OP Auto Clicker 4.x
Win32 dialog — a fixed,
non-resizable light-gray window with native-style group boxes and push
buttons, every control visible without scrolling. It is backed by a
dedicated QPC spin-scheduled worker thread, uniform/Gaussian timing jitter,
global `SendInput` injection and background HWND message delivery.

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
`target/release/macro_clicker.exe`.

## Run

```
cargo run --release
```

## Test commands

```
cargo test --release
```

Eight integration test binaries run platform-independent logic (67 tests):

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
- `macro_tests` — the Record & Playback bookkeeping: the first step keeps
  the lead-in delay, later steps store measured gaps (clamped so 0 ms or
  non-monotonic polls never schedule an impossible click), the 2048-step
  cap refuses further clicks, clear resets the timeline, loop indexing
  wraps exactly, and finite loop counts produce an exact saturating click
  budget while "until stopped" stays unlimited.
- `schedule_tests` — the pure absolute-schedule arithmetic: exact drift-free
  cadence when on time, jitter within the catch-up slack absorbed without
  re-anchoring, a stall beyond the slack re-anchoring to "now" instead of
  firing a burst of instant make-up clicks, strictly monotonic deadlines,
  saturating arithmetic at the extremes and the slack-boundary behavior.
- `hotkey_tests` — supported virtual-key ranges, F8/Esc reservation, display
  names, the default F6/Classic-theme/100 % preferences, theme key/label
  stability and the window-scale sanitizer.
- `settings_tests` — full serialization round trip, tolerance of unknown
  keys, sections and comments, per-field recovery from malformed values,
  round trips for all four themes, fallback to the default theme for
  retired theme keys, legacy `dark_mode` migration precedence and
  `ui_scale` clamping.
- `icon_tests` — dimensions, transparent rounded corners, solid accent
  center, two-axis symmetry, the presence of both accent and background
  pixels, anti-aliased edges with a fully opaque core, and byte-for-byte
  deterministic generation of the procedural 32x32 application icon.

The tests include the platform-independent sources by path, so they compile
and pass on any platform even though the full binary is Windows-only.

## Interface

The main window is a pixel-faithful, fixed-size (560x456 logical points,
non-resizable, non-scrolling) recreation of the classic OP Auto Clicker 4.x
dialog. The **Settings…** button in the bottom-right corner opens a
Settings window with two persisted choices:

**Theme** — four complete themes, each tuned so every text, status and
accent color clears a WCAG 4.5:1 contrast ratio against its background:

- **Classic light (default)** — the classic native look: `#F0F0F0` face,
  white sunken numeric fields and combo boxes (white in every state — idle,
  hovered, pressed, open and disabled — with dark, readable text and light
  dropdown lists), gray 3D push buttons.
- **Dark** — the same dialog in Windows 11 dark-dialog colors: `#202020`
  face, `#2C2C2C` sunken fields with light text, matching dropdowns,
  buttons and group borders.
- **Midnight OLED** — a pure-black variant of Dark for OLED screens.
- **Terminal green** — green-on-black phosphor-terminal colors.

**Window scale** — 75 / 100 / 125 / 150 / 175 / 200 %. The UI zoom factor
follows the percentage and the app then re-applies its fixed logical size,
so the physical window really grows or shrinks and no control is ever cut
off (the resize lands one frame after the zoom, once egui reports the new
factor as applied).

Legacy settings files from the dark-mode-only build are still honored:
`dark_mode=true` loads as the Dark theme, `dark_mode=false` as Classic, and
a modern `theme=` key always wins over the legacy boolean.

All four themes share every layout metric, so switching never moves a
control.
In both, the bordered group boxes draw their titles on a face-colored gap
that breaks the top border, exactly like the native `BS_GROUPBOX` control,
and the viewport height hugs the content with roughly 11 px below the
footer row.

One honest platform note: the native OS title bar belongs to Windows, not
to the app — it follows the system app mode (black when Windows 11 is in
dark app mode), and egui/eframe 0.27 has no API to recolor the title bar of
a native-decorated window. A custom title bar would break the
classic-dialog goal, so the stock one is kept as-is; picking a matching
dark-family in-app theme is the supported way to make the window and title
bar agree.

- **Click interval** (full width) — four white numeric boxes for hours /
  mins / secs / milliseconds with labels to the right of each box, defaulting
  to 100 ms, followed by the 4.1 **Random offset ± [40] milliseconds** row.
  The offset is checkbox-gated: ticking it reveals/arms the magnitude (40 ms
  the first time, matching the reference build); unticked, timing is exact.
- **Click options** (left group box) — classic combo boxes for Mouse button
  (Left / Right / Middle) and Click type (Single / Double / Triple). Double
  and triple emit two or three down/up bursts separated by a short QPC-spun
  gap — never a sleep.
- **Click repeat** (right group box) — Repeat N times, or Repeat until
  stopped (the default); the worker auto-stops at the count.
- **Cursor position** (full width) — Current location (default) or Pick
  location radio, a real gray **Pick location** button that captures the next
  left-click anywhere on screen via `GetCursorPos` (Esc cancels, the button
  doubles as "Cancel pick" while armed), plus white X/Y boxes.
- **2x2 push-button grid** — **Start (F6)** and **Stop (F6)** (Stop is
  greyed out until a session is running; Start is disabled while running or
  while the interval is invalid), then **Hotkey setting** and
  **Record & Playback**.
- A single reserved one-line status strip under the group boxes shows
  semantic colored state (Ready / Running / Stopping / Panic / Warning /
  Error) without ever changing the window layout, and a status-bar style
  footer row: a short key reminder on the left (it tracks the actual
  assigned hotkey, e.g. "F6 start/stop · F8 emergency stop") and the
  **Settings…** button on the right.

Every push button is painted with full interaction feedback: the fill,
border and elevation all track the pointer (an eased ~180 ms hover lift
with a soft drop shadow — depth encodes interactivity — and a pressed
state that visibly sits down) plus a one-pixel top bevel highlight like
the native BS_PUSHBUTTON face, and floating surfaces (Settings, Hotkey
dialog, dropdowns) carry a soft shadow so layering reads as hierarchy.
The status strip leads with a semantic state dot (shape and position
carry the state, not color alone) that breathes gently on the existing
repaint heartbeat while a session runs, focused fields draw an accent
focus ring, and the dark themes give group captions their accent color
so the hierarchy reads at a glance.

The **Hotkey setting** dialog remaps the start/stop key (F1–F24, A–Z, 0–9,
Space); F8 is permanently reserved for Panic Stop and cannot be reassigned,
which the dialog states explicitly. Assignment reacts only to a fresh key
press made after clicking "Assign new key", so keys already held are never
captured by accident, and the toggle hotkey is inert while the dialog is
listening. Hotkeys use `GetAsyncKeyState`, so they work while the window is
unfocused. A generated crosshair icon is set on the window (title bar,
taskbar and Alt-Tab) without shipping any binary asset.

There is no diagnostics surface: every clicking control lives on the one
main screen, and the Settings window carries only the theme picker, the
window-scale picker and **Reset all settings to factory defaults**, so the
main dialog keeps its exact classic control set.

Settings persist automatically to
`%APPDATA%\op-auto-clicker\settings.conf` (a hand-editable, tolerant
key=value document) and reload on launch. A stale window handle from a
previous session is never reused.

## Architecture

| File | Responsibility |
| --- | --- |
| `src/main.rs` | `eframe::run_native` entry point, viewport setup, platform gating |
| `src/app.rs` | Classic Win32-style egui dialog (fixed-size, non-scrolling), Settings and hotkey windows, macro record/playback flow, hotkey polling, location pick, settings persistence |
| `src/engine.rs` | Dedicated worker thread, atomic config snapshot, time-critical priority, RAII timer resolution, QPC spin loop, single/double/triple bursts, panic checks |
| `src/timing.rs` | QPC wrappers, `timeBeginPeriod(1)` RAII guard, `spin_loop` spin wait (no sleep) |
| `src/win32.rs` | Narrow FFI wrappers for `SendInput`, `PostMessageW`, `SendMessageTimeoutW`, `GetCursorPos`, `GetAsyncKeyState`, `EnumWindows`, events and QPC; each unsafe block has a SAFETY comment |
| `src/hotkey.rs` | Virtual-key constants, hotkey validation and key names (platform-independent, fully tested) |
| `src/icon.rs` | Procedurally generated 32x32 RGBA crosshair icon via signed-distance pixel math; no binary asset |
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
- The engine-level background delivery path (retained in `src/engine.rs`
  and `src/win32.rs`, not exposed in the UI) validates the HWND, caches
  the client rectangle in atomics
  for minimized targets, clamps client coordinates, then delivers
  `WM_MOUSEMOVE`, the button-specific down message (`WM_LBUTTONDOWN`,
  `WM_RBUTTONDOWN` or `WM_MBUTTONDOWN` with the matching `MK_*` flag) and its
  up message with packed `MAKELPARAM` coordinates through `PostMessageW` (or
  `SendMessageTimeoutW` when configured).
- `mouse_event` and `keybd_event` are never used.

## Manual acceptance checklist

1. Run `cargo run --release`; the fixed-size light-gray classic window
   appears with every control visible (no scrollbars, cannot be resized) and
   stays responsive while idle; Stop is greyed out until a session runs.
2. Leave the defaults (100 ms, Left, Single, Repeat until stopped, Current
   location), press Start or F6: clicks register in a text editor at ~10 CPS;
   the status line updates and Stop becomes enabled.
3. Tick Random offset (the magnitude box shows 40 ms): observed intervals
   vary uniformly between 60 ms and 140 ms; unticking it restores exact
   timing.
4. Choose Right button and Double click: each repetition sends two right
   down/up bursts; Triple sends three.
5. Pick location: press Pick location (the button becomes "Cancel pick"),
   then left-click the target point anywhere on screen; X/Y populate and
   clicks land there even after you move the mouse away. Esc while armed
   cancels the pick without changing the stored point.
6. Set Repeat to 25 times: the session auto-stops at 25 and the status line
   reports completion.
7. Open Hotkey setting, assign F7, close the dialog: F7 now starts and stops
   the session; trying to assign F8 is rejected; Esc cancels capture.
   Crucially, test this **unfocused**: click into another app (or a game)
   and press the hotkey — the session must start and stop from there too,
   because the app keeps polling on a 20 Hz heartbeat even while idle and
   in the background. On launch the status strip shows the idle line
   ("Ready.") rather than sitting blank, and Ready-kind confirmations like
   "Session complete — 25 clicks delivered." or "Factory defaults
   restored." are visibly painted.
8. Close and reopen the app: every setting (interval, offset, button, type,
   repeat, position, hotkey, theme, window scale) is restored from
   `%APPDATA%`.
9. Open Settings… and press **Reset all settings to factory defaults**:
   every control returns to its default (100 ms, Left, Single, repeat
   until stopped, Current location, F6, Classic light at 100 %) and the
   status strip confirms "Factory defaults restored."
10. Press F8 mid-run: the session stops within one interval, the status turns
    red and no mouse button remains stuck.
11. Open Settings…, pick each of the four themes in turn: every window
    restyles in place and nothing moves; dark themes keep readable text in
    every field, button and dropdown state.
12. Start a "Repeat until stopped" session, then press Stop: the status
    shows "Stopping…" for at most a moment and then settles on
    "Stopped — N clicks delivered." — it must never remain stuck on
    "Stopping…". While running, the status dot pulses gently; hovering
    any enabled button lifts it slightly with a soft shadow; pressing it
    sits it back down.
13. Still in Settings…, set Window scale to 150 %: the window grows and
    every control stays fully visible; set it back to 100 % and the window
    returns to its original size; both choices survive a restart.
14. Close the window: the worker is signaled, joined and its event handle
    closed cleanly.
15. The title bar and taskbar read "MacroClicker" in every theme.
16. Open Record & Playback, press Record, click 3–4 spots in another
    window (clicks on MacroClicker itself are ignored), press Stop
    recording: the step count matches. Press Play with "Play 2 times":
    every spot is clicked in order twice with your original rhythm, then
    "Playback complete" appears. Playback honors F8 panic and the Stop
    button (a manual stop reports "Playback stopped — N clicks
    delivered."); "Play until stopped" loops until stopped.
