use super::*;
use crate::harness::t3::transcript::{
    tokens_from_projection, turn_token_line, turns_from_projection,
};

#[test]
fn dates_are_utc_days() {
    assert_eq!(day_key(0), "1970-01-01");
    assert_eq!(day_key(86_400), "1970-01-02");
    assert_eq!(day_key(1_582_934_400), "2020-02-29");
    assert_eq!(day_key(1_794_268_800), "2026-11-10");
}

#[test]
fn counts_add_up_and_old_days_drop() {
    let mut doc = json!({ "version": 1, "days": {} });
    record(&mut doc, "2026-11-10", "messages", 3);
    record(&mut doc, "2026-11-10", "messages", 3);
    record(&mut doc, "2026-11-09", "vaultWrites", 3);
    assert_eq!(doc["days"]["2026-11-10"]["messages"], 2);
    let view = with_totals(doc.clone());
    assert_eq!(view["totals"]["messages"], 2);
    assert_eq!(view["totals"]["vaultWrites"], 1);
    for n in 0..5 {
        record(&mut doc, &format!("2026-01-{n:02}"), "messages", 2);
    }
    assert_eq!(doc["days"].as_object().unwrap().len(), 2);
    assert!(doc["days"].get("2026-01-00").is_none());
}

#[tokio::test]
async fn a_note_lands_on_disk_and_a_broken_file_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage.json");
    let usage = Usage::new(path.clone());
    usage
        .note_at_for("", Event::Message, 1_794_268_800)
        .await
        .unwrap();
    usage
        .note_at_for("", Event::IconFetch, 1_794_268_800)
        .await
        .unwrap();
    let snap = usage.snapshot().await.unwrap();
    assert_eq!(snap["days"]["2026-11-10"]["messages"], 1);
    assert_eq!(snap["totals"]["iconFetches"], 1);
    tokio::fs::write(&path, "{").await.unwrap();
    assert!(usage.note_at_for("", Event::Message, 0).await.is_err());
    assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "{");
}

#[test]
fn one_bots_messages_do_not_land_on_another() {
    let mut doc = json!({ "version": 1, "days": {} });
    record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
    record_for(&mut doc, "2026-11-10", "ada", "attachments", 400);
    record_for(&mut doc, "2026-11-09", "milo", "messages", 400);
    assert_eq!(bot_counts(&doc, "ada"), (1, 1));
    assert_eq!(bot_counts(&doc, "milo"), (1, 0));
    assert_eq!(bot_counts(&doc, "missing"), (0, 0));
    let view = with_totals(doc);
    assert_eq!(view["totals"]["messages"], 2);
    assert_eq!(view["totals"]["attachments"], 1);
    assert!(view["totals"].get("bots").is_none());
}

#[test]
fn tokens_sum_finished_turns_and_skip_what_t3_cannot_price() {
    let projection = json!({
        "providerTurns": [
            { "turnTokenUsage": { "inputTokens": 10, "outputTokens": 4, "cachedInputTokens": 3, "cacheCreationTokens": 1 } },
            { "turnTokenUsage": { "usageStatus": "unavailable", "inputTokens": 99, "outputTokens": 99 } },
            { "turnTokenUsage": { "localCommand": true, "inputTokens": 50, "outputTokens": 50 } },
            { "completedAt": "2026-11-10T00:00:00Z" }
        ]
    });
    let (totals, known) = tokens_from_projection(&projection);
    assert!(known);
    assert_eq!(totals.turns, 1);
    assert_eq!(totals.input, 10);
    assert_eq!(totals.output, 4);
    assert_eq!(totals.cached, 3);
    assert_eq!(totals.cache_write, 1);
    let (empty, seen) = tokens_from_projection(&json!({ "providerTurns": [] }));
    assert!(!seen);
    assert_eq!(empty, TokenTotals::default());
    let body = bot_payload(2, 1, &totals, true, "");
    assert_eq!(body["messages"], 2);
    assert_eq!(body["tokensKnown"], true);
    assert_eq!(body["error"], "");
}

#[test]
fn a_token_line_matches_one_turn_only_when_the_ids_match() {
    let projection = json!({
        "providerTurns": [
            {
                "id": "m2",
                "turnTokenUsage": {
                    "inputTokens": 10,
                    "outputTokens": 2,
                    "cachedInputTokens": 4,
                    "cacheCreationTokens": 1
                }
            },
            {
                "id": "other",
                "turnTokenUsage": {
                    "inputTokens": 99,
                    "outputTokens": 99,
                    "cachedInputTokens": 99,
                    "cacheCreationTokens": 99
                }
            },
            {
                "id": "bad",
                "turnTokenUsage": {
                    "usageStatus": "unavailable",
                    "inputTokens": 5,
                    "outputTokens": 5,
                    "cachedInputTokens": 5,
                    "cacheCreationTokens": 5
                }
            },
            {
                "messageId": "local",
                "turnTokenUsage": {
                    "localCommand": true,
                    "inputTokens": 7,
                    "outputTokens": 7,
                    "cachedInputTokens": 1,
                    "cacheCreationTokens": 1
                }
            },
            {
                "turnTokenUsage": {
                    "inputTokens": 3,
                    "outputTokens": 3,
                    "cachedInputTokens": 0,
                    "cacheCreationTokens": 0
                }
            }
        ]
    });
    let line = turn_token_line(&projection, "m2").unwrap();
    assert_eq!(line["inputTokens"], 10);
    assert_eq!(line["outputTokens"], 2);
    assert_eq!(line["cachedInputTokens"], 4);
    assert_eq!(line["cacheCreationTokens"], 1);
    assert_ne!(line["inputTokens"].as_u64().unwrap(), 10 + 4);
    assert!(turn_token_line(&projection, "other").unwrap()["inputTokens"] == 99);
    assert!(turn_token_line(&projection, "no-such-id").is_none());
    assert!(turn_token_line(&projection, "bad").is_none());
    assert!(turn_token_line(&projection, "local").is_none());
    assert!(turn_token_line(&projection, "").is_none());
}

#[tokio::test]
async fn a_missing_file_does_not_invent_counts() {
    let dir = tempfile::tempdir().unwrap();
    let usage = Usage::new(dir.path().join("missing.json"));
    let snap = usage.snapshot().await.unwrap();
    assert_eq!(snap["days"], json!({}));
    assert!(snap["totals"].as_object().unwrap().is_empty());
    assert!(snap["totals"].get("messages").is_none());
}

fn fixture_turns() -> Vec<Turn> {
    let f: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/t3/thread-projection.json"
    ))
    .unwrap();
    turns_from_projection(&f["value"])
}

#[test]
fn iso_times_parse_to_milliseconds() {
    assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_iso_ms("1970-01-02T00:00:01.250Z"), Some(86_401_250));
    assert_eq!(
        parse_iso_ms("2026-11-10T08:23:56.048Z"),
        Some(1_794_299_036_048)
    );
    assert_eq!(parse_iso_ms("nope"), None);
}

#[test]
fn a_real_thread_gives_three_turns_with_a_time_and_a_model() {
    let turns = fixture_turns();
    assert_eq!(turns.len(), 3);
    assert!(turns
        .iter()
        .all(|t| t.model == "claude-opus-5-5" && t.at_ms > 0 && t.input > 0));
}

#[test]
fn a_period_counts_the_persons_days_and_fills_the_empty_ones() {
    let turns = vec![
        Turn {
            at_ms: parse_iso_ms("2026-11-10T23:30:00Z").unwrap(),
            model: "a".into(),
            input: 100,
            output: 10,
            cached: 60,
            cache_write: 5,
        },
        Turn {
            at_ms: parse_iso_ms("2026-11-04T12:00:00Z").unwrap(),
            model: "b".into(),
            input: 50,
            output: 5,
            cached: 0,
            cache_write: 0,
        },
        Turn {
            at_ms: parse_iso_ms("2026-10-01T12:00:00Z").unwrap(),
            model: "a".into(),
            input: 1000,
            output: 100,
            cached: 0,
            cache_write: 0,
        },
    ];
    let now = parse_iso_ms("2026-11-10T23:45:00Z").unwrap();
    let none = std::collections::BTreeMap::new();
    // UTC: all three turns are on their own days; "today" holds one.
    let today = report(&turns, &none, period_days("today", now, 0).unwrap(), 0);
    assert_eq!(today["totals"]["turns"], 1);
    assert_eq!(today["series"].as_array().unwrap().len(), 1);
    // The week: 7 days filled, 2 turns (the October one is out).
    let week = report(&turns, &none, period_days("7d", now, 0).unwrap(), 0);
    assert_eq!(week["series"].as_array().unwrap().len(), 7);
    assert_eq!(week["totals"]["turns"], 2);
    assert_eq!(week["totals"]["tokens"], 165);
    assert_eq!(week["byModel"][0]["model"], "a");
    // Someone two hours east is already on the 11th: the 23:30 turn is their tomorrow-today.
    let east = report(&turns, &none, period_days("today", now, 120).unwrap(), 120);
    assert_eq!(east["series"][0]["day"], "2026-11-11");
    assert_eq!(east["totals"]["turns"], 1);
    // All time: only days with data, oldest first.
    let all = report(&turns, &none, period_days("all", now, 0).unwrap(), 0);
    assert_eq!(all["totals"]["turns"], 3);
    let days: Vec<&str> = all["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["day"].as_str().unwrap())
        .collect();
    assert_eq!(days, ["2026-10-01", "2026-11-04", "2026-11-10"]);
    assert!(period_days("century", now, 0).is_none());
}

#[test]
fn messages_and_attachments_join_the_series_by_day() {
    let mut doc = json!({ "version": 1, "days": {} });
    record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
    record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
    record_for(&mut doc, "2026-11-10", "ada", "attachments", 400);
    record_for(&mut doc, "2026-11-10", "bob", "messages", 400);
    let ada = activity_by_day(&doc, "ada");
    let now = parse_iso_ms("2026-11-10T12:00:00Z").unwrap();
    let r = report(&[], &ada, period_days("7d", now, 0).unwrap(), 0);
    assert_eq!(
        (
            r["totals"]["messages"].as_u64(),
            r["totals"]["attachments"].as_u64()
        ),
        (Some(2), Some(1))
    );
    assert_eq!(r["series"][6]["messages"], 2);
    let machine = activity_by_day(&doc, "");
    assert_eq!(
        report(&[], &machine, period_days("today", now, 0).unwrap(), 0)["totals"]["messages"],
        3
    );
}
