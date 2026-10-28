//! Verified self-replacement: turning a downloaded release EXE
//! into the running program, safely.
//!
//! A recognized source checkout first fast-forwards its clean `main`
//! branch to the exact release tag; from there both installation kinds
//! use the same verified flow: download → verify → swap → relaunch:
//!
//! 1. The new EXE is streamed (by `win32::https_download_to_file`)
//!    to `macro_clicker.exe.update-<pid>` **next to the running
//!    EXE**, so the final step is a same-volume rename, never a
//!    copy that could half-complete.
//! 2. [`verify_downloaded_exe`] re-reads it and demands all three:
//!    a real AMD64 PE header, the exact byte size GitHub
//!    published, and a SHA-256 identical to GitHub's published
//!    fingerprint. Any mismatch deletes the download and leaves
//!    the installed program untouched.
//! 3. [`swap_in_new_exe`] renames the running EXE to
//!    `.old-<pid>` — Windows permits renaming a loaded program
//!    image even though overwriting it would fail — then renames
//!    the verified download into its place. If the second rename
//!    fails, the original is renamed straight back.
//! 4. The app launches the new file and exits; on its next start
//!    [`cleanup_stale_update_files`] removes the `.old-<pid>`
//!    remains (retrying briefly, because the old process may
//!    still be exiting while the new one starts).
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
/// `backup` are siblings of `exe` with a per-process suffix, so
/// two instances updating at once never fight over one temp name
/// and a crashed attempt's remains are recognizable for cleanup.
#[derive(Clone, Debug)]
pub struct UpdatePaths {
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
/// caller's per-attempt discriminator (the app passes its PID).
pub fn update_paths(exe: &Path, unique: &str) -> UpdatePaths {
    UpdatePaths {
        exe: exe.to_path_buf(),
        download: sibling_with_suffix(exe, &format!(".update-{unique}")),
        backup: sibling_with_suffix(exe, &format!(".old-{unique}")),
    }
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
        if cancel.load(Ordering::Relaxed) {
            terminate_child(&mut child);
            let _ = fs::remove_file(&stdout_path);
            let _ = fs::remove_file(&stderr_path);
            return Err(SOURCE_UPDATE_CANCELLED.to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() > timeout {
                    terminate_child(&mut child);
                    let _ = fs::remove_file(&stdout_path);
                    let _ = fs::remove_file(&stderr_path);
                    return Err(format!("{label} timed out"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
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

/// Fast-forwards a recognized source checkout to an exact release tag.
///
/// Safety rails, in order: no tracked changes, the current branch is
/// `main`, `origin` is this project's GitHub repository, the release
/// tag resolves to a commit, the current HEAD is an ancestor of that
/// commit, and the manifest at the destination names the expected
/// version. Any refusal happens before the working tree is changed
/// (fetching refs aside). The checked-out source is updated even if a
/// later binary download fails; callers word that state honestly.
pub fn fast_forward_source_checkout(
    checkout: &SourceCheckout,
    version: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let clean_version = version.trim().strip_prefix('v').unwrap_or(version.trim());
    if update::parse_version(clean_version).is_none() {
        return Err(format!("release version {version} is not a version number"));
    }

    let short_timeout = Duration::from_secs(20);
    let status = run_git(
        checkout,
        &["status", "--porcelain=v1", "--untracked-files=no"],
        cancel,
        short_timeout,
        "git status",
    )?;
    if !status.trim().is_empty() {
        return Err(
            "the source checkout has uncommitted tracked changes; commit or stash them before updating in the app"
                .to_string(),
        );
    }

    let branch = run_git(
        checkout,
        &["branch", "--show-current"],
        cancel,
        short_timeout,
        "git branch",
    )?;
    if branch.trim() != "main" {
        let current = branch.trim();
        let shown = if current.is_empty() {
            "a detached HEAD".to_string()
        } else {
            format!("branch {current}")
        };
        return Err(format!(
            "the source checkout is on {shown}, not main; switch to main before updating in the app"
        ));
    }

    let origin = run_git(
        checkout,
        &["remote", "get-url", "origin"],
        cancel,
        short_timeout,
        "git remote",
    )?;
    if !is_supported_origin_url(origin.trim()) {
        return Err(
            "the source checkout's origin is not the MacroClicker GitHub repository".to_string(),
        );
    }

    run_git(
        checkout,
        &["fetch", "--prune", "--tags", "origin", "main"],
        cancel,
        Duration::from_secs(120),
        "git fetch",
    )?;

    let tag_ref = format!("refs/tags/v{clean_version}^{{commit}}");
    let target = run_git(
        checkout,
        &["rev-parse", "--verify", &tag_ref],
        cancel,
        short_timeout,
        "git rev-parse",
    )?;
    let target = target.trim();
    if target.len() < 7 || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "Git did not resolve release tag v{clean_version} to a commit"
        ));
    }

    let head = run_git(
        checkout,
        &["rev-parse", "HEAD"],
        cancel,
        short_timeout,
        "git rev-parse",
    )?;
    let head = head.trim();
    if head != target {
        let merge_base = run_git(
            checkout,
            &["merge-base", "HEAD", target],
            cancel,
            short_timeout,
            "git merge-base",
        )?;
        if merge_base.trim() != head {
            return Err(format!(
                "the source checkout cannot fast-forward cleanly to v{clean_version}; no source files were changed"
            ));
        }
        run_git(
            checkout,
            &["merge", "--ff-only", target],
            cancel,
            Duration::from_secs(60),
            "git merge",
        )?;
        let new_head = run_git(
            checkout,
            &["rev-parse", "HEAD"],
            cancel,
            short_timeout,
            "git rev-parse",
        )?;
        if new_head.trim() != target {
            return Err(format!(
                "the source checkout did not land exactly on v{clean_version}; no program file was replaced"
            ));
        }
    }

    let manifest_text = bounded_file_text(&checkout.manifest, 256 * 1024);
    if package_version_from_manifest(&manifest_text).as_deref() != Some(clean_version) {
        return Err(format!(
            "the source checkout at v{clean_version} does not name that version in its Cargo manifest; no program file was replaced"
        ));
    }
    Ok(())
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

/// Puts the verified download where the running EXE lives,
/// parking the current file at the backup path first. Any
/// failure after the first rename puts the original back before
/// reporting, so the app is never left without its old,
/// known-good program file. A stale backup from a crashed
/// earlier attempt is cleared first — the one case where the
/// backup destination might already exist.
pub fn swap_in_new_exe(paths: &UpdatePaths) -> Result<(), String> {
    let _ = fs::remove_file(&paths.backup);
    fs::rename(&paths.exe, &paths.backup)
        .map_err(|error| format!("the current EXE couldn't be moved aside: {error}"))?;
    match fs::rename(&paths.download, &paths.exe) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::rename(&paths.backup, &paths.exe);
            Err(format!(
                "the new EXE couldn't be put in place (the current one was restored): {error}"
            ))
        }
    }
}

/// Undoes a completed [`swap_in_new_exe`] when the new program
/// could not be launched: the new file is moved back to the
/// (now vacant) download path and deleted, and the backup
/// returns to the EXE's place. Best-effort on purpose — this
/// runs while reporting an error, and the startup cleanup
/// collects anything it cannot move.
pub fn undo_swap(paths: &UpdatePaths) {
    let _ = fs::rename(&paths.exe, &paths.download);
    let _ = fs::rename(&paths.backup, &paths.exe);
    let _ = fs::remove_file(&paths.download);
}

/// Lists leftover update files next to `exe` (any
/// `<exe>.old-*` / `<exe>.update-*` sibling). Capped so a
/// pathological directory can never turn startup cleanup into
/// an unbounded collection.
pub fn stale_update_files(exe: &Path) -> Vec<PathBuf> {
    const MAX_LISTED: usize = 128;
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    let Some(base) = exe
        .file_name()
        .map(|file_name| file_name.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    let prefixes = [format!("{base}.old-"), format!("{base}.update-")];
    let mut stale = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if stale.len() >= MAX_LISTED {
                break;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix.as_str()))
            {
                stale.push(entry.path());
            }
        }
    }
    stale
}

/// Deletes every leftover update file next to `exe`, returning
/// how many were removed. A file still held by a running older
/// instance simply refuses and is left for the next launch —
/// cleanup must never fail startup.
pub fn cleanup_stale_update_files(exe: &Path) -> usize {
    stale_update_files(exe)
        .iter()
        .filter(|path| fs::remove_file(path).is_ok())
        .count()
}
