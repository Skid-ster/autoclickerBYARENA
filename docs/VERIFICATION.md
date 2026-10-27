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

Then follow the eleven-step manual checklist in `autoclicker/README.md`:
dashboard rendering, interval and random-offset behavior, right/middle and
double/triple bursts, picked fixed coordinates, repeat-count auto-stop,
hotkey remap and F8 reservation, settings persistence across restarts,
unfocused and minimized background targets, F8 panic stop (button must never
stick), and clean shutdown with no leaked handles.

The repository's GitHub Actions workflow performs an additional, genuinely
Windows-native gate on every push and pull request: `cargo fmt --check`,
`cargo clippy`, `cargo test`, a release build on `windows-latest`
(x86_64-pc-windows-msvc), and uploads `op_auto_clicker.exe` as a build
artifact. Visual appearance and real click delivery remain manual
acceptance items because they require an interactive desktop.
