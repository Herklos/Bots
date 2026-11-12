use super::{transcript, with_tokens};
use crate::chat::transcript_tag;
use serde_json::{json, Value};
use std::path::PathBuf;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/t3")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn sample() -> Value {
    fixture("html-render.json")
}

fn tool(id: &str, name: &str, status: &str, output: Value) -> Value {
    json!({
        "id": id,
        "ordinal": 41,
        "type": "dynamic_tool",
        "status": status,
        "startedAt": "2026-11-12T12:00:00Z",
        "toolName": name,
        "output": output,
    })
}

fn page(id: &str, output: Value) -> Value {
    tool(id, "mcp__t3-code__html_render", "completed", output)
}

fn view(items: Vec<Value>) -> Value {
    transcript(&json!({ "turnItems": items }))
}

fn reply() -> Value {
    json!({
        "id": "m",
        "ordinal": 50,
        "type": "assistant_message",
        "text": "See the chart.",
        "status": "completed",
        "startedAt": "2026-11-12T12:00:01Z",
    })
}

#[test]
fn one_completed_page_is_a_render_and_the_reply_stays_a_message() {
    let direct = sample()["direct"].clone();
    let with_page = view(vec![page("a", direct), reply()]);
    let messages_only = view(vec![reply()]);
    assert_eq!(with_page["messages"], messages_only["messages"]);
    assert_eq!(with_page["events"], messages_only["events"]);
    assert_eq!(with_page["requests"], messages_only["requests"]);
    assert_eq!(with_page["working"], messages_only["working"]);
    assert_eq!(with_page["activeRunId"], messages_only["activeRunId"]);
    assert!(messages_only.get("renders").is_none());
    let row = &with_page["renders"][0];
    assert_eq!(row["id"], "render-a");
    assert_eq!(row["ordinal"], 41);
    assert_eq!(row["at"], "2026-11-12T12:00:00Z");
    assert_eq!(row["kind"], "html");
    assert_eq!(row["title"], "Spend by month");
    assert_eq!(row["height"], 420);
    assert_eq!(row["attachmentId"], "att-spend-html");
    assert_eq!(row["heights"], json!([[320, 380], [900, 400]]));
    assert!(row.get("message").is_none());
}

#[test]
fn each_stored_envelope_projects_the_same_page() {
    let saved = sample();
    for output in [
        saved["jsonText"].clone(),
        saved["structured"].clone(),
        saved["blocks"].clone(),
    ] {
        let row = &view(vec![page("a", output)])["renders"][0];
        assert_eq!(row["attachmentId"], "att-spend-html");
        assert_eq!(row["title"], "Spend by month");
        assert_eq!(row["height"], 420);
        assert!(row.get("heights").is_none());
    }
}

#[test]
fn dotted_and_thread_scoped_tool_names_are_pages() {
    let output = sample()["direct"].clone();
    let saved = sample();
    for name in [
        saved["dottedName"].as_str().unwrap(),
        saved["scopedName"].as_str().unwrap(),
        "html_render",
        "mcp__t3_code__html_render",
    ] {
        let row = &view(vec![tool("a", name, "completed", output.clone())])["renders"][0];
        assert_eq!(row["kind"], "html");
        assert_eq!(row["attachmentId"], "att-spend-html");
    }
}

#[test]
fn two_pages_keep_call_order() {
    let output = sample()["jsonText"].clone();
    let mut second = page("b", output.clone());
    second["ordinal"] = json!(42);
    let rows = view(vec![page("a", output), second, reply()])["renders"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["id"], "render-a");
    assert_eq!(rows[0]["ordinal"], 41);
    assert_eq!(rows[1]["id"], "render-b");
    assert_eq!(rows[1]["ordinal"], 42);
    assert_eq!(view(vec![reply()])["messages"].as_array().unwrap().len(), 1);
}

#[test]
fn a_failed_or_running_page_and_an_unknown_item_add_nothing() {
    let saved = sample();
    let direct = saved["direct"].clone();
    let cases = [
        tool("a", "mcp__t3-code__html_render", "failed", direct.clone()),
        tool("a", "mcp__t3-code__html_render", "running", direct.clone()),
        tool(
            "a",
            "mcp__t3-code__html_render",
            "completed",
            saved["failed"].clone(),
        ),
        tool("a", "execute", "completed", direct.clone()),
        tool("a", "Read", "completed", json!({})),
        tool(
            "a",
            "mcp__t3-code__html_preview",
            "completed",
            json!({ "contentHeight": 400 }),
        ),
        json!({ "id": "a", "type": "command_execution", "status": "completed" }),
        page(
            "a",
            json!({ "htmlRender": { "title": "Spend", "height": 420 } }),
        ),
    ];
    for item in cases {
        let projected = view(vec![item, reply()]);
        assert!(projected.get("renders").is_none());
        assert_eq!(projected["messages"].as_array().unwrap().len(), 1);
    }
    let running = view(vec![tool(
        "a",
        "mcp__t3-code__html_render",
        "running",
        direct,
    )]);
    assert_eq!(running["working"], true);
}

#[test]
fn a_bad_measurement_is_dropped_and_the_page_remains() {
    let output = json!({
        "htmlRender": {
            "attachmentId": "att-spend-html",
            "title": "Spend by month",
            "height": 10.2,
            "heights": [[320, 380], [0, 900]]
        }
    });
    let row = &view(vec![page("a", output)])["renders"][0];
    assert_eq!(row["height"], 80);
    assert!(row.get("heights").is_none());
    let tall = json!({
        "htmlRender": { "attachmentId": "att-spend-html", "title": "", "height": 9000 }
    });
    let row = &view(vec![page("a", tall)])["renders"][0];
    assert_eq!(row["height"], 2000);
    assert_eq!(row["title"], "HTML");
}

#[test]
fn an_mcp_app_keeps_the_reference_without_its_policy() {
    let row = &view(vec![tool(
        "app",
        "mcp__weather__forecast",
        "completed",
        sample()["mcp"].clone(),
    )])["renders"][0];
    assert_eq!(row["id"], "render-app");
    assert_eq!(row["kind"], "mcp-app");
    assert_eq!(row["attachmentId"], "app-forecast-html");
    assert_eq!(row["server"], "weather");
    assert_eq!(row["tool"], "forecast");
    assert_eq!(row["resourceUri"], "ui://weather/forecast");
    assert_eq!(row["permissions"], json!({ "clipboardWrite": {} }));
    assert_eq!(row["prefersBorder"], true);
    assert!(row.get("csp").is_none());
    let refused = tool(
        "app",
        "weather.forecast",
        "completed",
        json!({ "t3McpApp": { "attachmentId": "app-forecast-html", "server": "weather", "tool": "forecast", "resourceUri": "https://example.test" } }),
    );
    assert!(view(vec![refused]).get("renders").is_none());
}

#[test]
fn a_wrapped_item_still_projects() {
    let wrapped = json!({ "item": page("a", sample()["direct"].clone()) });
    assert_eq!(view(vec![wrapped])["renders"][0]["id"], "render-a");
}

#[test]
fn twenty_pages_is_the_cap() {
    let output = sample()["jsonText"].clone();
    let items: Vec<Value> = (0..21)
        .map(|n| {
            let mut item = page(&format!("r{n}"), output.clone());
            item["ordinal"] = json!(n);
            item
        })
        .collect();
    let rows = view(items)["renders"].clone();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 20);
    assert_eq!(rows[0]["id"], "render-r0");
    assert_eq!(rows[19]["id"], "render-r19");
    assert!(rows.iter().all(|row| row["id"] != "render-r20"));
}

#[test]
fn tokens_leave_the_page_in_place() {
    let projection = json!({ "turnItems": [page("a", sample()["direct"].clone()), reply()] });
    let tagged = with_tokens(transcript(&projection), &projection);
    assert_eq!(tagged["renders"][0]["attachmentId"], "att-spend-html");
    assert_eq!(tagged["messages"][0]["text"], "See the chart.");
    assert!(tagged["partial"].is_boolean());
}

#[test]
fn a_page_changes_the_chat_tag_and_a_chat_without_one_does_not_gain_the_key() {
    let body = json!({
        "messages": [{ "id": "m", "text": "hi" }],
        "events": [],
        "requests": [],
        "working": false,
        "pins": [],
        "readOnly": false,
        "threadId": "t",
    });
    let tag = transcript_tag(&body);
    let mut with_page = body.clone();
    with_page["renders"] = json!([{ "id": "render-a" }]);
    assert_ne!(transcript_tag(&with_page), tag);
    assert_eq!(transcript_tag(&body), tag);
}

#[test]
fn a_transcript_without_pages_matches_the_golden() {
    let projection = fixture("thread-projection.json");
    let projected = transcript(&projection["value"]);
    assert!(projected.get("renders").is_none());
    assert_eq!(projected["messages"].as_array().unwrap().len(), 6);
    assert!(projected["events"].as_array().unwrap().is_empty());
    let got = serde_json::to_string(&projected).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/t3/thread-transcript.golden.json");
    let expected = std::fs::read_to_string(&path).unwrap();
    assert_eq!(got, expected.trim_end());
}
