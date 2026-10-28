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

/// Host serving the latest-release metadata JSON.
pub const UPDATE_API_HOST: &str = "api.github.com";
/// Path of this project's latest-release metadata.
pub const UPDATE_API_PATH: &str = "/repos/Skid-ster/autoclickerBYARENA/releases/latest";
/// Human-facing page the "Download update" button opens. GitHub
/// redirects `/releases/latest` to the newest published release, so
/// the link never goes stale as versions advance.
pub const RELEASES_PAGE_URL: &str =
    "https://github.com/Skid-ster/autoclickerBYARENA/releases/latest";

/// Outcome of comparing the latest published release with the
/// running build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// The running build is the newest published release.
    UpToDate,
    /// A newer release exists; carries its version without a
    /// leading "v" (for example "1.1.1").
    Available(String),
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
/// the field appears exactly once, a version tag cannot contain
/// escapes or quotes, and anything unexpected yields None so the
/// caller reports "couldn't check" instead of guessing.
pub fn latest_tag_from_json(body: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let after_key = body.find(key)? + key.len();
    let after_colon = body[after_key..].trim_start().strip_prefix(':')?;
    let value = after_colon.trim_start().strip_prefix('"')?;
    let end = value.find('"')?;
    let tag = &value[..end];
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
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
    if latest > current {
        let display = tag.strip_prefix('v').unwrap_or(&tag);
        Ok(UpdateOutcome::Available(display.to_string()))
    } else {
        Ok(UpdateOutcome::UpToDate)
    }
}
