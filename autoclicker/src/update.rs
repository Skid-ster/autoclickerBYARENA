//! Update-check decision logic: what "the latest release" means and
//! whether it is newer than the running build.
//!
//! This module is deliberately free of platform APIs and networking:
//! it turns the body of GitHub's "latest release" API response into
//! a yes/no upgrade answer with plain string scans, so every rule is
//! unit-testable on any host. The HTTPS fetch itself lives in
//! `win32::https_get_text` (the OS WinHTTP stack) and the
//! orchestration — worker thread, state machine, Updates window —
//! in `app`. This module also owns the inputs the verified
//! self-update (`self_update`) depends on: the release asset's
//! expected size and SHA-256 fingerprint, the SHA-256 itself, and
//! the validated URL shapes a download or redirect may take.
//! Nothing here can block, allocate unboundedly or panic: inputs
//! are untrusted network bytes and every surprise becomes a plain
//! "couldn't check".

use std::time::Duration;

/// Host serving the latest-release metadata JSON.
pub const UPDATE_API_HOST: &str = "api.github.com";
/// Path of this project's latest-release metadata.
pub const UPDATE_API_PATH: &str = "/repos/Skid-ster/autoclickerBYARENA/releases/latest";
/// Request headers for the update check. The explicit API version
/// pins the response schema this parser was written against instead
/// of silently inheriting whatever default GitHub chooses later.
pub const UPDATE_API_HEADERS: &str =
    "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n";
/// Repository page root from which both human-facing release links
/// and the single release asset are addressed.
const REPOSITORY_URL: &str = "https://github.com/Skid-ster/autoclickerBYARENA";
/// Human-facing page for the newest published release. GitHub
/// redirects `/releases/latest` to that release, so the link never
/// goes stale as versions advance.
pub const RELEASES_PAGE_URL: &str =
    "https://github.com/Skid-ster/autoclickerBYARENA/releases/latest";
/// The one asset every release publishes and the self-update
/// downloads. CI enforces that a release carries exactly this file.
pub const RELEASE_ASSET_NAME: &str = "macro_clicker.exe";
/// Sanity ceiling for a downloadable update. The real EXE is a few
/// MB; a release claiming more than this is treated as broken
/// metadata, not as something to stream onto the user's disk.
pub const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
/// How long a completed check stays fresh enough that merely
/// reopening the Updates window does not spend another request.
/// Manual "Check now" always bypasses this interval.
pub const UPDATE_REFRESH_AFTER: Duration = Duration::from_secs(10 * 60);

/// Outcome of comparing the latest published release with the
/// running build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// No newer release exists. Carries the newest published
    /// version GitHub reported (without a leading "v"), so the UI
    /// can prove what it saw — including "this source build is
    /// ahead of the latest release" — instead of forcing the user
    /// to take a bare "up to date" on faith.
    UpToDate(String),
    /// A newer release exists; carries its version without a
    /// leading "v" (for example "1.1.1").
    Available(String),
}

/// What GitHub says the release asset must look like: exact byte
/// size and, when GitHub publishes one, the SHA-256 fingerprint it
/// computed over the uploaded file. The self-update refuses to run
/// a single downloaded byte unless both match, and refuses to run
/// at all when the fingerprint is missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub size: u64,
    pub sha256: Option<[u8; 32]>,
}

/// A parsed latest-release response: the version it names and the
/// asset expectations that came with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatestRelease {
    pub version: String,
    pub asset: Option<ReleaseAsset>,
}

/// Returns the bare numeric spelling of a parseable version. The
/// caller's component spelling is preserved (`v1.2` stays `1.2`,
/// not `1.2.0`) because it is used to address the actual GitHub
/// tag; reformatting it could point at a release that does not
/// exist.
fn bare_version(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('v').unwrap_or(trimmed);
    parse_version(digits).map(|_| digits)
}

/// Human-facing page for one known release version.
pub fn release_page_url_for_version(version: &str) -> Option<String> {
    bare_version(version).map(|digits| format!("{REPOSITORY_URL}/releases/tag/v{digits}"))
}

/// Direct browser download for the one asset every release
/// publishes. Keeping this beside the version parser means a
/// malformed network value can never be interpolated into a URL
/// the app asks Windows to open.
pub fn download_url_for_version(version: &str) -> Option<String> {
    bare_version(version)
        .map(|digits| format!("{REPOSITORY_URL}/releases/download/v{digits}/{RELEASE_ASSET_NAME}"))
}

/// The same validated download split into the (host, path) pair the
/// WinHTTP transport consumes, for the in-app verified download.
/// GitHub answers with a redirect to its file CDN; the transport
/// follows it and re-validates every hop with [`parse_https_url`].
pub fn download_request_for_version(version: &str) -> Option<(&'static str, String)> {
    bare_version(version).map(|digits| {
        (
            "github.com",
            format!(
                "/Skid-ster/autoclickerBYARENA/releases/download/v{digits}/{RELEASE_ASSET_NAME}"
            ),
        )
    })
}

/// Splits an absolute `https://` URL into the (host, path+query)
/// pair a follow-up request may use. This is the redirect gate:
/// anything that is not plain HTTPS on a plausible host — an
/// `http://` downgrade, embedded credentials, a nonstandard port,
/// control characters — is refused, so a surprising `Location`
/// header can never steer the downloader somewhere odd.
pub fn parse_https_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    if rest.is_empty()
        || rest
            .chars()
            .any(|c| c.is_control() || c == ' ' || c == '\\')
    {
        return None;
    }
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..host_end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = match authority.rsplit_once(':') {
        Some((host, "443")) => host,
        Some(_) => return None,
        None => authority,
    };
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    {
        return None;
    }
    let tail = rest[host_end..].split('#').next().unwrap_or("");
    let path = if tail.is_empty() {
        "/".to_string()
    } else if let Some(query) = tail.strip_prefix('?') {
        format!("/?{query}")
    } else {
        tail.to_string()
    };
    Some((host.to_string(), path))
}

/// Compact transfer wording for the install progress row, e.g.
/// "2.0 of 3.6 MB (55%)". A missing total (never expected for a
/// GitHub asset) degrades to the downloaded amount alone.
pub fn download_progress_label(done: u64, total: u64) -> String {
    let mb = |bytes: u64| bytes as f64 / 1_048_576.0_f64;
    if total == 0 {
        return format!("{:.1} MB downloaded", mb(done));
    }
    let percent = (done.min(total) as f64 * 100.0_f64 / total as f64).round() as u64;
    format!("{:.1} of {:.1} MB ({percent}%)", mb(done), mb(total))
}

/// Whether reopening the Updates window should spend a request.
/// `elapsed` is the time since the last completed attempt; no
/// attempt yet always refreshes.
pub fn should_refresh(elapsed: Option<Duration>) -> bool {
    elapsed.is_none_or(|duration| duration >= UPDATE_REFRESH_AFTER)
}

/// Compact, user-facing age for a completed check. Exact seconds
/// stop mattering quickly; the wording only needs to distinguish
/// "this answer is fresh" from "press Check now".
pub fn checked_ago_label(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 5 {
        return "just now".to_string();
    }
    if seconds < 60 {
        return format!("{seconds} seconds ago");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return if minutes == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{minutes} minutes ago")
        };
    }
    let hours = minutes / 60;
    if hours < 24 {
        return if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{hours} hours ago")
        };
    }
    let days = hours / 24;
    if days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{days} days ago")
    }
}

/// Short headline for a failed check. The detailed message stays
/// available to the UI as a secondary line; this only turns the
/// common failure classes into language that tells the user what
/// to do next.
pub fn error_headline(message: &str) -> &'static str {
    if message.contains("HTTP 403") || message.contains("HTTP 429") {
        "GitHub is rate-limiting checks"
    } else if message.contains("HTTP ") {
        "GitHub returned an error"
    } else if message.contains("unreadable")
        || message.contains("not a version")
        || message.contains("non-text")
        || message.contains("oversized")
    {
        "GitHub sent an unexpected reply"
    } else if message.contains("fingerprint") || message.contains("not a Windows program") {
        "The download failed verification"
    } else if message.contains("timed out") {
        "The check timed out"
    } else {
        "Couldn't reach GitHub"
    }
}

/// Parses a dotted version ("1.2.3", optionally "v1.2.3") into
/// numeric components. Missing trailing components count as zero,
/// so "1.2" parses as (1, 2, 0). Anything else — empty,
/// non-numeric, more than three components, suffixes like "-beta"
/// — returns None: a version we cannot order must never provoke
/// an upgrade prompt.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let mut parts = [0_u32; 3];
    for (index, part) in digits.split('.').enumerate() {
        if index >= 3 || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        parts[index] = part.parse::<u32>().ok()?;
    }
    Some((parts[0], parts[1], parts[2]))
}

/// True when `latest` orders strictly after `current`. An
/// unparseable value on either side is never "newer".
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// Extracts the `tag_name` value from a GitHub release JSON body by
/// tolerant scanning. The full JSON grammar is unnecessary here:
/// a version tag cannot contain escapes or quotes, escaped
/// lookalike text inside string values is skipped (its opening
/// quote is backslash-escaped), candidate tags are length- and
/// control-character-bounded, and anything unexpected yields None
/// so the caller reports "couldn't check" instead of guessing.
pub fn latest_tag_from_json(body: &str) -> Option<String> {
    const KEY: &str = "\"tag_name\"";
    const MAX_TAG_LEN: usize = 64;
    let bytes = body.as_bytes();
    for (index, _) in body.match_indices(KEY) {
        if index > 0 && bytes[index - 1] == b'\\' {
            continue;
        }
        let Some(after_colon) = body[index + KEY.len()..].trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(value) = after_colon.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = value.find('"') else {
            continue;
        };
        let tag = &value[..end];
        if tag.is_empty() || tag.len() > MAX_TAG_LEN || tag.chars().any(char::is_control) {
            continue;
        }
        return Some(tag.to_string());
    }
    None
}

/// Turns a latest-release response body into the upgrade decision
/// for the running `current_version`, together with the asset
/// expectations a verified self-update needs. The Err texts are
/// written for the user-facing status line in the Updates window.
pub fn evaluate_release(
    body: &str,
    current_version: &str,
) -> Result<(UpdateOutcome, Option<ReleaseAsset>), String> {
    let Some(tag) = latest_tag_from_json(body) else {
        return Err("the update server sent an unreadable reply".to_string());
    };
    let Some(latest) = parse_version(&tag) else {
        return Err(format!(
            "the latest release tag ({tag}) is not a version number"
        ));
    };
    let Some(current) = parse_version(current_version) else {
        return Err("this build's own version is unreadable".to_string());
    };
    let display = tag.strip_prefix('v').unwrap_or(&tag).to_string();
    let outcome = if latest > current {
        UpdateOutcome::Available(display)
    } else {
        UpdateOutcome::UpToDate(display)
    };
    Ok((outcome, release_asset_from_json(body)))
}

/// [`evaluate_release`] without the asset expectations, for
/// callers that only need the yes/no upgrade answer.
pub fn evaluate_update(body: &str, current_version: &str) -> Result<UpdateOutcome, String> {
    evaluate_release(body, current_version).map(|(outcome, _asset)| outcome)
}

/// Parses a latest-release body into its version and asset
/// expectations without comparing against a running build.
pub fn parse_latest_release(body: &str) -> Option<LatestRelease> {
    let tag = latest_tag_from_json(body)?;
    parse_version(&tag)?;
    Some(LatestRelease {
        version: tag.strip_prefix('v').unwrap_or(&tag).to_string(),
        asset: release_asset_from_json(body),
    })
}

/// Finds the first well-formed `"key": "value"` string pair at or
/// after `from`, returning the value and the index just past its
/// closing quote. Escaped lookalikes inside string values are
/// skipped exactly as in [`latest_tag_from_json`], and values are
/// length-bounded so a hostile body cannot make the scan copy
/// unbounded text.
fn json_string_after(body: &str, key: &str, from: usize) -> Option<(String, usize)> {
    const MAX_VALUE_LEN: usize = 512;
    let bytes = body.as_bytes();
    for (index, _) in body.match_indices(key) {
        if index < from || (index > 0 && bytes[index - 1] == b'\\') {
            continue;
        }
        let Some(after_colon) = body[index + key.len()..].trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(value) = after_colon.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = value.find('"') else {
            continue;
        };
        if end > MAX_VALUE_LEN {
            continue;
        }
        let value_start = body.len() - value.len();
        return Some((value[..end].to_string(), value_start + end + 1));
    }
    None
}

/// Finds the first well-formed `"key": <digits>` number pair in
/// `body` and parses it. Non-numeric, negative or overflowing
/// values are refused rather than coerced.
fn json_number_after(body: &str, key: &str) -> Option<u64> {
    let bytes = body.as_bytes();
    for (index, _) in body.match_indices(key) {
        if index > 0 && bytes[index - 1] == b'\\' {
            continue;
        }
        let Some(after_colon) = body[index + key.len()..].trim_start().strip_prefix(':') else {
            continue;
        };
        let digits = after_colon.trim_start();
        let digit_len = digits
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digit_len == 0 || digit_len > 19 {
            continue;
        }
        return digits[..digit_len].parse::<u64>().ok();
    }
    None
}

/// Extracts the [`RELEASE_ASSET_NAME`] entry's size and SHA-256
/// digest from a latest-release body. The scan anchors on the
/// asset's own `"name"` pair and only reads fields from that
/// asset's slice of the body (up to the next `"name"` key), so a
/// second asset can never lend this one its size or fingerprint.
/// A missing or implausible size yields None; a missing or
/// malformed digest yields an asset with `sha256: None`, which
/// the self-update treats as "cannot verify — use the browser".
pub fn release_asset_from_json(body: &str) -> Option<ReleaseAsset> {
    let mut search_from = 0_usize;
    loop {
        let (name, after_name) = json_string_after(body, "\"name\"", search_from)?;
        if name == RELEASE_ASSET_NAME {
            let window_end = body[after_name..]
                .find("\"name\"")
                .map(|offset| after_name + offset)
                .unwrap_or(body.len());
            let window = &body[after_name..window_end];
            let size = json_number_after(window, "\"size\"")?;
            if size == 0 || size > MAX_DOWNLOAD_BYTES {
                return None;
            }
            let sha256 = json_string_after(window, "\"digest\"", 0)
                .and_then(|(digest, _after)| digest.strip_prefix("sha256:").map(str::to_string))
                .and_then(|hex| parse_sha256_hex(&hex));
            return Some(ReleaseAsset { size, sha256 });
        }
        search_from = after_name;
    }
}

/// Parses exactly 64 hexadecimal characters into a SHA-256 digest.
pub fn parse_sha256_hex(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut digest = [0_u8; 32];
    for (index, slot) in digest.iter_mut().enumerate() {
        let high = (bytes[index * 2] as char).to_digit(16)?;
        let low = (bytes[index * 2 + 1] as char).to_digit(16)?;
        *slot = ((high << 4) | low) as u8;
    }
    Some(digest)
}

/// Lowercase hex spelling of a SHA-256 digest, matching the form
/// GitHub publishes in its `sha256:` asset digests.
pub fn sha256_hex(digest: &[u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// A dependency-free, streaming SHA-256 (FIPS 180-4). The
/// self-update verifies a downloaded EXE against the fingerprint
/// GitHub published for it before a single byte is allowed to
/// replace the running program, and that check deserves a small,
/// exactly-testable implementation rather than a new crate in the
/// supply chain of a code-execution path.
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

const SHA256_INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const SHA256_ROUND_CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn sha256_compress(state: &mut [u32; 8], block: &[u8]) {
    debug_assert_eq!(block.len(), 64);
    let mut schedule = [0_u32; 64];
    for (index, chunk) in block.as_chunks::<4>().0.iter().take(16).enumerate() {
        schedule[index] = u32::from_be_bytes(*chunk);
    }
    for index in 16..64 {
        let w15 = schedule[index - 15];
        let w2 = schedule[index - 2];
        let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
        let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
        state[0], state[1], state[2], state[3], state[4], state[5], state[6], state[7],
    );
    for index in 0..64 {
        let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(sum1)
            .wrapping_add(choose)
            .wrapping_add(SHA256_ROUND_CONSTANTS[index])
            .wrapping_add(schedule[index]);
        let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: SHA256_INITIAL_STATE,
            buffer: [0_u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    /// Feeds `data` into the hash. Any chunking yields the same
    /// digest; callers stream files in fixed blocks.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        if self.buffer_len > 0 {
            let take = (64 - self.buffer_len).min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                sha256_compress(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }
        while data.len() >= 64 {
            sha256_compress(&mut self.state, &data[..64]);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    /// Completes the hash and returns the 32-byte digest.
    pub fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            for slot in &mut self.buffer[self.buffer_len..] {
                *slot = 0;
            }
            let block = self.buffer;
            sha256_compress(&mut self.state, &block);
            self.buffer = [0_u8; 64];
        } else {
            for slot in &mut self.buffer[self.buffer_len..56] {
                *slot = 0;
            }
        }
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        sha256_compress(&mut self.state, &block);
        let mut digest = [0_u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}
