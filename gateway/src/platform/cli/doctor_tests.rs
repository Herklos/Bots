use super::host::{parse_df_available, tail_lines};
use super::{
    network_choice, render_json, render_text, secrets_from, EnvView, Report, TokenReport,
    LOG_BYTES, LOG_LINES,
};
use crate::harness::providers::ProviderHealth;
use crate::harness::t3::setup::token_from_session;
use crate::platform::redact::{redact, Secrets};
use serde_json::{json, Value};
use std::collections::HashMap;

fn empty_maps() -> (HashMap<String, String>, HashMap<String, String>) {
    (HashMap::new(), HashMap::new())
}

fn sample(log: &str) -> Report {
    Report {
        version: "0.3.4".into(),
        install_path: "/home/alice/.hbots/runtime/current/hbots".into(),
        installed: true,
        service_supported: true,
        service_installed: true,
        service_state: "installed".into(),
        port: 3780,
        network: "local".into(),
        network_error: None,
        gateway_running: false,
        gateway_version: None,
        t3_url: "http://127.0.0.1:3773".into(),
        t3_reachable: true,
        token: TokenReport::Accepted {
            scopes: vec!["preview:operate".into()],
        },
        providers: vec![ProviderHealth {
            instance_id: "claudeAgent".into(),
            display_name: "Claude Code".into(),
            enabled: true,
            installed: true,
            status: "ready".into(),
            models: 2,
        }],
        providers_error: None,
        storage_path: "/home/alice/.hbots/bots".into(),
        free_bytes: Some(1_500_000_000),
        logs: vec![log.into()],
    }
}

#[test]
fn the_report_redacts_secrets_in_text_and_json() {
    let log = "Bearer t3tok_live_secret_value for ada@example.com at /home/alice/.hbots";
    let secrets = Secrets {
        token: Some("t3tok_live_secret_value".into()),
        password: Some("hunter2-password".into()),
        extras: vec![],
        home: Some("/home/alice".into()),
    };
    let text = redact(&render_text(&sample(log)), &secrets);
    let json_text = redact(&render_json(&sample(log)), &secrets);
    assert!(text.contains("hbots 0.3.4"));
    assert!(text.contains("Token:     valid, scopes: preview:operate"));
    assert!(text.contains("Providers: 1 ready (Claude Code ready)"));
    assert!(text.contains("Network:   local"));
    assert!(text.contains("1.5 GB free"));
    assert!(!text.contains("t3tok_live_secret_value"));
    assert!(!text.contains("ada@example.com"));
    assert!(!text.contains("/home/alice"));
    assert!(text.contains('~'));
    let parsed: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(parsed["t3"]["token"]["valid"], true);
    assert_eq!(parsed["t3"]["token"]["scopes"][0], "preview:operate");
    assert!(parsed["t3"]["token"].get("value").is_none());
    assert!(!json_text.contains("t3tok_live_secret_value"));
    assert!(!parsed["logs"][0].as_str().unwrap().contains('@'));
}

#[test]
fn logs_keep_the_last_200_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gateway.log");
    let mut body = String::new();
    for index in 0..250 {
        body.push_str(&format!("line-{index}\n"));
    }
    std::fs::write(&path, body).unwrap();
    let lines = tail_lines(&path, LOG_LINES, LOG_BYTES);
    assert_eq!(lines.len(), 200);
    assert_eq!(lines[0], "line-50");
    assert_eq!(lines[199], "line-249");
}

#[test]
fn a_long_log_is_read_from_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gateway.log");
    let mut body = String::new();
    for index in 0..50 {
        body.push_str(&format!("line-{index:04} {}\n", "x".repeat(100)));
    }
    std::fs::write(&path, body).unwrap();
    let lines = tail_lines(&path, LOG_LINES, 500);
    assert!(lines.len() < 50);
    assert!(lines.iter().all(|line| line.starts_with("line-")));
    assert!(lines.last().unwrap().starts_with("line-0049"));
}

#[test]
fn df_available_is_the_fourth_field_in_kibibytes() {
    let text =
        "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk 100 40 60 40% /\n";
    assert_eq!(parse_df_available(text), Some(60 * 1024));
}

#[test]
fn a_saved_lan_choice_is_the_network_mode() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("bots/.botapp");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("access.json"), r#"{"network":"lan"}"#).unwrap();
    let (file, process) = empty_maps();
    let view = EnvView {
        file: &file,
        process: &process,
    };
    let choice = network_choice(dir.path(), &view);
    assert_eq!(choice.mode, crate::network::Mode::Lan);
    assert!(!choice.locked);
}

#[test]
fn a_token_file_is_loaded_without_being_the_report() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".t3-token"), "t3tok_from_file\n").unwrap();
    let (file, process) = empty_maps();
    let view = EnvView {
        file: &file,
        process: &process,
    };
    let secrets = secrets_from(dir.path(), &view);
    assert_eq!(secrets.token.as_deref(), Some("t3tok_from_file"));
    let report = Report {
        logs: vec!["using t3tok_from_file today".into()],
        install_path: dir.path().display().to_string(),
        storage_path: dir.path().join("bots").display().to_string(),
        ..sample("quiet")
    };
    let text = redact(&render_text(&report), &secrets);
    assert!(!text.contains("t3tok_from_file"));
    assert!(text.contains("[redacted]"));
}

#[test]
fn session_checks_keep_scopes_and_mark_a_rejection() {
    let rejected = token_from_session(None);
    assert!(!rejected.valid);
    assert!(rejected.scopes.is_empty());
    let body = json!({ "authenticated": false, "scopes": ["preview:operate"] });
    let check = token_from_session(Some(&body));
    assert!(!check.valid);
    assert_eq!(check.scopes, vec!["preview:operate".to_string()]);
    let accepted = token_from_session(Some(&json!({ "scopes": ["orchestration:operate"] })));
    assert!(accepted.valid);
    assert_eq!(accepted.scopes, vec!["orchestration:operate".to_string()]);
}
