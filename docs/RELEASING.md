# Releasing MacroClicker

MacroClicker releases are public, user-facing artifacts. Release notes
should read as project documentation—not as a conversation with one
person or a request for private acceptance testing.

## Versioning

MacroClicker follows semantic versioning:

- **Patch** (`1.5.1`): backward-compatible fixes.
- **Minor** (`1.6.0`): backward-compatible features or substantial polish.
- **Major** (`2.0.0`): breaking behavior, settings, or platform changes.

The Git tag, `autoclicker/Cargo.toml` package version, embedded Windows
FileVersion, changelog heading, and release-notes filename must agree.

## Release preparation

Before tagging:

1. Update the version in `autoclicker/Cargo.toml`.
2. Move completed entries from `CHANGELOG.md`'s `[Unreleased]` section
   into a dated version section.
3. Add `docs/releases/vX.Y.Z.md` using the structure below.
4. Update README test counts and behavior descriptions.
5. Update the manual acceptance checklist in `autoclicker/README.md`.
6. Record what was actually executed in `docs/VERIFICATION.md`; keep
   Windows runtime claims limited to observed behavior.
7. Open a pull request and wait for the Windows CI job to pass:
   formatting, clippy with warnings denied, tests, release build, and
   GUI-subsystem/version metadata verification.
8. Merge the pull request, then create and push an annotated `vX.Y.Z`
   tag on the merge commit.

The tag workflow requires the checked-in release-notes file. It appends
the exact published asset size and SHA-256, attaches
`macro_clicker.exe` and `SHA256SUMS.txt`, and publishes the GitHub
release.

## Release-notes structure

Use concise, public-facing language:

```markdown
One short paragraph describing the release.

## Highlights

- The most important user-visible outcome.

## Added

- New capability.

## Changed

- Existing behavior that changed.

## Fixed

- Defect and corrected behavior.

## Compatibility

- Windows versions, settings migration, and update paths.

## Known issues

- Only confirmed limitations that affect this release.

## Full changelog

Link to the GitHub comparison for the previous stable tag.
```

Omit empty categories. Do not include personal testing requests,
maintainer diary entries, internal agent instructions, or claims that
have not been verified. Put detailed engineering evidence in
`docs/VERIFICATION.md`, not in the release summary.

## After publishing

Verify all of the following:

1. The release is marked **Latest** and is neither draft nor prerelease.
2. `macro_clicker.exe` and `SHA256SUMS.txt` are attached.
3. The SHA-256 in the notes matches both the checksum file and the
   GitHub API's asset digest.
4. The previous stable version's updater evaluates the live
   latest-release response as **Available** with the new version.
5. The new version evaluates the same response as **Up to date**.
6. The release title, tag, changelog, and embedded EXE version agree.

Do not move or replace a published version tag to repair notes or a
binary. Publish a new patch version instead.
