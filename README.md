# OP Auto Clicker

A modern Windows x64 auto-clicker written in stable Rust with an egui
dashboard. A dedicated worker thread drives a QPC spin schedule with
Gaussian humanization, while the UI stays fully responsive.

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

The executable is `autoclicker/target/release/op_auto_clicker.exe`.

## Features

- Dedicated click worker thread — the render thread never runs timing code.
- QPC spin scheduling on the active click path with no sleep calls; idle
  parking on a Win32 manual-reset event.
- RAII `timeBeginPeriod(1)` / `timeEndPeriod(1)` around each active session.
- `THREAD_PRIORITY_TIME_CRITICAL` with verified failure reporting.
- Real Box-Muller Gaussian humanization, adjustable at runtime, with
  measured mean/sigma on the dashboard.
- Global arrayed `SendInput` mode and background HWND message mode
  (`WM_MOUSEMOVE` / `WM_LBUTTONDOWN` / `WM_LBUTTONUP`, `MK_LBUTTON`,
  `MAKELPARAM`) supporting unfocused and minimized targets.
- Lock-free single-producer/single-consumer telemetry ring, live CPS graph,
  interval histogram, consistency percentage and missed-deadline count.
- F8 emergency panic stop sampled before every click and inside every spin.
- Single-screen dark interface: click rate slider, mode-aware coordinate
  controls, repeat options, one Advanced section for expert timing fields,
  and a solid Start / Stop / Panic footer.
- Unit tests for Gaussian generation, ring-buffer behavior and config
  validation (23 tests; platform-independent modules compile anywhere).

The GUI, input injection and worker threads are Windows-specific and are
acceptance-tested on Windows; see [`autoclicker/README.md`](autoclicker/README.md)
for the full manual checklist and
[`docs/VERIFICATION.md`](docs/VERIFICATION.md) for the verification record.

## Repository layout

| Path | Purpose |
| --- | --- |
| `autoclicker/` | The Rust/egui application (the single shipped engine) |
| `docs/VERIFICATION.md` | Toolchains, test evidence, defect history and limitations |
