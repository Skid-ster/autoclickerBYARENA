# Changelog

All notable changes to MacroClicker are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.5.2] - 2026-10-29

### Changed

- Removed the manual Window scale percentage picker; automatic window
  adaptation remains available, and existing saved scale values are retained.

### Fixed

- Reflowed very narrow, tall windows into additional layouts so controls
  grow instead of leaving large gaps between cards and above the footer.
- Fit secondary windows temporarily when necessary without changing the
  saved or physical window size, and restore the dashboard zoom on close.
- Keep lengthy dialog messages and their controls reachable with scrolling;
  prevent unusually long update versions and status messages from clipping
  other controls, while retaining their full text on hover.
- Refresh the native minimum size after a monitor DPI change without
  requesting a window resize, and keep application settings authoritative
  instead of allowing a second eframe window-size store.

## [1.5.1] - 2026-10-29

### Changed

- Tall windows now reflow into a portrait layout so the controls grow
  when height is added, while classic and wide window shapes retain
  their respective compositions.

### Fixed

- Serialized click-worker stop and restart transitions and prevented
  concurrent configuration changes from producing mixed snapshots.
- Applied interval edits and validation consistently for the Start button
  and toggle hotkey, including intervals longer than ten seconds.
- Replayed recorded clicks once per recorded step, regardless of the
  dashboard's double/triple setting or ordinary-click random offset.
- Made settings saves atomic across concurrent attempts, retained and
  retried failed saves, and recovered invalid offsets without discarding
  unrelated preferences.
- Improved update cancellation, source-change reporting, worker drain,
  per-attempt cleanup, and recovery-backup preservation on rollback failure.

## [1.5.0] - 2026-10-28

### Added

- Source-checkout installations can now update in the app: a clean
  `main` checkout is fast-forwarded to the exact release tag before the
  verified release build is installed.
- The Updates window now shows the installation type, published download
  size, and a shape-coded update path covering each stage through restart.
- Public project documentation for contributing, security reporting,
  releases, and issue/pull-request intake.
- MIT License for the public source release.
- Tagged releases now attach `SHA256SUMS.txt` beside `macro_clicker.exe`.

### Changed

- The footer **Update vX.Y.Z** action now starts an available verified
  update directly when no click or macro session is active, opening the
  Updates window as the progress and cancellation surface.
- Tagged GitHub releases are published from checked-in public release
  notes, with the exact asset size and SHA-256 appended by CI.

### Fixed

- Cargo/source builds are no longer forced to use a manual browser
  download when they belong to a recognized MacroClicker checkout.
- Update cancellation and failure wording now distinguishes source
  preparation from the unchanged running program.

## [1.4.1] - 2026-10-28

### Fixed

- Prevented the window-driven auto-fit from multiplying the physical
  minimum window size, which could trap a large window at an inflated
  size until the application was reopened.
- Applied shrinking window fits immediately during inward drags.
- Re-derived the auto-fit when the user Window scale percentage changes
  so a stale fit cannot multiply with the new percentage.
- Interpreted live and persisted window sizes consistently in
  fit-independent points.

## [1.4.0] - 2026-10-28

### Changed

- Brought the main window into the Updates window's modern card design:
  rounded elevated groups, accent ticks, softer shadows, an accent-filled
  Start action, and a danger-outlined Stop action while running.
- Eased newly adopted window-fit zoom levels over approximately 180 ms
  instead of applying them in one frame.

## [1.3.0] - 2026-10-28

### Added

- One-click verified self-update: download with progress and cancellation,
  exact size and SHA-256 verification, rollback-safe replacement, and
  automatic restart into the new version.
- Startup confirmation after a successful in-app update.

## [1.2.0] - 2026-10-28

### Changed

- Replaced discrete scale tiers with continuous fit-to-window scaling.

### Fixed

- Hardened GitHub release discovery and update-state handling.

## [1.1.0] - 2026-10-28

### Added

- Built-in GitHub update checker, Updates window, and footer update flag.

## [1.0.1] - 2026-10-28

### Fixed

- Shipped the executable as a Windows GUI-subsystem binary with its own
  icon and embedded version metadata, eliminating the companion console
  window.

## [1.0.0] - 2026-10-28

### Added

- Initial public MacroClicker release: OP Auto Clicker 4.1-style dialog,
  precision QPC click engine, Record & Playback, global hotkeys, themes,
  resizable/scaling window, persistent settings, and Windows CI releases.

[Unreleased]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.5.2...HEAD
[1.5.2]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.5.1...v1.5.2
[1.5.1]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.5.0...v1.5.1
[1.5.0]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.4.1...v1.5.0
[1.4.1]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/Skid-ster/autoclickerBYARENA/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/Skid-ster/autoclickerBYARENA/releases/tag/v1.0.0
