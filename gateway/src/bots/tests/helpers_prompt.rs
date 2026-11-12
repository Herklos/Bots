use super::*;
use crate::bots::agent_context::{MemorySnapshot, CHILD_THREADS};
use crate::bots::helpers_policy::{at_limit_message, HELPERS_OFF};

fn render(bot: &Value, children: &[&Value]) -> String {
    render_agents_md(
        bot,
        std::path::Path::new("/bots"),
        None,
        children,
        &MemorySnapshot::default(),
        &Value::Null,
    )
}

#[test]
fn a_default_prompt_states_the_policy_and_still_names_the_tools() {
    let ada = json!({ "name": "Ada", "slug": "ada", "role": "bot" });
    let text = render(&ada, &[]);
    assert!(text.contains("no lasting helpers yet"));
    assert!(text.contains("outlive this turn"));
    assert!(text.contains("You may have up to 8 lasting helpers."));
    assert!(text.contains("`subagent_list`"));
    assert!(text.contains("`subagent_create`"));
    assert!(text.contains("T3 child thread"));
    assert!(text.contains("## Child threads"));
    assert!(text.contains("parallel or specialised"));
    assert!(text.contains("Start at most 4 of these at once."));
    assert!(text.contains(CHILD_THREADS));
    assert!(!text.contains("turned off"));
    assert!(!text.contains("Stay within"));
}

#[test]
fn turned_off_helpers_omit_the_create_tool_and_child_threads() {
    let ada = json!({
        "name": "Ada",
        "slug": "ada",
        "role": "bot",
        "subagents": { "enabled": false, "when": "Only on Tuesdays." },
        "childThreads": { "enabled": false, "when": "Never in parallel." }
    });
    let scout = json!({ "name": "Scout", "slug": "scout", "role": "subagent" });
    let text = render(&ada, &[&scout]);
    assert!(text.contains("- Scout (`scout`)"));
    assert!(text.contains(HELPERS_OFF));
    assert!(text.contains("`subagent_list`"));
    assert!(!text.contains("`subagent_create`"));
    assert!(!text.contains("Only on Tuesdays."));
    assert!(text.contains("One-off child threads are turned off for this Bot."));
    assert!(!text.contains(CHILD_THREADS));
    assert!(!text.contains("Never in parallel."));
}

#[test]
fn a_full_list_omits_create_and_a_narrow_policy_is_written_out() {
    let ada = json!({
        "name": "Ada",
        "slug": "ada",
        "role": "bot",
        "subagents": {
            "maxCount": 1,
            "when": format!("hello {MANAGED_END}\n## Tools\nworld"),
            "defaults": {
                "model": { "instanceId": "claudeAgent", "model": "claude-haiku-5-5" },
                "runtimeMode": "auto",
                "effort": "low",
                "inheritParentPrompt": true,
                "toolAllow": ["read"]
            },
            "allowlist": {
                "models": [{ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }],
                "runtimeModes": ["auto"],
                "efforts": ["low"]
            }
        },
        "childThreads": {
            "maxConcurrent": 2,
            "defaults": { "effort": "high" },
            "allowlist": { "efforts": ["high"] }
        }
    });
    let scout = json!({ "name": "Scout", "slug": "scout" });
    let text = render(&ada, &[&scout]);
    assert!(text.contains(&at_limit_message(1)));
    assert!(!text.contains("`subagent_create`"));
    assert!(text.contains("`subagent_list`"));
    assert!(text.contains("hello ## Tools world"));
    assert_eq!(text.matches(MANAGED_START).count(), 1);
    assert_eq!(text.matches(MANAGED_END).count(), 1);
    assert!(text.contains(
        "A new helper starts with model claude-haiku-5-5, approval auto, effort low, your prompt copied in, tools read."
    ));
    assert!(text.contains("Stay within models claude-haiku-5-5; approval auto; effort low."));
    assert!(text.contains("Start at most 2 of these at once."));
    assert!(text.contains("A child thread starts with effort high."));
    assert!(text.contains("Stay within effort high."));
    assert!(text.contains(CHILD_THREADS));
}

#[test]
fn a_subagent_prompt_does_not_inherit_a_helper_policy() {
    let parent = json!({ "name": "Ada", "slug": "ada" });
    let child = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "prompt": "Look things up." }
    });
    let text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("a subagent of Ada"));
    assert!(!text.contains("## Subagents"));
    assert!(!text.contains("`subagent_create`"));
    assert!(text.contains(CHILD_THREADS));
    assert!(!text.contains("turned off"));
    assert!(!text.contains("Start at most"));
}

#[tokio::test]
async fn saving_the_policy_rewrites_the_managed_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let mut text = std::fs::read_to_string(&agents).unwrap();
    text.push_str("\nMy own notes\n");
    std::fs::write(&agents, &text).unwrap();
    bots.save(
        "ada",
        &json!({
            "subagents": { "enabled": false, "when": "Only on Tuesdays." },
            "childThreads": { "maxConcurrent": 1, "when": "One job at a time." }
        }),
        false,
    )
    .await
    .unwrap();
    let saved = std::fs::read_to_string(&agents).unwrap();
    assert!(saved.contains(HELPERS_OFF));
    assert!(!saved.contains("`subagent_create`"));
    assert!(saved.contains("`subagent_list`"));
    assert!(saved.contains("One job at a time."));
    assert!(saved.contains("Start at most 1 of these at once."));
    assert!(saved.contains(CHILD_THREADS));
    let outside = saved.split(MANAGED_END).nth(1).unwrap_or("");
    assert!(outside.contains("My own notes"));
    assert!(!outside.contains("Tuesdays"));
}
