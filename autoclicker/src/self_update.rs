//! Verified self-replacement: turning a downloaded release EXE
//! into the running program, safely.
//!
//! A recognized source checkout first fast-forwards its clean `main`
//! branch to the exact release tag; from there both installation kinds
//! use the same verified flow: download → verify → swap → relaunch:
//!
//! 1. The new EXE is streamed (by `win32::https_download_to_file`)
//!    to a uniquely reserved `macro_clicker.exe.update-<attempt>` **next to the running
//!    EXE**, so the final step is a same-volume rename, never a
//!    copy that could half-complete.
//! 2. [`verify_downloaded_exe`] re-reads it and demands all three:
//!    a real AMD64 PE header, the exact byte size GitHub
//!    published, and a SHA-256 identical to GitHub's published
//!    fingerprint. Any mismatch deletes the download and leaves
//!    the installed program untouched.
//! 3. [`swap_in_new_exe`] renames the running EXE to
//!    `.old-<attempt>` — Windows permits renaming a loaded program
//!    image even though overwriting it would fail — then renames
//!    the verified download into its place. If the second rename
//!    fails, the original is renamed straight back.
//! 4. The app launches the new file and exits; on its next start
//!    [`cleanup_update_attempt`] removes only this attempt's
//!    remains (retrying briefly, because the old process may
//!    still be exiting while the new one starts). Other attempts'
//!    files and failed-rollback backups are never swept automatically.
//!
//! Everything here is `std`-only file and process work, so the
//! path classification, swap, rollback, and verification logic are
//! exercised by ordinary tests on any platform. Actually invoking Git
//! and the rename-a-running-image permission are the platform/runtime
//! facts reserved for Windows acceptance.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Windows process-creation flag that keeps console helpers such as
/// Git from flashing a console window when launched by this GUI app.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use crate::update::{self, Sha256};

/// The three paths one update attempt juggles. `download` and
/// `backup` are siblings of `exe` with a per-attempt suffix, so
/// instances never reuse one temp name and a failed attempt's
/// recovery files remain identifiable.
#[derive(Clone, Debug)]
pub struct UpdatePaths {
    pub id: String,
    pub exe: PathBuf,
    pub download: PathBuf,
    pub backup: PathBuf,
}

fn sibling_with_suffix(exe: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = exe
        .file_name()
        .map(|file_name| file_name.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    exe.with_file_name(name)
}

/// Builds the path set for one update attempt. `unique` is the
/// caller's per-attempt discriminator (the app reserves a fresh ID).
pub fn update_paths(exe: &Path, unique: &str) -> UpdatePaths {
    UpdatePaths {
        id: unique.to_string(),
        exe: exe.to_path_buf(),
        download: sibling_with_suffix(exe, &format!(".update-{unique}")),
        backup: sibling_with_suffix(exe, &format!(".old-{unique}")),
    }
}

/// Reserve a new download path without overwriting leftovers from another
/// attempt. A fresh suffix also keeps a failed rollback's backup distinct
/// from a later retry, including after the OS reuses a process id.
pub fn reserve_update_paths(exe: &Path) -> Result<UpdatePaths, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    for _ in 0..32 {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let id = format!("{}-{stamp}-{sequence}", std::process::id());
        let paths = update_paths(exe, &id);
        if paths.backup.exists() {
            continue;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&paths.download)
        {
            Ok(_) => return Ok(paths),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "the update file could not be reserved next to the program: {error}"
                ))
            }
        }
    }
    Err(
        "could not reserve a unique update file; existing recovery files were preserved"
            .to_string(),
    )
}

/// Only IDs issued by this updater may identify files for post-restart cleanup.
pub fn valid_update_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|byte| byte.is_ascii_digit() || byte == b'-')
}

/// Clean only this successfully relaunched attempt, never another running
/// instance's download or a previous failed rollback's recovery backup.
pub fn cleanup_update_attempt(paths: &UpdatePaths) -> usize {
    if !paths.exe.is_file() || !valid_update_id(&paths.id) {
        return 0;
    }
    [&paths.backup, &paths.download]
        .into_iter()
        .filter(|path| fs::remove_file(path).is_ok())
        .count()
}

/// A recognized MacroClicker source checkout behind a Cargo-built
/// executable. `root` is the repository root, `crate_dir` is the
/// `autoclicker/` crate directory and `profile` is the Cargo profile
/// directory (`debug` or `release`) the running EXE came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCheckout {
    pub root: PathBuf,
    pub crate_dir: PathBuf,
    pub manifest: PathBuf,
    pub profile: String,
}

/// How this process is installed, for update routing and for telling
/// the user exactly what an in-app update will change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallKind {
    /// A normal portable/release EXE outside a Cargo target tree.
    Portable,
    /// A Cargo-built EXE inside a recognizable MacroClicker checkout.
    SourceCheckout(SourceCheckout),
    /// A Cargo-built EXE whose surrounding checkout is not recognized.
    CargoBuild,
    /// The process could not identify its own EXE path.
    Unknown,
}

/// Derives the source-checkout paths implied by a Cargo-built EXE
/// path, without touching the filesystem. Only the project layout
/// (`<root>/autoclicker/target/<profile>/macro_clicker.exe`, or the
/// same with the crate at the repository root) is recognized; test
/// binaries under `deps/` and arbitrary Cargo artifacts are not
/// treated as the installed application.
fn source_checkout_paths(exe: &Path) -> Option<SourceCheckout> {
    let profile = exe.parent()?.file_name()?.to_string_lossy().into_owned();
    if profile != "debug" && profile != "release" {
        return None;
    }
    let target_dir = exe.parent()?.parent()?;
    if target_dir.file_name()?.to_string_lossy() != "target" {
        return None;
    }
    let crate_dir = target_dir.parent()?.to_path_buf();
    let root = if crate_dir.file_name().map(|name| name.to_string_lossy())
        == Some(std::borrow::Cow::Borrowed("autoclicker"))
    {
        crate_dir.parent()?.to_path_buf()
    } else {
        crate_dir.clone()
    };
    Some(SourceCheckout {
        manifest: crate_dir.join("Cargo.toml"),
        root,
        crate_dir,
        profile,
    })
}

/// Returns the checkout behind `exe` when the path-derived layout is
/// backed by the files that make it this project: a Git checkout, the
/// crate manifest and the crate entry point. This deliberately refuses
/// lookalike target trees rather than running Git somewhere surprising.
pub fn source_checkout_for_exe(exe: &Path) -> Option<SourceCheckout> {
    let checkout = source_checkout_paths(exe)?;
    let git_marker = checkout.root.join(".git");
    let entry_point = checkout.crate_dir.join("src").join("main.rs");
    if git_marker.exists() && checkout.manifest.is_file() && entry_point.is_file() {
        Some(checkout)
    } else {
        None
    }
}

/// Classifies the running executable for update routing.
pub fn install_kind_for_exe(exe: &Path) -> InstallKind {
    if let Some(checkout) = source_checkout_for_exe(exe) {
        InstallKind::SourceCheckout(checkout)
    } else if is_cargo_target_exe(exe) {
        InstallKind::CargoBuild
    } else {
        InstallKind::Portable
    }
}

/// Whether `exe` lives in a Cargo build tree (`target/debug` or
/// `target/release`, possibly a subdirectory like `deps`). A bare
/// development artifact is not silently treated as a portable install:
/// source checkouts get their own explicit update path above, and
/// unrecognized Cargo artifacts keep the browser download.
pub fn is_cargo_target_exe(exe: &Path) -> bool {
    let components: Vec<String> = exe
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    components
        .windows(2)
        .any(|pair| pair[0] == "target" && (pair[1] == "debug" || pair[1] == "release"))
}

/// Error text returned when the user cancels a source-checkout update.
/// Callers compare against it to land on the ordinary "cancelled"
/// terminal state instead of presenting cancellation as a failure.
pub const SOURCE_UPDATE_CANCELLED: &str = "source update cancelled";

/// What could be established after a possibly-mutating Git command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceChange {
    Unchanged,
    AdvancedToTarget,
    OtherHead,
    Uncertain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUpdateFailure {
    pub reason: String,
    pub change: SourceChange,
}

impl SourceUpdateFailure {
    fn before_merge(reason: String) -> Self {
        Self {
            reason,
            change: SourceChange::Unchanged,
        }
    }

    pub fn cancelled(&self) -> bool {
        self.reason == SOURCE_UPDATE_CANCELLED
    }

    pub fn description(&self, version: &str) -> String {
        let reason = if self.cancelled() {
            "The source update was cancelled".to_string()
        } else {
            self.reason.clone()
        };
        let context = match self.change {
            SourceChange::Unchanged => " This update did not change the checkout's HEAD or tracked files.",
            SourceChange::AdvancedToTarget => {
                return format!("{reason}. The source checkout's HEAD advanced to v{version}; inspect git status before retrying. The running program was not replaced.");
            }
            SourceChange::OtherHead => " The source checkout's HEAD changed during the update; inspect git status and HEAD before retrying.",
            SourceChange::Uncertain => " The source checkout may have changed; inspect git status and HEAD before retrying.",
        };
        format!("{reason}.{context} The running program was not replaced.")
    }
}

/// Successful source preparation records whether this attempt actually
/// moved HEAD, rather than claiming a checkout already on the tag changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceUpdateSuccess {
    pub advanced: bool,
}

/// Whether an `origin` URL names this project's GitHub repository in
/// one of the standard HTTPS or SSH spellings. The source updater runs
/// Git only against the expected upstream, never against whatever a
/// lookalike checkout happens to configure.
pub fn is_supported_origin_url(url: &str) -> bool {
    // GitHub owner/repository paths are case-insensitive, and clone URLs
    // preserve whatever spelling the user copied. Normalize case (and a
    // trailing slash/`.git`) before requiring the exact project path.
    let normalized = url.trim().trim_end_matches('/').to_ascii_lowercase();
    let normalized = normalized.strip_suffix(".git").unwrap_or(&normalized);
    matches!(
        normalized,
        "https://github.com/skid-ster/autoclickerbyarena"
            | "git@github.com:skid-ster/autoclickerbyarena"
            | "ssh://git@github.com/skid-ster/autoclickerbyarena"
    )
}

/// Extracts the `[package]` version from Cargo manifest text. This is
/// a deliberately small line parser for the one field the source
/// updater must confirm after a fast-forward; malformed or non-version
/// values yield None rather than being coerced.
pub fn package_version_from_manifest(text: &str) -> Option<String> {
    let mut in_package = false;
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        let clean = value.strip_prefix('v').unwrap_or(value);
        if update::parse_version(clean).is_some() {
            return Some(clean.to_string());
        }
        return None;
    }
    None
}

fn bounded_file_text(path: &Path, max_bytes: u64) -> String {
    let Ok(mut file) = fs::File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    if file
        .by_ref()
        .take(max_bytes)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn collapsed_command_detail(stdout_path: &Path, stderr_path: &Path) -> String {
    let mut combined = bounded_file_text(stderr_path, 16 * 1024);
    if combined.trim().is_empty() {
        combined = bounded_file_text(stdout_path, 16 * 1024);
    }
    let mut detail = String::new();
    for word in combined.split_whitespace() {
        if !detail.is_empty() {
            detail.push(' ');
        }
        detail.push_str(word);
        if detail.len() >= 360 {
            break;
        }
    }
    while detail.len() > 360 {
        if detail.pop().is_none() {
            break;
        }
    }
    detail
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs one fixed command without a shell, with output redirected to
/// bounded log files so the caller can poll cancellation and enforce a
/// deadline without risking a pipe deadlock. On Windows the helper is
/// created without a console window, so Git never flashes terminal
/// windows over the GUI. Only the source updater's hard-coded Git
/// invocations use this; no user text becomes an argument.
fn run_update_command(
    program: &str,
    args: &[OsString],
    cwd: &Path,
    cancel: &AtomicBool,
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err(SOURCE_UPDATE_CANCELLED.to_string());
    }
    let stamp = format!("macroclicker-source-update-{}-{label}", std::process::id());
    let stdout_path = std::env::temp_dir().join(format!("{stamp}.stdout.log"));
    let stderr_path = std::env::temp_dir().join(format!("{stamp}.stderr.log"));
    let stdout_file = fs::File::create(&stdout_path)
        .map_err(|error| format!("the source update log couldn't be created: {error}"))?;
    let stderr_file = fs::File::create(&stderr_path)
        .map_err(|error| format!("the source update log couldn't be created: {error}"))?;

    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("CARGO_TERM_COLOR", "never");
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|error| format!("{label} couldn't be started: {error}"))?;

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if cancel.load(Ordering::Relaxed) {
                    terminate_child(&mut child);
                    let _ = fs::remove_file(&stdout_path);
                    let _ = fs::remove_file(&stderr_path);
                    return Err(SOURCE_UPDATE_CANCELLED.to_string());
                }
                if started.elapsed() > timeout {
                    terminate_child(&mut child);
                    let _ = fs::remove_file(&stdout_path);
                    let _ = fs::remove_file(&stderr_path);
                    return Err(format!("{label} timed out"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                terminate_child(&mut child);
                let _ = fs::remove_file(&stdout_path);
                let _ = fs::remove_file(&stderr_path);
                return Err(format!("{label} couldn't be monitored: {error}"));
            }
        }
    };

    let stdout = bounded_file_text(&stdout_path, 64 * 1024);
    let result = if status.success() {
        Ok(stdout)
    } else {
        let detail = collapsed_command_detail(&stdout_path, &stderr_path);
        if detail.is_empty() {
            Err(format!("{label} failed ({status})"))
        } else {
            Err(format!("{label} failed ({status}): {detail}"))
        }
    };
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);
    result
}

fn run_git(
    checkout: &SourceCheckout,
    args: &[&str],
    cancel: &AtomicBool,
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    let mut full_args: Vec<OsString> = vec![OsString::from("-C"), checkout.root.clone().into()];
    full_args.extend(args.iter().map(OsString::from));
    run_update_command("git", &full_args, &checkout.root, cancel, timeout, label)
}

/// Fast-forwards a recognized checkout to an exact release tag. Existing
/// clean-tree, branch, origin, tag and ancestry guards remain in force.
/// The destination manifest is inspected *before* merge. If a mutating
/// command fails or is cancelled, an independent bounded probe reports
/// whether HEAD advanced, another HEAD appeared, or the tree is uncertain.
pub fn fast_forward_source_checkout(
    checkout: &SourceCheckout,
    version: &str,
    cancel: &AtomicBool,
) -> Result<SourceUpdateSuccess, SourceUpdateFailure> {
    fast_forward_source_checkout_with(checkout, version, cancel, |args, flag, timeout, label| {
        run_git(checkout, args, flag, timeout, label)
    })
}

fn inspect_after_merge_failure<F>(
    git: &mut F,
    before: &str,
    target: &str,
    reason: String,
) -> SourceUpdateFailure
where
    F: FnMut(&[&str], &AtomicBool, Duration, &str) -> Result<String, String>,
{
    // Never reuse the user's already-set cancellation flag for recovery.
    let recovery_cancel = AtomicBool::new(false);
    let timeout = Duration::from_secs(5);
    let head = git(
        &["rev-parse", "--verify", "HEAD"],
        &recovery_cancel,
        timeout,
        "git recovery HEAD",
    );
    let change = match head {
        Ok(head) if head.trim() == target && head.trim() != before => {
            SourceChange::AdvancedToTarget
        }
        Ok(head) if head.trim() != before && !head.trim().is_empty() => SourceChange::OtherHead,
        Ok(head) if head.trim() == before => {
            // A killed merge can modify the worktree before it changes HEAD.
            match git(
                &["status", "--porcelain=v1", "--untracked-files=no"],
                &recovery_cancel,
                timeout,
                "git recovery status",
            ) {
                Ok(status) if status.trim().is_empty() => SourceChange::Unchanged,
                _ => SourceChange::Uncertain,
            }
        }
        _ => SourceChange::Uncertain,
    };
    SourceUpdateFailure { reason, change }
}

fn fast_forward_source_checkout_with<F>(
    checkout: &SourceCheckout,
    version: &str,
    cancel: &AtomicBool,
    mut git: F,
) -> Result<SourceUpdateSuccess, SourceUpdateFailure>
where
    F: FnMut(&[&str], &AtomicBool, Duration, &str) -> Result<String, String>,
{
    let clean_version = version.trim().strip_prefix('v').unwrap_or(version.trim());
    if update::parse_version(clean_version).is_none() {
        return Err(SourceUpdateFailure::before_merge(format!(
            "release version {version} is not a version number"
        )));
    }
    let short = Duration::from_secs(20);
    let status = git(
        &["status", "--porcelain=v1", "--untracked-files=no"],
        cancel,
        short,
        "git status",
    )
    .map_err(SourceUpdateFailure::before_merge)?;
    if !status.trim().is_empty() {
        return Err(SourceUpdateFailure::before_merge(
            "the source checkout has uncommitted tracked changes; commit or stash them before updating in the app".to_string(),
        ));
    }
    let branch = git(&["branch", "--show-current"], cancel, short, "git branch")
        .map_err(SourceUpdateFailure::before_merge)?;
    if branch.trim() != "main" {
        let shown = if branch.trim().is_empty() {
            "a detached HEAD".to_string()
        } else {
            format!("branch {}", branch.trim())
        };
        return Err(SourceUpdateFailure::before_merge(format!(
            "the source checkout is on {shown}, not main; switch to main before updating in the app"
        )));
    }
    let origin = git(
        &["remote", "get-url", "origin"],
        cancel,
        short,
        "git remote",
    )
    .map_err(SourceUpdateFailure::before_merge)?;
    if !is_supported_origin_url(origin.trim()) {
        return Err(SourceUpdateFailure::before_merge(
            "the source checkout's origin is not the MacroClicker GitHub repository".to_string(),
        ));
    }
    git(
        &["fetch", "--prune", "--tags", "origin", "main"],
        cancel,
        Duration::from_secs(120),
        "git fetch",
    )
    .map_err(SourceUpdateFailure::before_merge)?;
    let tag_ref = format!("refs/tags/v{clean_version}^{{commit}}");
    let target = git(
        &["rev-parse", "--verify", &tag_ref],
        cancel,
        short,
        "git rev-parse",
    )
    .map_err(SourceUpdateFailure::before_merge)?;
    let target = target.trim();
    if target.len() < 7 || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SourceUpdateFailure::before_merge(format!(
            "Git did not resolve release tag v{clean_version} to a commit"
        )));
    }
    let head = git(&["rev-parse", "HEAD"], cancel, short, "git rev-parse")
        .map_err(SourceUpdateFailure::before_merge)?;
    let head = head.trim();
    if head != target {
        let merge_base = git(
            &["merge-base", "HEAD", target],
            cancel,
            short,
            "git merge-base",
        )
        .map_err(SourceUpdateFailure::before_merge)?;
        if merge_base.trim() != head {
            return Err(SourceUpdateFailure::before_merge(format!(
                "the source checkout cannot fast-forward cleanly to v{clean_version}; no source files were changed"
            )));
        }
    }

    let manifest_relative = checkout
        .manifest
        .strip_prefix(&checkout.root)
        .map_err(|_| {
            SourceUpdateFailure::before_merge(
                "the source manifest is outside the checkout".to_string(),
            )
        })?;
    if manifest_relative
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(SourceUpdateFailure::before_merge(
            "the source manifest path is invalid".to_string(),
        ));
    }
    let manifest_at_target = format!(
        "{target}:{}",
        manifest_relative.to_string_lossy().replace('\\', "/")
    );
    let target_manifest = git(
        &["show", &manifest_at_target],
        cancel,
        short,
        "git show manifest",
    )
    .map_err(SourceUpdateFailure::before_merge)?;
    if package_version_from_manifest(&target_manifest).as_deref() != Some(clean_version) {
        return Err(SourceUpdateFailure::before_merge(format!(
            "the destination tag v{clean_version} does not name that version in its Cargo manifest"
        )));
    }

    let advanced = head != target;
    if advanced {
        if let Err(reason) = git(
            &["merge", "--ff-only", target],
            cancel,
            Duration::from_secs(60),
            "git merge",
        ) {
            return Err(inspect_after_merge_failure(&mut git, head, target, reason));
        }
        let new_head = match git(&["rev-parse", "HEAD"], cancel, short, "git rev-parse") {
            Ok(head) => head,
            Err(reason) => return Err(inspect_after_merge_failure(&mut git, head, target, reason)),
        };
        if new_head.trim() != target {
            return Err(inspect_after_merge_failure(
                &mut git,
                head,
                target,
                format!("the source checkout did not land exactly on v{clean_version}"),
            ));
        }
    }
    let manifest_text = bounded_file_text(&checkout.manifest, 256 * 1024);
    if package_version_from_manifest(&manifest_text).as_deref() != Some(clean_version) {
        let reason = format!("the checked-out Cargo manifest does not name v{clean_version}");
        return Err(if advanced {
            inspect_after_merge_failure(&mut git, head, target, reason)
        } else {
            SourceUpdateFailure::before_merge(reason)
        });
    }
    Ok(SourceUpdateSuccess { advanced })
}

/// True when `header` (the first bytes of a file) opens as an
/// AMD64 Windows program: `MZ` magic, a sane PE header offset,
/// the `PE\0\0` signature and the x86-64 machine type. This is a
/// sanity gate behind the SHA-256 check, so a fingerprint match
/// on some future wrong-architecture asset still cannot install
/// a program this machine cannot run.
pub fn is_amd64_pe(header: &[u8]) -> bool {
    if header.len() < 0x40 || &header[0..2] != b"MZ" {
        return false;
    }
    let pe_offset =
        u32::from_le_bytes([header[0x3c], header[0x3d], header[0x3e], header[0x3f]]) as usize;
    if pe_offset < 0x40 || pe_offset + 6 > header.len() {
        return false;
    }
    &header[pe_offset..pe_offset + 4] == b"PE\0\0"
        && u16::from_le_bytes([header[pe_offset + 4], header[pe_offset + 5]]) == 0x8664
}

/// Streams a file through SHA-256 in fixed blocks, returning the
/// digest and the exact byte count read.
pub fn hash_file(path: &Path) -> io::Result<([u8; 32], u64)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut block = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut block)?;
        if read == 0 {
            break;
        }
        hasher.update(&block[..read]);
        total += read as u64;
    }
    Ok((hasher.finalize(), total))
}

/// The gate a finished download must pass before it may replace
/// anything: readable back, a real AMD64 program, exactly the
/// size GitHub published, and a SHA-256 identical to the
/// fingerprint GitHub published. Error texts are written for the
/// Updates window.
pub fn verify_downloaded_exe(
    path: &Path,
    expected_size: u64,
    expected_sha256: &[u8; 32],
) -> Result<(), String> {
    let mut header = [0_u8; 4096];
    let header_len = {
        let mut file = fs::File::open(path)
            .map_err(|error| format!("the downloaded update couldn't be read back: {error}"))?;
        let mut filled = 0_usize;
        while filled < header.len() {
            match file.read(&mut header[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) => {
                    return Err(format!(
                        "the downloaded update couldn't be read back: {error}"
                    ));
                }
            }
        }
        filled
    };
    if !is_amd64_pe(&header[..header_len]) {
        return Err("the downloaded file is not a Windows program this PC can run".to_string());
    }
    let (digest, size) = hash_file(path)
        .map_err(|error| format!("the downloaded update couldn't be read back: {error}"))?;
    if size != expected_size {
        return Err(format!(
            "the download is incomplete ({size} of {expected_size} bytes arrived)"
        ));
    }
    if digest != *expected_sha256 {
        return Err(
            "the download's SHA-256 fingerprint does not match the fingerprint GitHub \
             published for this release"
                .to_string(),
        );
    }
    Ok(())
}

/// Parks the current EXE, then places the verified download. If either
/// rename or restoration fails, the error says exactly where the known-good
/// backup remains; it is never discarded to make an error look successful.
fn rename_update_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

pub fn swap_in_new_exe(paths: &UpdatePaths) -> Result<(), String> {
    swap_in_new_exe_with(paths, rename_update_file)
}

fn swap_in_new_exe_with<R>(paths: &UpdatePaths, mut rename: R) -> Result<(), String>
where
    R: FnMut(&Path, &Path) -> io::Result<()>,
{
    if !paths.exe.is_file() {
        return Err(format!(
            "the current EXE is missing; any existing backup was kept at {}",
            paths.backup.display()
        ));
    }
    if paths.backup.exists() {
        return Err(format!(
            "the backup path {} is already occupied; it was preserved and nothing was replaced",
            paths.backup.display()
        ));
    }
    rename(&paths.exe, &paths.backup)
        .map_err(|error| format!("the current EXE couldn't be moved aside: {error}"))?;
    match rename(&paths.download, &paths.exe) {
        Ok(()) => Ok(()),
        Err(error) => match rename(&paths.backup, &paths.exe) {
            Ok(()) => Err(format!(
                "the new EXE couldn't be put in place (the current one was restored): {error}"
            )),
            Err(restore_error) => Err(format!(
                "the new EXE couldn't be put in place ({error}) and the current EXE could not be restored ({restore_error}). The known-good backup remains at {}; restore it manually before closing this app",
                paths.backup.display()
            )),
        },
    }
}

/// Restore the old executable if launching the replacement fails. On any
/// failure, keep both the known-good backup and any replacement candidate;
/// return a recovery instruction instead of claiming rollback succeeded.
pub fn undo_swap(paths: &UpdatePaths) -> Result<(), String> {
    undo_swap_with(paths, rename_update_file)
}

fn undo_swap_with<R>(paths: &UpdatePaths, mut rename: R) -> Result<(), String>
where
    R: FnMut(&Path, &Path) -> io::Result<()>,
{
    if !paths.backup.is_file() {
        return Err(format!(
            "the original backup was not found at {}; the replacement was left at {}",
            paths.backup.display(),
            paths.exe.display()
        ));
    }
    if !paths.exe.exists() {
        return rename(&paths.backup, &paths.exe).map_err(|error| {
            format!(
                "the original EXE could not be restored ({error}); its backup remains at {}",
                paths.backup.display()
            )
        });
    }
    if paths.download.exists() {
        return Err(format!(
            "rollback could not use the occupied temporary path {}; the known-good backup remains at {}",
            paths.download.display(), paths.backup.display()
        ));
    }
    rename(&paths.exe, &paths.download).map_err(|error| {
        format!(
        "the replacement could not be moved aside ({error}); the known-good backup remains at {}",
        paths.backup.display()
    )
    })?;
    if let Err(error) = rename(&paths.backup, &paths.exe) {
        // Keep a runnable path when possible, without touching the backup.
        // If this too fails, the replacement remains at the download path.
        let candidate = if !paths.exe.exists() && rename(&paths.download, &paths.exe).is_ok() {
            paths.exe.display().to_string()
        } else {
            paths.download.display().to_string()
        };
        return Err(format!(
            "the original EXE could not be restored ({error}); the known-good backup remains at {} and the replacement remains at {candidate}",
            paths.backup.display()
        ));
    }
    let _ = fs::remove_file(&paths.download);
    Ok(())
}

#[cfg(test)]
mod failure_path_tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> PathBuf {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "macroclicker-update-fault-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("scratch directory");
        path
    }

    #[test]
    fn failed_swap_and_failed_restore_preserve_the_known_good_backup() {
        let dir = scratch();
        let paths = update_paths(&dir.join("macro_clicker.exe"), "swap");
        fs::write(&paths.exe, b"old").expect("write old");
        fs::write(&paths.download, b"new").expect("write new");
        let mut calls = 0;
        let err = swap_in_new_exe_with(&paths, |from, to| {
            calls += 1;
            if calls >= 2 {
                Err(io::Error::other("injected rename failure"))
            } else {
                fs::rename(from, to)
            }
        })
        .expect_err("both later renames fail");
        assert!(err.contains("could not be restored"));
        assert!(err.contains(&paths.backup.display().to_string()));
        assert_eq!(fs::read(&paths.backup).expect("backup retained"), b"old");
        assert_eq!(
            fs::read(&paths.download).expect("candidate retained"),
            b"new"
        );
        assert!(!paths.exe.exists());
        assert_eq!(cleanup_update_attempt(&paths), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_rollback_preserves_both_backup_and_replacement() {
        let dir = scratch();
        let paths = update_paths(&dir.join("macro_clicker.exe"), "rollback");
        fs::write(&paths.exe, b"replacement").expect("write replacement");
        fs::write(&paths.backup, b"known good").expect("write backup");
        let err = undo_swap_with(&paths, |from, to| {
            if from == paths.backup {
                Err(io::Error::other("injected restoration failure"))
            } else {
                fs::rename(from, to)
            }
        })
        .expect_err("restoring backup must report failure");
        assert!(err.contains("could not be restored"));
        assert!(err.contains(&paths.backup.display().to_string()));
        assert_eq!(
            fs::read(&paths.backup).expect("backup retained"),
            b"known good"
        );
        assert_eq!(
            fs::read(&paths.exe).expect("candidate retained"),
            b"replacement"
        );
        let _ = fs::remove_dir_all(dir);
    }

    fn checkout(root: &Path) -> SourceCheckout {
        let crate_dir = root.join("autoclicker");
        SourceCheckout {
            root: root.to_path_buf(),
            manifest: crate_dir.join("Cargo.toml"),
            crate_dir,
            profile: "release".to_string(),
        }
    }

    #[test]
    fn destination_manifest_is_rejected_before_a_merge() {
        let dir = scratch();
        let checkout = checkout(&dir);
        let cancel = AtomicBool::new(false);
        let mut merged = false;
        let result =
            fast_forward_source_checkout_with(&checkout, "1.6.0", &cancel, |args, _, _, _| {
                match args[0] {
                    "status" | "fetch" => Ok(String::new()),
                    "branch" => Ok("main".to_string()),
                    "remote" => Ok("https://github.com/Skid-ster/autoclickerBYARENA".to_string()),
                    "rev-parse" if args[1] == "--verify" => Ok("bbbbbbbb".to_string()),
                    "rev-parse" => Ok("aaaaaaaa".to_string()),
                    "merge-base" => Ok("aaaaaaaa".to_string()),
                    "show" => Ok("[package]\nversion=\"9.0.0\"".to_string()),
                    "merge" => {
                        merged = true;
                        Ok(String::new())
                    }
                    other => panic!("unexpected command {other}"),
                }
            });
        let failure = result.expect_err("mismatched target manifest must fail");
        assert_eq!(failure.change, SourceChange::Unchanged);
        assert!(!merged);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cancellation_after_fast_forward_reports_advanced_head() {
        let dir = scratch();
        let checkout = checkout(&dir);
        let cancel = AtomicBool::new(false);
        let mut recovery_probe = false;
        let result = fast_forward_source_checkout_with(
            &checkout,
            "1.6.0",
            &cancel,
            |args, flag, _, label| match args[0] {
                "status" | "fetch" => Ok(String::new()),
                "branch" => Ok("main".to_string()),
                "remote" => Ok("https://github.com/Skid-ster/autoclickerBYARENA".to_string()),
                "rev-parse" if label == "git recovery HEAD" => {
                    recovery_probe = true;
                    assert!(
                        !flag.load(Ordering::Relaxed),
                        "recovery needs a fresh cancellation flag"
                    );
                    Ok("bbbbbbbb".to_string())
                }
                "rev-parse" if args[1] == "--verify" => Ok("bbbbbbbb".to_string()),
                "rev-parse" => Ok("aaaaaaaa".to_string()),
                "merge-base" => Ok("aaaaaaaa".to_string()),
                "show" => Ok("[package]\nversion=\"1.6.0\"".to_string()),
                "merge" => {
                    cancel.store(true, Ordering::Relaxed);
                    Err(SOURCE_UPDATE_CANCELLED.to_string())
                }
                other => panic!("unexpected command {other}"),
            },
        );
        let failure = result.expect_err("cancelled merge");
        assert!(recovery_probe);
        assert!(failure.cancelled());
        assert_eq!(failure.change, SourceChange::AdvancedToTarget);
        assert!(failure
            .description("1.6.0")
            .contains("HEAD advanced to v1.6.0"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unchanged_head_with_dirty_worktree_is_reported_as_uncertain() {
        let mut git = |args: &[&str], flag: &AtomicBool, _: Duration, _: &str| {
            assert!(!flag.load(Ordering::Relaxed));
            if args[0] == "rev-parse" {
                Ok("aaaaaaaa".to_string())
            } else {
                Ok(" M autoclicker/Cargo.toml".to_string())
            }
        };
        let failure = inspect_after_merge_failure(
            &mut git,
            "aaaaaaaa",
            "bbbbbbbb",
            SOURCE_UPDATE_CANCELLED.to_string(),
        );
        assert_eq!(failure.change, SourceChange::Uncertain);
        assert!(failure.description("1.6.0").contains("may have changed"));
    }
}
