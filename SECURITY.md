# Security policy

## Supported versions

Security fixes are provided for the latest stable MacroClicker release.
Older portable builds should be updated to the current release before a
report is investigated.

## Reporting a vulnerability

Do not publish exploit details, proof-of-concept code, credentials, or
personal data in a public issue.

To request a private reporting channel, open a minimal issue titled
`Security contact request` containing only a high-level description such
as “update verification issue” and a way for the maintainer to contact
you. Do not include technical details in that public issue.

Reports are most useful when they include:

- The affected MacroClicker version and installation type
- The Windows version
- A concise impact assessment
- Reproduction steps that can be shared privately
- Any suggested mitigation

## Update integrity

MacroClicker's in-app updater installs a release only after confirming
that the downloaded `macro_clicker.exe` is an AMD64 Windows program with
the exact byte size and SHA-256 digest published for that GitHub release
asset. A failed check deletes the download and leaves the installed
program unchanged.

Release binaries are currently unsigned. Windows SmartScreen warnings
should be reported as usability issues, not treated as proof that a
correctly checksummed release asset is malicious.
