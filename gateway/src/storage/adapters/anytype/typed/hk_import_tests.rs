//! Import mapping tests: pure mapper functions, then a two-pass import
//! against the local mock (one mock serves both spaces).

use super::hk_import;
use super::hk_map;
use crate::storage::adapters::anytype::tests::mock::Mock;

use crate::storage::adapters::anytype::client::AnytypeClient;
use crate::storage::adapters::anytype::schema::bootstrap;
use serde_json::{json, Value};

#[test]
fn scalar_value_normalizes_tokens_but_keeps_prose() {
    assert_eq!(hk_import::scalar_value("read-only"), "read_only");
    assert_eq!(hk_import::scalar_value("P1"), "p1");
    assert_eq!(hk_import::scalar_value("Doing"), "doing");
    assert_eq!(
        hk_import::scalar_value("Answers support email in French"),
        "Answers support email in French"
    );
}

#[test]
fn natural_key_prefers_the_type_key_then_falls_back() {
    let agent = json!({
        "id": "hk-agent-1",
        "name": "P",
        "properties": { "hermes_profile": "p", "slug": "wrong" },
    });
    assert_eq!(hk_import::natural_key("agent", &agent), "p");
    let group = json!({ "id": "hk-g", "name": "Support", "properties": { "slug": "support" } });
    assert_eq!(hk_import::natural_key("group", &group), "support");
    let nameless = json!({ "id": "hk-x", "properties": {} });
    assert_eq!(hk_import::natural_key("memory", &nameless), "hk-x");
}

#[test]
fn folded_json_merges_voice_and_routes() {
    let properties = json!({
        "voice_enabled": true,
        "tts_voice": "alloy",
        "profile_routes_default": "reviewer",
        "profile_route_targets": ["x"],
    })
    .as_object()
    .cloned()
    .unwrap();
    let folded = hk_import::folded_json(&properties);
    assert_eq!(folded.len(), 2);
    assert!(folded["m_voice_json"].is_object());
    assert!(folded["m_voice_json"].get("tts_voice").is_some());
    assert!(folded["m_profile_routes"].get("profile_routes_default").is_some());
}

#[test]
fn raw_remainder_keeps_unmapped_and_drops_mapped() {
    let properties = json!({
        "hermes_profile": "p",
        "mystery_field": "keep me",
        "autonomy": "read-only",
        "voice_enabled": true,
    })
    .as_object()
    .cloned()
    .unwrap();
    let raw = hk_import::raw_remainder(&properties, "agent");
    assert_eq!(raw.len(), 1);
    assert_eq!(raw["mystery_field"], "keep me");
}

#[test]
fn mapped_value_converts_by_format() {
    let rule = first_rule("group", "m_platform");
    assert_eq!(
        hk_import::mapped_value(&rule, &json!("Slack")).and_then(|v| v.as_str().map(String::from)),
        Some("slack".to_string())
    );
    let link_rule = first_rule("group", "m_members");
    assert_eq!(hk_import::mapped_value(&link_rule, &json!(["hk-1"])), None);
}

fn first_rule(hk_type: &str, prop: &str) -> hk_map::Rule {
    let rules = if hk_type == "agent" {
        hk_import::agent_rules()
    } else {
        hk_import::rules(hk_type)
    };
    rules.into_iter().find(|rule| rule.prop == prop).unwrap()
}

#[tokio::test]
async fn import_creates_then_updates_two_pass_links() {
    let mock = Mock::start().await;
    let bots = AnytypeClient::with_rate(&mock.url, "k", "space-bots", 10_000.0, 10_000.0).unwrap();
    let hk = AnytypeClient::with_rate(&mock.url, "k", "space-hk", 10_000.0, 10_000.0).unwrap();
    bootstrap(&bots, true).await.unwrap();

    // The HK org: a prompt, and two agents where one reports to the other.
    hk.create_object(
        &json!({
            "type": "prompt",
            "name": "QA prompt",
            "properties": { "prompt_kind": "system" },
        }),
        "hk-prompt",
    )
    .await
    .unwrap();
    let p = hk
        .create_object(
            &json!({
                "type": "agent",
                "name": "P",
                "properties": { "hermes_profile": "p", "autonomy": "read-only" },
            }),
            "hk-agent-p",
        )
        .await
        .unwrap();
    let p_id = p["id"].as_str().unwrap().to_string();
    hk.create_object(
        &json!({
            "type": "agent",
            "name": "Blanche",
            "properties": { "hermes_profile": "blanche", "reports_to": p_id },
        }),
        "hk-agent-b",
    )
    .await
    .unwrap();

    let report = hk_import::run(&bots, &hk, true).await;
    let unexpected: Vec<&Value> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| !e.as_str().unwrap_or("").contains("not found"))
        .collect();
    assert!(unexpected.is_empty(), "{report}");
    assert_eq!(report["created"].as_array().unwrap().len(), 3);

    let rows = bots.search(&json!({ "type": "m_bot" })).await.unwrap();
    for row in &rows {
        let id = row["id"].as_str().unwrap().to_string();
        let object = bots.get_object(&id).await.unwrap();
        let props = crate::storage::adapters::anytype::typed::mapper::properties_at(&object)
            .cloned()
            .unwrap_or_default();
        if props.get("m_slug") == Some(&json!("blanche")) {
            assert!(props.contains_key("m_source_id"));
        }
        if props.get("m_slug") == Some(&json!("p")) {
            assert_eq!(props.get("m_autonomy"), Some(&json!("read_only")));
        }
    }
    assert_eq!(report["unresolved"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn import_rerun_updates_instead_of_duplicating() {
    let mock = Mock::start().await;
    let bots = AnytypeClient::with_rate(&mock.url, "k", "space-bots", 10_000.0, 10_000.0).unwrap();
    let hk = AnytypeClient::with_rate(&mock.url, "k", "space-hk", 10_000.0, 10_000.0).unwrap();
    bootstrap(&bots, true).await.unwrap();
    hk.create_object(
        &json!({
            "type": "agent",
            "name": "P",
            "properties": { "hermes_profile": "p" },
        }),
        "hk-agent-p",
    )
    .await
    .unwrap();
    hk_import::run(&bots, &hk, true).await;
    hk_import::run(&bots, &hk, true).await;
    let rows = bots.search(&json!({ "type": "m_bot" })).await.unwrap();
    assert_eq!(rows.len(), 1, "a re-run must update, not duplicate");
}
