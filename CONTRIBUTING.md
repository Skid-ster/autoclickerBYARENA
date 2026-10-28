# Contributing to MacroClicker

Thank you for helping improve MacroClicker. This project is a Windows x64
application written in stable Rust with egui 0.27.

## Development setup

Requirements:

- Windows 10 21H2 or later, or Windows 11 x64
- Stable Rust for the MSVC target
- Visual Studio Build Tools with the Windows SDK

Clone the repository and run the following from `autoclicker/`:

```text
cargo build --release
cargo test --release
cargo run --release
```

The release executable is written to
`autoclicker/target/release/macro_clicker.exe`.

## Before opening a pull request

Run the same gates as Windows CI:

```text
cargo fmt --all --manifest-path autoclicker/Cargo.toml -- --check
cargo clippy --manifest-path autoclicker/Cargo.toml -- -D warnings
cargo test --manifest-path autoclicker/Cargo.toml
cargo build --release --manifest-path autoclicker/Cargo.toml
```

Pull requests should:

- Keep one focused change per pull request.
- Explain the problem, the chosen approach, and any compatibility impact.
- Add or update tests for behavioral changes.
- Update the README files, `CHANGELOG.md`, and the manual acceptance
  checklist when user-visible behavior changes.
- State clearly which Windows runtime behavior was exercised and which
  remains unverified.

## Project constraints

MacroClicker's reliability properties are part of its design:

- Click scheduling stays on the dedicated worker thread with the QPC
  absolute schedule and bounded catch-up.
- Input uses arrayed `SendInput`; do not introduce `mouse_event` or
  `keybd_event`.
- The main window remains naturally resizable, non-scrolling, and fully
  visible at its 560x456 floor.
- Themes share layout metrics; a theme change must never move a control.
- Collections and buffers must remain bounded.
- Transient UI states must resolve to an explicit terminal status.
- Settings parsing must remain tolerant of unknown or malformed values.
- Updates must never execute a binary that fails size and SHA-256
  verification.

See `docs/VERIFICATION.md` for the verification record and
`docs/RELEASING.md` for the release process.

## License

MacroClicker is licensed under the MIT License. By submitting a
contribution, you agree that it may be distributed under the terms in
[`LICENSE`](LICENSE).
