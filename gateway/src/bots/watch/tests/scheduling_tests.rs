//! The poller's schedule: backoff, the interval floor, and the 24-hour disable.

use super::super::*;
use super::*;

#[test]
fn backoff_doubles_up_to_one_hour_and_never_shrinks_below_the_interval() {
    assert_eq!(poller::backoff_ms(60_000, 1), 120_000);
    assert_eq!(poller::backoff_ms(60_000, 7), poller::BACKOFF_MAX_MS);
    assert_eq!(poller::backoff_ms(60_000, 40), poller::BACKOFF_MAX_MS);
    assert_eq!(
        poller::backoff_ms(7_200_000, 3),
        7_200_000,
        "floored at the interval"
    );
}

#[test]
fn the_next_run_is_at_least_the_interval_away() {
    let now = 1_000_000;
    for failures in [0u64, 3, 9] {
        assert!(poller::next_run_at(60_000, failures, now, 0) >= now + 60_000);
    }
    assert!(poller::next_run_ms(60_000, 0, now) >= now + 60_000);
    assert!(
        poller::next_run_ms(60_000, 2, now) <= now + poller::BACKOFF_MAX_MS + poller::JITTER_MS
    );
}

#[test]
fn a_watch_fails_too_long_only_after_a_full_day_of_failing() {
    let watch = watch_with(
        &mk_check("hash", json!({})),
        fire("on_change", 60),
        json!({
            "failures": 6, "failingSince": 1_000,
        }),
    );
    assert!(!poller::failing_too_long(
        &watch,
        1_000 + poller::DISABLE_AFTER_MS - 1
    ));
    assert!(poller::failing_too_long(
        &watch,
        1_000 + poller::DISABLE_AFTER_MS
    ));
    let recovered = watch_with(
        &mk_check("hash", json!({})),
        fire("on_change", 60),
        json!({
            "failures": 0, "failingSince": 0,
        }),
    );
    assert!(!poller::failing_too_long(&recovered, 9_000_000_000));
    let mut paused = watch.clone();
    paused["enabled"] = json!(false);
    assert!(
        !poller::failing_too_long(&paused, 9_000_000_000),
        "already paused stays paused"
    );
}

#[test]
fn a_watch_row_carries_kind_watch_and_the_disable_note() {
    let mut watch = watch_with(
        &mk_check("hash", json!({})),
        fire("on_change", 60),
        json!({
            "failures": 7, "lastError": "Disabled after 24 hours of failed checks.",
        }),
    );
    watch["pausedReason"] = json!("twenty-four hours of failed checks");
    watch["enabled"] = json!(false);
    let row = row(&watch);
    assert_eq!(row["kind"], "watch");
    assert_eq!(row["paused"], true);
    assert_eq!(
        row["lastError"],
        "Disabled after 24 hours of failed checks."
    );
    assert_eq!(row["target"], "example.com");
}
