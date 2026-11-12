use super::window::{around, Role};
use serde_json::{json, Value};

fn message(id: &str, kind: &str, text: &str) -> Value {
    json!({
        "id": id,
        "type": kind,
        "text": text,
        "status": "completed",
        "startedAt": "2026-01-01T00:00:00Z",
    })
}

fn history(count: usize) -> Value {
    let items: Vec<Value> = (0..count)
        .map(|i| {
            let kind = if i % 2 == 0 {
                "user_message"
            } else {
                "assistant_message"
            };
            json!({
                "id": format!("m{i}"),
                "type": kind,
                "text": format!("message {i}"),
                "status": "completed",
                "startedAt": format!("2026-01-01T00:00:{i:02}Z"),
            })
        })
        .collect();
    json!({ "turnItems": items, "partial": true })
}

#[test]
fn window_at_the_start_of_history() {
    let page = around(&history(6), "m0");
    let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["m0", "m1", "m2", "m3"]);
    assert_eq!(page.anchor, Some(0));
    assert_eq!(page.messages[0].at, "2026-01-01T00:00:00Z");
    assert_eq!(page.messages[0].role, Role::You);
    assert!(page.truncated);
}

#[test]
fn window_at_the_end_of_history() {
    let mut projection = history(6);
    projection["partial"] = json!(false);
    let page = around(&projection, "m5");
    let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["m2", "m3", "m4", "m5"]);
    assert_eq!(page.anchor, Some(3));
    assert_eq!(page.messages[3].role, Role::Bot);
    assert!(!page.truncated);
}

#[test]
fn a_message_is_cut_to_1200_characters() {
    let long = "é".repeat(1_201);
    let projection = json!({
        "turnItems": [
            message("long", "user_message", &long),
            message("next", "assistant_message", "ok"),
        ],
    });
    let page = around(&projection, "long");
    assert_eq!(page.messages[0].text.chars().count(), 1_200);
    assert!(page.messages[0].text.chars().all(|c| c == 'é'));
    assert_eq!(page.messages[1].text, "ok");
}

#[test]
fn a_routine_turn_is_labelled_routine() {
    let projection = json!({
        "turnItems": [
            message("1", "user_message", "hello"),
            json!({
                "id": "2",
                "type": "user_message",
                "text": "daily check",
                "status": "completed",
                "scheduledTaskId": "scheduled-task:1",
                "startedAt": "2026-01-02T00:00:00Z",
            }),
            message("3", "assistant_message", "done"),
        ],
    });
    let page = around(&projection, "2");
    assert_eq!(
        page.messages
            .iter()
            .map(|m| m.role.as_str())
            .collect::<Vec<_>>(),
        vec!["you", "routine", "bot"]
    );
    let routine = page.messages.iter().find(|m| m.id == "2").expect("routine");
    assert_eq!(routine.role, Role::Routine);
    assert_ne!(routine.role.as_str(), "you");
    assert_eq!(routine.text, "daily check");
}

#[test]
fn framed_turns_are_left_out() {
    let projection = json!({
        "turnItems": [
            message("u0", "user_message", "hello"),
            message("b1", "assistant_message", "hi"),
            message("u2", "user_message", "more"),
            message("b3", "assistant_message", "and more"),
            message("g4", "user_message", "[Group \"Duo\": Ada (@ada). You wrote:]\nask"),
            message("g5", "assistant_message", "from the group"),
            message("u6", "user_message", "thanks"),
            message("b7", "assistant_message", "ok"),
        ],
    });
    let page = around(&projection, "u6");
    let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["b1", "u2", "b3", "u6", "b7"]);
    assert!(page
        .messages
        .iter()
        .all(|m| { !m.text.contains("[Group") && m.text != "from the group" }));

    let hidden = around(&projection, "g4");
    assert!(hidden.messages.is_empty());
    assert!(hidden.anchor.is_none());

    let computer = json!({
        "turnItems": [
            message("1", "user_message", "hi"),
            message("2", "user_message", "[Computer] look here"),
            message("3", "assistant_message", "opened"),
            message("4", "user_message", "thanks"),
        ],
    });
    let page = around(&computer, "4");
    let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["1", "4"]);
    assert!(page
        .messages
        .iter()
        .all(|m| m.id != "2" && m.id != "3" && !m.text.contains("[Computer]")));
}
