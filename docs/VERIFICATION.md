# Verification record

This document records exactly how the application was verified outside
Windows, what was executed, and what remains to be run on a Windows
10 21H2+/11 x64 machine. The app targets Windows only; the Linux sandbox used
for development has no Windows SDK, display server, or access to crates.io.

## Polish and hardening round (current)

A final quality pass over the merged 4.1-style build, driven by running
clippy with warnings denied and re-reading every interaction:

- **Application icon.** A new portable `icon.rs` generates a 32x32 RGBA
  crosshair (two concentric rings, center dot, four cardinal ticks on a
  dark rounded square) from signed-distance pixel math, so no binary asset
  is committed. `main.rs` installs it through `ViewportBuilder::with_icon`;
  four tests cover dimensions, corner transparency, center color, two-axis
  symmetry and accent/background pixel presence. The bytes were also
  rendered to a PNG in the sandbox and visually confirmed.
- **Hotkey capture correctness.** The assignment dialog now acts only on a
  fresh press made after clicking "Assign new key" (keys already held are
  ignored, including F8 held through its rejection notice), the toggle
  hotkey is fully inert while the dialog is listening, and a key still held
  when capture closes cannot immediately start or stop a session.
- **Honest session-end reporting.** The "Session complete" status is only
  shown when a repeat-limited run actually reaches its click count; manual
  stops and worker errors keep their own Stopping/Error status instead of
  being overwritten.
- **Strict CI.** The workflow now runs `cargo clippy -- -D warnings`, so
  any new lint fails the Windows job. The only clippy findings on this
  codebase (`unnecessary_cast` on three `HWND as isize` conversions) are
  shim-only artifacts: the offline harness aliases `HWND` to `isize`,
  while real windows-sys 0.52 defines it as a raw pointer where the casts
  are required. The function carries an explanatory targeted `allow`.

Verification on rustc 1.88.0 against real egui 0.27.2: zero production
diagnostics, `cargo clippy -- -D warnings` clean for all workspace
sources, `rustfmt --check` clean, and **44/44 tests pass** (the previous
40 plus four icon tests). A deliberate type error injected into `app.rs`
was rejected with E0308 and reverted cleanly.

## OP Auto Clicker 4.1 parity round

The dashboard was rebuilt to match the OP Auto Clicker 4.1 screen, with the
UX discipline of the reference project
[MarshalX/clicker-rs](https://github.com/MarshalX/clicker-rs) (live
Valid/Invalid indicators, disabled-until-valid Start, semantic status line,
remappable hotkey dialog, reset control). clicker-rs's internals were **not**
adopted: its worker sleeps on the hot path, never joins its thread in
`Drop`, and has no fixed-position, repeat-count or background mode. The
existing QPC spin worker, atomic snapshot and SPSC telemetry were kept.

New functionality, all behind the same unchanged scheduling core:

- Four-card 4.1 layout: Click Interval (Hours/Mins/Secs/Milliseconds plus
  Random offset +/- ms), Click Options (Left/Right/Middle x
  Single/Double/Triple), Click Repeat (until stopped / N times), Cursor
  Position (current / picked X,Y via `GetCursorPos`).
- New portable modules `hotkey.rs` (virtual-key rules, F8/Esc reservation,
  key names) and `settings.rs` (dependency-free, tolerant key=value
  persistence at `%APPDATA%\op-auto-clicker\settings.conf`).
- Right/middle dispatch for both `SendInput` and background window messages;
  double/triple bursts with QPC-spun 50 ms inter-click gaps; uniform +/-
  offset sampling alongside the Gaussian humanizer.
- Default F6 start/stop hotkey, remappable through a capture dialog; F8 stays
  the immutable panic stop.
- Advanced window for background targeting, the nine Gaussian expert fields
  and all telemetry (metrics, CPS sparkline, histogram, activity log).

Verification on rustc 1.88.0 against the **real** egui/epaint/emath/ecolor
0.27.2 sources: `cargo check` reports zero diagnostics in the workspace
sources; `cargo test` passes **40/40** (the previous 23 plus 17 new tests
covering interval helpers, offset bounds and distribution, hotkey rules,
settings round trips and malformed-document recovery); a deliberate type
error injected into `app.rs` was rejected with E0308 and reverted cleanly.
`rustfmt --check` is clean and `cargo clippy` reports no lints in the
portable or application logic — the only findings are `unnecessary_cast`
items produced solely by the harness's `HWND = isize` shim alias; the real
windows-sys 0.52 `HWND` is a raw pointer, where those casts are required.
A Windows CI workflow (`.github/workflows/ci.yml`) now runs the format
check, clippy, the full suite, a release build and uploads the exe artifact
on a real `windows-latest` MSVC runner.

## Consolidation to a single engine

The project originally shipped two feature-parallel implementations
(`engine-a-cpp`, C++20/ImGui/DirectX 11, and `engine-b-rust`, Rust/egui).
The C++ engine was retired and the Rust engine was promoted to the single
shipped application, now in `autoclicker/` (binary `op_auto_clicker.exe`).
Decision factors:

- **Build reliability for the end user.** The C++ path required Visual Studio
  instance discovery, the C++ workload, CMake generator selection, Ninja and
  vcvars setup, which produced multiple rounds of machine-specific failures.
  A single `cargo build --release` invokes the MSVC linker automatically once
  the standard Rust toolchain is installed.
- **Safety of the concurrent core.** The worker thread, atomic config
  snapshot and lock-free SPSC telemetry are expressed directly in Rust's
  ownership model; the class of data-race and lifetime bugs fixed during
  development is not expressible in safe Rust.
- **Dependency and test surface.** One pinned toolchain (egui/eframe 0.27.2,
  windows-sys 0.52.0) and one set of 23 integration tests cover all
  platform-independent logic.

The historical engine-A evidence below is retained as the record of work
performed before consolidation; those sources no longer exist in the tree.

## Toolchains used in the Linux sandbox

Network access in the sandbox is restricted to GitHub, PyPI and the npm
registry. Toolchains were therefore assembled through those hosts:

| Tool | Source | Version |
| --- | --- | --- |
| GCC (portable C++20 logic, TSan) | system | g++ 12.2.0 |
| CMake | pip wheel `cmake` | 4.4.4 |
| Ninja | pip wheel `ninja` | 1.13.2 |
| Rust (rustc/cargo/rust-std) | npm packages `@rustbin/*` | 1.88.0 stable |
| rustfmt / clippy | npm packages `@rustbin/*` | 1.8.0 / 0.1.88 |
| tree-sitter + tree-sitter-rust | pip wheels | 0.26 / 0.24.2 |

## Engine A — commands executed and results

```
cmake -S engine-a-cpp -B /tmp/enginea-cmake -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/enginea-cmake --target test_humanize test_ring_buffer test_config
ctest --test-dir /tmp/enginea-cmake --output-on-failure
```

Result: **3/3 tests passed**. The configure step really fetched Dear ImGui
tag `v1.90.4` via FetchContent from GitHub; the fetched ImGui core
(`imgui.cpp`, `imgui_draw.cpp`, `imgui_tables.cpp`, `imgui_widgets.cpp`)
compiles warning-clean with `g++ -std=c++20 -Wall -Wextra`.

ThreadSanitizer run:

```
g++ -std=c++20 -Wall -Wextra -Wpedantic -Werror -g -O1 -fsanitize=thread \
    -I engine-a-cpp/src engine-a-cpp/tests/test_ring_buffer.cpp \
    engine-a-cpp/src/telemetry.cpp -o /tmp/tsan_ring && /tmp/tsan_ring
```

Result: **all checks passed, zero ThreadSanitizer data-race reports**,
including the real two-thread producer/consumer test (10,000 ordered values
with back pressure plus a 50,000-record threaded `TelemetryStore` run).

The full `AutoClickerEngineA` GUI target was also attempted: it fails only on
`fatal error: windows.h: No such file or directory`, i.e. the expected
platform boundary — no other diagnostic is produced.

## Engine B — commands executed and results

Direct strict-lint compilation of every test crate and the binary entry:

```
rustc --sysroot <sysroot> --edition 2021 -D warnings --test tests/humanize_tests.rs
rustc --sysroot <sysroot> --edition 2021 -D warnings --test tests/telemetry_tests.rs
rustc --sysroot <sysroot> --edition 2021 -D warnings --test tests/config_tests.rs
rustc --sysroot <sysroot> --edition 2021 -D warnings src/main.rs
```

Result: **23 tests pass, zero warnings** (humanize 6, telemetry 8, config 9);
the entry binary compiles and prints its Windows-only notice when run.

Real cargo workflow on a dependency-free copy of the portable crate (crates.io
is unreachable in the sandbox, so the external `eframe`/`egui`/`windows-sys`
dependencies are stripped only for this local run; the repository Cargo.toml
keeps the exact pins):

```
cargo test --release
cargo build --release
```

Result: **all integration tests pass under the release profile**
(`opt-level = 3`, `lto = true`, `codegen-units = 1`, `panic = "abort"`).

Windows FFI type checking: `win32.rs`, `timing.rs` and the `engine.rs` worker
were compiled with real rustc against a signature-faithful local stub of the
exact `windows-sys` 0.52.0 surface. Every stub signature, struct layout and
constant was copied from the upstream 0.52.0 source, and module paths were
verified there (message/window APIs in `WindowsAndMessaging`,
events/threading in `System::Threading`, `ClientToScreen` in
`Graphics::Gdi`). `cargo check` on this harness finishes clean.

All 11 Rust source files additionally parse without error under the
tree-sitter Rust grammar.

## Defects found by real compilation and fixed

1. The Rust ring stored `Vec<Cell<T>>`; `Cell` is `!Sync`, so
   `Arc<Shared>` could not be moved into the click worker (E0277). Replaced
   with an `Atom` trait over atomic slots (`AtomicI32` and a
   `ClickRecordAtom` encoding `f64` as bits), making the whole pipeline
   `Send + Sync` with no `unsafe` code.
2. Oldest-overwrite required the producer to mutate the consumer-owned tail
   index, racing `pop` and corrupting accounting under real threads
   (observed `popped + dropped != pushed`). Both engines now use a race-free
   drop-newest policy plus a back-pressured `try_push`; the producer only
   ever writes `head`, so accounting is exact under every interleaving.
3. `egui::Align::LEFT_TOP` does not exist in egui 0.27; corrected to
   `egui::Align2::LEFT_TOP` after auditing every egui call against the
   upstream 0.27.2 sources.
4. ImGui 1.90.4 has no `InputLong`; the C++ dashboard now uses
   `InputScalar` with `ImGuiDataType_S32` (default step arguments), matching
   the 32-bit Win64 `long` type.
5. Real `cargo build` on Windows 11 with rustc 1.99.0 failed with E0432:
   `CreateEventW` in windows-sys 0.52 is gated behind both
   `Win32_Foundation` and `Win32_Security` (it takes a
   `SECURITY_ATTRIBUTES` pointer). `Win32_Security` was added to the
   `windows-sys` feature list; every other imported symbol was re-checked
   against the windows-rs 0.52.0 source and stays Foundation-gated.
6. rustc 1.99 reports the future-incompatible `float_literal_f32_fallback`
   lint wherever an unsuffixed float literal is inferred as `f32`. Every
   `f32`-context literal in `app.rs` now carries an explicit `_f32` suffix
   (stroke widths, corner radii, `Vec2` geometry, spacing, graph/histogram
   arithmetic). All remaining unsuffixed literals were individually traced
   to `f64` fields/expressions, which the lint does not target.
7. The CPS graph mixed `f64` telemetry history with `f32` `Rect` geometry in
   one expression (E0308/E0277 on 1.99). The normalized value is now computed
   fully in `f64` and cast once at the `f32` boundary, with all geometric
   literals explicitly `f32`.
8. First real MSVC (14.44, VS 2022 Build Tools) compile of engine A failed in
   four translation units with C2589/C4003: the Windows SDK defines `min`/
   `max` as preprocessor macros in `windef.h`, which corrupt
   `std::min`/`std::max`/`std::numeric_limits<...>::min()` (non-Windows
   compilers never see these macros, which is why the portable tests missed
   it). The CMake build now defines `NOMINMAX` (plus `WIN32_LEAN_AND_MEAN`,
   per Microsoft's "Using the Windows Headers" guidance) at directory scope,
   covering every target including the fetched ImGui library. An MSVC-faithful
   macro simulation in the sandbox reproduces the original errors without the
   define and compiles warning-clean (`-Werror -Wpedantic`) with it; all
   eight affected call patterns were enumerated and none rely on macro
   semantics.
9. Dear ImGui 1.90.x keeps the `ImGui_ImplWin32_WndProcHandler` declaration
   inside an intentional `#if 0` block in `imgui_impl_win32.h` (verified in
   the v1.90.4 tag sources) so the helper header does not force
   `<windows.h>` on consumers; the upstream comment instructs applications
   to forward-declare the function themselves. `main.cpp` now carries that
   exact forward declaration, fixing C3861.
10. MSVC C4324 (structure padded due to the deliberate cache-line `alignas`
    annotations in the lock-free telemetry ring) is now silenced project-wide
    via `/wd4324`; the padding is intentional false-sharing protection, not a
    defect.
11. The egui dashboard used `Color32::from_rgba_unmultiplied` inside `const`
    initializers; that constructor is not `const` in egui/ecolor 0.27.2
    (E0015). The translucent palette constants now use the const-capable
    `Color32::from_rgba_premultiplied`; opaque colors keep
    `Color32::from_rgb`. Caught by the offline real-egui harness, which the
    sandbox rustc 1.88 can reproduce.

## Classic Win32 visual redesign (authentic OP Auto Clicker 4.x)

The earlier dark "modern dashboard" look was replaced after user feedback
with an authentic recreation of the classic OP Auto Clicker 3.0/4.x native
dialog, driven by published screenshots of the real 4.0/4.1 builds rather
than third-party lookalike clones:

- Fixed viewport (560x524 in this round, tightened by the later polish
  rounds below, currently 560x448) with
  `ViewportBuilder::with_resizable(false)`; the main
  window has no `ScrollArea` and every control is visible inside it.
- Classic light palette (`#F0F0F0` face), white sunken numeric fields, gray
  3D push buttons and a custom `group_box` widget that draws a bordered frame
  with the title breaking the top border like the native `BS_GROUPBOX`.
- Full-width **Click interval** group with four h/m/s/ms boxes (default
  100 ms) and the 4.1 **Random offset ± [40] milliseconds** row, now gated
  by a persisted `random_offset_enabled` preference (new `AppPrefs` field,
  serialized under `[hotkey] random_offset_enabled=`; older files with a
  non-zero saved offset load with the checkbox ticked).
- Side-by-side **Click options** / **Click repeat** groups and a full-width
  **Cursor position** group with Current location / Pick location radios, a
  real Pick button that arms capture of the next left-click anywhere on
  screen through `GetCursorPos` (Esc cancels), and X/Y boxes.
- 2x2 button grid: Start (F6) / Stop (F6, disabled until running) over
  Hotkey setting / Advanced…. Background delivery, Gaussian humanization,
  live graphs and reset-to-defaults moved into the Advanced window so the
  main screen stays faithful; F8 panic stop remains always armed and is
  documented inside the Hotkey dialog.

The change is presentation-only plus the one additive preference field: the
worker, QPC scheduler, SendInput/PostMessage dispatch, SPSC telemetry, F8
panic logic and configuration validation are untouched. Verification on
rustc 1.88.0 against the real egui 0.27.2 harness: `cargo check` and
`cargo clippy -- -D warnings` clean for the production sources,
`rustfmt --check` clean, and **44/44 tests pass** (settings tests extended
to cover the new checkbox field, including backward-compatible loading).
Final pixel-level visual acceptance is a Windows-only manual step and is
not claimed from the Linux harness.

## Classic-look polish round (screenshot-driven, presentation-only)

A Windows 11 screenshot of the redesign above showed four presentation
defects. Each was root-caused in the vendored real egui 0.27.2 sources and
fixed without touching any engine, scheduling, dispatch, telemetry or
persistence code path:

1. **Charcoal numeric fields and combo boxes.** egui's fill-less `Button`
   (which `DragValue`'s button mode creates, `widgets/button.rs:265` /
   `widgets/drag_value.rs`) and `ComboBox` (`containers/combo_box.rs:423`)
   paint `visuals.weak_bg_fill`, whose inactive default is the dark-theme
   `Color32::from_gray(60)` (`style.rs:1196`). `apply_style()` set only
   `bg_fill`, so every DragValue and ComboBox rendered dark with
   near-invisible text. Fix: `weak_bg_fill` (plus `bg_fill`, `bg_stroke`,
   `fg_stroke`) is now set explicitly for all five widget states — white
   when idle, the classic hover tint `(229,241,251)` when hovered, the
   pressed tint `(204,228,247)` when active, white with the input border
   when open (expanded combo), and a near-white `(250,250,250)` for the
   disabled state so the gated Random-offset box reads as a disabled white
   edit control. `extreme_bg_color` stays white for DragValue's
   keyboard-edit `TextEdit` mode, and `selection.stroke` is dark so selected
   dropdown rows stay readable on the light highlight.
2. **Half-clipped group-box titles.** `group_box()` painted the title with
   `ui.painter_at(rect)`, which clips to the frame rect — but the title sits
   half above `rect.top()`, so its upper half (and the face-colored backdrop
   that breaks the border) was cut off. The painter now uses a clip region
   expanded upward (`rect.expand2(vec2(12, 16))`), the backdrop fully covers
   the 1 px border line, and the `CentralPanel` top margin grew from 10 to
   14 px so the first title has headroom.
3. **Dead gray zone below the footer.** A headless layout probe driving the
   real `app::update` through the real egui 0.27.2 layout engine (same
   embedded fonts, logical points) measured the content bottom at y = 431 in
   a 560-wide window; the viewport is now 560x442 (was 560x524), leaving
   ~11 px under the footer hint. The same probe confirms that at 560x442 no
   text shape is clipped by its clip rect and no opaque dark-family fill is
   painted anywhere on the main screen.
4. **Cosmetics.** Group frames use near-square 2 px rounding with the same
   1 px border and 13 px proportional title; the two test-only `dead_code`
   warnings (`MouseButton::all`, `ClickType::all`/`label`, used only by the
   Windows-only `app.rs` that the path-included test builds omit) carry
   targeted `#[allow(dead_code)]` annotations instead of any crate-wide
   blanket.

Known and accepted: the native OS title bar may stay black when Windows 11
runs with dark app mode. egui/eframe 0.27 exposes no API to recolor the
title bar of a native-decorated window, and a custom title bar would break
the classic-dialog goal, so the stock chrome is kept and documented.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo check` clean,
`cargo clippy -- -D warnings` clean and **44/44 tests pass**; the layout
and style probes above ran headlessly against the real egui renderer
output. Pixel-level visual acceptance remains a Windows-only manual step
and is not claimed from the Linux harness.

## Dark mode + footer wording round (presentation-only)

User-requested follow-up: friendlier footer wording, a dark theme, and a
stability re-proof. No engine, scheduling, dispatch, telemetry or
validation code path changed; the only behavioral addition is one persisted
preference field.

- **Theme system.** All hard-coded `COLOR_*` constants became a `Palette`
  struct with two `static` instances. `LIGHT_PALETTE` is byte-identical to
  the previous classic palette; `DARK_PALETTE` mirrors the Windows 11
  dark-dialog family (`#202020` face, `#2C2C2C` sunken fields, light text,
  `#60A5FA` accent, dark chart surfaces). `apply_style()` derives every
  widget-state visual (including the `weak_bg_fill` family from the
  previous round, `extreme_bg_color` and `selection`) from the active
  palette, and `visuals.dark_mode` tracks it. Both palettes drive identical
  layout metrics, so switching themes cannot move a control.
- **Persistence.** New `AppPrefs::dark_mode` (default **false** — a fresh
  install keeps the classic light dialog), serialized as `dark_mode=` under
  a `[ui]` section. Files from older builds have no such key and load
  light; malformed values keep the default. Covered by a new settings test
  (6 settings tests now) plus a strengthened hotkey default-prefs test:
  the suite is **45 tests** total.
- **Footer.** The footer is now a status-bar style row: a short reminder on
  the left that tracks the real assigned hotkey ("F6 start/stop · F8
  emergency stop" by default) and the Dark mode checkbox on the right. The
  reminder no longer hard-codes F6 after a remap.
- **Viewport.** The footer row is slightly taller than the old one-line
  hint, so the fixed window is 560x448 (content bottom measured at
  y = 437.25 in BOTH themes by the headless real-egui layout probe,
  leaving ~11 px of margin).
- **Headless proof.** Three probes ran against the real egui 0.27.2
  renderer output: (1) the layout probe above; (2) a style probe asserting
  that at 560x448 the light theme paints only light-family opaque fills,
  the dark theme only dark-family fills, and no text shape is clipped in
  either theme; (3) a secondary-surface probe that opens the Advanced
  window and the Hotkey dialog by injecting real pointer clicks through
  egui's event pipeline and applies the same fill invariants there
  (theme-accent chart elements whitelisted by exact color).

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo check` clean,
`cargo clippy -- -D warnings` clean, **45/45 tests pass** (config 13,
humanize 9, telemetry 8, settings 6, hotkey 5, icon 4), and all six test
files also compile standalone with zero warnings. Pixel-level visual
acceptance remains a Windows-only manual step and is not claimed from the
Linux harness.

## Global-hotkey heartbeat fix + status-strip visibility fix

User-reported defect: "F6 doesn't work at all, but pressing Start in the
app works." Root cause confirmed by reading the code and reproduced
headlessly: the global F6/F8 hotkeys are polled with `GetAsyncKeyState`
once per UI frame inside `update()`, but egui is event-driven and the app
only requested continuous repaints while a session was already running
(or during capture/pick). An idle, unfocused app therefore produced no
frames at all — the idle repaint delay measured through the real egui
pipeline was literally `Duration::MAX` — so the hotkey poll never
executed and F6 appeared dead unless the window was being interacted
with.

- **Fix: unconditional heartbeat.** Every frame now re-arms a repaint —
  16 ms while running/capturing/picking (unchanged responsiveness), 50 ms
  while idle — so the poll runs continuously at ~20 Hz in the background
  for negligible CPU, and a hotkey press from inside any other
  application is noticed within one poll interval.
- **Second defect found and fixed while verifying:** the status strip
  skipped painting whenever `status_kind == Ready`, which silently hid
  every Ready-kind message — "Session complete: N clicks delivered.",
  "Factory defaults restored.", "Hotkey assignment cancelled.", and the
  MogClicker completion line. The painter now always draws; a fresh start
  shows the theme's idle line ("Ready." / "Ready to mog. Aura fully
  charged.") instead of a dead blank.
- **New headless probe (`verify_heartbeat`).** Ran FAILING before the fix
  (idle repaint delay `Duration::MAX` in all six themes) and green after:
  (1) four fully idle frames per theme must each re-arm the repaint
  within 50 ms; (2) end-to-end Ready-kind visibility — real injected
  clicks open the Advanced window, scroll its content to the bottom,
  press "Reset all settings to factory defaults", and the probe requires
  the Ready-kind confirmation text to be visibly painted in the status
  strip (classic and mog).
- The engine, scheduler, dispatch and telemetry code paths are untouched;
  the fix is two frame-pacing lines and the status painter.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo clippy -- -D warnings` clean,
**51/51 tests pass**, all six test files compile standalone with zero
warnings, and all five probes are green (layout still exactly
548.50 x 439.25 in all six themes; style, secondary-surface and
scale/title/icon probes unchanged). The actual unfocused-F6 experience
requires a real Windows desktop and is listed in the manual checklist.

## Real-time CPS stability round (chart estimator + bounded catch-up)

User-reported defect (with screenshot): the Real-time CPS chart showed a
comb-like baseline with periodic tall spikes — "Still its very unstable i
need it to be stable." Reading the code surfaced three independent root
causes, each confirmed before any fix was written:

1. **The chart estimator aliased against the UI frame rate.** The old
   `drain_and_aggregate` computed one `instant_cps` sample per *UI frame*
   from `delta_clicks / delta_ms`. Any frame in which no click completed
   pushed `0.0` into the history (the comb-like baseline), and a frame
   that happened to span several click completions pushed an inflated
   value (the spikes). Perfectly stable clicking therefore drew an
   unstable chart. The estimator was replaced: the worker now records the
   measured wall-clock gap between consecutive click completions in each
   `ClickRecord` (`actual_gap_ms`), and the UI pushes exactly one sample
   per drained click record (`1000 / actual_gap_ms`). No clicks, no new
   samples — zeros are never fabricated.
2. **The scheduler repaid lateness as a burst.** The absolute schedule
   (`next_deadline = target_tick`) carried unbounded debt: after an OS
   stall the next interval became genuinely short, producing a real burst
   of fast clicks and a real CPS spike. The deadline arithmetic moved
   into a new pure module `schedule.rs`: `next_target_tick` keeps the
   exact absolute cadence while dispatch jitter stays within a 2 ms
   catch-up slack (so rate never drifts), and re-anchors to "now" when a
   stall slips past the slack (so one late click stays one late click and
   no make-up clicks ever fire early). The module has no Windows imports
   and is exercised by its own standalone test binary on any platform.
3. **"Missed deadlines" counted every click.** `deadline_error_ms` was
   measured at burst *completion*, which includes the full button-hold
   duration (default mean 40 ms) — against a 1 ms threshold, every click
   was flagged as missed. Lateness is now measured at dispatch time,
   immediately after the spin-wait returns and before the down event, so
   the statistic reports scheduler error only.

The click path itself is unchanged in character: QPC spin-wait scheduling
(never sleeps), `SendInput`/`PostMessageW` dispatch, SPSC telemetry and
the F8 panic path are untouched except for the deadline arithmetic call
and the two added measurements.

Verification through the offline real-egui harness and standalone rustc:

- `cargo fmt --check` clean over every source and test file.
- **58/58 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
  hotkey 7, icon 6, schedule 7), and all seven test files also compile
  standalone with zero warnings via bare `rustc --test` against the
  pinned 1.88.0 sysroot.
- `cargo clippy -- -D warnings` on the harness lib (which compiles the
  real `app.rs`, `engine.rs`, `telemetry.rs` and `schedule.rs` against
  real egui 0.27.2): zero findings.
- The headless layout probe re-measured all six themes after the chart
  rewrite: maximum content extent is unchanged at **548.50x439.25** inside
  the fixed 560x448 window, and the idle repaint heartbeat still reports
  <= 50 ms for every theme, so the round-6 unfocused-hotkey fix is intact.
- Marker grep stays clean (none of the banned incompleteness markers, no
  `mouse_event`/`keybd_event`, no `std::thread::sleep` in any click path).

Windows acceptance for this round: run a session at a fixed interval with
Random offset 0 and humanization off, open Advanced…, and confirm the
Real-time CPS line is flat at the target rate with "Missed deadlines" at
or near zero. With Random offset or humanization enabled the line wanders
by design because the produced intervals genuinely vary.

## Masterclass polish round (four themes, stop confirmation, 2026 craft)

User-requested final refinement: fix the status strip staying on
"Stopping…" after a manual stop, retire the MogClicker experiment, ship
exactly four well-tuned themes, and apply a research-driven polish pass
(2026 interface-craft consensus: purposeful micro-interactions in the
150–300 ms window, depth as hierarchy rather than ornament, dark-mode
contrast discipline, confirmations that close the feedback loop).

Defect fixed — stuck "Stopping…" status:

- Pressing Stop (button or hotkey) set the transient "Stopping…" status,
  but the running→stopped completion detector only painted a final
  message for repeat-count sessions and macro playback. A manual stop of
  a "Repeat until stopped" session fell through every branch and the
  transient status stayed on screen forever.
- `update()` now closes the loop: when the worker drains while the status
  is still Stopping, the strip settles on "Stopped — N clicks delivered."
  (and macro playback distinguishes "Playback stopped" from "Playback
  complete"). Panic and error outcomes keep their own status, as before.

Theme set reduced to four (Classic light, Dark, Midnight OLED, Terminal
green):

- The MogClicker meme theme (a text-reskin experiment) and Vaporwave are
  removed: palettes, the `ThemeText` re-skin machinery, the runtime
  window retitle/icon swap, and the hot-pink icon color way are all gone;
  labels are now plain inline strings.
- `AppTheme::from_key` no longer recognizes "mog"/"vaporwave"; the
  tolerant settings parser falls back to the default theme, so an old
  settings file can never resurrect a retired theme (covered by updated
  hotkey and settings tests).

Polish pass (verified headlessly where possible):

- Every classic push button is now a state-aware painter: fill and border
  track idle/hover/press, a ~180 ms eased hover lift with a soft drop
  shadow encodes "what floats acts", and the pressed state sits back
  down. Global `style.animation_time` is 180 ms, inside the 150–300 ms
  window where motion reads as responsive.
- Floating surfaces (Settings, Hotkey dialog, dropdown popups) carry a
  soft offset shadow so layering reads as hierarchy.
- Contrast audit across all four palettes: every text, dim-text, accent
  and status color was computed against its background; the one failure
  (Classic warning amber, 4.12:1) was darkened to 4.75:1. All other
  combinations measure 4.7:1–17:1.

Verification (Linux harness, rustc 1.88.0, real egui 0.27.2):

- 67 module tests pass standalone and in the harness (config 13,
  humanize 9, telemetry 8, settings 8, hotkey 7, icon 6, schedule 7,
  macro 9); the retired-theme coverage was replaced with equally strict
  fallback and icon-determinism assertions, never weakened.
- `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.
- Layout probe: all four themes paint within 560x456, idle repaint
  heartbeat 10/10.
- Interaction probe: footer grid Start (F6) / Stop (F6) / Hotkey setting /
  Record & Playback in two aligned rows; the strings "Advanced", "Mog",
  "Vaporwave" are never painted; Settings opens with the factory reset
  present and the theme dropdown lists exactly the four shipped themes.

## Diagnostics removal + stability round (Advanced window retired)

User-requested follow-up to the MacroClicker round: remove the Advanced
button and the entire Advanced window ("nobody will be needing diagnostics
on their autoclicker") and maximize stability. Engine modules are
untouched; only UI surfaces changed.

What was removed from `src/app.rs` (UI only):

- The **Advanced…** button and the whole Advanced window: the delivery
  section (SendInput vs background window-message targeting UI), the
  expert Gaussian humanization section, the telemetry dashboard (metrics
  grid, real-time CPS sparkline, interval histogram) and the session log
  view.
- The per-frame telemetry drain/aggregate pass in `update()` — the UI no
  longer does any per-click bookkeeping work per frame, which removes the
  main steady-state allocation/scan cost while a session runs.
- The window-enumeration state (`windows`, `selected_window`) and all
  chart ring buffers (`cps_history`, sliding-window sample queue, stats).

What was kept, deliberately:

- `src/telemetry.rs`, the background delivery path in `src/engine.rs` /
  `src/win32.rs` and `src/humanize.rs` stay intact with their tests (the
  core-equivalence constraint); they are simply not exposed in the UI.
- **Reset all settings to factory defaults** moved from the Advanced
  window to the bottom of the Settings window.
- The session log Vec stays capped at 200 entries (it is no longer
  rendered; the cap keeps memory bounded).

Stability hardening:

- `sanitize_loaded_config()` runs at startup after settings load: a
  persisted config from an older build that selected background delivery,
  synchronous delivery or expert humanization is reset to the supported
  surface (global SendInput, no target HWND, humanization off) before the
  saved-fingerprint snapshot, so a stale file can never start the app in
  a mode the UI cannot display or undo.
- `handle_start` pins the same delivery invariants on every start.
- The main window returned to a tighter 560x456 (the third footer button
  row is gone; the status strip now hosts the small Settings… button).

Verification (Linux harness, rustc 1.88.0, real egui 0.27.2):

- All 67 module tests pass standalone and in the harness, unchanged.
- `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.
- Layout probe: all six themes paint within 560x456 with no overflow and
  the 10/10 idle repaint heartbeat intact.
- Interaction probe: the footer grid is exactly Start (F6) / Stop (F6) /
  Hotkey setting / Record & Playback in two aligned rows, the string
  "Advanced" is never painted, and clicking Settings… opens the Settings
  window containing the relocated factory-reset button.

## MacroClicker rebrand + Record & Playback round (fully beating 4.1)

User request: keep the look "super super" matched to OP Auto Clicker 4.1,
rename the main title to **MacroClicker**, and make the app actually fully
beat OP Auto Clicker 4.1. The one 4.1 capability still missing was the
bottom-row **Record & Playback** button, so this round ships it for real —
not as a dead control — plus the rebrand and the authentic 4.1 button rows.

What changed:

- **Rebrand.** The default window/taskbar title, the eframe app id, the
  first log line, the settings-file header comment and the binary name
  (`macro_clicker.exe`, CI artifact `macro_clicker-windows-x64`) now say
  MacroClicker. The MogClicker theme keeps its own "MogClicker" retitle.
  The settings folder stays `%APPDATA%\op-auto-clicker` so every existing
  user setting survives the rename.
- **Authentic 4.1 button rows.** The footer grid now mirrors 4.1 exactly:
  Start (F6) / Stop (F6), then Hotkey setting / Record & Playback, with
  this app's extra Settings… / Advanced… as a third row. The fixed window
  grew from 560x448 to 560x480 to seat the third row; the headless layout
  probe re-measured all six themes at 548.50x471.75 inside 560x480.
- **Record.** A new `MacroRecorder` (pure module `macro_rec.rs`) captures
  real clicks: while recording is armed the existing GetAsyncKeyState
  heartbeat (the same 16 ms poll that powers the global hotkeys, so it
  works while other apps are focused) edge-detects the three physical
  mouse buttons, reads GetCursorPos, and stores position + button + the
  measured gap since the previous click. Clicks are ignored while the
  foreground window belongs to this process (new
  `win32::foreground_is_own_process()` via GetForegroundWindow +
  GetWindowThreadProcessId + GetCurrentProcessId), so pressing "Stop
  recording" never records itself. The toggle hotkey and F8 also stop a
  recording; a 2048-step cap stops runaway recordings.
- **Playback.** `ClickEngine::start_with_macro` hands the recorded steps
  to the worker (one mutex clone at session entry — the click loop itself
  stays lock-free). Each iteration overrides position, button and gap from
  the current step through the existing dispatch and the bounded-catch-up
  absolute QPC schedule, so playback inherits drift-free timing, the F8
  panic path, telemetry and the CPS chart unchanged. Loop budgets reuse
  `max_clicks` (`steps x loops`, or unlimited for "until stopped"), and
  playback refuses background-HWND mode (global SendInput only). The UI
  never pushes its live config into the engine during playback, so the
  loop budget cannot be overwritten mid-run.

Verification through the offline real-egui harness and standalone rustc:

- `cargo fmt --check` clean over every source and test file.
- **67/67 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
  hotkey 7, icon 6, schedule 7, macro 9 — a new eighth binary), all eight
  test files also compile standalone with zero warnings via bare
  `rustc --test` against the pinned 1.88.0 sysroot.
- `cargo clippy -- -D warnings` on the harness lib (real `app.rs`,
  `engine.rs`, `macro_rec.rs` against real egui 0.27.2): zero findings.
- New `verify_macro` probe drives the real UI headlessly: the root
  viewport retitles to "MacroClicker" on frame one; the footer paints
  Start (F6), Stop (F6), Hotkey setting, Record & Playback, Settings… and
  Advanced…; clicking Record & Playback opens the window showing
  "Recorded steps: 0", Record, Clear, the loop radios and Play; the loop
  controls survive real pointer events.
- The layout probe re-measured all six themes at the new window size:
  maximum content extent 548.50x471.75 inside 560x480, idle repaint
  heartbeat <= 50 ms everywhere.
- Marker grep stays clean; no `mouse_event`/`keybd_event`; no
  `std::thread::sleep` anywhere in a click or playback path.

Windows acceptance for this round: confirm the title bar reads
MacroClicker; record a short macro across another window, replay it 2x and
until-stopped, stop with the button, the hotkey and F8; confirm clicks on
MacroClicker's own windows are never recorded; and confirm an ordinary
(non-macro) session still behaves exactly as before.

## MogClicker accent round (presentation-only)

User-requested follow-up: make the MogClicker meme theme the "absolute
best" with real mog accents. Wording was grounded in the actual slang
(Merriam-Webster: "mog — to look or perform far better than someone
else"; meme-culture staples "aura", "looksmaxxing" and "brutally
frame-mogged"). No engine, scheduling, dispatch, telemetry or validation
code path changed; no new preference fields.

- **Hot-pink group captions.** The `Palette` gained a `group_title` slot;
  the five non-meme themes bind it to their normal text color
  (byte-identical rendering), while MogClicker paints the four group-box
  captions in its hot-pink accent (255, 46, 136).
- **Themed window/taskbar icon.** `icon.rs` was refactored so the
  procedural crosshair generator takes its accent/background colors as
  parameters; `app_icon_rgba()` is pixel-identical to before, and a new
  `mog_icon_rgba()` renders the same geometry in hot pink on deep purple.
  While the Mog theme is active the app swaps the live window icon via
  `ViewportCommand::Icon` (handled by egui-winit 0.27), sent once per
  theme-family change. Two new icon tests pin the variant: identical
  alpha-channel geometry, exact (255, 46, 136) center dot, and at least
  200 visibly recolored pixels.
- **More meme wording** (all status-strip/log strings, zero layout
  impact): running status appends "+1000 aura per click.", stopping shows
  "Aura cooling down…", natural completion reports "Mogging complete —
  everyone brutally frame-mogged: N clicks delivered.", the activity log
  opens with "Mogging session started — looksmaxxing at … CPS.", and the
  Settings-window hint calls the theme a certified aura mogger. The
  strings moved into the `ThemeText` table (classic wording is unchanged
  verbatim for every other theme).
- **Headless proof (real egui 0.27.2).** All four probes re-ran green:
  layout still exactly 548.50 x 439.25 in all six themes (the new accents
  change zero metrics); style fills + text clipping clean across all six;
  injected-click secondary surfaces fine in classic/dark/mog; and the
  scale/title probe now additionally asserts that the Mog theme emits
  exactly one 32x32 `Icon` command whose center pixel is (255, 46, 136),
  that title/icon commands do not repeat, and that the default
  configuration sends neither resize nor icon commands.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo clippy -- -D warnings` clean,
**51/51 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
hotkey 7, icon 6), and all six test files also compile standalone with
zero warnings. The real taskbar-icon swap and on-screen appearance remain
Windows-only manual acceptance items and are not claimed from the Linux
harness.

## Theme gallery + Settings window + UI scaling round (presentation-only)

User-requested follow-up: a Settings entry point inside the app, a gallery
of themes including the "MogClicker" meme theme, and a user-selectable
window scale. No engine, scheduling, dispatch, telemetry or validation
code path changed; the behavioral additions are two persisted preference
fields and two viewport commands (window title and window size).

- **Theme system.** `AppPrefs::dark_mode` was generalized into an
  `AppTheme` enum with six variants — Classic light (default), Dark,
  Midnight OLED, Terminal green, Vaporwave, and MogClicker — each a full
  22-field `Palette` static. Legacy settings files still load:
  `dark_mode=true` maps to Dark, `dark_mode=false`/absent to Classic, and
  a modern `theme=` key always outranks the legacy boolean regardless of
  line order (tested both ways).
- **Themed wording.** A `ThemeText` table carries the handful of strings a
  theme may re-skin (window title, four group titles, Start/Stop verbs,
  footer). Only MogClicker deviates: title "MogClicker", Start → "Mog",
  Stop → "It's over", "Mogging interval"/"Mog options"/"Mog repeat"/
  "Aura position", footer "F6 to mog · F8 when it's over", and a running
  status of "Mogging at … CPS". Inner control labels keep their normal
  wording so the app stays usable. The OS title is switched at runtime via
  `ViewportCommand::Title`, sent only when it changes.
- **Settings window.** The footer's Dark-mode checkbox became a
  **Settings…** button opening a small egui window with the six-entry
  theme picker and a 75–200 % window-scale picker; both persist under
  `[ui]` (`theme=`, `ui_scale=`). Out-of-range scales clamp to 75–200 and
  malformed values keep 100 (tested).
- **Scaling.** Each frame the desired zoom (`ui_scale/100`) is compared to
  `Context::zoom_factor()`; on mismatch `set_zoom_factor()` is called, and
  one frame later — once egui reports the factor as applied — a single
  `ViewportCommand::InnerSize(560x448 logical)` re-asserts the fixed size
  so the physical window really scales and nothing is cut off.
- **Headless proof (real egui 0.27.2).** Four probes: (1) layout — content
  bounds measured at 548.50 x 439.25 in **all six themes** at width 560,
  so 560x448 still fits with ~9 px margin and no Mog label widens any
  row; (2) style — at 560x448, dark-family themes (Dark, OLED, Terminal,
  Mog) paint only dark-family opaque fills, light-family themes (Classic,
  Vaporwave) only light fills (each theme's exact accent color
  whitelisted), with zero clipped text; (3) secondary surfaces — real
  injected pointer clicks open the Hotkey dialog, Settings window and
  Advanced window in Classic, Dark and Mog, their marker texts appear, and
  the fill invariants hold with all surfaces open; (4) scale/title —
  `ui_scale=150` yields zoom 1.5 followed by exactly one
  `InnerSize(560,448)` command and then silence, the Mog theme emits
  `Title("MogClicker")` on the first frame, and a default configuration
  sends no resize commands and keeps zoom 1.0.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo clippy -- -D warnings` clean,
**49/49 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
hotkey 7, icon 4), and all six test files also compile standalone with
zero warnings. On-screen scaling behavior (actual pixel growth of the OS
window) and visual appearance remain Windows-only manual acceptance items
and are not claimed from the Linux harness.

## Dashboard redesign (both engines) — SUPERSEDED

> **Historical record only.** The dark four-tab design described below was
> later replaced by the authentic classic Win32 redesign documented directly
> above; the C++/ImGui "Engine A" it references has been removed and the
> Rust/egui app is the single shipped engine.

The original single-page cyberpunk dashboard showed every expert field at
once with five competing neon colors. Both engines now share one modern dark
design system and a four-tab information architecture:

- one blue interaction accent plus semantic green/amber/red only for status;
  neutral card surfaces, 8–10 px rounding, larger type, generous spacing.
- tabs **Clicker / Target / Analytics / Activity**; expert timing fields
  (sigma, clamps, hold durations, raw interval) move behind an
  "Advanced timing & humanization" collapsing section.
- the Clicker tab exposes one CPS slider as the single source of truth, a
  repeat mode (until stopped / after N clicks), and only the coordinate
  fields the selected dispatch mode actually consumes; background mode shows
  a direct prompt to the Target tab when no window is selected.
- the footer holds one large Start control plus Stop and Panic (F8); the
  header shows a colored status pill and the four key counters.

The change is presentation-only. The worker thread, QPC scheduler,
SendInput/PostMessage dispatch paths, SPSC telemetry, F8 panic logic, and all
configuration validation are untouched, so every existing test remains
valid. Verification for the redesign:

- Engine A's `main.cpp`, `app.cpp` and `theme.cpp` pass `-fsyntax-only
  -Wall -Wextra -Wpedantic` against the **real** Dear ImGui 1.90.4
  `imgui.h`/`imconfig.h` with Win32/D3D11 signature stubs; the portable CTest
  binaries still pass 3/3 with `-Werror`.
- Engine B passes the offline real-egui 0.27.2 harness with **zero
  diagnostics in the production sources** and 23/23 integration tests; a
  negative-control injection was rejected as expected.
- Final visual/interaction acceptance remains a Windows-only manual step.

## Offline real-egui type-check harness

crates.io is unreachable from the sandbox, so a temporary harness (kept
outside the repository) was assembled that compiles the **unmodified**
engine-b source files via `#[path]` includes against:

- the **real** egui/epaint/emath/ecolor 0.27.2 sources (GitHub tarball),
- the **real** parking_lot 0.12.1, ab_glyph 0.2.32, owned_ttf_parser 0.25.1,
  ttf-parser 0.25.1, smallvec, cfg-if, scopeguard and libc,
- thin local shims for `ahash` and `nohash_hasher` (only the
  `HashMap`/`HashSet`/`RandomState::with_seeds`/`hash_one` and
  `IntMap`/`IntSet`/`IsEnabled` surface these crates use),
- signature-only stubs of `eframe` (native entry point, including the real
  0.27.2 `on_exit(&mut self, Option<&glow::Context>)` signature) and
  `windows-sys` 0.52, with every FFI signature transcribed from the
  engine's own call sites and cross-checked against the windows-rs 0.52.0
  source on GitHub; the stub covers `GetCursorPos`, `GetAsyncKeyState` and
  the right/middle button constants added in the 4.1 parity round.

Result on rustc 1.88.0: `cargo check` finishes with **zero diagnostics in
the application sources** (warnings exist only inside vendored upstream
crates and the local shims), and the real 44 integration tests pass
(`13 + 9 + 8 + 5 + 5 + 4` across config, humanize, telemetry, hotkey,
settings and icon). A deliberate type error injected into `app.rs` was
correctly rejected (E0308), proving the harness really type-checks the
production files. The sandbox rustc (1.88) predates the
`float_literal_f32_fallback` lint, so defect 6 cannot be reproduced
locally; explicit suffixing removes every `f32` fallback by construction
instead.

## Static policy audits (grep-verified, current single engine)

Re-run after consolidation against `autoclicker/`:

- No placeholder markers (the banned marker list from the build prompt,
  including unimplemented/todo macros and ellipsis stubs) appear in any
  source file.
- No `mouse_event` / `keybd_event` call sites (comments mentioning the
  prohibition only).
- No sleep call in any click path (idle waiting uses a kernel event, active
  waiting uses QPC plus `std::hint::spin_loop`); the only textual mentions
  are comments stating the prohibition.
- `timeBeginPeriod(1)` paired with `timeEndPeriod(1)` via RAII.
- F8 panic flag sampled before every click and inside every hold and
  inter-click spin; the configurable F6 toggle is polled separately and can
  never be assigned F8 or Escape.
- Global dispatch uses arrayed `SendInput` with left/right/middle down/up
  flags; background dispatch uses `PostMessageW` / `SendMessageTimeoutW`
  with the matching `WM_*BUTTON*` messages, `MK_*` flags and `MAKELPARAM`
  client coordinates.
- No `std::thread::sleep` exists anywhere, including double/triple-click
  gaps, which are QPC-spun 50 ms waits that keep sampling the stop/panic
  flags.

The current tree was re-verified in the offline harness after the
hardening round: `cargo check` reports zero diagnostics in the
`autoclicker/` sources against real egui 0.27.2, all 44 integration tests
pass, `rustfmt --check` and `cargo clippy -- -D warnings` are clean and a
negative-control type error was rejected as expected. All f32 geometry
literals carry explicit `_f32` suffixes for rustc 1.99's
`float_literal_f32_fallback` lint; the icon module uses f64 pixel math
exclusively, so it introduces no f32 fallback at all.

## Required Windows acceptance run

On Windows 10 21H2+ or Windows 11 x64 with a stable Rust toolchain (the
MSVC target reuses the Visual Studio Build Tools linker automatically; no
CMake, Ninja or vcvars step is needed):

```
cd autoclicker
cargo build --release
cargo test --release
cargo run --release
```

Then follow the fourteen-step manual checklist in `autoclicker/README.md`
(step 7 now explicitly includes pressing the hotkey while another
application has focus):
dashboard rendering, interval and random-offset behavior, right/middle and
double/triple bursts, picked fixed coordinates, repeat-count auto-stop,
hotkey remap and F8 reservation, settings persistence across restarts,
unfocused and minimized background targets, F8 panic stop (button must never
stick), the six-theme switch (including the MogClicker retitle, hot-pink
icon swap and meme status lines), the window-scale resize, and clean
shutdown with no leaked handles.

The repository's GitHub Actions workflow performs an additional, genuinely
Windows-native gate on every push and pull request: `cargo fmt --check`,
`cargo clippy`, `cargo test`, a release build on `windows-latest`
(x86_64-pc-windows-msvc), and uploads `op_auto_clicker.exe` as a build
artifact. Visual appearance and real click delivery remain manual
acceptance items because they require an interactive desktop.
