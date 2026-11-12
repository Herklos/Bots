use super::{offered_tools, refuse_hidden, tools};
use crate::bots::mcp_identity::{bot_mcp_url, bot_slug_from_query};
use crate::bots::Bots;
use serde_json::{json, Value};

fn names(tools: &Value) -> Vec<String> {
    tools
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn null_tool_allow_keeps_the_full_gateway_tool_list() {
    for projects_on in [false, true] {
        let full = tools(projects_on);
        assert!(names(&full).len() > 1);
        assert_eq!(offered_tools(full.clone(), None), full);
        let unrestricted = [
            json!({ "subagent": null }),
            json!({ "subagent": { "prompt": "Look.", "toolAllow": null } }),
            json!({ "subagent": { "prompt": "Look." } }),
        ];
        for bot in unrestricted {
            assert_eq!(offered_tools(full.clone(), Some(&bot)), full);
        }
    }
}

#[test]
fn an_allowlist_hides_tools_that_are_not_named() {
    let full = tools(true);
    let all = names(&full);
    assert!(all.len() > 1);
    assert!(all.iter().any(|name| name == "vault_list"));
    let bot = json!({ "subagent": { "toolAllow": ["vault_list", "Read"] } });
    let offered = names(&offered_tools(full.clone(), Some(&bot)));
    assert_eq!(offered, vec!["vault_list".to_string()]);
    assert!(all.iter().any(|name| !offered.contains(name)));
    let empty = json!({ "subagent": { "toolAllow": [] } });
    assert!(names(&offered_tools(full.clone(), Some(&empty))).is_empty());
    let hidden = all.into_iter().find(|name| name != "vault_list").unwrap();
    assert!(refuse_hidden(&bot, "vault_list", &full).is_ok());
    assert_eq!(
        refuse_hidden(&bot, &hidden, &full).unwrap_err(),
        "That tool isn't allowed for this Bot."
    );
    assert!(refuse_hidden(&bot, "Read", &full).is_ok());
    let open = json!({ "subagent": { "toolAllow": null } });
    assert!(refuse_hidden(&open, &hidden, &full).is_ok());
}

#[test]
fn the_mcp_url_names_the_bot_once() {
    assert_eq!(
        bot_mcp_url("http://127.0.0.1:3780/mcp/bot", "scout"),
        "http://127.0.0.1:3780/mcp/bot?bot=scout"
    );
    assert_eq!(
        bot_mcp_url("http://127.0.0.1:3780/mcp/bot?x=1", "scout"),
        "http://127.0.0.1:3780/mcp/bot?x=1&bot=scout"
    );
    assert_eq!(
        bot_mcp_url("http://127.0.0.1:3780/mcp/bot?bot=scout", "other"),
        "http://127.0.0.1:3780/mcp/bot?bot=scout"
    );
    assert_eq!(
        bot_mcp_url("http://127.0.0.1:3780/mcp/bot", "Not_A_Slug"),
        "http://127.0.0.1:3780/mcp/bot"
    );
    assert_eq!(
        bot_slug_from_query(Some("bot=scout")),
        Some("scout".to_string())
    );
    assert_eq!(
        bot_slug_from_query(Some("x=1&bot=ada-junior-1")),
        Some("ada-junior-1".to_string())
    );
    assert_eq!(bot_slug_from_query(Some("bot=Not_A_Slug")), None);
    assert_eq!(bot_slug_from_query(None), None);
}

#[tokio::test]
async fn a_prompt_only_save_still_keeps_tool_allow() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(
        dir.path().to_path_buf(),
        "http://127.0.0.1:3780/mcp/bot".into(),
        "tok".into(),
    );
    let parent = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let parent_slug = parent["slug"].as_str().unwrap();
    let child = bots
        .create(&json!({
            "parentSlug": parent_slug,
            "name": "Scout",
            "subagent": {
                "prompt": "Look.",
                "inheritParentPrompt": true,
                "toolAllow": ["vault_list"]
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
    assert_eq!(saved["subagent"]["toolAllow"], json!(["vault_list"]));
    let offered = names(&offered_tools(tools(true), Some(&saved)));
    assert_eq!(offered, vec!["vault_list".to_string()]);
    let folder = dir.path().join(&child_slug);
    let mcp = std::fs::read_to_string(folder.join(".mcp.json")).unwrap();
    assert!(mcp.contains(&format!("bot={child_slug}")));
    let toml = std::fs::read_to_string(folder.join(".codex").join("config.toml")).unwrap();
    assert!(toml.contains(&format!("bot={child_slug}")));
}
