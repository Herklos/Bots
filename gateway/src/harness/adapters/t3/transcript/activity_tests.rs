use super::activity::{activity, apply_activity, call_detail, Level};
use super::project::{transcript, visible_items, with_tokens};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn projection() -> Value {
    serde_json::from_str(include_str!("fixtures/thread-projection.json")).unwrap()
}

fn items_of(body: &Value) -> Vec<Value> {
    visible_items(body)
}

#[test]
fn the_real_projection_has_stable_ordinals_and_the_fields_it_actually_carries() {
    let body = projection();
    let items = body["turnItems"].as_array().unwrap();
    let mut types = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    let mut tools = BTreeSet::new();
    let mut previous = -1_i64;
    for item in items {
        types.insert(item["type"].as_str().unwrap().to_string());
        statuses.insert(item["status"].as_str().unwrap().to_string());
        if let Some(name) = item["toolName"].as_str() {
            tools.insert(name.to_string());
        }
        let ordinal = item["ordinal"].as_i64().unwrap();
        assert!(ordinal > previous);
        previous = ordinal;
    }
    assert_eq!(
        types,
        BTreeSet::from([
            "assistant_message".into(),
            "checkpoint".into(),
            "command_execution".into(),
            "dynamic_tool".into(),
            "reasoning".into(),
            "user_message".into(),
        ])
    );
    assert_eq!(
        statuses,
        BTreeSet::from(["completed".into(), "failed".into()])
    );
    assert_eq!(tools, BTreeSet::from(["Read".into()]));
    let read = items
        .iter()
        .find(|item| item["toolName"] == "Read")
        .unwrap();
    assert!(read.get("error").is_none());
    assert!(read.get("input").is_none());
    let command = items
        .iter()
        .find(|item| item["type"] == "command_execution")
        .unwrap();
    assert!(command.get("command").is_none());
    assert!(command.get("output").is_none());
}

#[test]
fn quiet_matches_today_and_louder_levels_leave_messages_and_events_alone() {
    let body = projection();
    let base = transcript(&body);
    let items = items_of(&body);
    let quiet_bytes = serde_json::to_vec(&base).unwrap();
    let mut quiet = base.clone();
    apply_activity(&mut quiet, &items, Level::Quiet, false);
    assert_eq!(serde_json::to_vec(&quiet).unwrap(), quiet_bytes);
    assert!(quiet.get("activity").is_none());

    for level in [Level::Problems, Level::Summary, Level::Detail] {
        let mut loud = base.clone();
        apply_activity(&mut loud, &items, level, false);
        assert_eq!(loud["messages"], base["messages"]);
        assert_eq!(loud["events"], base["events"]);
        assert!(loud["activity"].is_array());
        assert!(loud["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|message| message["kind"] != "activity"));
    }
}

#[test]
fn the_real_projection_groups_the_failed_read_with_the_command_under_the_reply() {
    let body = projection();
    let items = items_of(&body);
    let rows = activity(&items, Level::Summary);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["status"], "failed");
    assert_eq!(rows[0]["ordinal"], 2000005);
    assert_eq!(rows[0]["detail"], false);
    assert_eq!(rows[0]["counts"]["commands"], 1);
    assert_eq!(rows[0]["counts"]["tools"], 1);
    assert_eq!(rows[0]["counts"]["edits"], 0);
    assert!(rows[0]["calls"][0].get("target").is_none());
    assert_eq!(rows[0]["calls"][0]["label"], "Read");
    assert_eq!(rows[0]["calls"][0]["status"], "failed");
    assert_eq!(rows[0]["calls"][1]["label"], "command");
    assert_eq!(rows[0]["calls"][1]["status"], "ok");

    let problems = activity(&items, Level::Problems);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0]["calls"].as_array().unwrap().len(), 1);
    assert_eq!(problems[0]["calls"][0]["id"], rows[0]["calls"][0]["id"]);

    let detail = activity(&items, Level::Detail);
    assert_eq!(detail[0]["detail"], true);
    assert_eq!(detail[0]["calls"][1]["target"], "");
}

#[test]
fn a_finished_reply_owns_the_row_and_a_live_reply_stays_below_it() {
    let user = json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"go"});
    let command = json!({"id":"c","type":"command_execution","status":"running","ordinal":4,"command":"cargo test"});
    let live = json!({"id":"a","type":"assistant_message","status":"streaming","streaming":true,"ordinal":9,"text":"..."});
    let items = vec![user, command, live];
    let running = activity(&items, Level::Summary);
    assert_eq!(running[0]["ordinal"], 4);
    assert_eq!(running[0]["status"], "running");

    let mut done = items;
    done[1]["status"] = json!("completed");
    done[2]["status"] = json!("completed");
    done[2]["streaming"] = json!(false);
    let finished = activity(&done, Level::Summary);
    assert_eq!(finished[0]["ordinal"], 9);
    assert_eq!(finished[0]["status"], "ok");
    assert_eq!(finished[0]["calls"][0]["target"], Value::Null);
    let detailed = activity(&done, Level::Detail);
    assert_eq!(detailed[0]["calls"][0]["target"], "cargo test");
}

#[test]
fn event_tools_stay_events_and_vault_calls_carry_no_secret() {
    let user = json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"go"});
    let routine = json!({"id":"s","type":"dynamic_tool","status":"completed","ordinal":2,"toolName":"mcp__t3-code__schedule_task","input":{"title":"Morning","scheduledTaskId":null}});
    let command = json!({"id":"c","type":"command_execution","status":"completed","ordinal":3,"command":"echo Bearer super-secret-token sk-abcdefghij123456","output":"Bearer super-secret-token"});
    let vault = json!({"id":"v","type":"dynamic_tool","status":"completed","ordinal":4,"toolName":"vault_fill_password","input":{"password":"vault-secret-value"},"output":"vault-secret-value"});
    let reply =
        json!({"id":"a","type":"assistant_message","status":"completed","ordinal":5,"text":"done"});
    let items = vec![user, routine, command, vault, reply];
    let view = transcript(&json!({"turnItems": items.clone()}));
    assert_eq!(view["events"][0]["text"], "Created routine");
    assert!(activity(&items, Level::Problems).is_empty());
    for level in [Level::Summary, Level::Detail] {
        let rows = activity(&items, level);
        let text = serde_json::to_string(&rows).unwrap();
        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sk-abcdefghij123456"));
        assert!(!text.contains("vault-secret-value"));
        assert!(rows[0]["calls"]
            .as_array()
            .unwrap()
            .iter()
            .all(|call| call["id"] != "s"));
    }
    let detail = call_detail(&items, "c").unwrap();
    let body = serde_json::to_string(&detail).unwrap();
    assert!(!body.contains("super-secret-token"));
    assert!(body.contains("Bearer [redacted]"));
    assert!(body.contains("[redacted]"));
    let hidden = call_detail(&items, "v").unwrap();
    assert_eq!(hidden["input"], "");
    assert_eq!(hidden["output"], "");
    assert!(!hidden.to_string().contains("vault-secret-value"));
    assert!(call_detail(&items, "missing").is_none());
    assert!(call_detail(&items, "s").is_none());
}

#[test]
fn summary_counts_edits_searches_browser_and_memory_without_their_output() {
    let items = vec![
        json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"go"}),
        json!({"id":"e","type":"file_change","status":"completed","ordinal":2,"path":"src/main.rs"}),
        json!({"id":"w","type":"web_search","status":"failed","ordinal":3,"query":"hbots gateway"}),
        json!({"id":"b","type":"dynamic_tool","status":"completed","ordinal":4,"toolName":"preview_open","input":{"url":"https://example.com"}}),
        json!({"id":"m","type":"dynamic_tool","status":"completed","ordinal":5,"toolName":"memory_read"}),
        json!({"id":"p","type":"dynamic_tool","status":"completed","ordinal":6,"toolName":"projects_list"}),
        json!({"id":"a","type":"assistant_message","status":"completed","ordinal":7,"text":"done"}),
    ];
    let rows = activity(&items, Level::Summary);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["counts"]["edits"], 1);
    assert_eq!(rows[0]["counts"]["searches"], 1);
    assert_eq!(rows[0]["counts"]["browser"], 1);
    assert_eq!(rows[0]["counts"]["memory"], 1);
    assert_eq!(rows[0]["counts"]["tools"], 1);
    assert_eq!(rows[0]["status"], "failed");
    assert!(rows[0]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .all(|call| call.get("target").is_none()));
    let detail = activity(&items, Level::Detail);
    let calls = detail[0]["calls"].as_array().unwrap();
    assert_eq!(calls[0]["target"], "src/main.rs");
    assert_eq!(calls[1]["target"], "hbots gateway");
    assert_eq!(calls[2]["target"], "https://example.com");
    assert_eq!(calls[3]["target"], "");
    assert_eq!(calls[4]["label"], "projects list");
    assert_eq!(calls[4]["target"], "");
}

#[test]
fn final_reply_only_hides_in_between_replies_after_the_turn_ends() {
    let items = vec![
        json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"go"}),
        json!({"id":"a","type":"assistant_message","status":"completed","ordinal":2,"text":"first"}),
        json!({"id":"b","type":"assistant_message","status":"completed","ordinal":3,"text":"last"}),
        json!({"id":"v","type":"user_message","status":"completed","ordinal":4,"text":"next"}),
    ];
    let mut view = transcript(&json!({"turnItems": items.clone()}));
    let before = view["messages"].clone();
    apply_activity(&mut view, &items, Level::Quiet, false);
    assert_eq!(view["messages"], before);
    apply_activity(&mut view, &items, Level::Quiet, true);
    assert_eq!(view["messages"][1]["hidden"], true);
    assert!(view["messages"][2].get("hidden").is_none());
    assert_eq!(view["messages"][1]["text"], "first");
    assert!(view.get("activity").is_none());

    let live = vec![
        json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"go"}),
        json!({"id":"a","type":"assistant_message","status":"completed","ordinal":2,"text":"first"}),
        json!({"id":"b","type":"assistant_message","status":"streaming","streaming":true,"ordinal":3,"text":"live"}),
    ];
    let mut streaming = transcript(&json!({"turnItems": live}));
    apply_activity(&mut streaming, &live, Level::Quiet, true);
    assert!(streaming["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|message| message.get("hidden").is_none()));
}

#[test]
fn detail_caps_text_replaces_binary_payloads_and_tokens_stay_on_messages() {
    let huge = "hello world ".repeat(300);
    let items = vec![
        json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"go"}),
        json!({"id":"c","type":"command_execution","status":"completed","ordinal":2,"command":"printf","output": huge,"input":{"mimeType":"image/png","type":"image"}}),
        json!({"id":"f","type":"command_execution","status":"declined","ordinal":3,"output":{"type":"file","mimeType":"application/octet-stream"}}),
        json!({"id":"a","type":"assistant_message","status":"completed","ordinal":4,"text":"done"}),
    ];
    let image = call_detail(&items, "c").unwrap();
    assert_eq!(image["input"], "image");
    assert_eq!(image["output"].as_str().unwrap().len(), 2048);
    assert_eq!(image["truncated"], true);
    let file = call_detail(&items, "f").unwrap();
    assert_eq!(file["output"], "file");
    let mut view = transcript(&json!({ "turnItems": items.clone() }));
    apply_activity(&mut view, &items, Level::Summary, false);
    let tagged = with_tokens(view.clone(), &json!({ "turnItems": items }));
    assert_eq!(tagged["activity"], view["activity"]);
    assert_eq!(
        tagged["messages"].as_array().unwrap().len(),
        view["messages"].as_array().unwrap().len()
    );
}

#[test]
fn an_empty_turn_and_a_request_add_no_row() {
    let items = vec![
        json!({"id":"u","type":"user_message","status":"completed","ordinal":1,"text":"hi"}),
        json!({"id":"q","type":"approval_request","status":"pending","ordinal":2,"prompt":"Run?"}),
        json!({"id":"r","type":"reasoning","status":"completed","ordinal":3,"text":"thinking"}),
        json!({"id":"a","type":"assistant_message","status":"completed","ordinal":4,"text":"ok"}),
    ];
    assert!(activity(&items, Level::Detail).is_empty());
    assert!(activity(&[], Level::Summary).is_empty());
}
