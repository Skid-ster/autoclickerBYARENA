// The path-included module forms a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/update.rs"]
mod update;

use update::{evaluate_update, is_newer, latest_tag_from_json, parse_version, UpdateOutcome};

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
        Ok(UpdateOutcome::UpToDate)
    );
    assert_eq!(
        evaluate_update(r#"{"tag_name":"v1.0.1"}"#, "1.1.0"),
        Ok(UpdateOutcome::UpToDate)
    );
}

#[test]
fn evaluate_errors_never_guess_an_answer() {
    assert!(evaluate_update("not json at all", "1.1.0").is_err());
    assert!(evaluate_update(r#"{"tag_name":"nightly"}"#, "1.1.0").is_err());
    assert!(evaluate_update(r#"{"tag_name":"v1.2.0"}"#, "unreadable").is_err());
}
