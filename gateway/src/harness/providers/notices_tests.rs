use super::*;
use serde_json::json;

fn window(id: &str, percent: f64, resets: &str) -> Value {
    json!({
        "id": id,
        "kind": "session",
        "label": id,
        "usedPercent": percent,
        "resetsAt": resets
    })
}

fn provider(id: &str, label: &str, state: &str, windows: Vec<Value>) -> Value {
    json!({
        "instanceId": id,
        "label": label,
        "state": state,
        "windows": windows,
        "note": "",
        "credentialFingerprint": "fp-secret-9f3a",
        "email": "person@example.com",
        "resetCredits": [{ "id": "credit-secret-11" }]
    })
}

fn models(rows: &[(&str, &str)]) -> Value {
    json!({
        "providers": rows.iter().map(|(id, slug)| json!({
            "instanceId": id,
            "models": [{ "slug": slug }]
        })).collect::<Vec<_>>()
    })
}

fn use_of(slug: &str, instance: &str) -> BotUse {
    BotUse {
        slug: slug.into(),
        instance_id: instance.into(),
    }
}

fn run(
    book: &[Record],
    bots: &[BotUse],
    limits: &[Value],
    model_rows: &[(&str, &str)],
) -> (Vec<Record>, Vec<Record>) {
    evaluate(
        book,
        bots,
        limits,
        &models(model_rows),
        "2026-11-12T14:00:00Z",
    )
}

fn levels(records: &[Record]) -> Vec<(String, u8)> {
    records
        .iter()
        .map(|record| (record.slug.clone(), record.threshold))
        .collect()
}

fn leaks(value: &Value) -> bool {
    let text = value.to_string();
    ["fp-secret-9f3a", "person@example.com", "credit-secret-11"]
        .iter()
        .any(|secret| text.contains(secret))
}

#[test]
fn crossing_80_warns_once_until_usage_falls() {
    let limits = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 82.0, "2026-11-12T17:00:00Z")],
    )];
    let bots = [use_of("ada", "claude")];
    let (kept, emitted) = run(&[], &bots, &limits, &[("claude", "haiku")]);
    assert_eq!(levels(&emitted), vec![("ada".into(), 80)]);
    let (again, quiet) = run(&kept, &bots, &limits, &[("claude", "haiku")]);
    assert!(quiet.is_empty());
    assert_eq!(again[0].at, "2026-11-12T14:00:00Z");

    let low = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 70.0, "2026-11-12T17:00:00Z")],
    )];
    let (cleared, none) = run(&again, &bots, &low, &[("claude", "haiku")]);
    assert!(none.is_empty());
    assert!(cleared.is_empty());
    let (back, warned) = run(&cleared, &bots, &limits, &[("claude", "haiku")]);
    assert_eq!(levels(&warned), vec![("ada".into(), 80)]);
    assert_eq!(back.len(), 1);
}

#[test]
fn crossing_95_and_a_drop_to_85_keeps_only_80() {
    let high = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 96.0, "2026-11-12T17:00:00Z")],
    )];
    let bots = [use_of("ada", "claude")];
    let (kept, emitted) = run(&[], &bots, &high, &[("claude", "haiku")]);
    assert_eq!(
        levels(&emitted),
        vec![("ada".into(), 80), ("ada".into(), 95)]
    );
    let mid = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 85.0, "2026-11-12T17:00:00Z")],
    )];
    let (left, quiet) = run(&kept, &bots, &mid, &[("claude", "haiku")]);
    assert!(quiet.is_empty());
    assert_eq!(levels(&left), vec![("ada".into(), 80)]);
}

#[test]
fn a_new_window_warns_again() {
    let first = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 82.0, "2026-11-12T17:00:00Z")],
    )];
    let bots = [use_of("ada", "claude")];
    let (kept, _) = run(&[], &bots, &first, &[("claude", "haiku")]);
    let reset = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 82.0, "2026-11-12T22:00:00Z")],
    )];
    let (_, emitted) = run(&kept, &bots, &reset, &[("claude", "haiku")]);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].resets_at, "2026-11-12T22:00:00Z");
}

#[test]
fn two_bots_each_get_one_notice_and_a_missing_bot_is_dropped() {
    let limits = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 81.0, "2026-11-12T17:00:00Z")],
    )];
    let bots = [use_of("ada", "claude"), use_of("bea", "claude")];
    let (kept, emitted) = run(&[], &bots, &limits, &[("claude", "haiku")]);
    assert_eq!(emitted.len(), 2);
    let left = [use_of("bea", "claude")];
    let (next, quiet) = run(&kept, &left, &limits, &[("claude", "haiku")]);
    assert!(quiet.is_empty());
    assert_eq!(levels(&next), vec![("bea".into(), 80)]);
}

#[test]
fn no_windows_means_no_notice() {
    let limits = vec![provider("claude", "Claude", "ok", vec![])];
    let (_, emitted) = run(
        &[],
        &[use_of("ada", "claude")],
        &limits,
        &[("claude", "haiku")],
    );
    assert!(emitted.is_empty());
}

#[test]
fn the_alternative_is_the_first_ready_provider_under_70() {
    let limits = vec![
        provider(
            "claude",
            "Claude",
            "ok",
            vec![window("5h", 96.0, "2026-11-12T17:00:00Z")],
        ),
        provider("busy", "Busy", "ok", vec![window("5h", 80.0, "")]),
        provider("away", "Away", "signed-out", vec![]),
        provider("codex", "Codex", "ok", vec![window("5h", 12.0, "")]),
    ];
    let (_, emitted) = run(
        &[],
        &[use_of("ada", "claude")],
        &limits,
        &[("claude", "haiku"), ("codex", "gpt")],
    );
    let top = emitted
        .iter()
        .find(|record| record.threshold == 95)
        .unwrap();
    assert_eq!(top.alternative_instance, "codex");
    assert_eq!(top.alternative_model, "gpt");
    assert!(!leaks(&serde_json::to_value(&emitted).unwrap()));
}

#[test]
fn an_only_provider_has_no_alternative() {
    let limits = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 96.0, "")],
    )];
    let (_, emitted) = run(
        &[],
        &[use_of("ada", "claude")],
        &limits,
        &[("claude", "haiku")],
    );
    assert!(emitted
        .iter()
        .all(|record| record.alternative_instance.is_empty()));
}

#[test]
fn equal_windows_keep_the_first() {
    let limits = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![
            window("session", 80.0, "2026-11-12T17:00:00Z"),
            window("weekly", 80.0, "2026-11-19T00:00:00Z"),
        ],
    )];
    let (_, emitted) = run(
        &[],
        &[use_of("ada", "claude")],
        &limits,
        &[("claude", "haiku")],
    );
    assert_eq!(emitted[0].window_id, "session");
}

#[test]
fn switching_away_keeps_the_old_notice_quiet_until_it_is_current_again() {
    let limits = vec![
        provider(
            "claude",
            "Claude",
            "ok",
            vec![window("5h", 90.0, "2026-11-12T17:00:00Z")],
        ),
        provider("codex", "Codex", "ok", vec![]),
    ];
    let bots = [use_of("ada", "claude")];
    let (kept, _) = run(
        &[],
        &bots,
        &limits,
        &[("claude", "haiku"), ("codex", "gpt")],
    );
    let away = [use_of("ada", "codex")];
    let (held, quiet) = run(
        &kept,
        &away,
        &limits,
        &[("claude", "haiku"), ("codex", "gpt")],
    );
    assert!(quiet.is_empty());
    assert_eq!(held[0].instance_id, "claude");
    let (_, again) = run(
        &held,
        &bots,
        &limits,
        &[("claude", "haiku"), ("codex", "gpt")],
    );
    assert!(again.is_empty());
}

#[test]
fn attach_puts_the_button_on_the_higher_threshold_only() {
    let limits = vec![
        provider(
            "claude",
            "Claude",
            "ok",
            vec![window("5h", 96.0, "2026-11-12T17:00:00Z")],
        ),
        provider("codex", "Codex", "ok", vec![]),
    ];
    let (kept, _) = run(
        &[],
        &[use_of("ada", "claude")],
        &limits,
        &[("claude", "haiku"), ("codex", "gpt")],
    );
    let mut view = json!({
        "messages": [{ "ordinal": 4 }],
        "events": [{ "ordinal": 2 }]
    });
    attach(&mut view, &kept, "ada", "claude");
    let events = view["events"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[1]["percent"], 80);
    assert_eq!(events[1]["alternativeModel"], "");
    assert_eq!(events[2]["percent"], 95);
    assert_eq!(events[2]["alternativeInstance"], "codex");
    assert_eq!(events[2]["alternativeModel"], "gpt");
    assert_eq!(events[1]["ordinal"], 5);
    assert_eq!(events[2]["ordinal"], 6);
    assert_eq!(events[1]["id"], "limit-ada-80-5h");
    assert!(!leaks(&view));
    attach(&mut view, &kept, "ada", "codex");
    assert_eq!(view["events"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn a_restart_does_not_send_the_same_notice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limit-notices.json");
    let limits = vec![provider(
        "claude",
        "Claude",
        "ok",
        vec![window("5h", 82.0, "2026-11-12T17:00:00Z")],
    )];
    let bots = [use_of("ada", "claude")];
    let models = models(&[("claude", "haiku")]);
    Book::open(path.clone())
        .sync(&bots, &limits, &models, "2026-11-12T14:00:00Z")
        .await;
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("fp-secret-9f3a"));
    assert!(saved.contains("\"threshold\": 80"));
    Book::open(path.clone())
        .sync(&bots, &limits, &models, "2026-11-12T15:00:00Z")
        .await;
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
}

#[test]
fn bot_uses_follow_the_running_model() {
    let bots = [json!({
        "slug": "ada",
        "model": { "instanceId": "claude", "model": "haiku" }
    })];
    let uses = bot_uses(&bots, &Value::Null);
    assert_eq!(uses[0].instance_id, "claude");
}
