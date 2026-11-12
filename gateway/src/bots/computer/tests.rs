use super::*;
use crate::harness::t3::stream::{
    hold_if_ready, preview_stream_url, tabs_for_screen, WatchSize, CANNOT_OPERATE, CARD_MAX_HEIGHT,
    CARD_MAX_WIDTH, CARD_QUALITY, VIEW_MAX_HEIGHT, VIEW_MAX_WIDTH, VIEW_QUALITY,
};

#[test]
fn control_can_operate_false_is_that_error_and_a_take_is_not_held() {
    let denied = json!({ "type": "control", "controller": "you", "canOperate": false });
    let mut sessions = Sessions::default();
    let err = hold_if_ready(&mut sessions, "ada", "tab-1", &denied).unwrap_err();
    assert_eq!(err, CANNOT_OPERATE);
    assert!(sessions.get("ada").is_none());

    let allowed = json!({ "type": "control", "controller": "you", "canOperate": true });
    hold_if_ready(&mut sessions, "ada", "tab-1", &allowed).unwrap();
    assert_eq!(
        sessions.get("ada").map(|hold| hold.tab.as_str()),
        Some("tab-1")
    );
}

#[test]
fn preview_sizes_are_sharper_for_the_large_view_and_a_hold() {
    let card = preview_stream_url("ws://t3", "thread", "tab", false, WatchSize::Card).unwrap();
    let pairs = card.query_pairs().collect::<Vec<_>>();
    assert!(pairs.contains(&("maxWidth".into(), CARD_MAX_WIDTH.into())));
    assert!(pairs.contains(&("maxHeight".into(), CARD_MAX_HEIGHT.into())));
    assert!(pairs.contains(&("quality".into(), CARD_QUALITY.into())));
    assert!(pairs.contains(&("interactive".into(), "false".into())));

    let large = preview_stream_url("ws://t3", "thread", "tab", false, WatchSize::Large).unwrap();
    let sharp = large.query_pairs().collect::<Vec<_>>();
    assert!(sharp.contains(&("maxWidth".into(), VIEW_MAX_WIDTH.into())));
    assert!(sharp.contains(&("maxHeight".into(), VIEW_MAX_HEIGHT.into())));
    assert!(sharp.contains(&("quality".into(), VIEW_QUALITY.into())));

    let held = preview_stream_url("ws://t3", "thread", "tab", true, WatchSize::Card).unwrap();
    let driving = held.query_pairs().collect::<Vec<_>>();
    assert!(driving.contains(&("interactive".into(), "true".into())));
    assert!(driving.contains(&("maxWidth".into(), VIEW_MAX_WIDTH.into())));
    assert!(driving.contains(&("maxHeight".into(), VIEW_MAX_HEIGHT.into())));
    assert!(driving.contains(&("quality".into(), VIEW_QUALITY.into())));
    assert!(!held.as_str().contains("resize"));
}

#[test]
fn watch_acks_and_does_not_take_control() {
    assert_eq!(watch_message()["type"], "ack");
    assert_ne!(watch_message()["type"], "takeControl");
    assert_eq!(control_message("take").unwrap()["type"], "takeControl");
    assert_eq!(
        control_message("release").unwrap()["type"],
        "releaseControl"
    );
    assert!(control_message("steal").is_err());
}

#[test]
fn open_command_steers_with_the_computer_frame() {
    let command = open_command("thread-ada");
    assert_eq!(command["type"], "message.dispatch");
    assert_eq!(command["threadId"], "thread-ada");
    assert_eq!(command["createdBy"], "user");
    assert_eq!(command["creationSource"], "web");
    assert_eq!(command["deliveryIntent"], "steer");
    assert_eq!(command["dispatchMode"]["type"], "start_immediately");
    assert_eq!(command["attachments"], json!([]));
    let text = command["text"].as_str().unwrap();
    assert!(text.contains("[Computer]"));
    assert!(text.contains("https://www.google.com"));
    assert!(text.contains("where you left off") || text.contains("left off"));
    assert_eq!(text, OPEN_PROMPT);
    assert_eq!(command["commandId"].as_str(), command["messageId"].as_str());
    assert!(!command["messageId"].as_str().unwrap_or("").is_empty());
}

#[test]
fn close_command_steers_with_the_computer_frame() {
    let command = close_command("thread-ada");
    assert_eq!(command["type"], "message.dispatch");
    assert_eq!(command["threadId"], "thread-ada");
    assert_eq!(command["deliveryIntent"], "steer");
    assert_eq!(command["attachments"], json!([]));
    let text = command["text"].as_str().unwrap();
    assert!(text.contains("[Computer]"));
    assert!(text.contains("Close your browser"));
    assert_eq!(text, CLOSE_PROMPT);
    assert_ne!(text, OPEN_PROMPT);
}

#[test]
fn only_server_tabs_are_listed() {
    let listed = tabs_for_screen(&json!({
        "sessions": [
            { "runtime": "server", "tabId": "t1", "navStatus": { "url": "https://example.com/a" } },
            { "runtime": "local", "tabId": "no", "navStatus": { "url": "https://hidden.example" } }
        ]
    }));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], "t1");
    assert_eq!(listed[0]["title"], "example.com");
}

#[test]
fn an_empty_list_and_a_bare_address_stay_honest() {
    assert!(tabs_for_screen(&json!({ "sessions": [] })).is_empty());
    assert!(tabs_for_screen(&json!({})).is_empty());
    let listed = tabs_for_screen(&json!({
        "sessions": [
            { "runtime": "server", "tabId": "blank", "navStatus": { "url": "" } },
            { "runtime": "server", "tabId": "raw", "navStatus": { "url": "not a url" } }
        ]
    }));
    assert_eq!(listed[0]["title"], "Page");
    assert_eq!(listed[1]["title"], "not a url");
    assert_eq!(listed[1]["url"], "not a url");
}

#[test]
fn a_click_fraction_maps_into_the_frame_and_does_not_take_control() {
    let messages = click_messages(0.5, 0.25, 800, 400).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["action"], "move");
    assert_eq!(messages[1]["action"], "down");
    assert_eq!(messages[2]["action"], "up");
    assert_eq!(messages[0]["x"], 400);
    assert_eq!(messages[0]["y"], 100);
    assert_eq!(messages[1]["x"], 400);
    assert_eq!(messages[2]["y"], 100);
    assert!(messages
        .iter()
        .all(|message| message["type"] != "takeControl"));
    assert!(click_messages(1.5, 0.0, 800, 400).is_err());
}

#[test]
fn an_unknown_tab_falls_back_and_a_non_server_tab_is_omitted() {
    let listed = tabs_for_screen(&json!({
        "sessions": [
            { "runtime": "server", "tabId": "first", "navStatus": { "url": "https://one.example" } },
            { "runtime": "local", "tabId": "hidden", "navStatus": { "url": "https://hidden.example" } },
            { "runtime": "server", "tabId": "second", "navStatus": { "url": "https://two.example" } }
        ]
    }));
    assert!(listed.iter().all(|tab| tab["id"] != "hidden"));
    assert_eq!(frame_tab(&listed, Some("missing")).unwrap()["id"], "first");
    assert_eq!(frame_tab(&listed, Some("second")).unwrap()["id"], "second");
    assert_eq!(frame_tab(&listed, None).unwrap()["id"], "first");
}

#[test]
fn keys_use_the_browser_stream_shape_and_escape_is_not_back() {
    let enter = key_message("Enter", 0).unwrap();
    assert_eq!(enter["type"], "key");
    assert_eq!(enter["key"], "Enter");
    assert_eq!(enter["code"], "Enter");
    assert_eq!(enter["keyCode"], 13);
    assert_eq!(enter["modifiers"], 0);
    assert!(enter.get("keycode").is_none());
    assert_eq!(key_message("Backspace", 0).unwrap()["key"], "Backspace");
    assert_eq!(key_message("Tab", 0).unwrap()["code"], "Tab");
    assert_eq!(key_message("ArrowLeft", 0).unwrap()["key"], "ArrowLeft");
    assert_eq!(key_message("Delete", 0).unwrap()["key"], "Delete");
    assert_eq!(key_message("PageDown", 0).unwrap()["code"], "PageDown");
    let escape = key_message("Escape", 0);
    assert!(escape.is_err());
    assert_ne!(escape.unwrap_or(json!({ "type": "nope" }))["type"], "back");
    let copy = edit_message("copy").unwrap();
    assert_eq!(copy["key"], "c");
    assert_eq!(copy["code"], "KeyC");
    assert_eq!(copy["modifiers"], META);
    assert!(edit_message("paste").is_err());
}

#[test]
fn navigation_keys_carry_windows_key_codes() {
    let codes = [
        ("Enter", 13),
        ("Tab", 9),
        ("Backspace", 8),
        ("Delete", 46),
        ("ArrowLeft", 37),
        ("ArrowUp", 38),
        ("ArrowRight", 39),
        ("ArrowDown", 40),
        ("Home", 36),
        ("End", 35),
        ("PageUp", 33),
        ("PageDown", 34),
    ];
    for (key, code) in codes {
        let message = key_message(key, 0).unwrap();
        assert_eq!(message["keyCode"], code, "{key}");
        assert_eq!(message["key"], key);
        assert_eq!(message["modifiers"], 0);
    }
}

#[test]
fn key_modifiers_forward_shift_alt_and_ctrl() {
    let shift_enter = key_message("Enter", SHIFT).unwrap();
    assert_eq!(shift_enter["keyCode"], 13);
    assert_eq!(shift_enter["modifiers"], SHIFT);
    let chord = key_message("ArrowLeft", ALT | CTRL).unwrap();
    assert_eq!(chord["keyCode"], 37);
    assert_eq!(chord["modifiers"], ALT | CTRL);
    let all = key_message("Tab", ALT | CTRL | SHIFT).unwrap();
    assert_eq!(all["modifiers"], ALT | CTRL | SHIFT);
    assert!(key_message("Enter", META).is_err());
    assert!(key_message("Enter", -1).is_err());
    assert!(key_message("Enter", ALT | META).is_err());
}

#[test]
fn text_is_one_message_and_a_long_clip_is_refused_without_echoing_it() {
    let message = text_message("hello").unwrap();
    assert_eq!(message["type"], "text");
    assert_eq!(message["text"], "hello");
    let secret = "s".repeat(TEXT_CAP + 1);
    let refused = text_message(&secret).unwrap_err();
    assert_eq!(refused, "That text is too long.");
    assert!(!refused.contains(secret.as_str()));
}

#[test]
fn a_scroll_stays_in_frame_pixels() {
    let message = wheel_message(0.0, 1.0, 800, 400, -120.0).unwrap();
    assert_eq!(message["type"], "wheel");
    assert_eq!(message["x"], 0);
    assert_eq!(message["y"], 400);
    assert_eq!(message["deltaY"], -120.0);
    assert_ne!(message["type"], "takeControl");
}

#[test]
fn release_drops_the_held_page() {
    let mut sessions = Sessions::default();
    sessions.take("ada", "first", 800, 400);
    assert_eq!(
        sessions.get("ada").map(|hold| hold.tab.as_str()),
        Some("first")
    );
    sessions.release("ada");
    assert!(sessions.get("ada").is_none());
    let click = click_messages(0.5, 0.25, 800, 400).unwrap();
    assert!(click.iter().all(|message| message["type"] != "takeControl"));
    assert!(sessions.get("ada").is_none());
}

#[test]
fn a_letterboxed_point_maps_into_the_jpeg_and_the_margin_is_not_a_click() {
    let inside = contain_fractions(400.0, 300.0, 800.0, 800.0, 800, 400)
        .unwrap()
        .unwrap();
    assert_eq!(inside.0, 0.5);
    assert_eq!(inside.1, 0.25);
    assert!(contain_fractions(400.0, 10.0, 800.0, 800.0, 800, 400)
        .unwrap()
        .is_none());
    assert!(contain_fractions(400.0, 10.0, 0.0, 800.0, 800, 400).is_err());
}

#[test]
fn a_jpeg_sof_gives_the_frame_size_a_hold_keeps() {
    let bytes = [
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x90, 0x02, 0x80, 0x03, 0x01, 0x11, 0x00,
        0x02, 0x11, 0x00, 0x03, 0x11, 0x00, 0xff, 0xd9,
    ];
    assert_eq!(jpeg_size(&bytes), Some((640, 400)));
    assert!(jpeg_size(&[0xff, 0xd8, 0xff, 0xd9]).is_none());
    let mut sessions = Sessions::default();
    sessions.take("ada", "first", 0, 0);
    sessions.remember_size("ada", 640, 400);
    sessions.take("ada", "first", 0, 0);
    let hold = sessions.get("ada").unwrap();
    assert_eq!((hold.width, hold.height), (640, 400));
}

#[test]
fn the_picture_stays_with_the_held_session_until_release() {
    let mut sessions = Sessions::default();
    sessions.take("ada", "first", 800, 400);
    sessions.note_image("ada", "jpeg");
    assert_eq!(sessions.picture("ada", "first").as_deref(), Some("jpeg"));
    assert!(sessions.picture("ada", "second").is_none());
    sessions.take("ada", "first", 800, 400);
    assert_eq!(sessions.picture("ada", "first").as_deref(), Some("jpeg"));
    sessions.release("ada");
    assert!(sessions.picture("ada", "first").is_none());
}
