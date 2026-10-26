# autoclickerBYARENA — OP 4.1 Killer dual-workspace auto-clicker suite

Two feature-parallel Windows x64 auto-clickers built from a single
specification:

| Workspace | Stack | Entry binary |
| --- | --- | --- |
| `engine-a-cpp/` | C++20, Dear ImGui (pinned `v1.90.4`), Win32, DirectX 11, CMake | `AutoClickerEngineA.exe` |
| `engine-b-rust/` | Rust stable 2021, eframe/egui (pinned `=0.27.2`), windows-sys (pinned `=0.52.0`), Cargo | `autoclicker_engine_b.exe` |

## Shared engineering properties

- Dedicated click worker thread, never the UI/render thread.
- QPC spin scheduling on the active click path with no sleep calls; idle
  parking on a Win32 event.
- RAII `timeBeginPeriod(1)` / `timeEndPeriod(1)` around each active session.
- `THREAD_PRIORITY_TIME_CRITICAL` with verified failure reporting.
- Real Box-Muller Gaussian humanization, runtime adjustable, with measured
  mean/sigma displayed on the dashboard.
- Global arrayed `SendInput` mode and background HWND message mode
  (`WM_MOUSEMOVE` / `WM_LBUTTONDOWN` / `WM_LBUTTONUP`, `MK_LBUTTON`,
  `MAKELPARAM`) supporting unfocused and minimized targets.
- Lock-free single-producer/single-consumer telemetry ring buffer, live CPS
  graph, interval histogram, consistency percentage and missed-deadline count.
- F8 emergency panic stop sampled before every click and inside every spin.
- Unit tests for Gaussian generation, ring buffer behavior and config
  validation in both workspaces.

## Build and test

### Engine A (requires Windows + MSVC + CMake 3.24)

```
cmake -S engine-a-cpp -B engine-a-cpp/build -G "Visual Studio 17 2022" -A x64
cmake --build engine-a-cpp/build --config Release --parallel
ctest --test-dir engine-a-cpp/build --build-config Release --output-on-failure
```

### Engine B (requires Windows + Rust stable)

```
cargo build --release --manifest-path engine-b-rust/Cargo.toml
cargo test --manifest-path engine-b-rust/Cargo.toml
```

The platform-independent logic of both engines (config validation, Gaussian
humanization, SPSC telemetry and statistics) is standard-language code and is
covered by tests that also compile on non-Windows runners. The GUI, input
injection and worker threads are Windows-specific and must be acceptance
tested on Windows 10 21H2+ or Windows 11 x64; see each workspace README for
the full manual checklist.

## Verification evidence

Exact commands, toolchain versions, results and known limitations from the
off-Windows verification runs (including ThreadSanitizer and real-cargo test
runs) are recorded in [`docs/VERIFICATION.md`](docs/VERIFICATION.md).
