use super::*;
use crate::harness::t3::setup::scopes_allow_preview;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

#[test]
fn options_take_values_where_the_node_gateway_does() {
    let (flags, rest) = parse_args(&args(&[
        "--port",
        "4000",
        "--t3-url=http://x/",
        "--no-service",
        "--yes",
        "1.2.3",
    ]));
    assert_eq!(flags.value("port"), Some("4000"));
    assert_eq!(flags.value("t3-url"), Some("http://x/"));
    assert!(flags.disabled("service"));
    assert!(flags.has("yes"));
    assert!(!flags.has("refresh-token"));
    assert!(!flags.has("preview"));
    let (refresh, _) = parse_args(&args(&["--refresh-token"]));
    assert!(refresh.has("refresh-token"));
    assert_eq!(rest, args(&["1.2.3"]));
}

#[test]
fn a_value_flag_at_the_end_is_not_a_value() {
    let (flags, _) = parse_args(&args(&["--port"]));
    assert_eq!(flags.value("port"), None);
}

#[test]
fn env_lines_are_read_like_the_node_gateway_reads_them() {
    let text = "# comment\nPORT=3780\n  T3_URL = \"http://127.0.0.1:3773\"\nHOST='127.0.0.1'\nlower=no\nEMPTY=\n";
    let map: HashMap<String, String> = parse_env(text).into_iter().collect();
    assert_eq!(map["PORT"], "3780");
    assert_eq!(map["T3_URL"], "http://127.0.0.1:3773");
    assert_eq!(map["HOST"], "127.0.0.1");
    assert_eq!(map["EMPTY"], "");
    assert!(!map.contains_key("lower"));
}

#[test]
fn setting_a_key_replaces_only_its_line() {
    let text = "PORTX=1\nPORT=3780\nHOST=127.0.0.1\n";
    assert_eq!(
        set_env_text(text, "PORT", "4000"),
        "PORTX=1\nPORT=4000\nHOST=127.0.0.1\n"
    );
    assert_eq!(set_env_text("HOST=a", "PORT", "1"), "HOST=a\nPORT=1\n");
    assert_eq!(set_env_text("", "PORT", "1"), "PORT=1\n");
}

#[test]
fn help_lists_the_commands() {
    for command in [
        "up", "status", "doctor", "update", "rollback", "service", "serve",
    ] {
        assert!(
            HELP.contains(&format!("  {command}")),
            "{command} missing from help"
        );
    }
}

#[test]
fn session_scopes_need_preview_operate() {
    assert!(scopes_allow_preview(&json!({
        "authenticated": true,
        "scopes": ["orchestration:operate", "preview:operate"]
    })));
    assert!(!scopes_allow_preview(&json!({
        "authenticated": true,
        "scopes": ["orchestration:operate"]
    })));
    assert!(!scopes_allow_preview(&json!({ "authenticated": false })));
    assert!(!scopes_allow_preview(&json!({
        "scopes": "preview:operate",
        "permissions": ["preview:operate"]
    })));
}

#[tokio::test]
async fn help_and_version_end_without_starting_the_gateway() {
    assert_eq!(run(&args(&["help"])).await, Some(0));
    assert_eq!(run(&args(&["--version"])).await, Some(0));
    assert_eq!(run(&args(&["serve"])).await, None);
    assert_eq!(run(&[]).await, None);
    assert_eq!(run(&args(&["nonsense"])).await, Some(2));
}
