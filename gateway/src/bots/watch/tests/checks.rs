//! The evaluators: each check type, both directions, and the fire modes.

use super::super::*;
use super::*;

#[test]
fn hash_fires_when_the_body_changes() {
    let check = mk_check("hash", json!({}));
    let first = evaluate_ok(&check, b"one", 200);
    let second = evaluate_ok(&check, b"two", 200);
    assert!(evaluate::outcome(&fire("on_change", 60), &check, &first, &fresh_state()).is_some());
    assert_none(evaluate::outcome(
        &fire("on_change", 60),
        &check,
        &first,
        &blank_state(),
    ));
    let after = json!({ "last": first.value, "lastTrue": Value::Null, "lastAt": "x" });
    assert!(evaluate::outcome(&fire("on_change", 60), &check, &second, &after).is_some());
    assert_none(evaluate::outcome(
        &fire("on_change", 60),
        &check,
        &second,
        &json!({ "last": second.value, "lastAt": "x" }),
    ));
}

#[test]
fn status_fires_when_the_class_changes() {
    let check = mk_check("status", json!({}));
    let down = evaluate_ok(&check, b"", 503);
    assert_eq!(down.value, "5xx");
    assert!(evaluate::outcome(
        &fire("on_change", 60),
        &check,
        &down,
        &json!({ "last": "2xx", "lastAt": "x" })
    )
    .is_some());
    assert_none(evaluate::outcome(
        &fire("on_change", 60),
        &check,
        &down,
        &json!({ "last": "5xx", "lastAt": "x" }),
    ));
}

#[test]
fn json_pointer_ops_compare_as_text_and_numbers() {
    let body = br#"{"state":"pass","attempts":3}"#;
    let changes = mk_check(
        "json",
        json!({ "type": "json", "pointer": "/state", "op": "changes" }),
    );
    let eval = evaluate_ok(&changes, body, 200);
    assert_eq!(eval.value, "pass");
    assert!(eval.truth.is_none());
    assert!(evaluate::outcome(
        &fire("on_change", 60),
        &changes,
        &eval,
        &json!({ "last": "fail", "lastAt": "x" })
    )
    .is_some());
    let equals = mk_check(
        "json",
        json!({ "type": "json", "pointer": "/state", "op": "equals", "value": "pass" }),
    );
    assert_eq!(evaluate_ok(&equals, body, 200).truth, Some(true));
    let not_equals = mk_check(
        "json",
        json!({ "type": "json", "pointer": "/state", "op": "not_equals", "value": "fail" }),
    );
    assert_eq!(evaluate_ok(&not_equals, body, 200).truth, Some(true));
    let gt = mk_check(
        "json",
        json!({ "type": "json", "pointer": "/attempts", "op": "gt", "value": 2 }),
    );
    assert_eq!(evaluate_ok(&gt, body, 200).truth, Some(true));
    let lt = mk_check(
        "json",
        json!({ "type": "json", "pointer": "/attempts", "op": "lt", "value": 2 }),
    );
    assert_eq!(evaluate_ok(&lt, body, 200).truth, Some(false));
    // A body that is not JSON at all is a poll failure, not a silent pass.
    assert!(evaluate::evaluate(&equals, 200, b"<html>").is_err());
}

#[test]
fn on_true_fires_only_on_the_false_to_true_edge() {
    let check = mk_check(
        "contains",
        json!({ "type": "contains", "substring": "BROKEN" }),
    );
    let broken = evaluate_ok(&check, b"page is BROKEN", 200);
    let fine = evaluate_ok(&check, b"page is fine", 200);
    let on_true = fire("on_true", 60);
    // false becomes true: fire.
    assert!(evaluate::outcome(
        &on_true,
        &check,
        &broken,
        &json!({ "lastTrue": false, "lastAt": "x" })
    )
    .is_some());
    // true stays true, false stays false: no refire.
    assert_none(evaluate::outcome(
        &on_true,
        &check,
        &broken,
        &json!({ "lastTrue": true, "lastAt": "x" }),
    ));
    assert_none(evaluate::outcome(
        &on_true,
        &check,
        &fine,
        &json!({ "lastTrue": false, "lastAt": "x" }),
    ));
    assert_none(evaluate::outcome(
        &on_true,
        &check,
        &fine,
        &json!({ "lastTrue": true, "lastAt": "x" }),
    ));
}

#[test]
fn every_mode_fires_on_each_matching_poll() {
    let check = mk_check(
        "regex",
        json!({ "type": "regex", "regex": "deploy [a-z]+" }),
    );
    let matched = evaluate_ok(&check, b"deploy one", 200);
    let every = fire("every", 60);
    assert!(evaluate::outcome(
        &every,
        &check,
        &matched,
        &json!({ "lastTrue": false, "lastAt": "x" })
    )
    .is_some());
    assert!(evaluate::outcome(
        &every,
        &check,
        &matched,
        &json!({ "lastTrue": true, "lastAt": "x" })
    )
    .is_some());
    let unmatched = evaluate_ok(&check, b"nothing here", 200);
    assert_none(evaluate::outcome(
        &every,
        &check,
        &unmatched,
        &json!({ "lastTrue": true, "lastAt": "x" }),
    ));
    // A matchless check with every behaves as on_change: without a boolean there
    // is no match to repeat, only a change to report.
    let plain = mk_check("hash", json!({}));
    let hash_hit = evaluate_ok(&plain, b"body", 200);
    assert!(evaluate::outcome(
        &every,
        &plain,
        &hash_hit,
        &json!({ "last": "old", "lastAt": "x" })
    )
    .is_some());
    assert_none(evaluate::outcome(
        &every,
        &plain,
        &hash_hit,
        &json!({ "last": hash_hit.value, "lastAt": "x" }),
    ));
}

#[test]
fn snippets_shrink_small_and_stay_on_char_boundaries() {
    let text = "é".repeat(1200);
    let cut = evaluate::snippet(&text);
    assert!(cut.chars().count() <= evaluate::SNIPPET_MAX / 2 + 2);
    assert!(cut.chars().all(|c| c == 'é' || c == '…'));
}

#[test]
fn json_pointers_walk_objects_and_arrays() {
    let doc = json!({ "a": { "b": [10, 20] } });
    assert_eq!(evaluate::pointer_value(&doc, "/a/b/1"), Some(json!(20)));
    assert_eq!(evaluate::pointer_value(&doc, "/a/missing"), None);
    assert_eq!(evaluate::pointer_value(&doc, "/"), None);
}
