use super::*;
use crate::storage::adapters::memory::MemoryWorkspace;

fn lib() -> Library {
    Library::new(
        Arc::new(MemoryWorkspace::new()),
        "http://127.0.0.1:3780/mcp/bot".into(),
        "tok".into(),
    )
}

#[test]
fn groups_and_kinds() {
    assert_eq!(group_day("2026-11-10", "2026-11-10", "2026-11-04"), "Today");
    assert_eq!(
        group_day("2026-11-08", "2026-11-10", "2026-11-04"),
        "This week"
    );
    assert_eq!(group_day("2026-01-01", "2026-11-10", "2026-11-04"), "Older");
    assert_eq!(group_day("2026-12-01", "2026-11-10", "2026-11-04"), "");
    assert_eq!(group_day("", "2026-11-10", "2026-11-04"), "");
    assert_eq!(file_kind("Report.html"), "page");
    assert_eq!(file_kind("shot.PNG"), "media");
    assert_eq!(file_kind("notes.txt"), "file");
    assert!(ascii_ext("meta.JSON", "json"));
    assert!(!ascii_ext("meta.json.bak", "json"));
    assert_eq!(media_kind("image/png"), "media");
    assert_eq!(media_kind("application/pdf"), "file");
}

#[test]
fn links_drop_junk_and_duplicates() {
    let found =
        web_links("See https://example.com/a), and https://example.com/a again. Not ftp://x");
    assert_eq!(found, vec!["https://example.com/a".to_string()]);
}

#[tokio::test]
async fn records_round_trip_through_the_workspace() {
    let library = lib();
    library
        .save_item("ada", &json!({ "kind": "plan", "title": "Week" }), 0)
        .await
        .unwrap();
    library
        .save_item("ada", &json!({ "kind": "note", "title": "Milk" }), 0)
        .await
        .unwrap();
    library
        .save_item(
            "ada",
            &json!({ "kind": "task", "title": "Call", "status": "doing" }),
            0,
        )
        .await
        .unwrap();
    library
        .save_item("ada", &json!({ "kind": "project", "title": "Home" }), 0)
        .await
        .unwrap();
    let err = library
        .save_item(
            "ada",
            &json!({ "kind": "plan", "title": "Bad", "status": "nope" }),
            0,
        )
        .await;
    assert!(err.is_err());
    let view = library
        .view(
            "ada",
            &[json!({ "text": "https://example.com/x", "at": "2026-11-10T00:00:00Z" })],
            1_762_732_800, // 2026-11-10
        )
        .await
        .unwrap();
    let kinds: Vec<_> = view["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"plan"));
    assert!(kinds.contains(&"note"));
    assert!(kinds.contains(&"task"));
    assert!(kinds.contains(&"project"));
    assert!(kinds.contains(&"link"));
    let plan = view["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "plan")
        .unwrap();
    assert_eq!(plan["status"], "draft");
    let id = plan["id"].as_str().unwrap();
    library.delete_item("ada", id).await.unwrap();
    let again = library.view("ada", &[], 1_762_732_800).await.unwrap();
    assert!(again["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["id"] != id));
    let raw = library.ws.read("ada/.bot/library.json").await.unwrap();
    assert!(raw.starts_with(b"{"));
}

#[tokio::test]
async fn files_stay_inside_library() {
    let library = lib();
    library
        .ws
        .write("ada/library/page.html", b"<p>kept</p>", None)
        .await
        .unwrap();
    library
        .ws
        .write("ada/secret.txt", b"no", None)
        .await
        .unwrap();
    let view = library.view("ada", &[], 1_700_000_000).await.unwrap();
    let titles: Vec<_> = view["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["page.html"]);
    assert_eq!(
        library
            .file_bytes("ada", "library/page.html")
            .await
            .unwrap(),
        b"<p>kept</p>"
    );
    assert!(library.file_bytes("ada", "../secret.txt").await.is_err());
    assert!(library.file_bytes("ada", "secret.txt").await.is_err());
    assert!(library.view("../ada", &[], 0).await.is_err());
}

#[tokio::test]
async fn connector_switch_rewrites_harness_files_and_hides_the_secret() {
    let library = lib();
    library
        .save_connector(
            "ada",
            &json!({ "name": "linear", "url": "https://mcp.example/linear", "enabled": true, "secret": "s3cret" }),
        )
        .await
        .unwrap();
    let view = library.view("ada", &[], 0).await.unwrap();
    let shown = &view["connectors"];
    let linear = shown
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "linear")
        .unwrap();
    assert_eq!(linear["enabled"], true);
    assert_eq!(linear["hasSecret"], true);
    assert!(linear.get("secret").is_none());
    let bot = shown
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "bot")
        .unwrap();
    assert_eq!(bot["locked"], true);
    let mcp: Value =
        serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
    assert_eq!(
        mcp["mcpServers"]["linear"]["url"],
        "https://mcp.example/linear"
    );
    assert!(mcp["mcpServers"]["bot"]["url"]
        .as_str()
        .unwrap()
        .contains("/mcp/bot"));
    let toml = String::from_utf8(library.ws.read("ada/.codex/config.toml").await.unwrap()).unwrap();
    assert!(toml.contains("[mcp_servers.linear]"));
    assert!(toml.contains("[mcp_servers.bot]"));
    library
        .save_connector(
            "ada",
            &json!({ "name": "linear", "url": "https://mcp.example/linear", "enabled": false }),
        )
        .await
        .unwrap();
    let mcp: Value =
        serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
    assert!(mcp["mcpServers"].get("linear").is_none());
    assert!(mcp["mcpServers"].get("bot").is_some());
    let claude: Value =
        serde_json::from_slice(&library.ws.read("ada/.claude/settings.json").await.unwrap())
            .unwrap();
    assert_eq!(claude["disabledMcpjsonServers"][0], "linear");
    assert!(library
        .save_connector(
            "ada",
            &json!({ "name": "executor", "url": "https://x", "enabled": true }),
        )
        .await
        .is_err());
    assert!(library
        .save_connector(
            "ada",
            &json!({ "name": "ok", "url": "ftp://x", "enabled": true }),
        )
        .await
        .is_err());
}

#[test]
fn merge_never_prints_a_disabled_secret_into_mcp() {
    let mcp = merge_mcp(
        &json!({}),
        &[
            json!({ "name": "linear", "url": "https://mcp.example/a", "enabled": false, "secret": "s3cret" }),
        ],
        "http://127.0.0.1/mcp/bot",
        "tok",
        "ada",
    );
    let text = mcp.to_string();
    assert!(!text.contains("s3cret"));
    assert!(!text.contains("linear"));
    assert!(text.contains("bot"));
}

#[tokio::test]
async fn every_board_kind_gets_its_default_and_a_note_has_none() {
    let library = lib();
    for (kind, status) in [
        ("project", "active"),
        ("task", "todo"),
        ("plan", "draft"),
        ("machine", "unknown"),
        ("service", "unknown"),
        ("repo", "active"),
        ("link", ""),
        ("note", ""),
    ] {
        library
            .save_item("ada", &json!({ "kind": kind, "title": kind }), 0)
            .await
            .unwrap();
        let view = library.view("ada", &[], 0).await.unwrap();
        let row = view["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == kind)
            .unwrap();
        assert_eq!(row["status"], status, "{kind}");
    }
    library
        .save_item(
            "ada",
            &json!({ "kind": "note", "title": "Aside", "status": "active" }),
            0,
        )
        .await
        .unwrap();
    let view = library.view("ada", &[], 0).await.unwrap();
    let aside = view["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "Aside")
        .unwrap();
    assert_eq!(aside["status"], "");
    assert!(library
        .save_item("ada", &json!({ "kind": "genealogy", "title": "No" }), 0)
        .await
        .is_err());
    assert!(library
        .save_item("ada", &json!({ "kind": "note", "title": "  " }), 0)
        .await
        .is_err());
    assert!(library
        .save_item(
            "ada",
            &json!({ "id": "missing", "kind": "note", "title": "Later" }),
            0
        )
        .await
        .is_err());
}

#[tokio::test]
async fn an_empty_secret_keeps_the_token_and_delete_drops_the_server() {
    let library = lib();
    library
        .save_connector(
            "ada",
            &json!({ "name": "notes", "url": "http://127.0.0.1:9/mcp", "enabled": true, "secret": "s3cret" }),
        )
        .await
        .unwrap();
    library
        .save_connector(
            "ada",
            &json!({ "name": "notes", "url": "https://mcp.example/notes", "enabled": true, "secret": "" }),
        )
        .await
        .unwrap();
    let shown = library.public_connectors("ada").await.unwrap();
    let notes = shown.iter().find(|c| c["name"] == "notes").unwrap();
    assert_eq!(notes["hasSecret"], true);
    assert_eq!(notes["url"], "https://mcp.example/notes");
    assert!(notes.get("secret").is_none());
    let mcp: Value =
        serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
    assert!(mcp["mcpServers"]["notes"]["headers"]["Authorization"]
        .as_str()
        .unwrap()
        .contains("s3cret"));
    library.delete_connector("ada", "notes").await.unwrap();
    let mcp: Value =
        serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
    assert!(mcp["mcpServers"].get("notes").is_none());
    assert!(mcp["mcpServers"].get("bot").is_some());
    let toml = String::from_utf8(library.ws.read("ada/.codex/config.toml").await.unwrap()).unwrap();
    assert!(!toml.contains("notes"));
    assert!(!toml.contains("s3cret"));
    let claude: Value =
        serde_json::from_slice(&library.ws.read("ada/.claude/settings.json").await.unwrap())
            .unwrap();
    assert_eq!(claude["disabledMcpjsonServers"], json!(["notes"]));
    assert!(library.delete_connector("ada", "bot").await.is_err());
    assert!(library.delete_connector("ada", "notes").await.is_err());
}

#[test]
fn connector_names_and_urls_are_short_http_tokens() {
    assert!(validate_connector_name("bot").is_err());
    assert!(validate_connector_name("executor").is_err());
    assert!(validate_connector_name("").is_err());
    assert!(validate_connector_name("has space").is_err());
    assert!(validate_connector_name(&"a".repeat(33)).is_err());
    assert!(validate_connector_name(&"a".repeat(32)).is_ok());
    assert!(validate_connector_name("linear_1").is_ok());
    assert!(validate_connector_url("http://127.0.0.1:9/mcp").is_ok());
    assert!(validate_connector_url("https://mcp.example/a").is_ok());
    assert!(validate_connector_url("ftp://mcp.example/a").is_err());
    assert!(validate_connector_url("https://").is_err());
    assert!(validate_connector_url("").is_err());
}

#[tokio::test]
async fn paths_outside_library_are_refused() {
    let library = lib();
    library
        .ws
        .write("ada/library/page.html", b"<p>kept</p>", None)
        .await
        .unwrap();
    assert!(library
        .file_bytes("ada", "library/../secret.txt")
        .await
        .is_err());
    assert!(library.file_bytes("ada", "").await.is_err());
    assert!(library.file_bytes("ada", ".").await.is_err());
    assert!(library
        .file_bytes("ada", "library/missing.html")
        .await
        .is_err());
    assert_eq!(
        library
            .file_bytes("ada", "/library/page.html")
            .await
            .unwrap(),
        b"<p>kept</p>"
    );
}

#[test]
fn harness_files_sort_servers_and_disable_only_what_is_off() {
    let mcp = merge_mcp(
        &json!({}),
        &[
            json!({ "name": "zebra", "url": "https://mcp.example/z", "enabled": true, "secret": "z" }),
            json!({ "name": "notes", "url": "https://mcp.example/n", "enabled": false, "secret": "hidden" }),
            json!({ "name": "bot", "url": "https://nope", "enabled": false, "secret": "no" }),
        ],
        "http://127.0.0.1/mcp/bot",
        "tok",
        "ada",
    );
    let toml = codex_toml(&mcp);
    let bot_at = toml.find("[mcp_servers.bot]").unwrap();
    let zebra_at = toml.find("[mcp_servers.zebra]").unwrap();
    assert!(bot_at < zebra_at);
    assert!(!toml.contains("notes"));
    assert!(!toml.contains("hidden"));
    assert!(toml.contains("tool_timeout_sec = 960"));
    assert!(toml.contains("https://mcp.example/z"));
    let servers = json!([
        { "name": "notes", "enabled": false },
        { "name": "zebra", "enabled": true },
        { "name": "bot", "enabled": false }
    ]);
    let claude = claude_settings(&json!({}), servers.as_array().unwrap());
    assert_eq!(claude["enableAllProjectMcpServers"], true);
    assert_eq!(claude["disabledMcpjsonServers"], json!(["notes"]));
}

#[cfg(unix)]
#[test]
fn claude_settings_adds_the_agents_md_hook_once() {
    let once = claude_settings(&json!({}), &[]);
    let twice = claude_settings(&once, &[]);
    assert_eq!(
        twice["hooks"]["UserPromptSubmit"].as_array().unwrap().len(),
        1
    );
    assert_eq!(twice["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    let prompt = twice["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(prompt.contains(".bot/hooks/agents-md.sh"));
    assert!(prompt.ends_with("prompt"));
    let other = json!({ "hooks": { "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "true" }] }] } });
    let merged = claude_settings(&other, &[]);
    assert_eq!(
        merged["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn the_shelf_keeps_pictures_files_and_links_and_drops_records_and_connectors() {
    let view = json!({ "entries": [
        { "id": "attachment:a1", "kind": "media", "title": "chart.png", "detail": "image/png", "group": "Today" },
        { "id": "attachment:a2", "kind": "media", "title": "clip.mp4", "detail": "video/mp4", "group": "Today" },
        { "id": "file:ada/library/Notes today.html", "kind": "page", "title": "Notes today.html", "detail": "Private", "group": "Today" },
        { "id": "file:ada/library/plan.pdf", "kind": "file", "title": "plan.pdf", "detail": "Workspace file", "group": "Older" },
        { "id": "https://example.com/a", "kind": "link", "title": "example.com", "detail": "https://example.com/a", "group": "Today" },
        { "id": "abc123", "kind": "task", "title": "Ship it", "detail": "doing", "group": "Today" },
        { "id": "xyz", "kind": "note", "title": "A note", "detail": "", "group": "Today" },
    ], "connectors": [{ "name": "notes" }] });
    let shelf = shelf_from("ada", &view);
    assert_eq!(
        shelf["counts"],
        json!({ "pictures": 1, "files": 3, "links": 1 })
    );
    assert_eq!(shelf["pictures"][0]["url"], "/api/bots/ada/attachments/a1");
    assert_eq!(
        shelf["files"][1]["url"],
        "/api/bots/ada/library/file?path=library/Notes%20today.html"
    );
    assert_eq!(shelf["links"][0]["url"], "https://example.com/a");
    assert!(shelf.get("connectors").is_none());
    assert!(!shelf.to_string().contains("Ship it"));
}
