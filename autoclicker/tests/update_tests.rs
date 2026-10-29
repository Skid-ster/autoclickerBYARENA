// The path-included module forms a standalone test crate, so items only used
// by the Windows application modules appear dead in this narrow inclusion.
#![allow(dead_code)]

#[path = "../src/update.rs"]
mod update;

use std::time::Duration;

use update::{
    byte_size_label, checked_ago_label, download_progress_label, download_request_for_version,
    download_url_for_version, error_headline, evaluate_release, evaluate_update, is_newer,
    latest_tag_from_json, parse_https_url, parse_latest_release, parse_sha256_hex, parse_version,
    release_asset_from_json, release_page_url_for_version, sha256_hex, should_refresh, Sha256,
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
fn v1_6_to_v1_7_release_is_available_then_up_to_date_with_asset_metadata() {
    let body = r#"{"tag_name":"v1.7.0","assets":[{"name":"macro_clicker.exe","size":4000000,"digest":"sha256:80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce90073"}]}"#;
    let (old, old_asset) = evaluate_release(body, "1.6.0").expect("valid release");
    let (current, current_asset) = evaluate_release(body, "1.7.0").expect("valid release");
    assert_eq!(old, UpdateOutcome::Available("1.7.0".to_string()));
    assert_eq!(current, UpdateOutcome::UpToDate("1.7.0".to_string()));
    assert_eq!(old_asset, current_asset);
    let asset = old_asset.expect("verified executable metadata");
    assert_eq!(asset.size, 4_000_000);
    assert_eq!(
        asset.sha256.map(|digest| sha256_hex(&digest)).as_deref(),
        Some("80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce90073")
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

fn sha256_of(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    sha256_hex(&hasher.finalize())
}

#[test]
fn sha256_matches_the_fips_test_vectors() {
    assert_eq!(
        sha256_of(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_of(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_of(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    let million_a = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256_of(&million_a),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn sha256_streaming_agrees_with_one_shot_at_every_chunking() {
    let data: Vec<u8> = (0_u32..10_000).map(|i| (i * 31 % 251) as u8).collect();
    let expected = sha256_of(&data);
    for chunk in [1_usize, 3, 63, 64, 65, 1000, 8192] {
        let mut hasher = Sha256::new();
        for piece in data.chunks(chunk) {
            hasher.update(piece);
        }
        assert_eq!(sha256_hex(&hasher.finalize()), expected, "chunk={chunk}");
    }
}

#[test]
fn sha256_hex_round_trips_and_rejects_garbage() {
    let digest =
        parse_sha256_hex("80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce90073");
    assert_eq!(
        digest.map(|d| sha256_hex(&d)).as_deref(),
        Some("80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce90073")
    );
    assert!(
        parse_sha256_hex("80ADA94DB2C40C701F0EB9A159F2C4B8E459531F94BE9C526D655B011CE90073")
            .is_some()
    );
    for bad in [
        "",
        "abcd",
        "zzada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce9007",
        "80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce900733",
    ] {
        assert_eq!(parse_sha256_hex(bad), None, "must reject {bad:?}");
    }
}

const RELEASE_BODY_WITH_ASSETS: &str = r#"{"tag_name":"v1.3.0","name":"MacroClicker v1.3.0","assets":[{"url":"https://api.github.com/x","id":1,"node_id":"RA_1","name":"macro_clicker.exe","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"url":"https://api.github.com/users/github-actions%5Bbot%5D"},"content_type":"application/x-msdownload","state":"uploaded","size":3764736,"digest":"sha256:80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce90073","download_count":0,"browser_download_url":"https://github.com/Skid-ster/autoclickerBYARENA/releases/download/v1.3.0/macro_clicker.exe"},{"url":"https://api.github.com/y","id":2,"node_id":"RA_2","name":"decoy.zip","label":"","uploader":{"login":"someone"},"content_type":"application/zip","state":"uploaded","size":111,"digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","browser_download_url":"https://example.com/decoy.zip"}]}"#;

#[test]
fn release_asset_comes_from_the_exe_entry_only() {
    let asset = release_asset_from_json(RELEASE_BODY_WITH_ASSETS);
    assert_eq!(
        asset.as_ref().map(|a| a.size),
        Some(3_764_736),
        "decoy asset must not lend its size"
    );
    assert_eq!(
        asset
            .and_then(|a| a.sha256)
            .map(|d| sha256_hex(&d))
            .as_deref(),
        Some("80ada94db2c40c701f0eb9a159f2c4b8e459531f94be9c526d655b011ce90073"),
        "decoy asset must not lend its fingerprint"
    );
}

#[test]
fn release_asset_degrades_safely_when_metadata_is_missing() {
    // No asset at all.
    assert_eq!(release_asset_from_json(r#"{"tag_name":"v1.3.0"}"#), None);
    // Asset without a digest: size known, fingerprint absent ->
    // the self-update must refuse and use the browser.
    let asset = release_asset_from_json(r#"{"assets":[{"name":"macro_clicker.exe","size":1234}]}"#);
    assert_eq!(asset.as_ref().map(|a| a.size), Some(1234));
    assert_eq!(asset.and_then(|a| a.sha256), None);
    // A digest belonging to a LATER asset must not be stolen.
    let asset = release_asset_from_json(
        r#"{"assets":[{"name":"macro_clicker.exe","size":1234},{"name":"other.bin","size":9,"digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}"#,
    );
    assert_eq!(asset.and_then(|a| a.sha256), None);
    // Implausible sizes are refused outright.
    assert_eq!(
        release_asset_from_json(r#"{"assets":[{"name":"macro_clicker.exe","size":0}]}"#),
        None
    );
    assert_eq!(
        release_asset_from_json(r#"{"assets":[{"name":"macro_clicker.exe","size":999999999999}]}"#),
        None
    );
    // A malformed digest is just an absent fingerprint.
    let asset = release_asset_from_json(
        r#"{"assets":[{"name":"macro_clicker.exe","size":1234,"digest":"sha256:not-hex"}]}"#,
    );
    assert_eq!(asset.and_then(|a| a.sha256), None);
}

#[test]
fn evaluate_release_couples_the_decision_with_the_asset() {
    let (outcome, asset) =
        evaluate_release(RELEASE_BODY_WITH_ASSETS, "1.2.0").expect("realistic body must evaluate");
    assert_eq!(outcome, UpdateOutcome::Available("1.3.0".to_string()));
    assert_eq!(asset.map(|a| a.size), Some(3_764_736));
    let parsed = parse_latest_release(RELEASE_BODY_WITH_ASSETS);
    assert_eq!(parsed.map(|r| r.version).as_deref(), Some("1.3.0"));
    assert_eq!(parse_latest_release("not json"), None);
}

#[test]
fn download_request_splits_the_validated_url() {
    assert_eq!(
        download_request_for_version("v1.3.0"),
        Some((
            "github.com",
            "/Skid-ster/autoclickerBYARENA/releases/download/v1.3.0/macro_clicker.exe".to_string()
        ))
    );
    assert_eq!(download_request_for_version("1.2.3.4"), None);
    assert_eq!(download_request_for_version("../../etc"), None);
}

#[test]
fn https_redirect_targets_are_strictly_gated() {
    assert_eq!(
        parse_https_url(
            "https://release-assets.githubusercontent.com/github-production-release-asset/1/abc?sp=r&sig=xyz"
        ),
        Some((
            "release-assets.githubusercontent.com".to_string(),
            "/github-production-release-asset/1/abc?sp=r&sig=xyz".to_string()
        ))
    );
    assert_eq!(
        parse_https_url("https://github.com"),
        Some(("github.com".to_string(), "/".to_string()))
    );
    assert_eq!(
        parse_https_url("https://github.com:443/a?b=c#frag"),
        Some(("github.com".to_string(), "/a?b=c".to_string()))
    );
    for bad in [
        "http://github.com/file.exe",
        "https://user:pass@github.com/file.exe",
        "https://github.com:8443/file.exe",
        "https://",
        "https:///file.exe",
        "ftp://github.com/file.exe",
        "https://git hub.com/file.exe",
        r"https://github.com\@evil.example/file.exe",
    ] {
        assert_eq!(parse_https_url(bad), None, "must reject {bad:?}");
    }
}

#[test]
fn download_progress_wording_is_bounded_and_honest() {
    assert_eq!(download_progress_label(0, 3_764_736), "0.0 of 3.6 MB (0%)");
    assert_eq!(
        download_progress_label(3_764_736, 3_764_736),
        "3.6 of 3.6 MB (100%)"
    );
    assert_eq!(
        download_progress_label(9_999_999, 3_764_736),
        "9.5 of 3.6 MB (100%)"
    );
    assert_eq!(download_progress_label(1_048_576, 0), "1.0 MB downloaded");
}

#[test]
fn byte_size_labels_use_the_same_units_as_progress() {
    assert_eq!(byte_size_label(0), "0 B");
    assert_eq!(byte_size_label(1023), "1023 B");
    assert_eq!(byte_size_label(1536), "1.5 KB");
    assert_eq!(byte_size_label(3_833_856), "3.7 MB");
}

#[test]
fn verification_failures_get_their_own_headline() {
    assert_eq!(
        error_headline("the download's SHA-256 fingerprint does not match"),
        "The download failed verification"
    );
    assert_eq!(
        error_headline("the downloaded file is not a Windows program this PC can run"),
        "The download failed verification"
    );
}
