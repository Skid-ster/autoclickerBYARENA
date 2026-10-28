## Summary

Describe the user-visible problem and the change that resolves it.

## Changes

- Describe each meaningful change.

## Verification

- [ ] `cargo fmt --all --manifest-path autoclicker/Cargo.toml -- --check`
- [ ] `cargo clippy --manifest-path autoclicker/Cargo.toml -- -D warnings`
- [ ] `cargo test --manifest-path autoclicker/Cargo.toml`
- [ ] `cargo build --release --manifest-path autoclicker/Cargo.toml`
- [ ] Manual Windows checks listed below, or an explicit note that they remain unobserved

## Windows acceptance notes

List any real Windows behavior that still needs to be exercised, including display scaling, update/install, hotkey, or click-delivery paths touched by this change.

## Documentation

- [ ] README and manual acceptance checklist updated when behavior changed
- [ ] `CHANGELOG.md` updated for user-visible changes
- [ ] Release notes added or updated when preparing a versioned release
