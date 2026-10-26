# Verification record

This document records exactly how the suite was verified outside Windows, what
was executed, and what remains to be run on a Windows 10 21H2+/11 x64 machine.
Both engines target Windows only; the Linux sandbox used for development has no
Windows SDK, DirectX runtime, display server, or access to crates.io.

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

## Static policy audits (grep-verified on both engines)

- No placeholder markers (the banned marker list from the build prompt,
  including unimplemented/todo macros and ellipsis stubs) appear in any
  source file.
- No `mouse_event` / `keybd_event` call sites (comments mentioning the
  prohibition only).
- No sleep call in any click path (idle waiting uses a kernel event, active
  waiting uses QPC plus `_mm_pause` / `std::hint::spin_loop`); the only
  textual mentions are comments stating the prohibition.
- `timeBeginPeriod(1)` paired with `timeEndPeriod(1)` via RAII in both.
- F8 panic flag sampled before every click and inside every hold spin.
- Global dispatch uses arrayed `SendInput`; background dispatch uses
  `PostMessageW` / `SendMessageTimeoutW` with `MK_LBUTTON` and
  `MAKELPARAM` client coordinates.

## Required Windows acceptance run

On Windows 10 21H2+ or Windows 11 x64, with VS 2022 (C++20 + Windows SDK) and
stable Rust:

```
cmake -S engine-a-cpp -B engine-a-cpp/build -G "Visual Studio 17 2022" -A x64
cmake --build engine-a-cpp/build --config Release --parallel
ctest --test-dir engine-a-cpp/build --build-config Release --output-on-failure

cargo build --release --manifest-path engine-b-rust/Cargo.toml
cargo test  --release --manifest-path engine-b-rust/Cargo.toml
```

Then follow the nine-step manual checklist in each workspace README:
dashboard rendering, global clicks, live Gaussian stats, invalid-config
rejection, unfocused and minimized background targets, F8 panic stop (button
must never stick), sub-millisecond missed-deadline accounting, and clean
shutdown with no leaked handles.
