use super::*;

#[test]
pub(super) fn versions_compare_like_hbots() {
    let ord = |a, b| compare_versions(a, b).unwrap();
    assert_eq!(ord("1.2.3", "1.2.3"), Ordering::Equal);
    assert_eq!(ord("1.10.0", "1.9.9"), Ordering::Greater);
    assert_eq!(ord("v2.0.0", "1.99.0"), Ordering::Greater);
    assert_eq!(ord("1.0.0-preview.1", "1.0.0"), Ordering::Less);
    assert_eq!(ord("1.0.0", "1.0.0-preview.9"), Ordering::Greater);
    assert_eq!(ord("1.0.0-preview.2", "1.0.0-preview.10"), Ordering::Less);
    assert_eq!(ord("1.0.0-alpha", "1.0.0-1"), Ordering::Greater);
    assert!(compare_versions("nope", "1.0.0").is_err());
    assert!(compare_versions("1.2", "1.0.0").is_err());
    assert!(compare_versions("1.2.3.4", "1.0.0").is_err());
}

#[test]
pub(super) fn preview_channel_follows_the_version() {
    assert!(is_preview("1.0.0-preview.1"));
    assert!(!is_preview("1.0.0"));
}

#[test]
pub(super) fn sums_are_found_by_exact_name() {
    let hash = "a".repeat(64);
    let sums = format!("{hash}  hbots-1.0.0-x86_64-linux.tar.gz\n{} *hbots-1.0.0-aarch64-macos.tar.gz\nnot a line\n", "B".repeat(64));
    assert_eq!(
        expected_sum(&sums, "hbots-1.0.0-x86_64-linux.tar.gz"),
        Some(hash)
    );
    assert_eq!(
        expected_sum(&sums, "hbots-1.0.0-aarch64-macos.tar.gz"),
        Some("b".repeat(64))
    );
    assert_eq!(expected_sum(&sums, "hbots-1.0.0-x86_64-linux.tar"), None);
    assert_eq!(
        expected_sum(
            &format!("{} hbots-x.tar.gz", "c".repeat(63)),
            "hbots-x.tar.gz"
        ),
        None
    );
}

#[test]
pub(super) fn asset_names() {
    assert_eq!(
        asset_name("1.2.3", "aarch64-macos"),
        "hbots-1.2.3-aarch64-macos.tar.gz"
    );
}

pub(super) fn release_list() -> Vec<Value> {
    vec![
        json!({ "tag_name": "v1.1.0", "prerelease": false, "draft": false, "body": "notes", "html_url": "https://example.test/r/1.1.0",
                "assets": [ { "name": "hbots-1.1.0-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/a.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/s" } ] }),
        json!({ "tag_name": "v1.2.0-preview.1", "prerelease": true, "draft": false,
                "assets": [ { "name": "hbots-1.2.0-preview.1-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/p.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/ps" } ] }),
        json!({ "tag_name": "v1.3.0", "prerelease": false, "draft": true, "assets": [] }),
        json!({ "tag_name": "v1.0.0", "prerelease": false, "draft": false,
                "assets": [ { "name": "hbots-1.0.0-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/old.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/olds" } ] }),
    ]
}

#[test]
pub(super) fn newest_stable_release_is_picked_and_drafts_and_previews_skipped() {
    let r = pick_release(&release_list(), None, false, None, "x86_64-linux").unwrap();
    assert_eq!(r.version, "1.1.0");
    assert_eq!(r.archive, "https://dl.test/a.tgz");
    assert_eq!(r.sums, "https://dl.test/s");
    assert_eq!(r.notes, "notes");
    let preview = pick_release(&release_list(), None, true, None, "x86_64-linux").unwrap();
    assert_eq!(preview.version, "1.2.0-preview.1");
    assert!(pick_release(&release_list(), Some("1.3.0"), false, None, "x86_64-linux").is_none());
    assert_eq!(
        pick_release(&release_list(), Some("v1.0.0"), false, None, "x86_64-linux")
            .unwrap()
            .version,
        "1.0.0"
    );
}

#[test]
pub(super) fn a_download_base_builds_the_urls() {
    let r = pick_release(
        &release_list(),
        Some("1.1.0"),
        false,
        Some("https://mirror.test/x"),
        "x86_64-linux",
    )
    .unwrap();
    assert_eq!(
        r.archive,
        "https://mirror.test/x/v1.1.0/hbots-1.1.0-x86_64-linux.tar.gz"
    );
    assert_eq!(r.sums, "https://mirror.test/x/v1.1.0/SHA256SUMS");
}

#[test]
pub(super) fn version_comparison_and_iso_time() {
    assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
    assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
    for secs in [0u64, 1_700_000_000, 951_782_400, 1_234_567_890] {
        assert_eq!(chrono_free_ms(&iso8601(secs)), Some(secs * 1000));
    }
    assert_eq!(chrono_free_ms("yesterday"), None);
}

#[test]
pub(super) fn backoff_grows_and_stops_at_the_cap() {
    assert_eq!(backoff(0), CHECK_TTL);
    assert_eq!(backoff(1), CHECK_TTL * 2);
    assert_eq!(backoff(40), MAX_BACKOFF);
}
