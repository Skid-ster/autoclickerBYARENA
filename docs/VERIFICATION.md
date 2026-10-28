# Verification record

This document records exactly how the application was verified outside
Windows, what was executed, and what remains to be run on a Windows
10 21H2+/11 x64 machine. The app targets Windows only; the Linux sandbox used
for development has no Windows SDK, display server, or access to crates.io.

## Continuous fit-to-window scaling round (current)

The shipped tier system was still judged "off" on the user's 1440p
display, and the user asked for two things with no exaggeration:
research exactly how OP Auto Clicker 4.1 resizes/scales, then beat
that behavior. Both the research and the replacement design turned
out to need more care than the previous rounds gave them.

### What is actually proven about OP Auto Clicker 4.1

"OP Auto Clicker 4.1" is not one publicly inspectable artifact, so
the evidence has to be separated by lineage:

- **The winget-catalogued 4.1 portable build.** Microsoft's
  winget-pkgs manifest for `OPAutoClicker.OPAutoClicker` 4.1 points
  to a SourceForge `AutoClicker.exe` with SHA-256
  `1ce7da6f2813c2ad1d2e496be6714e08cd618e6d9fe2df26c2bd4d894c9a6ec1`.
  Public binary analyses of that exact hash (Gridinsoft,
  Joe Sandbox/JoeReverser and ANY.RUN) identify a native PE32+
  x86-64 GUI program, FileVersion 4.1, built with **AutoIt v3**.
  AutoIt's own `GUICreate` documentation says its default dialog
  is "non sizable and non maximizable" unless the program opts in
  with `WS_SIZEBOX`/`WS_MAXIMIZEBOX`, and `GUICtrlSetResizing`
  documents that controls react to resizing only through explicit
  docking modes. No public analysis inspected exposes this build's
  actual `GUICreate` call, style bits, dimensions, resizing modes
  or manifest DPI values, so those specifics are **not claimed**.
- **The original open-source Orphamiel line.** The inspected
  `matt81093/AutoClicker` source is a .NET Framework 4.5 WinForms
  program, AssemblyVersion 1.0.0.2: `Form1.Designer.cs` sets a
  284x280 client size, `AutoScaleDimensions` 6x13 with
  `AutoScaleMode.Font`, `FormBorderStyle.FixedSingle` and
  `MaximizeBox=false`; the inspected project/config files contain
  no application DPI declaration. Two descendant repositories keep
  the same fixed-single, maximize-disabled arrangement. This proves
  the behavior of that source line; it does **not** prove that a
  later closed binary labelled 4.1 has the same dimensions or code.
- **OP's public feature/download pages** list no window-resize or
  in-app UI-scaling feature.

The robust conclusion across the identified artifacts is therefore
narrower than earlier rounds stated: the classic OP presentation
in evidence is a fixed dialog with no free resize/maximize and no
app-level proportional scaling system. An earlier entry in this
file called OP 4.1 a ".NET Framework 4.7.2 application"; that was
unsupported and is corrected below. MacroClicker is claimed to
beat the *evidenced resizing/scaling behavior*, not undisclosed
internals of every binary ever mirrored under the 4.1 label.

### The replacement: fit the design canvas to the window

The four discrete scale tiers (x1.10/1.25/1.40/1.60) are gone.
Window-driven presentation is now one continuous uniform fit:

- The decision variable is the neutral-point window size
  `B = live points x total applied zoom` (the physical size at
  100 % zoom for a given monitor DPI). It is invariant under the
  zoom changes it drives, so a feedback loop is mathematically
  impossible rather than merely unlikely.
- `fit_total = min(B.x / canvas.x, B.y / canvas.y)` against the
  compact 560x456 canvas or the wide 904x456 canvas. The user's
  own percentage divides back out of the window-driven share; the
  total zoom is capped at the previously verified 2.80 envelope;
  the factor is quantized **down** to whole percent (so it can
  never push the live size under the OS-declared floor); a new
  factor applies only after the neutral size has held still for
  six frames and differs from the applied factor by more than a
  0.011 dead band. The cold-start path runs the identical
  calculation, so the opening frame and settled frame agree.
- The wide three-across layout is chosen by shape, not by a width
  tier: it engages when the window aspect is at least 1.58:1 and
  the wide fit's predicted live width is at least 760 pt, and
  holds down to 1.46:1 / 700 pt. A high personal percentage in a
  small window therefore keeps the proven compact stack instead of
  squeezing three boxes into an unproven width.
- Vertical surplus loosens further than the previous round: each
  inter-group gap may absorb up to 72 pt, the spring above the
  bottom cluster counts as one equal share beside the gaps, and
  the status/actions/footer remain pinned to the bottom edge.
- A fresh install (no settings file existed at load, size still
  exactly the classic floor) opens once at a display-aware size:
  the same fraction of the monitor the classic dialog occupies on
  the 1920x1080 reference, clamped to [1, 2] and measured in
  neutral points so Windows DPI scaling is never double-counted.
  Any existing settings file — including one deliberately storing
  the floor — bypasses this completely.

Persistence, the 560x456 OS minimum, native resizing/snap/maximize,
themes and the engine are unchanged; the code diff is confined to
`app.rs` (fit evaluation, wide gate, first-run sizing, wording) and
documentation comments in `hotkey.rs`.

### Measured (headless real-egui harness, this round)

The sandbox toolchain and egui harness were rebuilt from pinned
sources: Rust 1.88.0, real egui/epaint/emath/ecolor 0.27.2 with
vendored ab-glyph/ttf-parser/parking_lot dependencies, local
ahash/nohash/eframe shims, and a signature-faithful windows-sys
0.52 stub (signatures copied from microsoft/windows-rs 0.52.0; the
stub's event wait deliberately fails so the click worker exits —
its harness-only status text is the one visible difference from a
Windows run). The harness drives the app's real `update` through
`egui::Context::run`, modeling the OS by holding the neutral
physical size fixed while zoom changes, granting `InnerSize`
requests, and ignoring them while maximized.

Settled end-states (100 % user scale unless noted; every case:
36 text shapes, **zero** outside the live rect, **zero** pairwise
text overlaps, zoom unchanged over 30 further frames):

| Case | Neutral size | Total zoom | Live points | Layout |
| --- | ---: | ---: | ---: | --- |
| Classic floor | 560x456 | x1.00 | 560x456 | compact |
| Enlarged | 900x700 | x1.53 | 588x458 | compact |
| Snapped half of 2560x1440 | 1280x1392 | x2.28 | 561x611 | compact |
| Tall 973x1404 | 973x1404 | x1.73 | 562x812 | compact |
| Wide 1920x1040 | 1920x1040 | x2.12 | 906x491 | wide |
| Laptop 1366x720 | 1366x720 | x1.51 | 905x477 | wide |
| User 200 % at floor | 1120x912 | x2.00 | 560x456 | compact |
| User 75 % maximized-size | 2560x1392 | x2.798 | 915x498 | wide |
| **2560x1440 maximized** | 2560x1392 | **x2.80** | 914x497 | wide |

Plus the boundary behaviors:

- **Drag**: 24 frames of continuous resizing from 560x456 to
  1800x1000 — the zoom never moved during the drag, then landed
  once at x1.99 after the size settled and held.
- **Fresh install, 2560x1440 monitor**: opens at neutral 747x608
  (the 1080p-reference fraction), settling at x1.33 / 562x457 pt.
- **Fresh install, 1920x1080 monitor**: opens exactly at the
  classic 560x456 floor.
- **Existing settings storing the floor, 2560x1440 monitor**: the
  floor is respected exactly; no first-run upgrade.
- **Adaptation off, 1600x1200**: zoom stays at the user's x1.00
  and the fixed classic composition centers — the escape hatch is
  unchanged.

For comparison, the previously shipped tier system fed the same
2560x1392 maximized window at x1.60 (1600x870 live points, the
880 pt content column centered with ~360 pt of side margin). The
fit system feeds it at x2.80 (914x497 live points): every control
is physically 75 % larger again and the text bounding box spans
x18..888 of the 914 pt width.

Also re-verified this round: `cargo fmt --check` clean; clippy
`--lib -- -D warnings` exit 0 in the harness; all **77** standalone
test-crate tests pass under `rustc -D warnings --test` (config 13,
hotkey 7, humanize 9, icon 6, macro 9, schedule 7, settings 9,
telemetry 8, update 9); `src/main.rs` compiles under `-D
warnings`. Windows CI remains the authority for MSVC
compilation/clippy/tests/release build.

Still honestly unobserved: the feel of the one-step rescale at the
end of a real Windows drag and the maximized 1440p result on the
user's actual monitor. The probes prove settled geometry, input
invariance and non-oscillation in real egui layout; they are not a
substitute for looking at the panel, which is step 17 of the
manual checklist in `autoclicker/README.md`.

## Built-in updater round

The user asked for an in-app updater: notice when a new release
exists, offer an update button that sits blanked out with "no new
updates available" while current, in its own separate tab/button.
Shipped (version 1.1.0):

- **Check**: one HTTPS GET to
  `api.github.com/repos/Skid-ster/autoclickerBYARENA/releases/latest`
  over the OS WinHTTP stack (`win32::https_get_text`) — system
  proxy (automatic detection), TLS and certificate validation from
  Windows, no new crates (two added `windows-sys` features).
  User-Agent is `MacroClicker/<version>`; nothing else about the
  user is sent. 10 s timeouts, 256 KiB body cap, non-200 is a
  clean failure. The decision logic is a new platform-free
  `update` module: tolerant scan of the JSON `tag_name`, numeric
  version ordering, and refusal to guess on anything unorderable.
- **Orchestration**: a detached worker thread reports exactly one
  verdict over an mpsc channel; the UI state
  (NotChecked → Checking → UpToDate / Available / Failed) always
  terminally resolves — even a disconnected channel lands in
  Failed, never an eternal spinner. Runs once per launch by
  default (new persisted `check_updates` preference, toggle in
  the Updates window) and manually via "Check now".
- **UI**: an **Updates…** button beside Settings… in the footer —
  it relabels itself green "Update available!" when a check has
  proven a newer release — opening an Updates window with the
  current version, the status line, **Check now**, and
  **Download update…** greyed out until an update is actually
  available. Download opens the GitHub latest-release page
  (ShellExecuteW) in the browser.
- **Deliberate boundary**: the app never downloads or replaces
  its own binary. Self-swapping a running unsigned exe is how
  installs get bricked; instead the button lands the user on the
  exact release and they replace one file (settings in
  `%APPDATA%` persist). Source builders keep their
  `git pull` + `cargo build` flow — the check tells them when.

Verification: 9 new `update_tests` (version parse/order, JSON tag
extraction incl. garbage refusal, the full evaluate decision) —
77 tests total, run green locally per-suite with standalone
`rustc --test` (the suites path-include platform-free modules,
which update.rs was designed to be), plus rustfmt clean.
Windows-only compile/clippy coverage for the WinHTTP and egui
wiring is CI's, as usual; WinHTTP signatures were confirmed
against the windows-sys 0.52 documentation before writing the
FFI, and the CI "Verify GUI subsystem" step still guards the
shipped binary.

Still honestly unobserved: the live round trip from a real
Windows desktop (WinHTTP against api.github.com through a home
router, a proxy, or a captive portal), the Updates window's
visuals on hardware, and GitHub rate limiting under enthusiastic
"Check now" use (60 unauthenticated requests/hour/IP — the 403
path degrades to the Failed state by design).

## Finished-exe round: GUI subsystem, embedded icon and metadata

Two reports about the first published release: launching the
downloaded exe opened a **console window next to the app**, and the
release page carried a puzzling `.sha256` download. Both traced to
how the binary was packaged, not to application code — this round
changes no app logic at all.

### The console window

`main.rs` had no `windows_subsystem` attribute, so the linker
produced a *console-subsystem* PE and Windows allocated a console
whenever the exe was launched from Explorer (running via
`cargo run` from an existing terminal masked it — the app just
shared that console). Release builds now compile with
`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`:
the downloaded exe opens **only** the autoclicker, while debug
builds keep their console for diagnostics. Nothing in the codebase
prints to stdout or spawns child processes (verified by grep), so
the GUI subsystem loses nothing. Because the sandbox cannot
download CI artifacts (GitHub's storage hosts are unreachable from
here), the proof is a new **permanent CI step**, "Verify GUI
subsystem and embedded metadata", which reads the Subsystem field
from the freshly built exe's PE optional header on the Windows
runner and fails the build unless it is `2` (IMAGE_SUBSYSTEM_WINDOWS_GUI).
It also asserts the version metadata below, so the console can
never silently come back. Passed on PR run 40736837930.

### Icon and version metadata in the exe

A new `build.rs` (using the pinned, build-time-only `winresource`
crate) embeds into the PE resources:
- `assets/macro_clicker.ico`: the crosshair icon rendered natively
  at 16/20/24/32/40/48/64/128/256 px from the exact signed-distance
  spec in `src/icon.rs` (geometry scales by size/32, 0.5 px AA
  ramps stay native, straight alpha instead of egui's
  premultiplied). The 32 px render was cross-checked against every
  property `icon_tests.rs` asserts and matches (transparent
  corners, opaque blue-accent center, alpha symmetry, solid core;
  counts: 48 AA edge pixels, 241 accent, 591 dark). The file icon,
  taskbar icon and window icon are now the same artwork everywhere.
- Version strings (ProductName "MacroClicker", FileDescription,
  InternalName, OriginalFilename) and numeric FILEVERSION /
  PRODUCTVERSION packed from the crate version, now 1.0.1.

Resource embedding is cosmetic and **warns-and-continues** if the
SDK resource compiler is missing, so it can never break a build;
conversely, the CI step's `ProductName` assertion (via Windows'
own VersionInfo API) proves the whole resource compile — icon
included — succeeded on the release binary, since one `.rc` builds
both and a bad icon path fails `rc.exe` outright.

### Leaner binary, cleaner releases

`strip = true` joins the release profile's existing fat LTO and
single codegen unit. The separate `.sha256` release asset is gone:
it was an integrity fingerprint of the exe, but GitHub now prints
each asset's `sha256:` digest directly under its download link
(visible in the user's own screenshot of the page), making the
extra file pure clutter. Removed from the v1.0.0 release and from
the workflow; README's Download section now points at GitHub's
built-in digest instead.

All 68 tests, clippy `-D warnings` and rustfmt pass on
windows-latest; no source file under `src/` changed except the one
attribute block in `main.rs`.

Still honestly unobserved: the embedded icon as rendered by a real
Explorer/taskbar at each size (the ICO passed structural and
picture checks locally, and CI proved it is inside the exe, but no
Windows desktop was available to look at it), and SmartScreen's
reaction is unchanged — the exe is still unsigned, which metadata
does not fix.

## Fill-the-window round: height-aware layout and the 1.60 tier

The two-axis step system shipped, Aurora was confirmed present in the
running build, and the user's verdict on their 1440p screen was still
"doesn't scale properly" — correctly. Measuring the shipped build
with this round's probe expectations *before* changing anything put
numbers on the screenshot: in a grown window the first group frame
started 99 pt below the top and the footer floated 200 pt above the
bottom edge. The zoom had done all it mathematically could: in a
window ~3x taller than the 456 pt dialog, the step is width-capped
(the live width may not go under the 560 pt floor), so even a perfect
step leaves hundreds of points of surplus height — and the layout
policy spent that surplus on *centering* the column between two
voids (`MAIN_GAP_EXTRA_MAX` was 14 pt). The bug was not the zoom
anymore; it was that the composition refused to use the height the
user gave it.

### What changed

- **The column now fills the frame.** Groups always flow from the
  top of the panel, exactly as they do at the floor size. Surplus
  height loosens every inter-group gap by an equal share (the spring
  counts as two shares; per-gap loosening now caps at 40 pt instead
  of 14), and whatever remains gathers as one spring directly above
  the status strip — so the status strip, action buttons and footer
  ride at the window's bottom edge like a status bar at *every*
  height, and the leftover space reads as client background between
  content and status bar (the pattern every native resizable window
  uses) instead of an island floating mid-window. The distribution
  always sums to the full surplus, so the column ends flush with the
  panel's bottom margin at any size, and at the floor (surplus 0)
  every value is 0: the 560x456 and wide-372 compositions are
  byte-identical to before (probe-verified: same frame rects, same
  footer gap).
- **A fourth step tier, x1.60** (engage 1600 / release 1544 neutral
  points, same 56 pt hysteresis as the other bands), so large and
  maximized windows on 1440p-and-up displays get a presence the
  1.40 ceiling couldn't give. Two guards keep it inside proven
  territory:
  - *Total-zoom envelope 2.80* (user % x step): the largest total
    reachable before this tier existed (200 % x 1.40). At 200 % the
    step still caps at 1.40 exactly as shipped before; the new tier
    only ever engages where the resulting physical minimum size
    stays inside the previously verified envelope.
  - *Wide proven floor for the height vote*: when the wide
    arrangement is active, the height vote's fit cap uses a 700 pt
    live-width floor (the wide row was verified down to 714 pt), so
    an automatic step can never squeeze the three-abreast row
    narrower than the proven case. Width votes and the user's own
    percentage are unaffected, as before.

### Measured (headless real-egui probes, physical-pixel rigs)

The user's exact window, 973x1404 px: at 100 % display scaling it
now settles at step x1.60 feeding 608x878 pt — groups at y 14..388
with gaps loosened 13 -> 53 pt, footer 19 pt from the bottom edge;
at 125 % display scaling, step x1.25 feeding 622x898 pt with the
same filled geometry. Resolution sweep, all asserting settled zoom,
containment, top-anchored first group and bottom-pinned footer:
1600x972 @125 % (x1.25), portrait 700x1200 (fit-capped x1.25, fed
exactly 560 wide), 150 % short-wide (x2.10 total, unchanged), 1440p
@125 % 2000x1300 (x1.60), 200 % envelope (x2.80, not 3.20), wide
squeeze guard 1024x1500 (held at x1.40 feeding 731 pt instead of
640), 1080p laptop 1366x720 (x1.25), 1440p maximized 2560x1392
(x1.60). Floor probes (five-theme metric identity, exact frames),
paint, interact and persistence probes all pass unchanged in
expectation; 68 unit tests pass unweakened; fmt clean; clippy
`--lib -- -D warnings` exit 0. One harness modeling correction was
needed: the rig now ignores `InnerSize` commands while maximized,
matching Windows (the app never relied on them landing; the
maximized persistence guard is untouched).

Deliberately *not* done: raising the content width caps (736/880)
so maximized windows would stretch controls edge-to-edge — comically
wide buttons are worse than side margins, and the caps were a
considered decision of the composition round; uncapped gap
loosening (equal spread of all sections reads as stretched, and
related controls should stay grouped); any change to group
*interiors* (field widths, row heights) at surplus sizes.

Still honestly unobserved: how the bottom-cluster pinning and the
1.60 tier feel during a real Windows drag on the user's 1440p panel
(the probes assert settled end-states and per-frame geometry, not
frame pacing on hardware), and whether the spring region reads as
calm client background in person.

## Two-dimensional scaling and Aurora theme round

The user's screenshot after the visual-depth round showed the real
scaling bug precisely: a very tall window (roughly 910x1400) with the
whole interface sitting small in the vertical middle, empty panels
above and below. The scale-step system from the adaptive round voted
on window **width only**, so a window grown tall never earned a larger
step. The user also asked for research into how OP Auto Clicker 4.1
does scaling, the most stable possible design, and sanctioned a
"super modern theme". All three landed here. Engine, scheduler,
dispatch and telemetry are untouched; the diff is `app.rs` (step
decision + one palette) and `hotkey.rs` (one theme variant).

### Research findings (what "OP 4.1 scaling" actually is)

- **No in-app resize/scaling feature is evidenced for OP 4.1.** The
  4.1 feature pages list no resize or UI-scale capability
  (https://opautoclicker.io/features.html,
  https://opautoclicker.io/download.html). *(Correction, added in
  the continuous-fit round above: this bullet originally called
  4.1 a ".NET Framework 4.7.2 application" and speculated about
  its DPI manifest. That was unsupported. The winget-catalogued
  4.1 build is publicly analysed as AutoIt v3, and the inspected
  open-source Orphamiel source is .NET Framework 4.5 / version
  1.0.0.2 with a fixed-single, maximize-disabled form. See the
  current round for the full evidence boundary.)* The durable
  lesson stands: a fixed dialog never surprises anyone, so every
  dynamic behavior we add must be provably incapable of surprising
  (no feedback loops, no fighting the user's chosen size).
- **egui's model is already the stable one, and we already use it.**
  `pixels_per_point = zoom_factor * native_pixels_per_point`; the
  native factor comes from the OS and must never be fought, while all
  app-level zoom is expressed through `zoom_factor` so it survives
  monitor changes (https://github.com/emilk/egui/issues/3602,
  https://docs.rs/egui/latest/egui/struct.Context.html). winit puts
  the process in Per-Monitor-V2 awareness — the mode Microsoft
  recommends (https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context)
  and WinForms guidance uniformly endorses for per-monitor support
  (https://comcomponent.com/en/blog/winforms-high-dpi-guide/).
- The stability rules the step system already followed remain the
  design constraints: decide from **zoom-invariant measures**
  (neutral points = live points x total zoom = physical size at
  native scaling) so feedback is mathematically impossible;
  hysteresis on every threshold; steps apply only after the size
  settles. This round changes *what votes*, not those rules.

### The fix: both axes vote, height is fit-capped

- The tier table (800/1024/1440 engage, 744/968/1384 release,
  x1.10/x1.25/x1.40) is unchanged. Width votes through it exactly as
  before (`tier_for_extent` is the old `auto_zoom_for` body verbatim).
  Height now votes too, converted to width-equivalent neutral points
  by the floor dialog's aspect ratio (560/456), so one table, one
  hysteresis implementation, and "twice the floor's height" means the
  same as "twice its width".
- **The height vote is fit-capped**: it may not exceed the largest
  tier that keeps live points at or above the 560x456 floor on both
  axes under the user's own percentage. Without the cap, a tall
  narrow window's height vote would push live width under the OS
  minimum and the window would be grown wider than the user chose —
  the exact surprise OP's fixed dialog never inflicts. Width-earned
  steps are *not* capped (pre-existing, verified behavior: user 150 %
  in a short wide window still totals x2.10, and the long-standing
  grow-only floor correction covers that case).
- The cold-start step (`initial_auto_zoom`, used by the window
  builder size and by first-frame state) runs the same two-axis rule
  with rising edges only, so the opening frame and the settled frame
  always agree. Persistence is untouched: sizes are still recorded
  as live points x applied step and restored by dividing the opening
  step back out — the formula never cared which axis earned the step.
- Rejected alternatives, with reasons: a **continuous** fit-zoom
  (zoom = clamped size ratio) would re-zoom on every drag frame,
  re-tessellate constantly, and thread a continuous value through
  the discrete persistence conversions — felt instability for zero
  benefit; a **fourth tier** (x1.60) was deferred rather than
  shipped uncalibrated; **uncapped** height votes and height-driven
  wide layout were rejected above.

Measured on the harness (physical-pixel rigs, real egui 0.27.2):
910x1400 @100 % settles at step x1.40 feeding 650x1000 points (the
screenshot case, previously x1.10); portrait 700x1200 fit-caps at
x1.25 feeding exactly the 560x960 floor with the physical size
untouched; 150 % in 1500x900 still totals x2.10; the persisted tall
size opens pre-divided at [650, 1000] and opens verbatim with the
Settings toggle off. All previous scale/layout probe expectations
hold unchanged (1280x777 neutral -> x1.25, wide band, tiers at
1920 points).

### Aurora modern theme

A fifth theme (Settings key `aurora`, label "Aurora modern"): deep
indigo-slate surfaces (#12131B face, #1B1D29 cards, #0E0F16 sunken
fields, #0C0D13 status well) with a violet accent/title (#9F8CFF).
It inherits the full dark depth system (cards, shadow, rim light,
well, pulse) automatically — themes are pure palette values.
Contrast proofed by script over every enabled pair: worst text
6.11:1 (title/accent on card), every border >= 3.0:1 against both
adjacent surfaces (group 3.38/3.06, button 3.49/3.16), disabled text
3.44:1 (best of the dark family). Theme tests iterate
`AppTheme::all()`, so the new key/label round-trips are covered with
no test edits; the layout probe now asserts floor metric identity
across **five** themes, and the paint probe asserts Aurora's four
cards, split backdrop, well, shadows and resting dot from painted
shapes.

Verification: `cargo fmt --check` clean; 68 unit tests pass
unweakened; clippy `--lib -- -D warnings` exit 0; paint, layout,
interact and viewport probes all pass including the new tall,
portrait, user-percent and opening-step cases above; the diff adds
no `unwrap`/`expect` and no TODO markers. Windows CI (fmt, clippy
with warnings denied, tests, release build) re-verifies on MSVC.

Still honestly unobserved: how the new height votes *feel* during a
real Windows drag (the settle-debounce should make them land once,
after the drag, like the width votes — but frame pacing on real
hardware is the user's to judge), and how Aurora reads on a real
panel.

## Visual depth and acknowledgement round

A pure paint-and-state polish round: no layout metric, engine,
scheduler, dispatch, telemetry or settings-semantic change. The diff is
confined to `autoclicker/src/app.rs`. The north star, in the user's
words, was "absolute best looking, FULLY stable" — so every change here
is either a palette value proven by a contrast script, or paint
confined inside geometry that already existed, or a self-terminating
state acknowledgement. Nothing can move a control.

What changed:

- **Elevated group cards (dark themes).** Each group box now fills with
  a new explicit `group_fill` palette surface one step above the panel
  (Dark 40 on 32, OLED 12 on black, Matrix (13,23,13) on (8,14,8)),
  carries a 1 px inner top rim light, and drops a restrained shadow
  reserved *before* the frame paints (a `Shape::Noop` slot on the same
  paint layer, filled once the frame rect exists, so it can never land
  on top of contents). The shadow is skipped where it is invisible or
  un-native: OLED's pure-black face and the Classic theme. Classic's
  `group_fill` equals its `face`, so its group paint is byte-identical
  to the previous round. The title backdrop — which straddles the
  panel above the border and the card below it — is now painted in two
  halves split at the border midline, each half in its own surface
  color, so the native caption break stays seamless on the layered
  surfaces (in Classic both halves are the same color by construction).
- **Deeper sunken fields (dark themes).** With the cards raised, input
  fields dropped below the panel instead (Dark 44→30, OLED 18→8,
  Matrix (12,24,12)→(7,15,7), with matching disabled variants), so the
  depth ordering reads consistently: fields sink, cards sit, buttons
  rise. Field text contrast went *up* (Dark 11.71→13.98:1).
- **Crisper structure lines (dark themes).** Group and button borders
  were raised so every dark-theme border now clears the 3.0:1 non-text
  guideline against both surfaces it touches (they previously sat at
  2.2–2.9:1). Classic's borders are the literal Windows colors and
  stay (grouping there is redundantly encoded by caption + spacing).
  Disabled-control text — WCAG-exempt, but on screen constantly — was
  brightened in the dark themes without losing the disabled read.
- **Sunken status well (dark themes).** The status strip paints a
  recessed `well_fill` plate with a group-colored hairline inside the
  long-reserved 16 px strip. The dot and text coordinates are
  untouched; Classic keeps the flat native status line.
- **Status acknowledgement pulse.** When the status *kind* changes
  (not on same-kind message churn), the state dot swells once and a
  ring expands off it, easing out over 280 ms, then returns to exactly
  its resting 3.5 px radius. Paint-only: a single `f64` timestamp, a
  pure progress helper, and the existing repaint heartbeat holds its
  fast 16 ms tick only while the pulse is live, so the motion is
  smooth without a permanent fast idle and cannot stick — the helper
  returns `None` outside the 280 ms window by construction. It
  composes with the Running dot's alpha breathing (radius vs alpha)
  instead of replacing it.
- **Pressed-button bevel inverts.** A pressed hand-painted button now
  draws its inner top line dark instead of pale — a pushed native
  button catches shadow on its top edge, not light.
- **Hover help.** Pick location, Hotkey setting, Record & Playback and
  Settings gained one-line hover tooltips (zero layout surface).

Deliberately rejected this round, with reasons: a panel gradient and a
theme-switch crossfade (the title backdrop straddles two surfaces, and
`palette()` returns `&'static Palette` so closures can hold it across
mutable calls — an interpolated owned palette would mean a broad
borrow-model refactor for a 280 ms effect); eased layout/zoom
transitions (layout-moving motion is out of contract and would put the
calibrated floor geometry at risk); new typography (font *metrics* are
layout). Each rejection keeps the stability budget spent where it
shows every frame.

Contrast (recomputed by script over every enabled text/surface pair
the new surfaces touch, plus borders; sRGB luminance, no eyeballing):
worst enabled text per theme — Classic 4.71:1 (unchanged), Dark 5.50:1
(dim on card), OLED 6.61:1 (dim on card), Matrix 5.02:1 (dim on card);
all ≥4.5:1. Worst dark-theme non-text border 3.10:1 (all ≥3.0:1, up
from 2.2–2.9:1). Disabled-control text, WCAG-exempt, improved in every
dark theme (minima 2.96→3.25 Dark, 3.15→3.46 OLED, 2.47→2.86 Matrix).

Verification (Linux harness: the workspace compiled against real
egui/epaint 0.27.2 with a shim eframe/windows-sys; probes assert on
painted shapes from headless frames):

- 68 unit tests pass unweakened; `cargo fmt --check` clean; clippy
  `--lib -- -D warnings` exit 0; the diff adds no `unwrap`/`expect`,
  no TODO/stub markers, and touches no file other than `app.rs` plus
  docs.
- New paint probe (per theme): exactly four group frames painted in
  `group_fill` with the new border; both backdrop halves present and
  meeting at border-top + 0.5; exactly one 14 px status well in the
  dark themes and none in Classic; card shadows present in Dark and
  Matrix, absent in Classic and OLED; the resting dot is exactly one
  3.5 px circle. Pulse lifecycle: after a pick-arm/pick-cancel kind
  change at idle, the repaint heartbeat holds ≤16 ms and a ring/dot
  beyond 3.5 px paints; after 0.5 s of frames the heartbeat is back
  at 50 ms and the dot is exactly 3.5 px again. Pressed Start paints
  a dark top bevel, hovered a light one. A sustained hover paints the
  Pick-location tooltip text.
- Regression probes, all green: layout (floor geometry and theme
  metric identity across all four themes, grown-narrow cap and
  vertical symmetry, wide band 880 centered with the three-abreast
  equal-height row at tier ×1.40), interact (real click start/stop
  cycle, disabled painting, hotkey dialog), viewport (restore
  command, maximized persistence silence, user-resize silence,
  1600×972 physical @1.25 DPI feeding 1024 pt at tier ×1.25).

Still honestly unobserved, as always: how the card shadow, rim light
and pulse *feel* on a real Windows display at real frame pacing, and
whether the well's hairline reads as intended under Windows'
compositor. The geometry claims above are assertions over painted
shapes; the aesthetic claim awaits the user's eyes.

## Adaptive scale and wide layout round

At large window sizes the corrected composition still presented a
fixed-size dialog centered in otherwise empty space (visible in the
user's 1601x972 screenshot: correct alignment, but a modest island).
This round makes the window size drive the presentation, behind a
Settings toggle ("Scale and rearrange with window size", default on;
off restores exactly the composition of the correction round below).
Engine, win32, timing, config, humanize, telemetry, schedule and macro
files are untouched; `hotkey.rs`/`settings.rs` gain one persisted
boolean, and one settings test fixture gains the matching field (the
round-trip test then covers the new key).

Design (thresholds in *neutral points*: live points times the total
applied zoom — the width at 100 % total zoom, i.e. physical size at
native scaling. Invariant under the step changes it drives, which is
what makes feedback loops impossible, and under user-percentage
changes, so steps engage at the same physical size for every user
percentage instead of fighting it. Persisted sizes keep their own
*step-independent* measure, below):

- **Scale steps** engage at widths 800 / 1024 / 1440 with factors
  1.10 / 1.25 / 1.40 and release below 744 / 968 / 1384 (hysteresis, so
  a width parked inside a band holds its step). Total zoom is the
  user's persisted percentage times the step factor, applied through
  the existing zoom bookkeeping (min-size redeclaration, grow-only
  floor correction, settle window). A step never sends an `InnerSize`:
  the physical window stays where the user put it and the interface
  rescales inside it.
- **Debounce**: a step applies only after the scale-independent size
  has held still (within 1.5 pt) for 6 consecutive frames while asking
  for that step, and only while the zoom machinery is idle — so steps
  land just after a drag ends, never mid-drag, and never compound with
  an in-flight zoom or restore. The layout switch needs no debounce
  (it cannot feed back into the measured quantity) and follows with
  hysteresis only.
- **Wide layout** at the 1024 step: Click options | Click repeat |
  Cursor position three-abreast (Cursor's pick row folds into stacked
  radio / button / coordinate rows), action buttons as one row of
  four, content cap 880. The row's boxes share one bottom edge through
  the same measured-height feedback as the classic pair. The vertical
  rhythm generalizes to (fill height, gap count): compact (456, 3) —
  arithmetically identical to the previous round — wide (372, 2),
  the fill height calibrated by measurement below.
- **Persistence in step-independent points**: the observer records
  live points times the applied step factor; restore and the builder
  size divide the opening step back out, so the same *physical*
  window returns on every launch under any step. (This is also the
  measure the existing [560, 4x560] sanitizer stays exact in for every
  user percentage, and old files' plain points are already in it.) Files from earlier versions
  (plain points at factor 1.0) read identically. One real bug was
  caught by the viewport probe during development: on the frame a step
  flips, the live points still belong to the old factor (egui applies
  zoom next frame), so the observer — running later in the same
  `update` — persisted points x new factor (1500 became 2100). The
  flip now starts the observation settle window immediately; the
  probe's drag-persist-relaunch round-trip passes (see below).

Verification (offline harness as before — vendored pristine egui
0.27.2, real app modules, windows-sys/eframe shims — extended with a
*physical-window rig*: it feeds points = physical px /
(native ppp x applied zoom) exactly like egui-winit converts, and
applies `InnerSize` commands back to physical pixels):

- **Regression (adaptation off)**: the correction round's full matrix
  re-run with `auto_scale=false` — four themes, floor through
  1200x900 — geometry, title colors, gaps and zoom-locked-at-1.0 all
  pass, proving the toggle-off state is byte-for-byte the previous
  shipped composition.
- **Steps and wide geometry**: physical 700/900/1280/1800 px settle to
  zoom 1.00/1.10/1.25/1.40 with compact/compact/wide/wide layouts; in
  wide, the three boxes measure equal width/height/bottom (289x151 at
  1280 px), the interval spans the 880 cap, four buttons share one
  row, the X field sits inside the Cursor box, nothing paints outside
  the surface, and top/bottom slack balances within tolerance. The
  user-percentage composes multiplicatively (125 % x 1.25 = 1.5625
  asserted on the context). At the user's own setup (1600x972 at
  125 % display scaling, i.e. 1280 neutral points) the layout lands on
  the 1.25 wide step and the content band fills x72..952 of 1024 fed
  points instead of an island.
- **Stability**: hysteresis walk 1280 -> 1000 (holds 1.25 in-band) ->
  940 (drops 1.10) -> 1000 (holds) -> 1100 (rises); a 25-step
  frame-by-frame drag 700 -> 1780 px asserts zoom stays 1.00 for every
  drag frame and lands on 1.40 after the hold; a 1022/1034 px boundary
  wobble (2 frames each, 24 flips) never changes step.
- **Persistence**: restore emits `InnerSize(1024x640)` for a persisted
  1280x800 (base / 1.25) and the physical rig returns to exactly
  1280x800; drag to 1500x900 physical persists `window_width=1500`
  (base, not the 1071 live points) and a relaunch restores 1500x900
  physical; maximized freeze and classic-floor open unchanged.
- **Interaction**: engine start/stop and the milliseconds drag pass in
  compact (560x456, 760x668) and in wide mode through the four-across
  button row; the pick-location arm/cancel flow passes through the
  stacked Cursor box; the Settings toggle is present and clickable.
- `cargo fmt --check` clean, lib clippy `-D warnings` clean, 68
  pre-existing tests pass (harness `cargo test` and standalone
  `rustc --test` per suite), marker/`thread::sleep`/new-`unwrap`
  sweeps clean.

Honest limits: the rig models egui-winit's conversion arithmetically
(points = physical / (native x zoom), deferred zoom application) but
does not run winit itself; how a step *feels* the moment it lands
after a drag, behavior while dragging between monitors of different
DPI, and Segoe UI metric differences (the measured-distribution code
adapts, but the 372 pt wide fill height is font-sensitive by a few
points — a small vertical off-center at most) remain for the manual
Windows run.

## Layout composition correction round (previous; its fixed-size composition is the adaptation-off state above)

The natural-resizing round below shipped with a defect its own probes did
not catch, and this round fixes it. Stating the failure plainly: the
previous round verified growth by checking that *painted output* filled
the window, but it never asserted the painted rectangles of the group-box
frames themselves. An egui `Frame` paints exactly the rect its contents
claim (`crates/egui/src/containers/frame.rs`, `Prepared::paint` uses
`content_ui.min_rect()`), so each group box hugged its widest inner row:
at a real 946x834 Windows window the Click interval border stopped
roughly two thirds across, Cursor position stopped short, the Click
options / Click repeat pair was uneven, and all extra height poured into
three ever-deepening gaps. The full-window panel fill made the old
"painted extent" checks pass anyway. Every claim from the resizing round
other than its growth composition (native resizable frame, 560x456
floor, persisted normal size, zoom handling) still stands and is
unchanged; the sentence "(frames already reflow to available width)" in
that round's notes was wrong and is superseded by this section.

What the composition does now (all in `app.rs`; engine, win32, timing,
config, humanize, telemetry, schedule, macro and settings files are
untouched):

- **Group boxes claim their width.** `group_box` calls
  `ui.set_min_width(ui.available_width())` before adding contents, so the
  painted border spans the full content column at every size and in every
  theme. This also fixes the classic-size alignment for the first time:
  at 560x456 all four borders now share one left and one right edge.
- **The side-by-side pair is truly paired.** `group_box` reports its
  contents' natural height; the taller of Click options / Click repeat is
  fed back (rounded up to whole points) as both boxes' minimum, applied
  by pinning the frame's bottom at an absolute y. Two egui 0.27.2 traps
  were found and verified against the sources while doing this: (a)
  `Frame` size comes from the content min rect, never from the frame
  itself; (b) `Ui::set_min_height` spans the requested height *forward
  from the live cursor* (`placer.rs` `set_min_height` ->
  `next_widget_space_ignore_wrap_justify`), so in absolute coordinates it
  overshoots by the widget's position in the window — measured directly
  (a 60pt request low in the window produced 139/122pt frames in a
  scratch binary) and replaced by an exact `allocate_rect` bottom pin
  (frames then measured exactly 88/88/93/59 across positions). Width
  claims via `set_min_width` are unaffected because the cursor sits at
  the content's left edge in the top-down column.
- **Combo boxes fill their grid cells** (`.width(ui.available_width())`),
  so Click options has no right-side void when the window is wide.
- **Interval units spread without overflow.** The four field+label groups
  are measured each frame and only their true slack is distributed
  between them; a group is never forced wider than it is, so no font or
  size can overflow a cell and stretch the frame (an equal-columns
  variant was measured doing exactly that to the last "milliseconds"
  group at the floor size and was rejected).
- **Bounded gaps, then vertical centering.** Each of the three gaps
  absorbs at most 14 extra points; beyond that, half the remaining
  height pads above the column (bottom half is leftover panel space,
  which needs no insertion), so tall windows compose one centered dialog
  cluster instead of separated islands. Behavior at and near the floor
  (up to ~498pt tall) is identical to before.
- **Group titles paint their accent token.** `group_box` laid the title
  galley out with `pal.group_title` but painted the text with `pal.text`;
  the paint now uses `pal.group_title`, realizing the per-theme accent
  captions (identical in Classic, so native fidelity is kept). Contrast
  re-computed by the WCAG relative-luminance formula against each
  theme's face: Classic 14.95:1, Dark 6.41:1, OLED (on black) 10.47:1,
  Matrix 11.68:1 — all above the 4.5:1 floor.

Verification (offline harness rebuilt in the sandbox: vendored pristine
egui 0.27.2 + path-dependency chain, the app's real modules symlinked in,
windows-sys/eframe shims; nothing from the harness ships):

- **Bug reproduced by measurement before fixing.** With the merged code,
  painted group frames measured at 760x668 points: Click interval
  x12..470 (278 short of the 748 right edge), Click options 211 wide vs
  Click repeat 364 (heights 88 vs 80), Cursor position x12..402,
  inter-group gaps 83.6pt, title painted (235,235,235) instead of the
  (96,165,250) accent — matching the user's Windows screenshot exactly.
- **Layout probe now asserts the painted frames themselves** (the blind
  spot is closed): all four borders' left/right alignment, width =
  min(window-24, 736), pair width/height/top equality, gutter, all three
  gaps bounded and matched, nothing painted outside the surface, four
  interval fields with the last label reaching the frame's right area,
  combo right reach, exact per-theme title color, heartbeat <= 100ms,
  vertical centering balance on tall windows. Green for all four themes
  across the floor, the user's 760x668 size, 1200x900, wide-short,
  narrow-tall, UI scale 125, a frame-by-frame drag sweep up to
  1200x896 and back, and a sub-floor 520x430 robustness case. Sample at
  the user's size (Dark): borders x12..748, pair 364.5x88 both, gaps
  27/27/26; at the floor: x12..548, pair 264.5x88, gaps 13/13/12.
- **Interaction probe** (start/stop cycle through painted buttons,
  milliseconds DragValue dragged at its new spread position 100 -> 108,
  Settings opens) and **viewport probe** (persisted InnerSize restore,
  MinInnerSize redeclaration, live resize persisted on exit, maximized
  freeze) pass at the floor and at 760x668.
- `cargo fmt --check` clean; lib clippy `-D warnings` clean in the
  harness; all 68 pre-existing tests pass unmodified (both via `cargo
  test` in the harness and as standalone `rustc --test` builds per
  suite); marker sweeps (`thread::sleep`, `mouse_event`, TODO-class,
  new `unwrap`/`expect`) clean; `git diff` touches only `app.rs` plus
  docs.

Honest limits: the harness runs egui's layout and painting but not
eframe/winit or the DWM. Exact pixel rounding of centered text, the feel
of dragging a live edge, snap behavior, and font-metric differences
between egui's bundled font and Windows' Segoe UI (which the
measure-then-distribute code adapts to at runtime, but which can shift
individual labels by a point or two) remain for the manual Windows run.

## Natural window resizing round (previous; growth-composition notes superseded by the correction round above)

The main window stopped being fixed-size and now resizes the way every
normal Windows app does, with the classic 560x456 dialog as the floor.
No engine, scheduling, dispatch, telemetry or validation code changed
(engine, win32, timing, config, humanize, telemetry, schedule and macro
files are untouched); the behavioral additions are the persisted window
size, two viewport commands and the live-size observer.

Mechanics, verified against the upstream egui 0.27.2 sources rather than
assumed:

- **Native frame + declared floor.** `main.rs` now uses
  `ViewportBuilder::with_resizable(true)` and
  `with_min_inner_size(MAIN_WINDOW_SIZE)` (egui
  `crates/egui/src/viewport.rs` builder methods `with_resizable` /
  `with_min_inner_size`), opening at
  `OpAutoClickerApp::persisted_window_size()`. A resizable frame is the
  `WS_THICKFRAME | WS_MAXIMIZEBOX` window, so edge/corner drag,
  title-bar maximize, Win+arrow and Snap Layouts come from the OS with
  no custom hit-testing, and winit feeds the minimum into
  `WM_GETMINMAXINFO` so neither dragging nor snapping can go below the
  size where every control fits. Deliberately no maximum size:
  maximize and snap stay unrestricted.
- **Growth is designed, metrics are shared.** Extra width is shared by
  the group boxes (frames already reflow to available width), the
  side-by-side `ui.columns(2)` pair and the footer button grid (already
  width-derived); every hard-coded field width, padding, font size and
  control height stays fixed. Past a 736-point content cap (floor
  content 536 + 200) the side margin grows symmetrically so the column
  centers instead of buttons widening without limit. Extra height is
  split evenly into the three gaps above the status strip, so no
  control gets taller and the status/buttons/footer block stays
  anchored to the bottom. Both helpers reduce to the original metrics
  at 560x456 (12-point margins, 7/7/6 gaps) and depend only on window
  size, so all four themes measure identically.
- **Scale composition without double-scaling (documented decision).**
  The task wording ("re-issue MinInnerSize with the scaled floor") was
  resolved against the code: egui-winit converts every viewport size
  from points to physical pixels with the *current* pixels-per-point
  (zoom x native) when handling `ViewportCommand::MinInnerSize`
  (`crates/egui-winit/src/lib.rs`, the MinInnerSize arm multiplies by
  `pixels_per_point`). The app therefore re-issues the floor in points
  (560x456) one frame after a zoom change — the physical floor rescales
  downstream; multiplying in the app as well would double-scale it.
  On that same settled frame a pending startup/factory-reset restore
  applies the persisted size verbatim, while a plain scale change only
  grows the window when the new zoom left its point size below the
  floor (egui rescales the settle frame's `screen_rect` by the
  old/new zoom ratio, `crates/egui/src/context.rs` — the viewport
  probe traces both branches exactly). An enlarged window is never
  forced back down.
- **Persistence.** New `[ui] window_width=` / `window_height=` keys
  through the same tolerant parser and atomic writer. The live size is
  read from `ctx.input(|i| i.screen_rect)` each frame, sanitized in the
  portable prefs module (non-finite/missing keeps the floor per
  dimension; finite values round to whole points and clamp to
  [floor, 4x floor] = up to 2240x1824) and flows through the existing
  dirty fingerprint plus 1-second autosave throttle and the on-exit
  flush, so a drag-resize writes at most once per second. Maximized,
  minimized and fullscreen sizes are never recorded (checked via
  `i.viewport().maximized` etc.), and observation pauses during
  restore/zoom transitions behind a 750 ms settle deadline that always
  expires — every new transient state terminally resolves even if the
  OS refuses the exact requested size. Factory reset re-arms the
  restore, returning the window to the classic size at 100 %.

Design research (2025/2026 sources read before touching pixels) changed
no palette, type or motion values: the surviving, already-shipped set
(purposeful 150–300 ms feedback motion, depth that encodes
interactivity, semantic status dot, per-theme contrast discipline) was
re-confirmed and kept; glass/liquid-glass, immersive 3D, oversized
typography and AI chrome were rejected for a classic utility dialog.
The unchanged palettes were re-audited arithmetically (relative
luminance over every actually-painted text/status/accent pairing,
including button text on idle/hover/pressed fills): zero pairs below
4.5:1; worst cases are Classic success-on-face 4.71:1, Dark
danger-on-face 5.63:1, OLED dim-on-face 7.10:1, Terminal dim-on-face
5.34:1.

Verification (Linux harness, rustc/cargo 1.88.0 via `@rustbin` at
`/tmp/sysroot`, real egui/epaint/emath/ecolor 0.27.2 plus vendored
parking_lot, libc, cfg-if, scopeguard, ab-glyph, owned-ttf-parser,
ttf-parser, smallvec and headless ahash/nohash-hasher/windows-sys/eframe
shims at `/tmp/eb-real` + `/tmp/eb-vendor`, repo sources symlinked,
tests copied verbatim):

- `cargo fmt --all -- --check` clean in the repo crate.
- **68/68 tests pass** standalone via bare `rustc --test` *and* under
  `cargo test` in the harness (config 13, humanize 9, telemetry 8,
  settings 9 — the new window-size round-trip/clamp/garbage test —
  hotkey 7, icon 6, schedule 7, macro 9). The full round-trip test now
  also carries a non-default 720x560 size; no test was weakened.
- `cargo clippy --lib -- -D warnings` clean for the harness library
  (the real `app.rs`/`engine.rs`/`win32.rs` against real egui);
  warnings exist only inside vendored upstream/shim crates and are
  capped as dependency lints. A deliberate type error injected into
  `app.rs` was rejected (E0308/E0277) and reverted cleanly.
- Layout probe drives the real `App::update` through
  `egui::Context::run` for all four themes at 560x456, 720x560 and
  1000x720: painted extent stays inside the window at every size,
  every core control text is painted, control text heights are
  identical across sizes, the footer bottom gap is a constant 19.25 pt
  at all sizes, the group-title left edge is 21.0 pt at floor/mid and
  141.0 pt at large (12 or 132 margin + 9 title offset — the cap and
  centering measured, not asserted blind), button text centers move
  exactly with the shared extra width, all four themes produce
  byte-identical coordinates per size, and the idle heartbeat is
  10/10 frames per combination.
- Interaction probe (floor and 1000x720) locates controls by painted
  text and clicks with synthesized pointer events: the footer is two
  aligned rows (Start/Stop over Hotkey setting/Record & Playback) with
  column centers matching to < 2 pt, the 3.5 pt semantic status dot is
  painted beside the status text, Settings opens with the factory
  reset, the theme dropdown lists exactly the four shipped themes,
  and "Advanced", "Mog" and "Vaporwave" are never painted. A fit probe
  confirms the Settings window (including the longer scale hint) stays
  inside the floor window.
- Viewport probe inspects the actual `ViewportCommand`s: startup
  emits `MinInnerSize([560 456])` + `InnerSize([720 560])` exactly
  once and then silence while idle; a 100 % -> 150 % change that stays
  above the floor emits only the re-declared `MinInnerSize`; the same
  change from the floor emits `MinInnerSize` + one
  `InnerSize([560 456])` growth; a live 720x560 size is written to
  `settings.conf` on exit while a maximized 1000x720 leaves the floor
  values untouched. A builder probe compiles `main.rs`'s exact
  `ViewportBuilder` chain against real egui and asserts
  `inner_size`, `min_inner_size` and `resizable` carry the intended
  values (the binary entry itself is finally compiled by the
  windows-latest CI job).
- Marker sweep over `src/` + `tests/` clean, no sleep in any click
  path, banned legacy input APIs only in the pre-existing prohibition
  comment, no new runtime `unwrap`/`expect`.

Not proven headlessly and not claimed: the physical drag/snap/maximize
feel, snap-zone refusal under 560 points, restore-after-maximize, and
title-bar behavior on a real Windows desktop — these are step 17 of
the manual acceptance checklist in `autoclicker/README.md`.

## Polish and hardening round

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

## Global-hotkey heartbeat fix + status-strip visibility fix

User-reported defect: "F6 doesn't work at all, but pressing Start in the
app works." Root cause confirmed by reading the code and reproduced
headlessly: the global F6/F8 hotkeys are polled with `GetAsyncKeyState`
once per UI frame inside `update()`, but egui is event-driven and the app
only requested continuous repaints while a session was already running
(or during capture/pick). An idle, unfocused app therefore produced no
frames at all — the idle repaint delay measured through the real egui
pipeline was literally `Duration::MAX` — so the hotkey poll never
executed and F6 appeared dead unless the window was being interacted
with.

- **Fix: unconditional heartbeat.** Every frame now re-arms a repaint —
  16 ms while running/capturing/picking (unchanged responsiveness), 50 ms
  while idle — so the poll runs continuously at ~20 Hz in the background
  for negligible CPU, and a hotkey press from inside any other
  application is noticed within one poll interval.
- **Second defect found and fixed while verifying:** the status strip
  skipped painting whenever `status_kind == Ready`, which silently hid
  every Ready-kind message — "Session complete: N clicks delivered.",
  "Factory defaults restored.", "Hotkey assignment cancelled.", and the
  MogClicker completion line. The painter now always draws; a fresh start
  shows the theme's idle line ("Ready." / "Ready to mog. Aura fully
  charged.") instead of a dead blank.
- **New headless probe (`verify_heartbeat`).** Ran FAILING before the fix
  (idle repaint delay `Duration::MAX` in all six themes) and green after:
  (1) four fully idle frames per theme must each re-arm the repaint
  within 50 ms; (2) end-to-end Ready-kind visibility — real injected
  clicks open the Advanced window, scroll its content to the bottom,
  press "Reset all settings to factory defaults", and the probe requires
  the Ready-kind confirmation text to be visibly painted in the status
  strip (classic and mog).
- The engine, scheduler, dispatch and telemetry code paths are untouched;
  the fix is two frame-pacing lines and the status painter.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo clippy -- -D warnings` clean,
**51/51 tests pass**, all six test files compile standalone with zero
warnings, and all five probes are green (layout still exactly
548.50 x 439.25 in all six themes; style, secondary-surface and
scale/title/icon probes unchanged). The actual unfocused-F6 experience
requires a real Windows desktop and is listed in the manual checklist.

## Real-time CPS stability round (chart estimator + bounded catch-up)

User-reported defect (with screenshot): the Real-time CPS chart showed a
comb-like baseline with periodic tall spikes — "Still its very unstable i
need it to be stable." Reading the code surfaced three independent root
causes, each confirmed before any fix was written:

1. **The chart estimator aliased against the UI frame rate.** The old
   `drain_and_aggregate` computed one `instant_cps` sample per *UI frame*
   from `delta_clicks / delta_ms`. Any frame in which no click completed
   pushed `0.0` into the history (the comb-like baseline), and a frame
   that happened to span several click completions pushed an inflated
   value (the spikes). Perfectly stable clicking therefore drew an
   unstable chart. The estimator was replaced: the worker now records the
   measured wall-clock gap between consecutive click completions in each
   `ClickRecord` (`actual_gap_ms`), and the UI pushes exactly one sample
   per drained click record (`1000 / actual_gap_ms`). No clicks, no new
   samples — zeros are never fabricated.
2. **The scheduler repaid lateness as a burst.** The absolute schedule
   (`next_deadline = target_tick`) carried unbounded debt: after an OS
   stall the next interval became genuinely short, producing a real burst
   of fast clicks and a real CPS spike. The deadline arithmetic moved
   into a new pure module `schedule.rs`: `next_target_tick` keeps the
   exact absolute cadence while dispatch jitter stays within a 2 ms
   catch-up slack (so rate never drifts), and re-anchors to "now" when a
   stall slips past the slack (so one late click stays one late click and
   no make-up clicks ever fire early). The module has no Windows imports
   and is exercised by its own standalone test binary on any platform.
3. **"Missed deadlines" counted every click.** `deadline_error_ms` was
   measured at burst *completion*, which includes the full button-hold
   duration (default mean 40 ms) — against a 1 ms threshold, every click
   was flagged as missed. Lateness is now measured at dispatch time,
   immediately after the spin-wait returns and before the down event, so
   the statistic reports scheduler error only.

The click path itself is unchanged in character: QPC spin-wait scheduling
(never sleeps), `SendInput`/`PostMessageW` dispatch, SPSC telemetry and
the F8 panic path are untouched except for the deadline arithmetic call
and the two added measurements.

Verification through the offline real-egui harness and standalone rustc:

- `cargo fmt --check` clean over every source and test file.
- **58/58 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
  hotkey 7, icon 6, schedule 7), and all seven test files also compile
  standalone with zero warnings via bare `rustc --test` against the
  pinned 1.88.0 sysroot.
- `cargo clippy -- -D warnings` on the harness lib (which compiles the
  real `app.rs`, `engine.rs`, `telemetry.rs` and `schedule.rs` against
  real egui 0.27.2): zero findings.
- The headless layout probe re-measured all six themes after the chart
  rewrite: maximum content extent is unchanged at **548.50x439.25** inside
  the fixed 560x448 window, and the idle repaint heartbeat still reports
  <= 50 ms for every theme, so the round-6 unfocused-hotkey fix is intact.
- Marker grep stays clean (none of the banned incompleteness markers, no
  `mouse_event`/`keybd_event`, no `std::thread::sleep` in any click path).

Windows acceptance for this round: run a session at a fixed interval with
Random offset 0 and humanization off, open Advanced…, and confirm the
Real-time CPS line is flat at the target rate with "Missed deadlines" at
or near zero. With Random offset or humanization enabled the line wanders
by design because the produced intervals genuinely vary.

## Maximum looks + absolute stability round

User-requested follow-up to the masterclass round: push the visual craft
as far as it goes without breaking the classic-dialog character, and
harden every failure path that could leave the app lying to the user.

Stability hardening (engine + UI):

- Mutex poison recovery: every `lock().expect(...)` on the engine's
  macro-sequence and error mutexes became `lock_recovering()`, which
  takes the inner data from a poisoned lock instead of cascading a
  second panic into the UI thread. The protected values are plain data
  that stays valid even if the writer died mid-update, so recovering is
  strictly safer than crashing. Behavior is identical when healthy; the
  only remaining `expect` is the fail-fast event-creation check at boot.
- Dead-worker watchdog: `ClickEngine::worker_alive()` (JoinHandle
  liveness) plus `force_idle()`, checked every frame while a session is
  marked running. If the dedicated worker thread ever died, the UI now
  forces the engine idle, paints an explicit Error status and recovers
  the Start/Stop controls instead of reporting a running session that no
  thread is driving. The check is gated on the running state, so it
  costs nothing while idle and stays silent in the headless harness.
- Confirmed already-safe paths, re-audited this round: settings writes
  are atomic (temp file + rename, so a crash mid-write can never corrupt
  the file), all UI collections are bounded (log 200, macro 2048,
  telemetry ring fixed), and the engine Drop joins the worker.

Looks (within the classic-dialog discipline):

- Semantic status dot leading the status strip: a 3.5 px filled circle
  in the state color, so shape and position carry the state rather than
  color alone. While a session runs the dot breathes on a ~1.75 s sine
  using the repaint heartbeat that already ticks every 16 ms when
  running — zero additional repaint cost, and the pulse reads as
  "working" from the corner of the eye.
- One-pixel top bevel highlight on enabled push buttons, the same face
  cue the native BS_PUSHBUTTON control carries.
- Accent focus ring (1.5 px) on focused fields and text selection,
  replacing the old text-colored 1 px stroke.
- The dark themes (Dark, Midnight OLED, Terminal green) paint group-box
  captions in their accent color — hierarchy at a glance; Classic keeps
  faithful dark captions. All accent-on-face combinations were already
  measured at 6.4:1–11.7:1 in the previous round's contrast audit.

Verification (Linux harness, rustc 1.88.0, real egui 0.27.2):

- 67/67 module tests standalone and in the harness, unchanged.
- `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.
- Layout probe: all four themes paint within 560x456; idle repaint
  heartbeat 10/10.
- Interaction probe: footer grid intact, the status dot circle is
  painted, the theme picker lists exactly the four shipped themes, the
  factory reset is present, and no retired text is ever painted.

## Masterclass polish round (four themes, stop confirmation, 2026 craft)

User-requested final refinement: fix the status strip staying on
"Stopping…" after a manual stop, retire the MogClicker experiment, ship
exactly four well-tuned themes, and apply a research-driven polish pass
(2026 interface-craft consensus: purposeful micro-interactions in the
150–300 ms window, depth as hierarchy rather than ornament, dark-mode
contrast discipline, confirmations that close the feedback loop).

Defect fixed — stuck "Stopping…" status:

- Pressing Stop (button or hotkey) set the transient "Stopping…" status,
  but the running→stopped completion detector only painted a final
  message for repeat-count sessions and macro playback. A manual stop of
  a "Repeat until stopped" session fell through every branch and the
  transient status stayed on screen forever.
- `update()` now closes the loop: when the worker drains while the status
  is still Stopping, the strip settles on "Stopped — N clicks delivered."
  (and macro playback distinguishes "Playback stopped" from "Playback
  complete"). Panic and error outcomes keep their own status, as before.

Theme set reduced to four (Classic light, Dark, Midnight OLED, Terminal
green):

- The MogClicker meme theme (a text-reskin experiment) and Vaporwave are
  removed: palettes, the `ThemeText` re-skin machinery, the runtime
  window retitle/icon swap, and the hot-pink icon color way are all gone;
  labels are now plain inline strings.
- `AppTheme::from_key` no longer recognizes "mog"/"vaporwave"; the
  tolerant settings parser falls back to the default theme, so an old
  settings file can never resurrect a retired theme (covered by updated
  hotkey and settings tests).

Polish pass (verified headlessly where possible):

- Every classic push button is now a state-aware painter: fill and border
  track idle/hover/press, a ~180 ms eased hover lift with a soft drop
  shadow encodes "what floats acts", and the pressed state sits back
  down. Global `style.animation_time` is 180 ms, inside the 150–300 ms
  window where motion reads as responsive.
- Floating surfaces (Settings, Hotkey dialog, dropdown popups) carry a
  soft offset shadow so layering reads as hierarchy.
- Contrast audit across all four palettes: every text, dim-text, accent
  and status color was computed against its background; the one failure
  (Classic warning amber, 4.12:1) was darkened to 4.75:1. All other
  combinations measure 4.7:1–17:1.

Verification (Linux harness, rustc 1.88.0, real egui 0.27.2):

- 67 module tests pass standalone and in the harness (config 13,
  humanize 9, telemetry 8, settings 8, hotkey 7, icon 6, schedule 7,
  macro 9); the retired-theme coverage was replaced with equally strict
  fallback and icon-determinism assertions, never weakened.
- `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.
- Layout probe: all four themes paint within 560x456, idle repaint
  heartbeat 10/10.
- Interaction probe: footer grid Start (F6) / Stop (F6) / Hotkey setting /
  Record & Playback in two aligned rows; the strings "Advanced", "Mog",
  "Vaporwave" are never painted; Settings opens with the factory reset
  present and the theme dropdown lists exactly the four shipped themes.

## Diagnostics removal + stability round (Advanced window retired)

User-requested follow-up to the MacroClicker round: remove the Advanced
button and the entire Advanced window ("nobody will be needing diagnostics
on their autoclicker") and maximize stability. Engine modules are
untouched; only UI surfaces changed.

What was removed from `src/app.rs` (UI only):

- The **Advanced…** button and the whole Advanced window: the delivery
  section (SendInput vs background window-message targeting UI), the
  expert Gaussian humanization section, the telemetry dashboard (metrics
  grid, real-time CPS sparkline, interval histogram) and the session log
  view.
- The per-frame telemetry drain/aggregate pass in `update()` — the UI no
  longer does any per-click bookkeeping work per frame, which removes the
  main steady-state allocation/scan cost while a session runs.
- The window-enumeration state (`windows`, `selected_window`) and all
  chart ring buffers (`cps_history`, sliding-window sample queue, stats).

What was kept, deliberately:

- `src/telemetry.rs`, the background delivery path in `src/engine.rs` /
  `src/win32.rs` and `src/humanize.rs` stay intact with their tests (the
  core-equivalence constraint); they are simply not exposed in the UI.
- **Reset all settings to factory defaults** moved from the Advanced
  window to the bottom of the Settings window.
- The session log Vec stays capped at 200 entries (it is no longer
  rendered; the cap keeps memory bounded).

Stability hardening:

- `sanitize_loaded_config()` runs at startup after settings load: a
  persisted config from an older build that selected background delivery,
  synchronous delivery or expert humanization is reset to the supported
  surface (global SendInput, no target HWND, humanization off) before the
  saved-fingerprint snapshot, so a stale file can never start the app in
  a mode the UI cannot display or undo.
- `handle_start` pins the same delivery invariants on every start.
- The main window returned to a tighter 560x456 (the third footer button
  row is gone; the status strip now hosts the small Settings… button).

Verification (Linux harness, rustc 1.88.0, real egui 0.27.2):

- All 67 module tests pass standalone and in the harness, unchanged.
- `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.
- Layout probe: all six themes paint within 560x456 with no overflow and
  the 10/10 idle repaint heartbeat intact.
- Interaction probe: the footer grid is exactly Start (F6) / Stop (F6) /
  Hotkey setting / Record & Playback in two aligned rows, the string
  "Advanced" is never painted, and clicking Settings… opens the Settings
  window containing the relocated factory-reset button.

## MacroClicker rebrand + Record & Playback round (fully beating 4.1)

User request: keep the look "super super" matched to OP Auto Clicker 4.1,
rename the main title to **MacroClicker**, and make the app actually fully
beat OP Auto Clicker 4.1. The one 4.1 capability still missing was the
bottom-row **Record & Playback** button, so this round ships it for real —
not as a dead control — plus the rebrand and the authentic 4.1 button rows.

What changed:

- **Rebrand.** The default window/taskbar title, the eframe app id, the
  first log line, the settings-file header comment and the binary name
  (`macro_clicker.exe`, CI artifact `macro_clicker-windows-x64`) now say
  MacroClicker. The MogClicker theme keeps its own "MogClicker" retitle.
  The settings folder stays `%APPDATA%\op-auto-clicker` so every existing
  user setting survives the rename.
- **Authentic 4.1 button rows.** The footer grid now mirrors 4.1 exactly:
  Start (F6) / Stop (F6), then Hotkey setting / Record & Playback, with
  this app's extra Settings… / Advanced… as a third row. The fixed window
  grew from 560x448 to 560x480 to seat the third row; the headless layout
  probe re-measured all six themes at 548.50x471.75 inside 560x480.
- **Record.** A new `MacroRecorder` (pure module `macro_rec.rs`) captures
  real clicks: while recording is armed the existing GetAsyncKeyState
  heartbeat (the same 16 ms poll that powers the global hotkeys, so it
  works while other apps are focused) edge-detects the three physical
  mouse buttons, reads GetCursorPos, and stores position + button + the
  measured gap since the previous click. Clicks are ignored while the
  foreground window belongs to this process (new
  `win32::foreground_is_own_process()` via GetForegroundWindow +
  GetWindowThreadProcessId + GetCurrentProcessId), so pressing "Stop
  recording" never records itself. The toggle hotkey and F8 also stop a
  recording; a 2048-step cap stops runaway recordings.
- **Playback.** `ClickEngine::start_with_macro` hands the recorded steps
  to the worker (one mutex clone at session entry — the click loop itself
  stays lock-free). Each iteration overrides position, button and gap from
  the current step through the existing dispatch and the bounded-catch-up
  absolute QPC schedule, so playback inherits drift-free timing, the F8
  panic path, telemetry and the CPS chart unchanged. Loop budgets reuse
  `max_clicks` (`steps x loops`, or unlimited for "until stopped"), and
  playback refuses background-HWND mode (global SendInput only). The UI
  never pushes its live config into the engine during playback, so the
  loop budget cannot be overwritten mid-run.

Verification through the offline real-egui harness and standalone rustc:

- `cargo fmt --check` clean over every source and test file.
- **67/67 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
  hotkey 7, icon 6, schedule 7, macro 9 — a new eighth binary), all eight
  test files also compile standalone with zero warnings via bare
  `rustc --test` against the pinned 1.88.0 sysroot.
- `cargo clippy -- -D warnings` on the harness lib (real `app.rs`,
  `engine.rs`, `macro_rec.rs` against real egui 0.27.2): zero findings.
- New `verify_macro` probe drives the real UI headlessly: the root
  viewport retitles to "MacroClicker" on frame one; the footer paints
  Start (F6), Stop (F6), Hotkey setting, Record & Playback, Settings… and
  Advanced…; clicking Record & Playback opens the window showing
  "Recorded steps: 0", Record, Clear, the loop radios and Play; the loop
  controls survive real pointer events.
- The layout probe re-measured all six themes at the new window size:
  maximum content extent 548.50x471.75 inside 560x480, idle repaint
  heartbeat <= 50 ms everywhere.
- Marker grep stays clean; no `mouse_event`/`keybd_event`; no
  `std::thread::sleep` anywhere in a click or playback path.

Windows acceptance for this round: confirm the title bar reads
MacroClicker; record a short macro across another window, replay it 2x and
until-stopped, stop with the button, the hotkey and F8; confirm clicks on
MacroClicker's own windows are never recorded; and confirm an ordinary
(non-macro) session still behaves exactly as before.

## MogClicker accent round (presentation-only)

User-requested follow-up: make the MogClicker meme theme the "absolute
best" with real mog accents. Wording was grounded in the actual slang
(Merriam-Webster: "mog — to look or perform far better than someone
else"; meme-culture staples "aura", "looksmaxxing" and "brutally
frame-mogged"). No engine, scheduling, dispatch, telemetry or validation
code path changed; no new preference fields.

- **Hot-pink group captions.** The `Palette` gained a `group_title` slot;
  the five non-meme themes bind it to their normal text color
  (byte-identical rendering), while MogClicker paints the four group-box
  captions in its hot-pink accent (255, 46, 136).
- **Themed window/taskbar icon.** `icon.rs` was refactored so the
  procedural crosshair generator takes its accent/background colors as
  parameters; `app_icon_rgba()` is pixel-identical to before, and a new
  `mog_icon_rgba()` renders the same geometry in hot pink on deep purple.
  While the Mog theme is active the app swaps the live window icon via
  `ViewportCommand::Icon` (handled by egui-winit 0.27), sent once per
  theme-family change. Two new icon tests pin the variant: identical
  alpha-channel geometry, exact (255, 46, 136) center dot, and at least
  200 visibly recolored pixels.
- **More meme wording** (all status-strip/log strings, zero layout
  impact): running status appends "+1000 aura per click.", stopping shows
  "Aura cooling down…", natural completion reports "Mogging complete —
  everyone brutally frame-mogged: N clicks delivered.", the activity log
  opens with "Mogging session started — looksmaxxing at … CPS.", and the
  Settings-window hint calls the theme a certified aura mogger. The
  strings moved into the `ThemeText` table (classic wording is unchanged
  verbatim for every other theme).
- **Headless proof (real egui 0.27.2).** All four probes re-ran green:
  layout still exactly 548.50 x 439.25 in all six themes (the new accents
  change zero metrics); style fills + text clipping clean across all six;
  injected-click secondary surfaces fine in classic/dark/mog; and the
  scale/title probe now additionally asserts that the Mog theme emits
  exactly one 32x32 `Icon` command whose center pixel is (255, 46, 136),
  that title/icon commands do not repeat, and that the default
  configuration sends neither resize nor icon commands.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo clippy -- -D warnings` clean,
**51/51 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
hotkey 7, icon 6), and all six test files also compile standalone with
zero warnings. The real taskbar-icon swap and on-screen appearance remain
Windows-only manual acceptance items and are not claimed from the Linux
harness.

## Theme gallery + Settings window + UI scaling round (presentation-only)

User-requested follow-up: a Settings entry point inside the app, a gallery
of themes including the "MogClicker" meme theme, and a user-selectable
window scale. No engine, scheduling, dispatch, telemetry or validation
code path changed; the behavioral additions are two persisted preference
fields and two viewport commands (window title and window size).

- **Theme system.** `AppPrefs::dark_mode` was generalized into an
  `AppTheme` enum with six variants — Classic light (default), Dark,
  Midnight OLED, Terminal green, Vaporwave, and MogClicker — each a full
  22-field `Palette` static. Legacy settings files still load:
  `dark_mode=true` maps to Dark, `dark_mode=false`/absent to Classic, and
  a modern `theme=` key always outranks the legacy boolean regardless of
  line order (tested both ways).
- **Themed wording.** A `ThemeText` table carries the handful of strings a
  theme may re-skin (window title, four group titles, Start/Stop verbs,
  footer). Only MogClicker deviates: title "MogClicker", Start → "Mog",
  Stop → "It's over", "Mogging interval"/"Mog options"/"Mog repeat"/
  "Aura position", footer "F6 to mog · F8 when it's over", and a running
  status of "Mogging at … CPS". Inner control labels keep their normal
  wording so the app stays usable. The OS title is switched at runtime via
  `ViewportCommand::Title`, sent only when it changes.
- **Settings window.** The footer's Dark-mode checkbox became a
  **Settings…** button opening a small egui window with the six-entry
  theme picker and a 75–200 % window-scale picker; both persist under
  `[ui]` (`theme=`, `ui_scale=`). Out-of-range scales clamp to 75–200 and
  malformed values keep 100 (tested).
- **Scaling.** Each frame the desired zoom (`ui_scale/100`) is compared to
  `Context::zoom_factor()`; on mismatch `set_zoom_factor()` is called, and
  one frame later — once egui reports the factor as applied — a single
  `ViewportCommand::InnerSize(560x448 logical)` re-asserts the fixed size
  so the physical window really scales and nothing is cut off.
- **Headless proof (real egui 0.27.2).** Four probes: (1) layout — content
  bounds measured at 548.50 x 439.25 in **all six themes** at width 560,
  so 560x448 still fits with ~9 px margin and no Mog label widens any
  row; (2) style — at 560x448, dark-family themes (Dark, OLED, Terminal,
  Mog) paint only dark-family opaque fills, light-family themes (Classic,
  Vaporwave) only light fills (each theme's exact accent color
  whitelisted), with zero clipped text; (3) secondary surfaces — real
  injected pointer clicks open the Hotkey dialog, Settings window and
  Advanced window in Classic, Dark and Mog, their marker texts appear, and
  the fill invariants hold with all surfaces open; (4) scale/title —
  `ui_scale=150` yields zoom 1.5 followed by exactly one
  `InnerSize(560,448)` command and then silence, the Mog theme emits
  `Title("MogClicker")` on the first frame, and a default configuration
  sends no resize commands and keeps zoom 1.0.

Verification for this round on rustc 1.88.0 against the offline real-egui
harness: `rustfmt --check` clean, `cargo clippy -- -D warnings` clean,
**49/49 tests pass** (config 13, humanize 9, telemetry 8, settings 8,
hotkey 7, icon 4), and all six test files also compile standalone with
zero warnings. On-screen scaling behavior (actual pixel growth of the OS
window) and visual appearance remain Windows-only manual acceptance items
and are not claimed from the Linux harness.

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

Then follow the fourteen-step manual checklist in `autoclicker/README.md`
(step 7 now explicitly includes pressing the hotkey while another
application has focus):
dashboard rendering, interval and random-offset behavior, right/middle and
double/triple bursts, picked fixed coordinates, repeat-count auto-stop,
hotkey remap and F8 reservation, settings persistence across restarts,
unfocused and minimized background targets, F8 panic stop (button must never
stick), the six-theme switch (including the MogClicker retitle, hot-pink
icon swap and meme status lines), the window-scale resize, and clean
shutdown with no leaked handles.

The repository's GitHub Actions workflow performs an additional, genuinely
Windows-native gate on every push and pull request: `cargo fmt --check`,
`cargo clippy`, `cargo test`, a release build on `windows-latest`
(x86_64-pc-windows-msvc), and uploads `op_auto_clicker.exe` as a build
artifact. Visual appearance and real click delivery remain manual
acceptance items because they require an interactive desktop.
