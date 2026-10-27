# Verification record

This document records exactly how the application was verified outside
Windows, what was executed, and what remains to be run on a Windows
10 21H2+/11 x64 machine. The app targets Windows only; the Linux sandbox used
for development has no Windows SDK, display server, or access to crates.io.

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

## Dashboard redesign (both engines)

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
- signature-only stubs of `eframe` (native entry point) and `windows-sys`
  0.52, with every FFI signature transcribed from the engine's own call
  sites and cross-checked against the windows-rs 0.52.0 source on GitHub.

Result on rustc 1.88.0: `cargo check` finishes with **zero diagnostics in
the engine-b sources** (warnings exist only inside vendored upstream
crates), and the real 23 integration tests pass (`9 + 6 + 8`). A deliberate
type error injected into `app.rs` was correctly rejected, proving the
harness really type-checks the production files. The sandbox rustc (1.88)
predates the `float_literal_f32_fallback` lint, so defect 6 cannot be
reproduced locally; explicit suffixing removes every `f32` fallback by
construction instead. The C++ portable translation units were rebuilt
directly with `g++ -std=c++20 -Wall -Wextra -Wpedantic -Werror`: all three
test binaries pass.

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
- F8 panic flag sampled before every click and inside every hold spin.
- Global dispatch uses arrayed `SendInput`; background dispatch uses
  `PostMessageW` / `SendMessageTimeoutW` with `MK_LBUTTON` and
  `MAKELPARAM` client coordinates.

The consolidated tree was re-verified in the offline harness after the
rename: `cargo check` reports zero diagnostics in the `autoclicker/`
sources against real egui 0.27.2, the 23 integration tests pass, and a
negative-control type error was rejected as expected. All f32 geometry
literals carry explicit `_f32` suffixes for rustc 1.99's
`float_literal_f32_fallback` lint.

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

Then follow the nine-step manual checklist in `autoclicker/README.md`:
dashboard rendering, global clicks, live Gaussian stats, invalid-config
rejection, unfocused and minimized background targets, F8 panic stop (button
must never stick), sub-millisecond missed-deadline accounting, and clean
shutdown with no leaked handles.
