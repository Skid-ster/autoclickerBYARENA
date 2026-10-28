// The path-included module forms a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/update.rs"]
mod update;

use std::time::Duration;

use update::{
    checked_ago_label, download_url_for_version, error_headline, evaluate_update, is_newer,
    latest_tag_from_json, parse_version, release_page_url_for_version, should_refresh,
    UpdateOutcome, UPDATE_REFRESH_AFTER,
};

#[test]
fn parse_version_accepts_plain_and_v_prefixed_tags() {
    assert_eq!(parse_version("1.0.1"), Some((1, 0, 1)));
    assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
    assert_eq!(parse_version("  v10.20.30  "), Some((10, 20, 30)));
}

#[test]
fn parse_version_pads_missing_components_with_zero() {
    assert_eq!(parse_version("2"), Some((2, 0, 0)));
    assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
}

#[test]
fn parse_version_rejects_anything_unorderable() {
    for bad in [
        "",
        "v",
        "nightly",
        "1.0.x",
        "1.0.1-beta",
        "1..2",
        ".1.2",
        "1.2.3.4",
        "-1.0.0",
        "99999999999999999999.0.0",
    ] {
        assert_eq!(parse_version(bad), None, "must reject {bad:?}");
    }
}

#[test]
fn is_newer_compares_component_wise() {
    assert!(is_newer("v1.0.2", "1.0.1"));
    assert!(is_newer("v1.1.0", "1.0.9"));
    assert!(is_newer("v2", "1.9.9"));
    assert!(!is_newer("v1.0.1", "1.0.1"));
    assert!(!is_newer("v1.0.0", "1.0.1"));
    assert!(!is_newer("garbage", "1.0.1"));
    assert!(!is_newer("v9.9.9", "garbage"));
}

#[test]
fn latest_tag_comes_out_of_a_realistic_release_body() {
    let body = r#"{"url":"https://api.github.com/x","assets_url":"https://api.github.com/y","tag_name":"v1.1.0","target_commitish":"main","name":"MacroClicker v1.1.0"}"#;
    assert_eq!(latest_tag_from_json(body), Some("v1.1.0".to_string()));
}

#[test]
fn latest_tag_tolerates_spacing_and_refuses_garbage() {
    assert_eq!(
        latest_tag_from_json("{ \"tag_name\" :  \"v1.2.0\" }"),
        Some("v1.2.0".to_string())
    );
    assert_eq!(latest_tag_from_json("{}"), None);
    assert_eq!(latest_tag_from_json(""), None);
    assert_eq!(latest_tag_from_json(r#"{"tag_name":"#), None);
    assert_eq!(latest_tag_from_json(r#"{"tag_name":""}"#), None);
    assert_eq!(latest_tag_from_json(r#"{"tag_name":42}"#), None);
}

#[test]
fn evaluate_reports_available_without_the_v() {
    let body = r#"{"tag_name":"v1.1.1"}"#;
    assert_eq!(
        evaluate_update(body, "1.1.0"),
        Ok(UpdateOutcome::Available("1.1.1".to_string()))
    );
}

#[test]
fn evaluate_reports_up_to_date_for_equal_and_older() {
    assert_eq!(
        evaluate_update(r#"{"tag_name":"v1.1.0"}"#, "1.1.0"),
        Ok(UpdateOutcome::UpToDate("1.1.0".to_string()))
    );
    assert_eq!(
        evaluate_update(r#"{"tag_name":"v1.0.1"}"#, "1.1.0"),
        Ok(UpdateOutcome::UpToDate("1.0.1".to_string()))
    );
}

#[test]
fn evaluate_errors_never_guess_an_answer() {
    assert!(evaluate_update("not json at all", "1.1.0").is_err());
    assert!(evaluate_update(r#"{"tag_name":"nightly"}"#, "1.1.0").is_err());
    assert!(evaluate_update(r#"{"tag_name":"v1.2.0"}"#, "unreadable").is_err());
}

#[test]
fn latest_tag_skips_escaped_lookalikes_and_bad_candidates() {
    let body = r#"{"notes":"example: \"tag_name\":\"v9.9.9\"","tag_name":"v1.2.0"}"#;
    assert_eq!(latest_tag_from_json(body), Some("v1.2.0".to_string()));
    let body = r#"{"tag_name":42,"tag_name":"v1.2.0"}"#;
    assert_eq!(latest_tag_from_json(body), Some("v1.2.0".to_string()));
    let long = format!(r#"{{"tag_name":"v{}"}}"#, "9".repeat(80));
    assert_eq!(latest_tag_from_json(&long), None);
}

#[test]
fn release_urls_are_built_only_from_valid_versions() {
    assert_eq!(
        release_page_url_for_version("1.2.0"),
        Some("https://github.com/Skid-ster/autoclickerBYARENA/releases/tag/v1.2.0".to_string())
    );
    assert_eq!(
        download_url_for_version("v1.2"),
        Some(
            "https://github.com/Skid-ster/autoclickerBYARENA/releases/download/v1.2/macro_clicker.exe"
                .to_string()
        )
    );
    for bad in ["", "nightly", "1.2.3-beta", "v1/../../x", "1.2.3.4"] {
        assert_eq!(
            release_page_url_for_version(bad),
            None,
            "must reject {bad:?}"
        );
        assert_eq!(download_url_for_version(bad), None, "must reject {bad:?}");
    }
}

#[test]
fn refresh_policy_needs_no_attempt_or_a_stale_one() {
    assert!(should_refresh(None));
    assert!(!should_refresh(Some(Duration::ZERO)));
    assert!(!should_refresh(Some(
        UPDATE_REFRESH_AFTER - Duration::from_secs(1)
    )));
    assert!(should_refresh(Some(UPDATE_REFRESH_AFTER)));
    assert!(should_refresh(Some(UPDATE_REFRESH_AFTER * 3)));
}

#[test]
fn checked_ago_label_uses_natural_units() {
    assert_eq!(checked_ago_label(Duration::from_secs(0)), "just now");
    assert_eq!(checked_ago_label(Duration::from_secs(42)), "42 seconds ago");
    assert_eq!(checked_ago_label(Duration::from_secs(60)), "1 minute ago");
    assert_eq!(
        checked_ago_label(Duration::from_secs(42 * 60)),
        "42 minutes ago"
    );
    assert_eq!(
        checked_ago_label(Duration::from_secs(60 * 60)),
        "1 hour ago"
    );
    assert_eq!(
        checked_ago_label(Duration::from_secs(26 * 60 * 60)),
        "1 day ago"
    );
    assert_eq!(
        checked_ago_label(Duration::from_secs(49 * 60 * 60)),
        "2 days ago"
    );
}

#[test]
fn error_headlines_name_the_actionable_failure_class() {
    assert_eq!(
        error_headline("the update server answered HTTP 403"),
        "GitHub is rate-limiting checks"
    );
    assert_eq!(
        error_headline("the update server answered HTTP 503"),
        "GitHub returned an error"
    );
    assert_eq!(
        error_headline("the update server sent an unreadable reply"),
        "GitHub sent an unexpected reply"
    );
    assert_eq!(
        error_headline("the request timed out"),
        "The check timed out"
    );
    assert_eq!(
        error_headline("WinHttpConnect failed (GetLastError=12007)"),
        "Couldn't reach GitHub"
    );
}
