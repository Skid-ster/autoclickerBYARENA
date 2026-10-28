//! Verified self-replacement: turning a downloaded release EXE
//! into the running program, safely.
//!
//! The flow the app drives is download → verify → swap → relaunch:
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
//! Everything here is `std`-only file work, so the swap, the
//! rollback and the verification are exercised by ordinary tests
//! on any platform; the only Windows fact relied on is the
//! rename-a-running-image permission above.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::update::Sha256;

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

/// Whether `exe` lives in a Cargo build tree (`target/debug` or
/// `target/release`, possibly a subdirectory like `deps`). A
/// development binary must never replace itself with a release
/// build — the next `cargo build` would silently undo it and the
/// developer would be testing a file they did not compile — so
/// the app offers these the browser download instead.
pub fn is_cargo_target_exe(exe: &Path) -> bool {
    let components: Vec<String> = exe
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    components
        .windows(2)
        .any(|pair| pair[0] == "target" && (pair[1] == "debug" || pair[1] == "release"))
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
