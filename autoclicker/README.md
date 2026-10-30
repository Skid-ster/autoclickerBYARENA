# MacroClicker (Rust / eframe egui / windows-sys)

**MacroClicker** is a Windows x64 auto-clicker written in stable Rust and
egui. The familiar clicker controls are available in **Classic**, **Modern**
and customizable **Custom** styles, each with **Light** and **Dark** modes.
An opt-in real-click timing test measures a selected configuration. The
dashboard remains resizable and non-scrolling at supported sizes. It is backed by a
dedicated QPC spin-scheduled worker, uniform/Gaussian timing jitter, global
`SendInput` injection and background HWND message delivery.

## Requirements

- Windows 10 21H2 or Windows 11, x64
- A current stable Rust toolchain (edition 2021); CI uses stable
- Network access on first build to fetch the pinned crates from crates.io

One unbenchmarked reference configuration reported by the user is an Intel
Core i5-13400, GeForce RTX 5060 Ti 8 GB and 16 GB RAM. The app is not locked
to that hardware. Windows controls hybrid-core placement; the program does
not pin the worker to a P-core or E-core, and the GPU does not accelerate
click scheduling. Timing, display scaling and input behavior depend on the
CPU, Windows configuration, workload, display and target application. The
active worker requests time-critical priority and a 1 ms timer period, then
busy-spins and may consume a logical CPU and additional power even at long
intervals. Tune and verify on the actual machine; no native benchmark has been
run on the reported PC. See the [hardware guidance](../README.md#hardware-and-performance)
in the root README.

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

Fourteen integration test binaries run platform-independent logic (153 test
cases, including the self-update module's fault-injection cases). Windows
also runs the dashboard and click-worker unit tests:

- `config_tests` — invalid intervals, negative sigma, inverted clamps, hold
  ranges, background HWND requirement, sub-millisecond acceptance, the atomic
  snapshot round trip and concurrent coherent reads, h/m/s/ms interval helpers, random-offset validation,
  long-interval commits, and the 4.1 baseline defaults.
- `coordinates_tests` — pixel round trips across representative sizes and
  every supported axis extent, negative origins, bounds and overflow checks,
  and independent signed mouse-message coordinate words.
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
- `stability_tests` — explicit late-cycle, worst-deadline and end-overrun
  thresholds; sample minimums, invalid/short runs, warnings, failures,
  interruptions, rounded metrics and privacy-limited reports. No real input
  is sent.
- `texture_tests` — deterministic, bounded procedural material texture
  generation, including checks across every row and column.
- `schedule_tests` — the pure absolute-schedule arithmetic: exact drift-free
  cadence when on time, jitter within the catch-up slack absorbed without
  re-anchoring, a stall beyond the slack re-anchoring to "now" instead of
  firing a burst of instant make-up clicks, strictly monotonic deadlines,
  saturating arithmetic at the extremes and the slack-boundary behavior.
- `hotkey_tests` — supported virtual-key ranges, F8/Esc reservation, display
  names, the default F6/Classic-theme/100 %/classic-size preferences,
  style/mode mapping, theme key stability and window-size sanitizers.
- `settings_tests` — full serialization round trip, tolerance of unknown
  keys, sections and comments, per-field recovery from malformed values,
  round trips for all current and previous themes, fallback to the default
  theme for retired keys, legacy `dark_mode` migration precedence,
  `ui_scale` clamping and window-size round trip, clamping to
  [560x456, 4x that], per-dimension garbage/non-finite tolerance, and
  complete-file replacement, failed-write recovery, bounded reads and
  invalid UTF-8 rejection.
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
  file fails), replace-with-backup swaps, injected rename and rollback
  failures, preservation of recovery files, unique per-attempt cleanup,
  development-build (`target/`) detection, recognized source-checkout
  layout and origin checks, source-change reporting, and manifest-version
  extraction.
- `window_layout_tests` — classic, portrait, narrow, slender and wide
  selection and hysteresis; tall-window zoom, shrink fitting, legacy scales,
  dialog zoom and zoom limits.

The tests include the platform-independent sources by path, so they compile
and pass on any platform even though the full binary is Windows-only.

## Interface

The main window keeps the classic OP Auto Clicker 4.x dialog's controls
in a modern card surface — the same visual language as
the Updates window: rounded elevated group cards led by an accent
tick, an accent-filled **Start** button (the one primary action, like
**Install** there), flat secondary buttons, and soft shadows — in a
non-scrolling window that resizes like any normal Windows app: drag any edge to push or pull it, grab any corner, double-click or
maximize from the title bar, and use Win+arrow keys and Snap Layouts.
The classic 560x456 size is the fit-independent minimum. The app
declares it to the OS divided by the currently applied window fit,
including a temporary dialog fit, because egui-winit multiplies the
live-point minimum by the
total zoom; the resulting physical floor follows Windows DPI and any
legacy saved scale, but never the transient fit. When the window moves
between displays with different DPI scaling, the native minimum is
re-declared without requesting a resize. Neither a drag
nor a snap can take the window below the size where every control fits
and a large window can always be pulled back down. Growing the window is designed, not stretched:
group boxes always claim the full content width (their borders stay
flush at every size), the classic side-by-side Click options / Click
repeat pair stays equal in width and height, and the button grid shares the extra
width up to a content cap (beyond that the content column centers
instead of producing over-wide buttons). In portrait compositions,
extra height is balanced inside the four cards after the gaps and the
space above the status strip reach a small limit, rather than becoming
a large empty band. The status, buttons and footer stay near the bottom.
On top of that, the
window size drives the presentation itself (Settings → "Scale and
rearrange with window size", on by default): the interface fits
itself to the window **continuously**. The decision is made in
neutral points — the live point size times the total zoom already
applied, i.e. the physical size at 100 % zoom — which is invariant
under the very zoom changes it drives, so feedback is mathematically
impossible. That neutral size is fitted uniformly against the
active design canvas (560x456 for classic, 400x600 for portrait,
280x700 for narrow portrait, 220x740 for slender portrait, and
904x456 for wide): the smaller of the width and height ratios is the
fit factor. The result accounts for any legacy saved scale, is capped
at 280 % total zoom, and is rounded down to a whole percent.
A growing fit is adopted after the size has held still for a few
frames and the applied zoom **eases** to it over ~180 ms (cubic
ease-out); a shrinking fit is requested without growth easing and takes
effect at egui's next frame boundary, so it does not require a restart. A small dead band keeps pixel rounding from nudging
the fit back and forth. The easing is presentation-only — the fit's
neutral-point measure uses the *applied* zoom, so it stays exact at
every intermediate value, and the landing always sets the exact fit
factor. Tall windows reflow the interval fields, stack the middle
cards and give the footer reminder its own row; narrower compositions
rearrange controls further so they can grow as height is added.
Panoramic windows use the wide three-across layout. Shape changes use
arrive-and-hold hysteresis to avoid flicker. Settings, Hotkey, Record &
Playback and Updates temporarily use a fitting zoom if the dashboard's
zoom would make them too wide, then restore it when closed without
changing the physical window size. These secondary windows scroll vertically
when unusually long notices need extra space. Long status and update labels
are ellipsized as needed, with full text available on hover.
A fresh install on a monitor
larger than the 1920x1080 reference opens once at a proportionally
larger size (so 1440p first runs are not comparatively tiny); an
existing settings file — even one that deliberately stores the
classic floor — is always respected exactly. Sizes are remembered
in fit-independent points, so the same physical window comes back
on every launch whatever fit factor is active. The size you leave the window at is saved
to the settings file and restored on the next launch (maximized state
is never saved as a raw size: restoring comes back at your last normal
size). The bottom-right **Settings…** button opens the appearance and
automatic adaptation controls.

**Appearance** — choose a **Style** and a **Mode** independently:

- **Classic + Light** (default) keeps the light-gray dialog, white cards,
  familiar border-breaking captions and original controls.
- **Classic + Dark** keeps the existing Windows-style dark appearance.
- **Modern + Light** uses cool porcelain surfaces, teal-accented vector
  headings, inset rounded inputs and bundled Outfit typography.
- **Modern + Dark** uses the same design with layered midnight-slate surfaces
  and a brighter teal accent. Both have softly shaded header bands, fine
  heading rules, regular and medium-weight type, a mint-to-blue Start action,
  neutral disabled Stop, softly surfaced secondary actions and dimmed dialog
  backgrounds. Radio choices and checkboxes use distinct shapes with clear
  selected and focus states. Original vector action marks make controls
  recognizable at a glance and give way to their labels when space is limited.
- **Custom + Light/Dark** applies two chosen RGB colors
  to a directional gradient on the dashboard, cards, dialogs and primary
  action. Surfaces sample a shared window-wide gradient rather than restarting
  it for each card; the preview and primary action show their full local range.
  Expand **Customize gradient** for the color pickers, 0–359° rotation,
  0–100% gradient strength and adjustable surface texture, plus reset. The
  static matte grain is generated locally and cached; set texture to 0% to turn
  it off. Colors are softened and foregrounds adapted to keep text readable.
  The controls persist even if another style is selected. At 0° the gradient
  runs left to right.

The choice applies to the dashboard and dialogs and is saved across restarts.
It retains all click settings and the current responsive composition.
The previous **Midnight OLED**, **Terminal green** and **Aurora modern**
looks remain under **Previous themes**. An existing saved theme stays in
place on upgrade; while a previous theme is selected, choose Classic,
Modern or Custom before changing its mode. The bundled font and its SIL Open Font
License are documented in [`assets/fonts`](assets/fonts/README.md); the full
license is also readable offline from Modern Settings.

**Automatic adaptation** — on by default; turn it off to keep the
classic layout without window-driven scaling. The manual Window scale
percentage picker has been removed. Existing `ui_scale` values are still
read and preserved so upgrading does not unexpectedly change an older
installation's size or appearance.

Legacy settings files from the dark-mode-only build are still honored:
`dark_mode=true` loads Classic Dark, `dark_mode=false` loads Classic Light,
and a recognized `theme=` key always wins over the legacy boolean. The
current keys include `modern_light`, `modern_dark`, `custom_light` and
`custom_dark`; `classic`, `dark`, `oled`, `matrix` and `aurora` retain their
existing meanings.

Styles share the same responsive compositions and controls, though font
metrics and card padding differ. Switching appearance can move controls
slightly; it does not change the saved window dimensions or click settings.

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
  Hours and minutes are supported beyond the engine's old 10-second
  default limit. The Start button and toggle hotkey both validate the
  current editors before starting; a disabled Start button explains why
  on hover.
- **Click options** (left group box) — classic combo boxes for Mouse button
  (Left / Right / Middle) and Click type (Single / Double / Triple). Double
  and triple emit two or three down/up bursts separated by a short QPC-spun
  gap — never a sleep. Recorded macro steps remain
  single clicks regardless of this setting, so playback does not multiply
  what was recorded. Random offset applies to ordinary clicking, not to
  the recorded macro delays.
- **Click repeat** (right group box) — Repeat N times, or Repeat until
  stopped (the default); the worker auto-stops at the count.
- **Cursor position** (full width) — Current location (default) or Pick
  location radio, a real gray **Pick location** button that captures the next
  left-click anywhere on screen via `GetCursorPos` (Esc cancels, the button
  doubles as "Cancel pick" while armed), plus white X/Y boxes. Fixed points
  and recorded macro points use the full virtual desktop; if a point no
  longer lies on a connected display, the session stops with an error instead
  of clicking elsewhere.
- **2x2 push-button grid** — **Start (F6)** and **Stop (F6)** (Stop is
  greyed out until a session is running; Start is disabled while running or
  while the interval is invalid), then **Hotkey setting** and
  **Record & Playback**.
- A single reserved one-line status strip under the group boxes shows
  semantic colored state (Ready / Running / Stopping / Panic / Warning /
  Error) without ever changing the window layout. It is never blank: while
  idle it states exactly what Start will do (`Ready — left single click at
  the cursor, 10.00 CPS.`), and while running it carries the live click
  count or elapsed time on the same row. Ellipsized text is revealed on
  hover. Beside it, a status-bar style footer row: a short key reminder on the left (it tracks the actual
  assigned hotkey, e.g. "F6 start/stop · F8 emergency stop") and the
  **Updates…** and **Settings…** buttons on the right. The footer
  button mirrors the checker in place (**Checking…**, **Update
  vX.Y.Z**, or **Updating…**). A small red **1** marks a confirmed
  available update, and is absent during checks and installation or
  when no update is available. Cancelling or failing an installation
  restores the indicator if the update is still confirmed. When a
  verified update is ready and no click or macro session is active,
  pressing the footer button
  starts the complete update directly and opens the Updates window
  for progress and cancellation. The Updates window is a centered
  status card with a state glyph, a shape-coded stage path, and the
  facts needed to trust the result — **Installed**, **Latest**,
  **Installation**, **Size**, **Checked**, and **Channel** —
  so both the verdict and the exact update route are visible. It
  checks once per launch, refreshes a stale result when opened, and
  can always be re-run with **Check now**; failures resolve to a
  plain-language headline with the technical detail underneath,
  never an endless spinner. The action stays disabled until an
  update is proven, then **Update to vX.Y.Z** does the work in
  place. Portable builds stream the new EXE beside the running one
  with progress and cancellation. A recognized source checkout is
  first required to be a clean `main` branch with this project's
  GitHub origin; the target manifest is checked before fast-forwarding
  to the exact release tag. Both paths then verify exact size and
  SHA-256 against GitHub's published values (no published fingerprint,
  no install). Before replacing the EXE, the app honors cancellation,
  waits for the click worker to stop, and saves settings. It then
  renames, relaunches, and closes. Failures report any source change;
  a failed rollback preserves the backup and explains how to recover
  rather than claiming restoration. The new build confirms with
  "Updated from v… — now running v…" and cleans only its own attempt's
  leftover files. A legacy updater that does not pass an attempt ID may
  leave its old backup beside the EXE; it is not automatically swept.
  **Release notes** and **Copy download link** remain available,
  and browser failures are reported in the window.

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
main screen, and the Settings window carries the appearance selectors,
automatic adaptation toggle and **Reset all settings to factory defaults**; it
also reports a settings-save failure if one occurs.

Settings persist automatically to
`%APPDATA%\op-auto-clicker\settings.conf` (a hand-editable, tolerant
key=value document) and reload on launch. A complete, uniquely named
temporary file is synced and atomically renamed over the old file; a
failed save stays dirty, is reported, and is retried. Reads are limited to
1 MiB. If an existing file cannot be read, is not UTF-8, or exceeds that
limit, it is preserved rather than overwritten: Settings shows its location
and asks you to close the app, repair or move the file, and reopen it. Saving
is blocked for that session, including on reset and exit. An invalid random
range is saved as zero instead of invalidating unrelated settings. An
update will not restart while settings cannot be saved. A stale window handle from
a previous session is never reused.

## Real-click stability test

**This sends real desktop clicks.** Choose a disposable safe target: the
configured current cursor or fixed position may point at controls in any
application. Do not test over important data or destructive actions.

Open **Settings… → Open stability test…**, choose **10–60 seconds**, and
press **Start real-click test**. The three-second countdown gives you time to
move the cursor; a fixed position still uses the configured point. The worker
runs a snapshot of the current interval, offset, button, click type and
position. It ignores the finite repeat limit only for this test and does not
change saved click settings. Stop, the enabled start/stop hotkey, F8, or
closing the test window cancels. A countdown that expires while the UI is
unresponsive is cancelled rather than starting clicks unexpectedly. Hotkeys
are UI-polled, but the worker's own deadline caps its interval, hold and burst
gap waits and it attempts button-up before finishing. A blocked OS call can
still delay completion.

A **good timing** verdict requires the full duration, at least **20 completed
cycles**, **no engine warning**, **at most 5% late**, a worst observed
deadline lateness of **at most 250 ms**, and an end overrun of **at most
250 ms**. A cycle is marked late if the scheduler was already overdue or
dispatch was more than 1 ms late. Double/triple clicks form one cycle.
Interrupted, failed, warned or short/undersampled runs do not pass. A single
large observed deadline slip cannot hide inside a low percentage of late cycles.

Progress, exact counts, rounded late percentage, average cycles per second,
worst observed deadline lateness and end overrun are shown. The maximum uses
the original deadline before catch-up re-anchoring and includes a deadline
missed as the test ends. These totals and maximum remain valid if the bounded
telemetry ring drops records. **Copy timing report** requests an optional
clipboard copy of the timing summary, omitting cursor coordinates, machine
identifiers, click settings other than duration, and raw error details. The
result measures this worker's timing and reports input-dispatch failures; it
cannot detect every kind of stall or prove the target received clicks, predict
future uptime or guarantee universal stability.

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
| `src/settings.rs` | Dependency-free key=value serialization, tolerant parsing and atomic settings writes |
| `src/stability.rs` | Conservative interpretation of real-click timing measurements |
| `src/texture.rs` | Deterministic, tileable procedural grain for Custom surfaces |
| `src/telemetry.rs` | Lock-free SPSC ring buffer over atomic `Atom` slots (`Send + Sync`, no `unsafe`), drop-newest back pressure, sliding-window statistics |
| `src/config.rs` | Configuration struct, validation, h/m/s/ms helpers and the all-atomic runtime snapshot |
| `src/coordinates.rs` | Bounded virtual-desktop normalization and mouse-message coordinate packing |
| `src/update.rs` | Pure update decisions: version ordering, release/asset JSON scanning (tag, size, SHA-256 digest), validated release/download/redirect URLs, refresh policy, error headlines and a dependency-free streaming SHA-256 |
| `src/self_update.rs` | Installation classification and verified self-replacement: guarded/cancellable Git fast-forward, PE and fingerprint checks, per-attempt swap, rollback and recovery preservation |
| `src/window_layout.rs` | Pure classic/portrait/wide composition selection, hysteresis and proportional fit arithmetic |

### Timing and threading rules

- The egui update/render thread never runs the timing loop. Two manual-reset
  Win32 events drive the worker (`WaitForSingleObject`): one wakes it for a
  session, one aborts it. While idle the worker parks on the wake event with
  a bounded 250 ms termination check. Session transitions are serialized
  outside the click loop; a stopping worker remains busy until fully drained.
- Active scheduling is a rate-based QPC deadline loop; the spin body is
  `std::hint::spin_loop` and there is no `std::thread::sleep` in the active
  click path — including inter-click gaps inside double/triple bursts. Inside
  a session only the last 12 ms before a dispatch is spun; when the wait is
  long enough that parking cannot move the dispatch instant, the part before
  that tail parks on the abort event in slices of at most 250 ms, so a stop,
  a panic or a session deadline is honoured inside one slice and an
  hour-long interval no longer holds a logical CPU at 100 %.
- Failure handling is explicit: a panic inside the click loop is caught and
  reported instead of taking the worker thread with it, a worker that is lost
  is replaced by the watchdog up to 8 times per run (and reported in recovery
  wording), and a worker that is alive but blocked inside an input call is
  observed and reported as late rather than assumed healthy.
- `timeBeginPeriod(1)` is acquired by an RAII guard per session and paired
  with `timeEndPeriod(1)` on drop.
- `SetThreadPriority(THREAD_PRIORITY_TIME_CRITICAL)` is verified; denial is
  surfaced on the dashboard and in the log.
- Configuration crosses threads through `AtomicConfigSnapshot`: the worker
  accepts only a complete published generation and otherwise keeps its last
  complete settings for that click. The read makes one bounded attempt, so
  it never locks or waits on the click path. Telemetry uses a lock-free
  single-producer/single-consumer ring.
- F8 (or the panic button) sets an atomic flag sampled before every click and
  on every spin iteration, including hold and inter-click spins. The mouse
  button is always released before the worker leaves a panicked hold.

### Input dispatch

- Global mode submits arrayed `INPUT` structures to `SendInput`, including a
  two-element move-plus-down array for fixed coordinates using
  `MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` with
  virtual-desktop normalized 0..65535 mapping. Points outside connected
  displays, invalid desktop metrics, and axes exceeding the absolute input
  precision limit fail before the button-down is sent. Left, right and middle
  buttons use their respective down/up flags; the injected count is checked.
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
   repeat, position, hotkey, theme, automatic adaptation and the window size
   from step 17) is restored from
   `%APPDATA%`.
9. Open Settings… and press **Reset all settings to factory defaults**:
   every control returns to its default (100 ms, Left, Single, repeat
   until stopped, Current location, F6, Classic light at 100 %) and the
   status strip confirms "Factory defaults restored."
10. Press F8 mid-run: the responsive UI detects it, the worker stops and
    attempts button-up, the status turns red, and no mouse button remains
    held. OS calls or an unresponsive UI can delay this response.
11. Open Settings… → Appearance. Try Classic Light, Classic Dark, Modern
    Light and Modern Dark, including each dialog and a restart. Confirm the
    selected appearance persists, controls stay usable at compact, tall and
    wide sizes, and saved OLED/Terminal/Aurora choices remain available under
    Previous themes without being changed automatically. In Modern, check
    the action icons and full labels, the neutral disabled Stop and the active
    Stop state. Check the square checkbox and circular radio selection marks,
    their hover/press feedback and their Tab/Space focus behavior. Icons may
    disappear in a narrow layout, but labels must remain readable; Classic
    retains its existing presentation.
12. Start a "Repeat until stopped" session, then press Stop: the status
    shows "Stopping…" for at most a moment and then settles on
    "Stopped — N clicks delivered." — it must never remain stuck on
    "Stopping…". The status dot briefly acknowledges a state change;
    hovering an enabled action lifts it slightly and pressing it lowers it.
13. In Settings…, confirm there is no Window scale percentage picker.
    Turn automatic adaptation off and resize: the classic layout remains.
    Turn it back on and resize again: the controls reflow and scale.
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
    stops at the classic 560x456 footprint (adjusted for Windows DPI and
    any legacy saved scale) — it must not go smaller, and every control stays whole
    at the limit. Grow the window in any direction and let go: just
    after the drag ends, the whole interface rescales once to fit the
    new size, **easing** smoothly onto the new scale in a fraction of
    a second rather than snapping (text, fields and buttons all
    physically larger, proportions unchanged); growth must not rescale
    repeatedly while you are still dragging, and the physical window
    size itself must not jump. Then, without reopening the app, drag
    the large window straight back down — including from a maximized
    window: the edge must keep following the pointer, the
    interface must fit down during the inward drag, and it must stop
    only at the same classic footprint, never stranded at the large
    size. Pull it panoramic:
    the three middle groups sit three-across with one row of four
    action buttons. Pull it tall without making it wider, including roughly
    556x1388 pixels if the display allows: the controls should become
    noticeably larger, with no huge gaps between cards or above the footer.
    At that tall size, open Settings, Hotkey, Record & Playback and Updates:
    each must fit and work, and closing it must restore the dashboard zoom
    without resizing the window. Drag back down across the shape boundaries:
    controls should fit without flicker or trapping the window at a large minimum. Snap
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
    the settings file back and confirm the saved size wins again. If two
    monitors have different Windows DPI settings, move the window between
    them, then drag, snap and maximize on each; the minimum should follow
    the current display and moving should not trigger an app-requested resize.
    Check that an unusually long error can be scrolled to its bottom controls
    and that an ellipsized status shows its full text on hover. With Random
    offset, finite repeat and fixed coordinates disabled, Tab through the
    dashboard to Updates and Settings, then Shift+Tab back; disabled fields
    must not trap focus. Repeat through the disabled controls in Record &
    Playback, the Updates window and the previous-theme Settings view, and
    confirm Enter activates the focused enabled control.
18. Updates: with the machine online, open **Updates…** — a stale or
    missing result refreshes on open, and the status card must show
    **Installed**, **Latest**, **Installation**, **Size**, **Checked: just now**,
    and **Channel**, with the verdict naming the version GitHub reported.
    With v1.9.0 published, a portable v1.8.0 EXE should show **Update
    v1.9.0** with a red **1** after confirming the available update. The
    badge must behave like the rest of the button when clicked. After
    updating, the v1.9.0 footer should show no red notification while up to
    date; a controlled newer-release fixture can test a future notification.
    It disappears during installation and returns after cancellation or
    failure if the newer version remains confirmed.
    With no session running, press the footer update button: the Updates
    window must open on live progress, move through **Download → Verify →
    Install → Restart**, then close and reopen into the selected newer
    version with an "Updated from v…" confirmation and settings and window
    size intact. That successful attempt's backup should be removed;
    unrelated recovery files must remain untouched. In the window itself,
    **Update to vX.Y.Z** must stay disabled unless a newer release is
    proven. Repeat an update and
    press **Cancel update** mid-transfer: it must stop, report that
    the running version was not changed, and keep the current build
    working. **Release notes** opens that version's GitHub page and
    **Copy download link** puts the exact asset URL on the clipboard.
    For the source path, launch a v1.8.0
    `target\release\macro_clicker.exe` from a clean `main` checkout still at
    v1.8.0 after v1.9.0 is published: **Installation** must say
    **Source checkout build**, the stages must begin with **Source**,
    the checkout must land exactly on that release tag, and the app
    must reopen as the new version. Make a tracked local edit and
    retry: the update must refuse before replacing the EXE; repeat
    from a non-`main` branch and confirm the same. Cancel during
    source preparation, download and verification; if the checkout
    has already advanced, the notice must say so. Try while clicking:
    installation must wait for the worker to stop, and cancelling while
    waiting must leave the original EXE in place. Disconnect the
    network and press **Check now**: the window must settle on a
    plain-language failure with technical detail and **Try again** —
    never an endless spinner. If an update was already confirmed
    before the network dropped, a failed refresh must keep it
    installable and say it is showing the last confirmed result.
    Untick **Check for updates on startup** and restart: no check
    runs in the background (the footer stays **Updates…**), while
    opening the window counts as asking and refreshes then. Finally
    confirm the remaining safety rails: an unrecognized Cargo
    artifact outside this checkout must offer only the browser
    download, and a portable copy in a read-only folder must fail
    cleanly with the current version untouched.
19. With a disposable blank target on each display, try fixed-position clicks
    and a one-step recording at corners and interior points, including a
    display left of or above the primary one and mixed DPI scales. Confirm
    the correct pixel is clicked. Disconnect a display or choose coordinates
    outside all displays: the worker must stop with a clear error and must
    not click the old cursor position. Check quick finite sessions and macros,
    Stop and F8 for their distinct terminal statuses.
20. Back up `%APPDATA%\op-auto-clicker\settings.conf`, then use a disposable
    oversized or invalid-UTF-8 settings file. Reopen the app, check the
    recovery instructions in Settings, and verify reset and exit do not
    overwrite that file. Close the app, restore the backup and reopen it.
21. Select Custom Light and Dark. Expand
    **Customize gradient**, try contrasting and identical colors, rotate,
    adjust gradient and texture strength (including 0%), then restart.
    Check dashboard and dialog readability,
    keyboard access, reset, persistence, responsive layouts and unchanged
    Classic/Modern appearances.
22. With a disposable safe target, verify the real-click test warning and
    10–60-second range. Check current and fixed positions, a finite repeat
    setting, countdown cancellation via Stop/F8/close, cancellation while
    clicking, timeout, long intervals/insufficient data, and dispatch error
    handling. Verify the live progress, result thresholds and optional copied
    report; test a resumed or delayed run without relying on it to detect
    every stall. Confirm no button remains held. The result is not proof that
    the target received input. These are hands-on checks, not CI automation.
23. Long intervals and idle CPU. Set 10 minutes, Left, Single, Repeat until
    stopped, and start a session on a disposable target. Watch Task Manager:
    the process must sit near 0 % CPU between clicks instead of holding a
    logical core at 100 %. Then press Stop (and separately F8) mid-wait: the
    session must end promptly rather than waiting out the interval, and it
    must settle on its normal terminal status. Start again immediately after
    a stop and confirm the next click is not dispatched early.
24. Status and counters. Before any action the status strip must show the
    idle line naming what Start will do — change the button, click type,
    coordinate mode, repeat limit and interval and confirm the sentence
    follows each change (including the `every H:MM:SS.mmm` form for slow
    intervals). While running at 10 CPS, confirm the counters appear on the
    status row without moving the layout, that a finite repeat shows
    delivered / total, and that hovering the strip explains them.
25. Recovery wording. If a session ever reports that the click worker stopped,
    the message must say it was restarted and what to do next — never that
    the app must be restarted per attempt, and never a bare failure with no
    next step. A panic inside the loop must produce a plain error status with
    the session ending, not a UI stuck showing a running session.
