use super::*;
use crate::bots::helpers_policy::{
    default_child_threads, default_subagents, HELPERS_OFF, MODEL_DENIED, NO_POLICY,
};

fn world() -> (tempfile::TempDir, Bots) {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    (dir, bots)
}

fn raw(dir: &std::path::Path, slug: &str) -> Value {
    let path = dir.join(slug).join(".bot").join("bot.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn selection(model: &str, effort: &str) -> Value {
    json!({
        "instanceId": "claudeAgent",
        "model": model,
        "options": [{ "id": "effort", "value": effort }]
    })
}

fn model_ref(model: &str) -> Value {
    json!({ "instanceId": "claudeAgent", "model": model })
}

async fn ada(bots: &Bots) -> String {
    bots.create(&json!({ "name": "Ada" })).await.unwrap()["slug"]
        .as_str()
        .unwrap()
        .to_string()
}

fn status(err: ApiError) -> (u16, String) {
    (err.status.as_u16(), err.message)
}

#[tokio::test]
async fn u1_missing_policy_keys_resolve_to_the_defaults() {
    let (dir, bots) = world();
    let slug = ada(&bots).await;
    let loaded = bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(loaded["subagents"], default_subagents());
    assert_eq!(loaded["childThreads"], default_child_threads());
    let listed = bots.list().await.unwrap();
    let row = listed.iter().find(|bot| bot["slug"] == slug).unwrap();
    assert_eq!(row["subagents"], default_subagents());
    assert_eq!(row["childThreads"], default_child_threads());
    let on_disk = raw(dir.path(), &slug);
    assert!(on_disk.get("subagents").is_none());
    assert!(on_disk.get("childThreads").is_none());
    bots.save(&slug, &json!({ "name": "Ada Lovelace" }), false)
        .await
        .unwrap();
    let still = raw(dir.path(), &slug);
    assert!(still.get("subagents").is_none());
    assert!(still.get("childThreads").is_none());
    assert_eq!(still["name"], "Ada Lovelace");
}

#[tokio::test]
async fn u2_disabled_helpers_refuse_create() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    bots.save(&slug, &json!({ "subagents": { "enabled": false } }), false)
        .await
        .unwrap();
    let err = bots
        .create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap_err();
    let (code, message) = status(err);
    assert_eq!(code, 409);
    assert_eq!(message, HELPERS_OFF);
    assert!(bots.get("scout").await.unwrap().is_none());
}

#[tokio::test]
async fn u3_max_count_zero_refuses_create() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    bots.save(&slug, &json!({ "subagents": { "maxCount": 0 } }), false)
        .await
        .unwrap();
    let err = bots
        .create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap_err();
    let (code, message) = status(err);
    assert_eq!(code, 409);
    assert_eq!(message, "This Bot already has 0 lasting helpers.");
}

#[tokio::test]
async fn u4_at_max_count_the_next_create_is_refused_and_list_works() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    bots.save(&slug, &json!({ "subagents": { "maxCount": 1 } }), false)
        .await
        .unwrap();
    bots.create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap();
    let err = bots
        .create(&json!({ "parentSlug": slug, "name": "Lookup" }))
        .await
        .unwrap_err();
    let (code, message) = status(err);
    assert_eq!(code, 409);
    assert_eq!(message, "This Bot already has 1 lasting helpers.");
    let listed = bots.list().await.unwrap();
    assert!(listed.iter().any(|bot| bot["slug"] == slug));
    assert!(listed.iter().any(|bot| bot["slug"] == "scout"));
    assert!(listed.iter().all(|bot| bot["slug"] != "lookup"));
}

#[tokio::test]
async fn u5_an_invalid_default_runtime_mode_is_rejected() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    for mode in ["ask", "independent"] {
        let err = bots
            .save(
                &slug,
                &json!({ "subagents": { "defaults": { "runtimeMode": mode } } }),
                false,
            )
            .await
            .unwrap_err();
        assert_eq!(status(err).0, 400, "{mode}");
    }
}

#[tokio::test]
async fn u6_a_when_longer_than_2000_characters_is_rejected() {
    let (dir, bots) = world();
    let slug = ada(&bots).await;
    let err = bots
        .save(
            &slug,
            &json!({ "subagents": { "when": "a".repeat(2001) } }),
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(status(err).0, 400);
    let when = "b".repeat(2000);
    bots.save(&slug, &json!({ "childThreads": { "when": when } }), false)
        .await
        .unwrap();
    assert_eq!(
        bots.get(&slug).await.unwrap().unwrap()["childThreads"]["when"],
        "b".repeat(2000)
    );
    assert!(raw(dir.path(), &slug).get("subagents").is_none());
}

#[tokio::test]
async fn u7_a_subagent_cannot_store_helpers_policy() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    let child = bots
        .create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap();
    let child_slug = child["slug"].as_str().unwrap();
    assert!(child.get("subagents").is_none());
    assert!(child.get("childThreads").is_none());
    for patch in [
        json!({ "subagents": { "enabled": false } }),
        json!({ "childThreads": { "enabled": false } }),
    ] {
        let err = bots.save(child_slug, &patch, false).await.unwrap_err();
        let (code, message) = status(err);
        assert_eq!(code, 400);
        assert_eq!(message, NO_POLICY);
    }
}

#[tokio::test]
async fn u8_a_prompt_only_save_keeps_tool_allow_and_inherit() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    let child = bots
        .create(&json!({
            "parentSlug": slug,
            "name": "Scout",
            "subagent": {
                "prompt": "Look.",
                "inheritParentPrompt": true,
                "toolAllow": ["read", "search"]
            }
        }))
        .await
        .unwrap();
    let child_slug = child["slug"].as_str().unwrap().to_string();
    let saved = bots
        .save(
            &child_slug,
            &json!({ "subagent": { "prompt": "Only the prompt." } }),
            false,
        )
        .await
        .unwrap();
    assert_eq!(saved["subagent"]["prompt"], "Only the prompt.");
    assert_eq!(saved["subagent"]["inheritParentPrompt"], true);
    assert_eq!(saved["subagent"]["toolAllow"], json!(["read", "search"]));
}

#[tokio::test]
async fn u9_a_null_policy_leaf_removes_that_key() {
    let (dir, bots) = world();
    let slug = ada(&bots).await;
    let chosen = selection("claude-haiku-5-5", "low");
    bots.save(
        &slug,
        &json!({
            "subagents": { "enabled": false, "defaults": { "model": chosen, "effort": "low" } },
            "childThreads": { "defaults": { "effort": "high" } }
        }),
        false,
    )
    .await
    .unwrap();
    bots.save(
        &slug,
        &json!({
            "subagents": { "defaults": { "model": null } },
            "childThreads": { "defaults": { "effort": null } }
        }),
        false,
    )
    .await
    .unwrap();
    let on_disk = raw(dir.path(), &slug);
    assert!(on_disk["subagents"]["defaults"].get("model").is_none());
    assert_eq!(on_disk["subagents"]["enabled"], false);
    assert_eq!(on_disk["subagents"]["defaults"]["effort"], "low");
    assert!(on_disk["childThreads"]["defaults"].get("effort").is_none());
    let view = bots.get(&slug).await.unwrap().unwrap();
    assert!(view["subagents"]["defaults"]["model"].is_null());
    assert!(view["childThreads"]["defaults"]["effort"].is_null());
    assert_eq!(view["subagents"]["enabled"], false);
}

#[tokio::test]
async fn u10_an_unknown_allowlist_effort_is_rejected() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    let err = bots
        .save(
            &slug,
            &json!({ "subagents": { "allowlist": { "efforts": ["low", "turbo"] } } }),
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(status(err).0, 400);
}

#[tokio::test]
async fn u11_a_null_default_model_copies_the_parent_selection() {
    let (_dir, bots) = world();
    let chosen = selection("claude-sonnet-4-6", "low");
    let slug = bots
        .create(&json!({ "name": "Ada", "model": chosen }))
        .await
        .unwrap()["slug"]
        .as_str()
        .unwrap()
        .to_string();
    let child = bots
        .create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap();
    assert_eq!(child["model"], chosen);
}

#[tokio::test]
async fn u12_a_default_model_is_stored_with_its_effort() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    bots.save(
        &slug,
        &json!({
            "model": selection("claude-sonnet-4-6", "low"),
            "subagents": {
                "defaults": {
                    "model": model_ref("claude-haiku-5-5"),
                    "effort": "high"
                }
            }
        }),
        false,
    )
    .await
    .unwrap();
    let child = bots
        .create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap();
    assert_eq!(child["model"]["model"], "claude-haiku-5-5");
    assert_eq!(child["model"]["instanceId"], "claudeAgent");
    assert_eq!(child["model"]["options"][0]["id"], "effort");
    assert_eq!(child["model"]["options"][0]["value"], "high");
}

#[tokio::test]
async fn u13_a_create_model_outside_the_allowlist_is_rejected() {
    let (_dir, bots) = world();
    let slug = ada(&bots).await;
    bots.save(
        &slug,
        &json!({
            "subagents": { "allowlist": { "models": [model_ref("claude-haiku-5-5")] } }
        }),
        false,
    )
    .await
    .unwrap();
    let err = bots
        .create(&json!({
            "parentSlug": slug,
            "name": "Scout",
            "model": selection("claude-sonnet-4-6", "medium")
        }))
        .await
        .unwrap_err();
    let (code, message) = status(err);
    assert_eq!(code, 400);
    assert_eq!(message, MODEL_DENIED);
}

#[tokio::test]
async fn u14_a_parent_model_outside_the_allowlist_falls_back_or_refuses() {
    let (_dir, bots) = world();
    let haiku = crate::bots::provisioner::default_model();
    let slug = bots
        .create(&json!({ "name": "Ada", "model": selection("claude-opus-4-6", "low") }))
        .await
        .unwrap()["slug"]
        .as_str()
        .unwrap()
        .to_string();
    bots.save(
        &slug,
        &json!({
            "subagents": {
                "allowlist": {
                    "models": [model_ref("claude-sonnet-4-6"), model_ref("claude-haiku-5-5")]
                }
            }
        }),
        false,
    )
    .await
    .unwrap();
    let child = bots
        .create(&json!({ "parentSlug": slug, "name": "Scout" }))
        .await
        .unwrap();
    assert_eq!(child["model"]["model"], haiku["model"]);
    assert_eq!(child["model"]["instanceId"], haiku["instanceId"]);

    let other = bots
        .create(&json!({ "name": "Bea", "model": selection("claude-opus-4-6", "low") }))
        .await
        .unwrap()["slug"]
        .as_str()
        .unwrap()
        .to_string();
    bots.save(
        &other,
        &json!({
            "subagents": { "allowlist": { "models": [model_ref("claude-sonnet-4-6")] } }
        }),
        false,
    )
    .await
    .unwrap();
    let first = bots
        .create(&json!({ "parentSlug": other, "name": "Lookup" }))
        .await
        .unwrap();
    assert_eq!(first["model"]["model"], "claude-sonnet-4-6");
    assert_eq!(first["model"]["options"][0]["value"], "medium");

    let empty = bots
        .create(&json!({ "name": "Cy", "model": selection("claude-opus-4-6", "low") }))
        .await
        .unwrap()["slug"]
        .as_str()
        .unwrap()
        .to_string();
    bots.save(
        &empty,
        &json!({ "subagents": { "allowlist": { "models": [] } } }),
        false,
    )
    .await
    .unwrap();
    let err = bots
        .create(&json!({ "parentSlug": empty, "name": "None" }))
        .await
        .unwrap_err();
    let (code, message) = status(err);
    assert_eq!(code, 400);
    assert_eq!(message, MODEL_DENIED);
}

#[tokio::test]
async fn u15_a_default_runtime_mode_outside_the_allowlist_is_rejected() {
    let (dir, bots) = world();
    let slug = ada(&bots).await;
    let err = bots
        .save(
            &slug,
            &json!({
                "subagents": {
                    "defaults": { "runtimeMode": "full-access" },
                    "allowlist": { "runtimeModes": ["auto"] }
                }
            }),
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(status(err).0, 400);
    assert!(raw(dir.path(), &slug).get("subagents").is_none());
}
