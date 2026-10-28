# MacroClicker (Rust / eframe egui / windows-sys)

**MacroClicker** — a Windows x64 auto-clicker written in stable Rust with an
egui interface that reproduces the authentic classic OP Auto Clicker 4.x
Win32 dialog — a naturally resizable light-gray window (the classic size
is the enforced minimum) with native-style group boxes and push
buttons, every control visible without scrolling at any window size. It is backed by a
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

Ten integration test binaries run platform-independent logic (103 tests):

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
  names, the default F6/Classic-theme/100 %/classic-size preferences, theme
  key/label stability and the window-scale and window-size sanitizers.
- `settings_tests` — full serialization round trip, tolerance of unknown
  keys, sections and comments, per-field recovery from malformed values,
  round trips for all five themes, fallback to the default theme for
  retired theme keys, legacy `dark_mode` migration precedence,
  `ui_scale` clamping and window-size round trip, clamping to
  [560x456, 4x that] and per-dimension garbage/non-finite tolerance.
- `icon_tests` — dimensions, transparent rounded corners, solid accent
  center, two-axis symmetry, the presence of both accent and background
  pixels, anti-aliased edges with a fully opaque core, and byte-for-byte
  deterministic generation of the procedural 32x32 application icon.
- `update_tests` — version parsing and ordering (including `v`-prefixes,
  padded components and unorderable-tag refusal), hardened
  latest-release JSON tag extraction (escaped lookalikes and bad
  candidates are skipped), validated release-page/direct-download
  URL construction, stale-check refresh timing, "checked … ago"
  wording, actionable failure headlines, and the full upgrade
  decision (available / up to date with the version GitHub actually
  reported / clean error) over canned GitHub responses, extraction
  of the release asset's size and SHA-256 digest (without ever
  borrowing a neighboring asset's), the dependency-free SHA-256
  against the FIPS 180-4 vectors in one-shot and streamed forms,
  hex-digest round trips, the strict HTTPS redirect gate, and the
  download-progress wording.
- `self_update_tests` — the verified self-replacement over real
  temp files: the AMD64 PE gate, file hashing, acceptance of
  exactly the published bytes (one flipped byte or a truncated
  file fails), the replace-with-backup swap, rollback when the
  download is missing or the new program won't launch, clearing
  a crashed attempt's stale backup, development-build (`target/`)
  detection, and leftover-file cleanup that touches nothing else.

The tests include the platform-independent sources by path, so they compile
and pass on any platform even though the full binary is Windows-only.

## Interface

The main window keeps the classic OP Auto Clicker 4.x dialog's exact
control layout in a modern card surface — the same visual language as
the Updates window: rounded elevated group cards led by an accent
tick, an accent-filled **Start** button (the one primary action, like
**Install** there), flat secondary buttons, and soft shadows — in a
non-scrolling window that resizes like any normal Windows app: drag any edge to push or pull it, grab any corner, double-click or
maximize from the title bar, and use Win+arrow keys and Snap Layouts.
The classic 560x456 size is the fit-independent minimum. The app
declares it to the OS divided by the currently displayed window fit,
because egui-winit multiplies the declared live-point minimum by the
total zoom; the resulting physical floor therefore follows your own
Window scale percentage but never the transient fit, so neither a drag
nor a snap can take the window below the size where every control fits
and a large window can always be pulled back down. Growing the window is designed, not stretched:
group boxes always claim the full content width (their borders stay
flush at every size), the side-by-side Click options / Click repeat pair
stays equal in width and height, and the button grid shares the extra
width up to a content cap (beyond that the content column centers
instead of producing over-wide buttons). Extra height loosens the
gaps between groups by a bounded amount and gathers the rest above
the status strip, so the groups flow from the top and the status,
buttons and footer ride at the window's bottom edge at any height —
the window is filled top-to-bottom, never a fixed island centered
in empty space. On top of that, the
window size drives the presentation itself (Settings → "Scale and
rearrange with window size", on by default): the interface fits
itself to the window **continuously**. The decision is made in
neutral points — the live point size times the total zoom already
applied, i.e. the physical size at 100 % zoom — which is invariant
under the very zoom changes it drives, so feedback is mathematically
impossible. That neutral size is fitted uniformly against the
active design canvas (560x456 for the classic stack, 904x456 for
the wide layout): the smaller of the width and height ratios is
the fit factor, so the dialog scales proportionally, the limiting
axis fills exactly, and nothing is stretched or clipped. The
result (divided by your own Window scale percentage and capped so
the total never exceeds 280 %) is rounded down to a whole percent.
A growing fit is adopted after the size has held still for a few
frames and the applied zoom **eases** to it over ~180 ms (cubic
ease-out); a shrinking fit is adopted on the frame the window gets
smaller, so an inward drag never has to fight a stale scale or wait
for a restart. A small dead band keeps pixel rounding from nudging
the fit back and forth. The easing is presentation-only — the fit's
neutral-point measure uses the *applied* zoom, so it stays exact at
every intermediate value, and the landing always sets the exact fit
factor. The wide three-across layout engages in
panoramic windows (aspect and predicted live width both qualify,
with arrive-and-hold hysteresis). A fresh install on a monitor
larger than the 1920x1080 reference opens once at a proportionally
larger size (so 1440p first runs are not comparatively tiny); an
existing settings file — even one that deliberately stores the
classic floor — is always respected exactly. Sizes are remembered
in fit-independent points, so the same physical window comes back
on every launch whatever fit factor is active. The size you leave the window at is saved
to the settings file and restored on the next launch (maximized state
is never saved as a raw size: restoring comes back at your last normal
size). The **Settings…** button in the bottom-right corner opens a
Settings window with two persisted choices:

**Theme** — five complete themes, each tuned so every text, status and
accent color clears a WCAG 4.5:1 contrast ratio against its background:

- **Classic light (default)** — `#F0F0F0` face with the groups as white
  elevated cards and a white status plate (exactly the Updates window's
  surfaces), white numeric fields and combo boxes (white in every state —
  idle, hovered, pressed, open and disabled — with dark, readable text and
  light dropdown lists), flat light push buttons with the accent Start.
- **Dark** — the same dialog in Windows 11 dark-dialog colors: `#202020`
  face, group boxes as `#282828` elevated cards with a soft shadow and a
  rim-lit top edge, `#1E1E1E` sunken fields with light text, a recessed
  `#181818` status readout, matching dropdowns, buttons and group
  borders. The other dark themes share the same layered depth system.
- **Midnight OLED** — a pure-black variant of Dark for OLED screens.
- **Terminal green** — green-on-black phosphor-terminal colors.
- **Aurora modern** — deep indigo-slate surfaces with a violet accent,
  the contemporary dark look; shares the dark themes' layered depth
  (elevated cards, sunken fields, status well).

**Window scale** — 75 / 100 / 125 / 150 / 175 / 200 %. Changing the
percentage first re-derives the window fit from the unchanged neutral
window size, so the new percentage never multiplies a stale fit. One
frame later, once egui reports the new total zoom as applied, the app
re-declares the fit-compensated minimum (560x456 divided by the
displayed auto-fit; egui-winit converts points to physical pixels with
the total zoom, leaving a physical floor of 560x456 at the chosen
percentage) and enlarges the window only if the new zoom left it below
that floor. A window you dragged larger is never forced back down by
a scale change, and no control is ever cut off.

Legacy settings files from the dark-mode-only build are still honored:
`dark_mode=true` loads as the Dark theme, `dark_mode=false` as Classic, and
a modern `theme=` key always wins over the legacy boolean.

All five themes share every layout metric at every window size, so
switching never moves a control.
Each group card's caption rides its top border on a two-surface gap
(panel color above, card color below), led by a short accent tick,
and at the classic size the window hugs the content exactly as before.

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
  **Updates…** and **Settings…** buttons on the right. The footer
  button mirrors the checker in place (**Checking…**, or
  **Update vX.Y.Z** when a download is ready). The Updates window is
  a centered status card with a state glyph and the three facts
  needed to trust it — **Installed**, **Latest on GitHub** and
  **Last checked** — so "up to date" always names the version GitHub
  actually reported (and a source build newer than the latest
  release says so). It checks once per launch, refreshes a stale
  result when the window is opened, and can always be re-run with
  **Check now**; failures resolve to a plain-language headline with
  the technical detail underneath, never an endless spinner. The
  action button stays greyed out until an update is proven, then
  **Install vX.Y.Z** updates in place: it streams the new EXE next
  to the running one with a progress bar and a working **Cancel**,
  verifies its exact size and SHA-256 fingerprint against the
  values GitHub published for that release (no published
  fingerprint, no install — the browser download is offered
  instead), moves the running EXE aside, puts the verified one
  in its place, launches it and closes; any failure along the
  way restores the current build untouched and says so. The new
  build confirms with "Updated from v… — now running v…" and
  sweeps the leftover file. **Release notes** and **Copy download
  link** remain beside it, and if Windows refuses to open the
  browser the window says so and the copy-link path still works.

Every push button is painted with full interaction feedback: the fill,
border and elevation all track the pointer (an eased ~180 ms hover lift
with a soft drop shadow — depth encodes interactivity — and a pressed
state that visibly sits down). **Start** is the accent-filled primary
action; **Stop** carries a danger-colored outline while a session runs;
the rest stay neutral. Floating surfaces (Settings, Hotkey dialog,
dropdowns) carry a soft shadow so layering reads as hierarchy.
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
| `src/main.rs` | `eframe::run_native` entry point, native resizable viewport with persisted size and classic minimum, platform gating |
| `src/app.rs` | Classic Win32-style egui dialog (resizable above the classic 560x456 floor, non-scrolling), window-size restore/observation, Settings and hotkey windows, macro record/playback flow, hotkey polling, location pick, settings persistence |
| `src/engine.rs` | Dedicated worker thread, atomic config snapshot, time-critical priority, RAII timer resolution, QPC spin loop, single/double/triple bursts, panic checks |
| `src/timing.rs` | QPC wrappers, `timeBeginPeriod(1)` RAII guard, `spin_loop` spin wait (no sleep) |
| `src/win32.rs` | Narrow FFI wrappers for `SendInput`, `PostMessageW`, `SendMessageTimeoutW`, `GetCursorPos`, `GetAsyncKeyState`, `EnumWindows`, events and QPC; each unsafe block has a SAFETY comment |
| `src/hotkey.rs` | Virtual-key constants, hotkey validation and key names (platform-independent, fully tested) |
| `src/icon.rs` | Procedurally generated 32x32 RGBA crosshair icon via signed-distance pixel math; no binary asset |
| `src/humanize.rs` | Thread-local SplitMix64 PRNG (OS/QPC entropy), Box-Muller Gaussian and uniform +/- offset samples with clamping |
| `src/settings.rs` | Dependency-free key=value settings serialization and tolerant parsing (platform-independent, fully tested) |
| `src/telemetry.rs` | Lock-free SPSC ring buffer over atomic `Atom` slots (`Send + Sync`, no `unsafe`), drop-newest back pressure, sliding-window statistics |
| `src/config.rs` | Configuration struct, validation, h/m/s/ms helpers and the all-atomic runtime snapshot |
| `src/update.rs` | Pure update decisions: version ordering, release/asset JSON scanning (tag, size, SHA-256 digest), validated release/download/redirect URLs, refresh policy, error headlines and a dependency-free streaming SHA-256 |
| `src/self_update.rs` | Verified self-replacement over `std` file ops: PE gate, size+fingerprint verification, rename-aside/swap-in with rollback, undo after a failed launch, stale-file cleanup, Cargo-target detection |

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

1. Run `cargo run --release`; the light-gray classic window
   appears with every control visible (no scrollbars) and
   stays responsive while idle; Stop is greyed out until a session runs.
   The window carries a normal resizable frame: the right resize cursor
   appears on every edge and diagonal arrows on the corners.
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
   repeat, position, hotkey, theme, window scale and the window size from
   step 17) is restored from
   `%APPDATA%`.
9. Open Settings… and press **Reset all settings to factory defaults**:
   every control returns to its default (100 ms, Left, Single, repeat
   until stopped, Current location, F6, Classic light at 100 %) and the
   status strip confirms "Factory defaults restored."
10. Press F8 mid-run: the session stops within one interval, the status turns
    red and no mouse button remains stuck.
11. Open Settings…, pick each of the five themes in turn: every window
    restyles in place and nothing moves; dark themes keep readable text in
    every field, button and dropdown state.
12. Start a "Repeat until stopped" session, then press Stop: the status
    shows "Stopping…" for at most a moment and then settles on
    "Stopped — N clicks delivered." — it must never remain stuck on
    "Stopping…". While running, the status dot pulses gently; hovering
    any enabled button lifts it slightly with a soft shadow; pressing it
    sits it back down.
13. Still in Settings…, set Window scale to 150 % with the window at its
    smallest size: the window grows and every control stays fully
    visible. Drag the window generously larger, then switch the scale
    between 100 % and 150 %: the scale change must never shrink the
    window back down; both scale choices survive a restart.
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
17. Resizing: drag each edge in and out, then a corner toward the middle
    and back out: the window follows with the OS resize cursors and
    stops at the classic 560x456 footprint scaled by your own UI
    percentage — it must not go smaller, and every control stays whole
    at the limit. Grow the window in any direction and let go: just
    after the drag ends, the whole interface rescales once to fit the
    new size, **easing** smoothly onto the new scale in a fraction of
    a second rather than snapping (text, fields and buttons all
    physically larger, proportions unchanged); growth must not rescale
    repeatedly while you are still dragging, and the physical window
    size itself must not jump. Then, without reopening the app, drag
    the large window straight back down — including from a maximized or
    200 %-scale window: the edge must keep following the pointer, the
    interface must fit down during the inward drag, and it must stop
    only at the same classic footprint, never stranded at the large
    size. Pull it panoramic:
    the three middle groups sit three-across with one row of four
    action buttons. Pull it tall: the gaps between groups loosen and
    the status, buttons and footer stay pinned at the bottom. Snap
    it to half of a 1440p screen and maximize it: in both cases the
    interface fills the window at a large, readable scale — never a
    small dialog marooned in empty space. Double-click the title
    bar and use Win+arrow / Snap Layouts: snap left and right must
    work on normal desktop resolutions (a snap zone narrower than
    560 points is refused, which is expected). Leave the window at a
    non-standard size, close and reopen: it comes back at that size
    and scale. Maximize, close and reopen: it comes back
    unmaximized at the last normal dragged size. Finally delete the
    settings file and relaunch on the large monitor: the first-run
    window opens proportionally larger than the classic dialog; put
    the settings file back and confirm the saved size wins again.
18. Updates: with the machine online, open **Updates…** — a stale or
    missing result refreshes on open, and the status card must show
    **Installed**, **Latest on GitHub** and **Last checked: just now**
    with the verdict naming the version GitHub reported. The real
    end-to-end rehearsal is the v1.3.0 → v1.4.0 hop — the first
    physical self-update: v1.3.0's footer must turn into **Update
    v1.4.0** without opening anything, and pressing **Install
    v1.4.0** in its window must download, verify, and reopen as
    v1.4.0. In any v1.3.0+ window, **Install vX.Y.Z** must be greyed out unless a
    newer release is proven; when clicked it shows live download
    progress, then "Verifying…", and the app closes and reopens by
    itself as the new version with the status strip confirming
    "Updated from v… — now running v…", settings and window size
    intact and no `macro_clicker.exe.old-*` file left beside the
    EXE. Repeat and press **Cancel download** mid-transfer: it must
    stop, report "nothing was changed", and the current build must
    keep working. **Release notes** opens that version's GitHub
    page and **Copy download link** puts the exact asset URL on the
    clipboard. Disconnect the network and press **Check now**: the
    window must settle on a plain-language failure with the
    technical detail under it and a **Try again** button — never an
    endless spinner. If an update was already confirmed before the
    network dropped, a failed refresh must keep that update
    installable and say it is showing the last confirmed result.
    Untick **Check for updates on startup** and restart: no check
    runs in the background (the footer stays **Updates…**), while
    opening the window counts as asking and refreshes then. Finally
    confirm the safety rails: run a copy from `target\release`
    (it must offer the browser download, never **Install**), and
    run one from a read-only folder (the install must fail cleanly
    with the current version untouched).
