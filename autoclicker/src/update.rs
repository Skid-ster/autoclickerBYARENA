//! Update-check decision logic: what "the latest release" means and
//! whether it is newer than the running build.
//!
//! This module is deliberately free of platform APIs and networking:
//! it turns the body of GitHub's "latest release" API response into
//! a yes/no upgrade answer with plain string scans, so every rule is
//! unit-testable on any host. The HTTPS fetch itself lives in
//! `win32::https_get_text` (the OS WinHTTP stack) and the
//! orchestration — worker thread, state machine, Updates window —
//! in `app`. Nothing here can block, allocate unboundedly or panic:
//! inputs are untrusted network bytes and every surprise becomes a
//! plain "couldn't check".

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
        .map(|digits| format!("{REPOSITORY_URL}/releases/download/v{digits}/macro_clicker.exe"))
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
/// for the running `current_version`. The Err texts are written for
/// the user-facing status line in the Updates window.
pub fn evaluate_update(body: &str, current_version: &str) -> Result<UpdateOutcome, String> {
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
    if latest > current {
        Ok(UpdateOutcome::Available(display))
    } else {
        Ok(UpdateOutcome::UpToDate(display))
    }
}
