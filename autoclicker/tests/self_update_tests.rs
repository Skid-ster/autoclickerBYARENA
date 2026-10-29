// The path-included modules form a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/update.rs"]
mod update;

#[path = "../src/self_update.rs"]
mod self_update;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use self_update::{
    cleanup_update_attempt, hash_file, install_kind_for_exe, is_amd64_pe, is_cargo_target_exe,
    is_supported_origin_url, package_version_from_manifest, source_checkout_for_exe,
    swap_in_new_exe, undo_swap, update_paths, verify_downloaded_exe, InstallKind,
};
use update::Sha256;

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn scratch_dir(label: &str) -> PathBuf {
    let n = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "macroclicker-self-update-test-{}-{label}-{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir must be creatable");
    dir
}

/// A minimal but structurally honest AMD64 PE header followed by
/// a payload, built the way `is_amd64_pe` reads it.
fn fake_exe_bytes(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0_u8; 512];
    bytes[0] = b'M';
    bytes[1] = b'Z';
    let pe_offset: u32 = 0x80;
    bytes[0x3c..0x40].copy_from_slice(&pe_offset.to_le_bytes());
    let pe = pe_offset as usize;
    bytes[pe..pe + 4].copy_from_slice(b"PE\0\0");
    bytes[pe + 4..pe + 6].copy_from_slice(&0x8664_u16.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn sha256_of(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize()
}

#[test]
fn update_paths_are_siblings_with_attempt_suffixes() {
    let exe = Path::new("/apps/macro_clicker.exe");
    let paths = update_paths(exe, "4242");
    assert_eq!(paths.exe, exe);
    assert_eq!(
        paths.download,
        Path::new("/apps/macro_clicker.exe.update-4242")
    );
    assert_eq!(paths.backup, Path::new("/apps/macro_clicker.exe.old-4242"));
}

#[test]
fn cargo_target_builds_are_recognized() {
    assert!(is_cargo_target_exe(Path::new(
        "C:/dev/autoclickerBYARENA/autoclicker/target/release/macro_clicker.exe"
    )));
    assert!(is_cargo_target_exe(Path::new(
        "/home/dev/proj/target/debug/deps/macro_clicker-abc123"
    )));
    assert!(!is_cargo_target_exe(Path::new(
        "C:/Users/sam/Downloads/macro_clicker.exe"
    )));
    assert!(!is_cargo_target_exe(Path::new(
        "C:/Tools/release/macro_clicker.exe"
    )));
    assert!(!is_cargo_target_exe(Path::new(
        "D:/games/target practice/macro_clicker.exe"
    )));
}

#[test]
fn pe_gate_requires_mz_signature_and_amd64() {
    assert!(is_amd64_pe(&fake_exe_bytes(b"x")));
    assert!(!is_amd64_pe(b""));
    assert!(!is_amd64_pe(b"MZ"));
    let mut not_mz = fake_exe_bytes(b"x");
    not_mz[0] = b'N';
    assert!(!is_amd64_pe(&not_mz));
    let mut wrong_arch = fake_exe_bytes(b"x");
    wrong_arch[0x84..0x86].copy_from_slice(&0x014c_u16.to_le_bytes()); // i386
    assert!(!is_amd64_pe(&wrong_arch));
    let mut wild_offset = fake_exe_bytes(b"x");
    wild_offset[0x3c..0x40].copy_from_slice(&0x7fff_0000_u32.to_le_bytes());
    assert!(!is_amd64_pe(&wild_offset));
    let mut bad_sig = fake_exe_bytes(b"x");
    bad_sig[0x80] = b'X';
    assert!(!is_amd64_pe(&bad_sig));
}

#[test]
fn hash_file_reports_digest_and_size() {
    let dir = scratch_dir("hash");
    let path = dir.join("blob.bin");
    let data: Vec<u8> = (0_u32..200_000).map(|i| (i % 251) as u8).collect();
    fs::write(&path, &data).expect("write must work");
    let (digest, size) = hash_file(&path).expect("hash must work");
    assert_eq!(size, data.len() as u64);
    assert_eq!(digest, sha256_of(&data));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn verify_accepts_only_the_exact_published_file() {
    let dir = scratch_dir("verify");
    let path = dir.join("new.exe");
    let bytes = fake_exe_bytes(b"release payload");
    fs::write(&path, &bytes).expect("write must work");
    let size = bytes.len() as u64;
    let digest = sha256_of(&bytes);
    assert!(verify_downloaded_exe(&path, size, &digest).is_ok());

    // One flipped byte anywhere fails the fingerprint.
    let mut tampered = bytes.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    fs::write(&path, &tampered).expect("write must work");
    let err = verify_downloaded_exe(&path, size, &digest).expect_err("tamper must fail");
    assert!(err.contains("fingerprint"), "got: {err}");

    // Truncation fails on size before the fingerprint is consulted.
    fs::write(&path, &bytes[..bytes.len() - 3]).expect("write must work");
    let err = verify_downloaded_exe(&path, size, &digest).expect_err("truncation must fail");
    assert!(err.contains("incomplete"), "got: {err}");

    // A non-program fails the PE gate even with a matching hash.
    let text = b"just some text, definitely not a program";
    fs::write(&path, text).expect("write must work");
    let err = verify_downloaded_exe(&path, text.len() as u64, &sha256_of(text))
        .expect_err("non-PE must fail");
    assert!(err.contains("not a Windows program"), "got: {err}");

    // A missing file is a clean error, never a panic.
    assert!(verify_downloaded_exe(&dir.join("nope.exe"), 1, &digest).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn swap_replaces_the_exe_and_parks_the_old_one() {
    let dir = scratch_dir("swap");
    let exe = dir.join("macro_clicker.exe");
    let paths = update_paths(&exe, "7");
    fs::write(&exe, b"old build").expect("write must work");
    fs::write(&paths.download, b"new build").expect("write must work");
    swap_in_new_exe(&paths).expect("swap must succeed");
    assert_eq!(fs::read(&exe).expect("read"), b"new build");
    assert_eq!(fs::read(&paths.backup).expect("read"), b"old build");
    assert!(!paths.download.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn swap_rolls_back_when_the_download_is_missing() {
    let dir = scratch_dir("rollback");
    let exe = dir.join("macro_clicker.exe");
    let paths = update_paths(&exe, "8");
    fs::write(&exe, b"only build").expect("write must work");
    // No download file: the second rename must fail and the
    // original must be renamed straight back into place.
    let err = swap_in_new_exe(&paths).expect_err("missing download must fail");
    assert!(err.contains("restored"), "got: {err}");
    assert_eq!(fs::read(&exe).expect("read"), b"only build");
    assert!(!paths.backup.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn swap_preserves_an_existing_backup_instead_of_overwriting_it() {
    let dir = scratch_dir("stale-backup");
    let exe = dir.join("macro_clicker.exe");
    let paths = update_paths(&exe, "9");
    fs::write(&exe, b"current").expect("write must work");
    fs::write(&paths.download, b"next").expect("write must work");
    fs::write(&paths.backup, b"ancient crash remains").expect("write must work");
    let error = swap_in_new_exe(&paths).expect_err("existing backup must be preserved");
    assert!(error.contains("preserved"));
    assert_eq!(fs::read(&exe).expect("read"), b"current");
    assert_eq!(
        fs::read(&paths.backup).expect("read"),
        b"ancient crash remains"
    );
    assert_eq!(fs::read(&paths.download).expect("read"), b"next");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn undo_swap_restores_the_original_after_a_failed_launch() {
    let dir = scratch_dir("undo");
    let exe = dir.join("macro_clicker.exe");
    let paths = update_paths(&exe, "11");
    fs::write(&exe, b"trusty").expect("write must work");
    fs::write(&paths.download, b"won't start").expect("write must work");
    swap_in_new_exe(&paths).expect("swap must succeed");
    undo_swap(&paths).expect("rollback must succeed");
    assert_eq!(fs::read(&exe).expect("read"), b"trusty");
    assert!(!paths.backup.exists());
    assert!(!paths.download.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn source_checkout_detection_requires_the_real_project_layout() {
    let root = scratch_dir("source-layout");
    let crate_dir = root.join("autoclicker");
    let exe = crate_dir
        .join("target")
        .join("release")
        .join("macro_clicker.exe");
    fs::create_dir_all(exe.parent().expect("exe parent")).expect("target dirs");
    fs::create_dir_all(crate_dir.join("src")).expect("src dir");
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(&exe, b"program").expect("exe");
    fs::write(
        crate_dir.join("Cargo.toml"),
        b"[package]\nversion = \"1.5.0\"\n",
    )
    .expect("manifest");
    fs::write(crate_dir.join("src").join("main.rs"), b"fn main() {}").expect("entry point");

    let checkout = source_checkout_for_exe(&exe).expect("real checkout must be recognized");
    assert_eq!(checkout.root, root);
    assert_eq!(checkout.crate_dir, crate_dir);
    assert_eq!(checkout.manifest, crate_dir.join("Cargo.toml"));
    assert_eq!(checkout.profile, "release");
    assert_eq!(
        install_kind_for_exe(&exe),
        InstallKind::SourceCheckout(checkout)
    );

    // A target tree without the project entry point is merely an
    // unrecognized Cargo artifact, never a place to run Git.
    fs::remove_file(crate_dir.join("src").join("main.rs")).expect("remove entry point");
    assert!(source_checkout_for_exe(&exe).is_none());
    assert_eq!(install_kind_for_exe(&exe), InstallKind::CargoBuild);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn portable_executables_are_not_mistaken_for_source_builds() {
    let dir = scratch_dir("portable-kind");
    let exe = dir.join("macro_clicker.exe");
    fs::write(&exe, b"program").expect("exe");
    assert_eq!(install_kind_for_exe(&exe), InstallKind::Portable);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn source_update_accepts_only_the_project_origin() {
    for url in [
        "https://github.com/Skid-ster/autoclickerBYARENA",
        "https://github.com/Skid-ster/autoclickerBYARENA.git",
        "https://github.com/skid-ster/autoclickerbyarena.git/",
        "git@github.com:Skid-ster/autoclickerBYARENA.git",
        "ssh://git@github.com/Skid-ster/autoclickerBYARENA",
    ] {
        assert!(is_supported_origin_url(url), "must accept {url}");
    }
    for url in [
        "https://github.com/Skid-ster/other",
        "https://example.com/Skid-ster/autoclickerBYARENA",
        "http://github.com/Skid-ster/autoclickerBYARENA",
        "https://github.com/Skid-ster/autoclickerBYARENA.evil",
        "",
    ] {
        assert!(!is_supported_origin_url(url), "must reject {url}");
    }
}

#[test]
fn manifest_version_comes_from_the_package_table_only() {
    let manifest = r#"
[build-dependencies]
winresource = "=0.1.34"

[package]
name = "macro_clicker"
version = "1.5.0" # release

[dependencies]
egui = "=0.27.2"
"#;
    assert_eq!(
        package_version_from_manifest(manifest).as_deref(),
        Some("1.5.0")
    );
    assert_eq!(package_version_from_manifest("version = \"9.9.9\""), None);
    assert_eq!(
        package_version_from_manifest("[package]\nversion = \"not-a-version\""),
        None
    );
}

#[test]
fn cleanup_removes_only_the_successful_attempts_files() {
    let dir = scratch_dir("cleanup");
    let exe = dir.join("macro_clicker.exe");
    let current = update_paths(&exe, "123");
    let other = update_paths(&exe, "124");
    fs::write(&exe, b"the program").expect("write must work");
    fs::write(&current.backup, b"a").expect("write");
    fs::write(&current.download, b"b").expect("write");
    fs::write(&other.backup, b"recovery").expect("other backup");
    fs::write(&other.download, b"other candidate").expect("other download");
    fs::write(dir.join("macro_clicker.exe.old"), b"not ours").expect("write");
    fs::write(dir.join("notes.txt"), b"keep me").expect("write");

    assert_eq!(cleanup_update_attempt(&current), 2);
    assert!(!current.backup.exists());
    assert!(!current.download.exists());
    assert_eq!(
        fs::read(&other.backup).expect("other backup retained"),
        b"recovery"
    );
    assert!(other.download.exists());
    assert!(exe.exists());
    assert!(dir.join("macro_clicker.exe.old").exists());
    assert!(dir.join("notes.txt").exists());
    // Idempotent: a second cleanup is a no-op, never an error.
    assert_eq!(cleanup_update_attempt(&current), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_executable_never_causes_cleanup_to_delete_recovery_files() {
    let dir = scratch_dir("missing-exe");
    let exe = dir.join("macro_clicker.exe");
    let paths = update_paths(&exe, "12");
    fs::write(&paths.backup, b"known good").expect("backup");
    fs::write(&paths.download, b"candidate").expect("download");
    assert!(swap_in_new_exe(&paths).is_err());
    assert_eq!(cleanup_update_attempt(&paths), 0);
    assert_eq!(fs::read(&paths.backup).expect("backup kept"), b"known good");
    assert_eq!(
        fs::read(&paths.download).expect("download kept"),
        b"candidate"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rollback_refuses_to_overwrite_an_occupied_temporary_file() {
    let dir = scratch_dir("occupied-rollback");
    let exe = dir.join("macro_clicker.exe");
    let paths = update_paths(&exe, "13");
    fs::write(&exe, b"replacement").expect("exe");
    fs::write(&paths.backup, b"known good").expect("backup");
    fs::write(&paths.download, b"other file").expect("occupied temp");
    let err = undo_swap(&paths).expect_err("occupied path must stop rollback");
    assert!(err.contains("backup"));
    assert_eq!(fs::read(&paths.backup).expect("backup kept"), b"known good");
    assert_eq!(fs::read(&paths.download).expect("temp kept"), b"other file");
    assert_eq!(fs::read(&exe).expect("exe kept"), b"replacement");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn update_attempts_reserve_distinct_files_and_cleanup_only_their_own() {
    let dir = scratch_dir("unique-attempts");
    let exe = dir.join("macro_clicker.exe");
    fs::write(&exe, b"running").expect("exe");
    let first = self_update::reserve_update_paths(&exe).expect("first reservation");
    let second = self_update::reserve_update_paths(&exe).expect("second reservation");
    assert_ne!(first.id, second.id);
    assert!(self_update::valid_update_id(&first.id));
    assert!(!self_update::valid_update_id("../other"));
    fs::write(&first.backup, b"first backup").expect("backup");
    fs::write(&second.backup, b"second backup").expect("backup");
    assert_eq!(self_update::cleanup_update_attempt(&first), 2);
    assert!(!first.backup.exists());
    assert!(!first.download.exists());
    assert_eq!(
        fs::read(&second.backup).expect("other backup"),
        b"second backup"
    );
    assert!(second.download.exists());
    let _ = fs::remove_dir_all(dir);
}
