//! The Vault tools of the Bots' own tool server (`POST /mcp/bot`).
//! Browser fill uses the T3 preview stream, the harness wired today. Other backends may be added.
//!
//! - `vault_fill_password` / `vault_fill_username`: type a saved login into the focused field of
//!   the Bot's browser tab, through T3's preview stream (take control, type, probe, release).
//!   Only when the tab is on the login's site or one of its subdomains.
//! - `vault_save_login`: moves a value T3's `request_secret` wrote to a file into the Vault.
//! - `vault_secret_file`: writes an API token (a login saved with `botCanRead`) to a private file.
//! - `vault_save_contact`: saves the user's contact info for forms.
//!
//! A result, an error or a log line never contains a password, a token or the typed text.
use crate::{
    t3::{ws_base, T3},
    vault::{self, Vault},
    AppState,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::HeaderValue, Error as WsError, Message,
};

/// Limit for a file T3 wrote for `request_secret`.
const MAX_SECRET_BYTES: u64 = 16 * 1024;
/// How long a token file lives (`vault_secret_file`).
const SECRET_FILE_TTL: Duration = Duration::from_secs(60 * 60);
const TAB_LIST_TIMEOUT: Duration = Duration::from_secs(10);
const TYPE_TIMEOUT: Duration = Duration::from_secs(15);
/// The gap between typing and the probe, so the probe's answer means the text is in.
const PROBE_DELAY: Duration = Duration::from_millis(150);
const RELEASE_TIMEOUT: Duration = Duration::from_secs(1);
const CONTACT_FIELDS: &[&str] = &[
    "label", "name", "email", "phone", "street", "apt", "city", "state", "zip", "country", "kind",
    "hints",
];
const NO_LOGIN: &str = "No login with that id. Check vault_list.";
const NOT_A_TAB_STREAM: &str = "Couldn't connect to the browser.";
const SECRET_MISSING: &str = "That secret file doesn't exist. It may have expired (T3 deletes them after 24 hours); ask the user again with request_secret, delivery \"file\".";

/// The tool names this module serves; `vault_list` stays in `mcp.rs`.
const NAMES: [&str; 5] = [
    "vault_fill_password",
    "vault_fill_username",
    "vault_save_login",
    "vault_secret_file",
    "vault_save_contact",
];

/// What the tools need, passed in so tests can supply their own.
pub(crate) struct Deps<'a> {
    pub t3: &'a T3,
    pub vault: &'a Vault,
    /// The Bots, as `bots.list()` gives them (each has `mainThreadId`).
    pub bots: &'a [Value],
    /// Where T3's `request_secret` (delivery "file") leaves values.
    pub secrets_dir: &'a Path,
    /// A private folder for `vault_secret_file`'s files.
    pub token_dir: &'a Path,
}

/// `T3_SECRETS_DIR`, or T3's default `~/.t3/userdata/secrets/files`.
pub(crate) fn secrets_dir_from_env() -> PathBuf {
    std::env::var("T3_SECRETS_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".t3/userdata/secrets/files")
        })
}

/// The MCP tool definitions for this module.
pub(crate) fn tool_defs() -> Vec<Value> {
    let thread = json!({ "type": "string", "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/bot.json)." });
    let login = json!({ "type": "string", "description": "The login's id from vault_list." });
    let tab = json!({ "type": "string", "description": "Your browser tab id, if you have more than one open." });
    vec![
        json!({
            "name": "vault_fill_password",
            "description": "Type a saved login's password into the focused field of your browser tab. First open the site's sign-in page and click into the password field with your browser tools. Only works when the tab is on the login's website or one of its subdomains. Returns only whether it was filled; you never see the password. Take a fresh snapshot afterwards.",
            "inputSchema": { "type": "object", "properties": { "loginId": login, "threadId": thread, "tabId": tab }, "required": ["loginId", "threadId"] },
        }),
        json!({
            "name": "vault_fill_username",
            "description": "Type a saved login's username into the focused field of your browser tab (same rules as vault_fill_password).",
            "inputSchema": { "type": "object", "properties": { "loginId": login, "threadId": thread, "tabId": tab }, "required": ["loginId", "threadId"] },
        }),
        json!({
            "name": "vault_save_login",
            "description": "Save a password, API token or key the user gave you to the Vault. First ask for the secret with T3's request_secret tool, delivery \"file\"; then pass the file it returns here, without reading it. The file is moved into the Vault and deleted. A login with the same site and username is updated. Returns {saved:true, loginId}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "website": { "type": "string", "description": "The site or service address, e.g. https://github.com" },
                    "secretFile": { "type": "string", "description": "The file request_secret returned." },
                    "username": { "type": "string", "description": "The username or email, if the site has one." },
                    "title": { "type": "string", "description": "A short name, e.g. \"Home Assistant token\". Defaults to the site." },
                    "hints": { "type": "string", "description": "When to use it." },
                    "apiToken": { "type": "boolean", "description": "true for an API token or key you need to read to call an API (vault_secret_file). Leave false for website passwords." },
                    "threadId": thread,
                },
                "required": ["website", "secretFile", "threadId"],
            },
        }),
        json!({
            "name": "vault_secret_file",
            "description": "Write a saved API token (a login saved with apiToken) to a private file for an API call, e.g. TOKEN=\"$(cat <file>)\" curl -H \"Authorization: Bearer $TOKEN\" ... Never print it. The file is deleted after an hour. Website passwords can't be read; fill them with vault_fill_password.",
            "inputSchema": { "type": "object", "properties": { "loginId": login, "threadId": thread }, "required": ["loginId", "threadId"] },
        }),
        json!({
            "name": "vault_save_contact",
            "description": "Save the user's contact info (name, email, phone, address) to the Vault after they give it to you in chat. Check vault_list first so you don't save a duplicate.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "label": { "type": "string", "description": "e.g. \"Home\" or \"Work\"" },
                    "name": { "type": "string" },
                    "email": { "type": "string" },
                    "emails": { "type": "array", "maxItems": 20, "items": { "type": "string" }, "description": "Email addresses, primary first." },
                    "phone": { "type": "string" },
                    "phones": { "type": "array", "maxItems": 20, "items": { "type": "string" }, "description": "Phone numbers, primary first." },
                    "street": { "type": "string" },
                    "apt": { "type": "string" },
                    "city": { "type": "string" },
                    "state": { "type": "string" },
                    "zip": { "type": "string" },
                    "country": { "type": "string" },
                    "kind": { "type": "string", "enum": ["home", "work", "other"] },
                    "hints": { "type": "string", "description": "When to use it." },
                    "threadId": thread,
                },
                "required": ["threadId"],
            },
        }),
    ]
}

pub(crate) fn is_vault_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Entry point from `mcp.rs`: the live Bots, Vault and T3 of this process.
pub(crate) async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let bots = state.bots.list().await.map_err(|e| e.message)?;
    let secrets_dir = secrets_dir_from_env();
    let token_dir = state
        .cfg
        .vault_key_file
        .parent()
        .unwrap_or(&state.cfg.data_root)
        .join("bot-secrets");
    dispatch(
        &Deps {
            t3: &state.t3,
            vault: &state.vault,
            bots: &bots,
            secrets_dir: &secrets_dir,
            token_dir: &token_dir,
        },
        name,
        args,
    )
    .await
}

pub(crate) async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "vault_fill_password" => fill(d, args, "password").await,
        "vault_fill_username" => fill(d, args, "username").await,
        "vault_save_login" => save_login(d, args).await,
        "vault_secret_file" => secret_file(d, args).await,
        "vault_save_contact" => save_contact(d, args).await,
        _ => Err(format!("Unknown tool {name}")),
    }
}

/// The Bot whose main thread is the call's `threadId`.
fn bot_for<'a>(bots: &'a [Value], args: &Value) -> Result<&'a Value, String> {
    let thread = args["threadId"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or("threadId is required: pass your main thread id from AGENTS.md.")?;
    bots.iter()
        .find(|b| b["mainThreadId"] == thread)
        .ok_or_else(|| "That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/bot.json).".into())
}

async fn login_by_id(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let id = args["loginId"].as_str().ok_or(NO_LOGIN)?;
    #[allow(
        clippy::map_err_ignore,
        reason = "a vault error can name the secret store; the Bot only hears that the login is missing"
    )]
    let item = d
        .vault
        .get(id, true)
        .await
        .map_err(|_| NO_LOGIN.to_string())?;
    if item["type"] != "login" {
        return Err(NO_LOGIN.into());
    }
    Ok(item)
}

/// The sites a login is saved for, as typed (`websites`, or the single `website`), without
/// any that have no domain.
pub(crate) fn login_sites(login: &Value) -> Vec<String> {
    let mut sites: Vec<String> = login["websites"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if sites.is_empty() {
        if let Some(w) = login["website"].as_str().filter(|w| !w.is_empty()) {
            sites.push(w.to_string());
        }
    }
    sites.retain(|s| !vault::site_domain(s).is_empty());
    sites
}

/// The saved site a page URL belongs to, if any.
fn site_for_page<'a>(page_url: &str, sites: &'a [String]) -> Option<&'a String> {
    sites.iter().find(|s| vault::matches_site(page_url, s))
}

/// A browser tab of a Bot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tab {
    pub tab_id: String,
    pub url: String,
}

/// The server-browser tabs in a `preview.list` result (other runtimes are left out).
pub(crate) fn server_tabs(result: &Value) -> Vec<Tab> {
    result["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["runtime"] == "server")
        .filter_map(|s| {
            Some(Tab {
                tab_id: s["tabId"].as_str()?.to_string(),
                url: s["navStatus"]["url"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_default()
}

/// The tab to type into: the one asked for, the only one open, or the only one on the login's site.
pub(crate) fn pick_tab<'a>(
    tabs: &'a [Tab],
    tab_id: Option<&str>,
    sites: &[String],
) -> Result<&'a Tab, String> {
    if let Some(id) = tab_id {
        return tabs
            .iter()
            .find(|t| t.tab_id == id)
            .ok_or_else(|| "That tab isn't open in your browser. Open the sign-in page with your browser tools first.".into());
    }
    match tabs {
        [] => Err("You have no browser tab open. Open the site's sign-in page with your browser tools first.".into()),
        [only] => Ok(only),
        many => {
            let on_site: Vec<&Tab> = many
                .iter()
                .filter(|t| site_for_page(&t.url, sites).is_some())
                .collect();
            match on_site.as_slice() {
                [one] => Ok(*one),
                _ => Err(format!(
                    "You have several tabs open; pass tabId. Tabs: {}",
                    many.iter()
                        .map(|t| format!("{} ({})", t.tab_id, host_of(&t.url)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
    }
}

async fn list_tabs(t3: &T3, thread: &str) -> Result<Vec<Tab>, String> {
    let rpc = async {
        let client = t3.rpc().await?;
        client
            .call("preview.list", json!({ "threadId": thread }))
            .await
    };
    let result: anyhow::Result<Value> = match tokio::time::timeout(TAB_LIST_TIMEOUT, rpc).await {
        Ok(r) => r,
        Err(_) => Err(anyhow::anyhow!("timed out")),
    };
    #[allow(
        clippy::map_err_ignore,
        reason = "the T3 error can include the harness address; the Bot gets a fixed retry message"
    )]
    let result = result.map_err(|_| {
        "Couldn't reach your browser on T3 right now. Try again in a moment.".to_string()
    })?;
    Ok(server_tabs(&result))
}

async fn fill(d: &Deps<'_>, args: &Value, field: &str) -> Result<Value, String> {
    let bot = bot_for(d.bots, args)?;
    let thread = args["threadId"].as_str().unwrap_or_default();
    let login = login_by_id(d, args).await?;
    let id = login["id"].as_str().unwrap_or_default().to_string();
    // The value is only ever typed; it is not returned or logged.
    let value = login[field]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("That login has no saved {field}."))?
        .to_string();
    let sites = login_sites(&login);
    if sites.is_empty() {
        return Err("That login has no website saved, so it can't be filled anywhere. Ask the user to add the website in Settings › Vault.".into());
    }
    let tabs = list_tabs(d.t3, thread).await?;
    let tab = pick_tab(&tabs, args["tabId"].as_str(), &sites)?;
    let Some(site) = site_for_page(&tab.url, &sites) else {
        let domains = sites
            .iter()
            .map(|s| vault::site_domain(s))
            .collect::<Vec<_>>()
            .join(", ");
        let shown = match host_of(&tab.url) {
            h if h.is_empty() => "an unknown page".to_string(),
            h => h,
        };
        tracing::warn!(
            "vault: refused {field} fill for login {id} on {shown}, bot {}",
            bot["slug"]
        );
        return Err(format!(
            "Refused: your tab is on {shown}, but this login is saved for {domains} (and its subdomains). \
             A lookalike site could take the password. If this really is the same service, ask the user \
             (question tool) to confirm and add this site to the login in Settings › Vault, or to sign in themselves with Take over."
        ));
    };
    type_into_tab(d.t3, thread, &tab.tab_id, &value).await?;
    tracing::info!(
        "vault: filled {field} for login {id} on {}, bot {}",
        host_of(&tab.url),
        bot["slug"]
    );
    Ok(json!({
        "filled": true,
        "field": field,
        "site": vault::site_domain(site),
        "tabId": tab.tab_id,
    }))
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Stage {
    /// Waiting for T3 to say we control the tab.
    Taking,
    /// The text is sent; waiting for the probe's answer.
    Typing,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    Ignore,
    /// We have control: send the text, then the probe.
    SendText,
    /// The probe came back: release control.
    Release,
    Fail(&'static str),
}

/// What one frame from T3's preview stream means at this stage.
pub(crate) fn step(stage: Stage, msg: &Value) -> Step {
    match msg["type"].as_str() {
        Some("control") => {
            if msg["canOperate"] == false {
                return Step::Fail("The gateway's T3 token can't operate the browser (it needs the preview:operate scope).");
            }
            match (stage, msg["controller"].as_str()) {
                (Stage::Taking, Some("you")) => Step::SendText,
                (Stage::Taking, Some("another-viewer")) => Step::Fail(
                    "The user is controlling this tab right now (Take over). Try again when they're done.",
                ),
                _ => Step::Ignore,
            }
        }
        Some("probe") if stage == Stage::Typing => Step::Release,
        _ => Step::Ignore,
    }
}

#[allow(
    clippy::map_err_ignore,
    reason = "the URL error can echo the stream address and the header error can echo the bearer token"
)]
async fn type_into_tab(t3: &T3, thread: &str, tab_id: &str, text: &str) -> Result<(), String> {
    let mut url = url::Url::parse(&format!("{}/api/preview-stream/ws", ws_base(&t3.base_url)))
        .map_err(|_| NOT_A_TAB_STREAM.to_string())?;
    url.query_pairs_mut()
        .append_pair("threadId", thread)
        .append_pair("tabId", tab_id)
        .append_pair("interactive", "true")
        .append_pair("maxWidth", "320")
        .append_pair("maxHeight", "240")
        .append_pair("quality", "10");
    let mut req = url
        .as_str()
        .into_client_request()
        .map_err(|_| NOT_A_TAB_STREAM.to_string())?;
    let auth = HeaderValue::from_str(&format!("Bearer {}", t3.token))
        .map_err(|_| NOT_A_TAB_STREAM.to_string())?;
    req.headers_mut().insert("authorization", auth);
    let (ws, _) = match tokio_tungstenite::connect_async(req).await {
        Ok(ok) => ok,
        Err(WsError::Http(res)) if res.status() == 404 => {
            return Err("That browser tab isn't open.".into())
        }
        Err(WsError::Http(res)) => {
            return Err(format!(
                "The browser stream refused the connection ({}).",
                res.status()
            ))
        }
        Err(_) => return Err(NOT_A_TAB_STREAM.into()),
    };
    tokio::time::timeout(TYPE_TIMEOUT, drive(ws, text))
        .await
        .unwrap_or_else(|_| Err("The browser didn't respond in time.".into()))
}

/// Takes control, types, probes and releases. Errors never include the text.
#[allow(
    clippy::map_err_ignore,
    reason = "a socket error can include the typed text or the stream address; the Bot only hears that the browser closed"
)]
async fn drive(mut ws: Ws, text: &str) -> Result<(), String> {
    const CLOSED: &str = "The browser closed the connection before the text was typed.";
    let send = |ws_msg: String| Message::text(ws_msg);
    ws.send(send(json!({ "type": "takeControl" }).to_string()))
        .await
        .map_err(|_| CLOSED.to_string())?;
    let mut stage = Stage::Taking;
    while let Some(frame) = ws.next().await {
        let frame = frame.map_err(|_| CLOSED.to_string())?;
        match frame {
            // Frames are paced by acks.
            Message::Binary(_) => {
                ws.send(send(json!({ "type": "ack" }).to_string()))
                    .await
                    .map_err(|_| CLOSED.to_string())?;
            }
            Message::Text(t) => {
                let Ok(msg) = serde_json::from_str::<Value>(t.as_str()) else {
                    continue;
                };
                match step(stage, &msg) {
                    Step::Ignore => {}
                    Step::Fail(reason) => return Err(reason.into()),
                    Step::SendText => {
                        stage = Stage::Typing;
                        ws.send(send(json!({ "type": "text", "text": text }).to_string()))
                            .await
                            .map_err(|_| CLOSED.to_string())?;
                        tokio::time::sleep(PROBE_DELAY).await;
                        ws.send(send(json!({ "type": "probe", "x": 1, "y": 1 }).to_string()))
                            .await
                            .map_err(|_| CLOSED.to_string())?;
                    }
                    Step::Release => {
                        ws.send(send(json!({ "type": "releaseControl" }).to_string()))
                            .await
                            .map_err(|_| CLOSED.to_string())?;
                        // Done when T3 says someone else has control, or after a second.
                        let _ = tokio::time::timeout(RELEASE_TIMEOUT, wait_released(&mut ws)).await;
                        return Ok(());
                    }
                }
            }
            Message::Close(_) => return Err(CLOSED.into()),
            _ => {}
        }
    }
    Err(CLOSED.into())
}

async fn wait_released(ws: &mut Ws) {
    while let Some(Ok(Message::Text(t))) = ws.next().await {
        if let Ok(msg) = serde_json::from_str::<Value>(t.as_str()) {
            if msg["type"] == "control" && msg["controller"] != "you" {
                return;
            }
        }
    }
}

/// The value of a file T3's `request_secret` wrote, without its trailing newline, and the file's
/// real path. Only a small, non-empty file directly inside `root` is accepted.
#[allow(
    clippy::map_err_ignore,
    reason = "an IO error includes the path of the secret file the Bot asked to read"
)]
pub(crate) fn read_secret(root: &Path, file: &str) -> Result<(String, PathBuf), String> {
    let real = std::fs::canonicalize(file).map_err(|_| SECRET_MISSING.to_string())?;
    let root = std::fs::canonicalize(root).map_err(|_| SECRET_MISSING.to_string())?;
    if real.parent() != Some(root.as_path()) {
        return Err("secretFile must be the file request_secret returned.".into());
    }
    let meta = std::fs::metadata(&real).map_err(|_| SECRET_MISSING.to_string())?;
    if !meta.is_file() || meta.len() > MAX_SECRET_BYTES {
        return Err("That isn't a secret file from request_secret.".into());
    }
    let text = std::fs::read_to_string(&real)
        .map_err(|_| "That isn't a secret file from request_secret.".to_string())?;
    let value = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text.as_str())
        .to_string();
    if value.is_empty() {
        return Err("That secret file is empty.".into());
    }
    Ok((value, real))
}

async fn save_login(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    bot_for(d.bots, args)?;
    let website = args["website"].as_str().unwrap_or("").trim().to_string();
    let domain = vault::site_domain(&website);
    if domain.is_empty() {
        return Err("website must be a site address like https://example.com".into());
    }
    let (secret, path) = read_secret(d.secrets_dir, args["secretFile"].as_str().unwrap_or(""))?;
    let username = args["username"].as_str().unwrap_or("").trim().to_string();
    let api_token = args["apiToken"] == true;
    let optional = |k: &str| {
        args[k]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let existing = d
        .vault
        .list()
        .await
        .map_err(|e| e.message)?
        .into_iter()
        .find(|i| {
            i["type"] == "login"
                && i["domain"] == domain.as_str()
                && i["username"].as_str().unwrap_or("") == username
        });

    let mut input = Map::new();
    input.insert("password".into(), json!(secret));
    input.insert("botCanRead".into(), json!(api_token));
    if let Some(title) = optional("title") {
        input.insert("title".into(), json!(title));
    }
    if let Some(hints) = optional("hints") {
        input.insert("hints".into(), json!(hints));
    }
    let (saved, updated) = match existing {
        Some(old) => {
            let id = old["id"].as_str().unwrap_or_default().to_string();
            (d.vault.update(&id, &Value::Object(input)).await, true)
        }
        None => {
            input.insert("type".into(), json!("login"));
            input.insert("website".into(), json!(website));
            input.insert("username".into(), json!(username));
            (d.vault.create(&Value::Object(input)).await, false)
        }
    };
    let item = saved.map_err(|e| e.message)?;
    let _ = std::fs::remove_file(&path);
    tracing::info!(
        "vault: {} login {} for {domain}{}",
        if updated { "updated" } else { "saved" },
        item["id"],
        if api_token { " (api token)" } else { "" }
    );
    Ok(json!({
        "saved": true,
        "updated": updated,
        "loginId": item["id"],
        "site": item["domain"],
        "title": item["title"],
        "apiToken": item["botCanRead"] == true,
    }))
}

async fn secret_file(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    bot_for(d.bots, args)?;
    let login = login_by_id(d, args).await?;
    if login["botCanRead"] != true {
        return Err("That's a website password, which Bots never read. Fill it into the sign-in page with vault_fill_password. If it's really an API token, ask the user to turn on \"Bots can read this\" for it in Settings › Vault.".into());
    }
    let token = login["password"]
        .as_str()
        .filter(|p| !p.is_empty())
        .ok_or("That login has no saved token.")?;
    prepare_token_dir(d.token_dir)?;
    let file = d.token_dir.join(hex::encode(rand::random::<[u8; 16]>()));
    #[allow(
        clippy::map_err_ignore,
        reason = "an IO error includes the path of the token file, next to the token itself"
    )]
    write_private(&file, token).map_err(|_| "Couldn't write the token file.".to_string())?;
    let path = file.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SECRET_FILE_TTL).await;
        let _ = tokio::fs::remove_file(path).await;
    });
    tracing::info!("vault: wrote token file for login {}", login["id"]);
    Ok(json!({
        "file": file.display().to_string(),
        "site": vault::site_domain(login_sites(&login).first().map(String::as_str).unwrap_or("")),
        "expiresInMinutes": SECRET_FILE_TTL.as_secs() / 60,
    }))
}

fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(file)?.write_all(text.as_bytes())
}

/// Creates the private folder and removes token files older than the TTL (left by an earlier run).
fn prepare_token_dir(dir: &Path) -> Result<(), String> {
    #[allow(
        clippy::map_err_ignore,
        reason = "an IO error includes the token-folder path"
    )]
    std::fs::create_dir_all(dir).map_err(|_| "Couldn't prepare the token folder.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    sweep(dir, SystemTime::now() - SECRET_FILE_TTL);
    Ok(())
}

/// Removes the files in `dir` last modified before `older_than`.
pub(crate) fn sweep(dir: &Path, older_than: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() && meta.modified().is_ok_and(|t| t < older_than) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

async fn save_contact(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    bot_for(d.bots, args)?;
    let mut input = Map::new();
    for f in CONTACT_FIELDS {
        if let Some(s) = args[*f].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            input.insert((*f).into(), json!(s));
        }
    }
    for f in ["emails", "phones"] {
        if let Some(list) = args[f].as_array() {
            input.insert(f.into(), json!(list));
        }
    }
    let has_something = ["name", "email", "phone", "street", "emails", "phones"]
        .iter()
        .any(|k| match input.get(*k) {
            Some(Value::Array(a)) => !a.is_empty(),
            Some(_) => true,
            None => false,
        });
    if !has_something {
        return Err("Give at least a name, email, phone or street.".into());
    }
    input.insert("type".into(), json!("contact"));
    let item = d
        .vault
        .create(&Value::Object(input))
        .await
        .map_err(|e| e.message)?;
    tracing::info!("vault: saved contact {}", item["id"]);
    Ok(json!({ "saved": true, "contactId": item["id"], "title": item["title"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bots() -> Vec<Value> {
        vec![json!({ "mainThreadId": "thread-main", "slug": "main", "name": "Main" })]
    }

    fn tab(id: &str, url: &str) -> Tab {
        Tab {
            tab_id: id.into(),
            url: url.into(),
        }
    }

    #[test]
    fn names_are_served_here() {
        assert!(is_vault_tool("vault_fill_password"));
        assert!(is_vault_tool("vault_save_contact"));
        assert!(!is_vault_tool("vault_list"));
        assert!(!is_vault_tool("projects_save"));
        let defs = tool_defs();
        let names: Vec<&str> = defs.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for n in NAMES {
            assert!(names.contains(&n), "{n} is listed");
        }
    }

    #[test]
    fn login_sites_prefer_the_list_and_drop_empty_ones() {
        let login = json!({ "website": "https://github.com", "websites": ["https://github.com", "not a site", "https://gist.github.com"] });
        assert_eq!(
            login_sites(&login),
            vec!["https://github.com", "https://gist.github.com"]
        );
        assert_eq!(
            login_sites(&json!({ "website": "example.org" })),
            vec!["example.org"]
        );
        assert!(login_sites(&json!({ "website": "" })).is_empty());
    }

    #[test]
    fn domain_matching_refuses_lookalikes() {
        let sites = vec!["https://github.com".to_string()];
        assert!(site_for_page("https://github.com/login", &sites).is_some());
        assert!(site_for_page("https://gist.github.com/x", &sites).is_some());
        assert!(site_for_page("https://github.com.evil.io/login", &sites).is_none());
        assert!(site_for_page("https://githubXcom.io/", &sites).is_none());
        // Plain http is only ever a match when the login was saved as http.
        assert!(site_for_page("http://github.com/", &sites).is_none());
    }

    #[test]
    fn server_tabs_keep_only_server_runtime() {
        let result = json!({ "sessions": [
            { "tabId": "a", "runtime": "server", "navStatus": { "url": "https://github.com/", "title": "x" } },
            { "tabId": "b", "runtime": "browser", "navStatus": { "url": "https://github.com/" } },
            { "runtime": "server" },
        ] });
        assert_eq!(server_tabs(&result), vec![tab("a", "https://github.com/")]);
        assert!(server_tabs(&json!({})).is_empty());
    }

    #[test]
    fn pick_tab_chooses_or_explains() {
        let sites = vec!["https://github.com".to_string()];
        let one = vec![tab("a", "https://example.com/")];
        assert_eq!(pick_tab(&one, None, &sites).unwrap().tab_id, "a");
        assert!(pick_tab(&[], None, &sites)
            .unwrap_err()
            .contains("no browser tab"));
        let two = vec![
            tab("a", "https://example.com/"),
            tab("b", "https://github.com/login"),
        ];
        assert_eq!(pick_tab(&two, None, &sites).unwrap().tab_id, "b");
        assert_eq!(pick_tab(&two, Some("a"), &sites).unwrap().tab_id, "a");
        assert!(pick_tab(&two, Some("zz"), &sites)
            .unwrap_err()
            .contains("isn't open"));
        let ambiguous = vec![
            tab("a", "https://github.com/"),
            tab("b", "https://github.com/x"),
            tab("c", "https://example.com/"),
        ];
        let err = pick_tab(&ambiguous, None, &sites).unwrap_err();
        assert!(err.contains("pass tabId") && err.contains("a (github.com)"));
    }

    #[test]
    fn stream_steps_follow_hbots_order() {
        let you = json!({ "type": "control", "controller": "you", "canOperate": true });
        let other =
            json!({ "type": "control", "controller": "another-viewer", "canOperate": true });
        let cannot = json!({ "type": "control", "controller": "you", "canOperate": false });
        let probe = json!({ "type": "probe", "x": 1, "y": 1 });
        assert_eq!(step(Stage::Taking, &you), Step::SendText);
        assert!(matches!(step(Stage::Taking, &other), Step::Fail(_)));
        assert!(matches!(step(Stage::Taking, &cannot), Step::Fail(_)));
        assert_eq!(step(Stage::Typing, &probe), Step::Release);
        assert_eq!(step(Stage::Taking, &probe), Step::Ignore);
    }

    #[test]
    fn secret_files_must_be_small_and_inside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("secrets");
        std::fs::create_dir_all(&root).unwrap();
        let inside = root.join("s1");
        std::fs::write(&inside, "tok-123\n").unwrap();
        let (value, real) = read_secret(&root, inside.to_str().unwrap()).unwrap();
        assert_eq!(value, "tok-123");
        assert_eq!(real, std::fs::canonicalize(&inside).unwrap());

        let outside = dir.path().join("outside");
        std::fs::write(&outside, "nope").unwrap();
        assert!(read_secret(&root, outside.to_str().unwrap())
            .unwrap_err()
            .contains("must be the file"));

        let empty = root.join("empty");
        std::fs::write(&empty, "\n").unwrap();
        assert!(read_secret(&root, empty.to_str().unwrap())
            .unwrap_err()
            .contains("empty"));

        let big = root.join("big");
        std::fs::write(
            &big,
            vec![b'a'; usize::try_from(MAX_SECRET_BYTES).unwrap_or(0) + 1],
        )
        .unwrap();
        assert!(read_secret(&root, big.to_str().unwrap())
            .unwrap_err()
            .contains("isn't a secret"));

        assert!(read_secret(&root, root.join("gone").to_str().unwrap())
            .unwrap_err()
            .contains("doesn't exist"));
        assert!(read_secret(&root, "").is_err());
    }

    #[test]
    fn sweep_removes_only_files_older_than_the_cutoff() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("fresh"), "x").unwrap();
        sweep(dir.path(), SystemTime::now() - Duration::from_secs(3600));
        assert!(dir.path().join("fresh").exists());
        sweep(dir.path(), SystemTime::now() + Duration::from_secs(60));
        assert!(!dir.path().join("fresh").exists());
    }

    #[tokio::test]
    async fn a_contact_is_saved_and_needs_a_name_or_a_way_to_reach_them() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let t3 = T3::new("http://127.0.0.1:1".into(), "t".into());
        let b = bots();
        let deps = Deps {
            t3: &t3,
            vault: &vault,
            bots: &b,
            secrets_dir: dir.path(),
            token_dir: dir.path(),
        };
        let saved = dispatch(&deps, "vault_save_contact", &json!({ "threadId": "thread-main", "name": "Ada", "emails": ["ada@example.com"], "city": "London" })).await.unwrap();
        assert_eq!(saved["saved"], true);
        let stored = vault.list().await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0]["type"], "contact");
        assert_eq!(stored[0]["email"], "ada@example.com");
        let err = dispatch(
            &deps,
            "vault_save_contact",
            &json!({ "threadId": "thread-main", "label": "Home" }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("Give at least"));
        let err = dispatch(
            &deps,
            "vault_save_contact",
            &json!({ "threadId": "nobody", "name": "x" }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("isn't a Bot's main conversation"));
    }

    #[tokio::test]
    async fn saving_a_login_moves_the_secret_and_never_returns_it() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = dir.path().join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let t3 = T3::new("http://127.0.0.1:1".into(), "t".into());
        let b = bots();
        let deps = Deps {
            t3: &t3,
            vault: &vault,
            bots: &b,
            secrets_dir: &secrets,
            token_dir: dir.path(),
        };

        let file = secrets.join("one");
        std::fs::write(&file, "first-secret\n").unwrap();
        let args = json!({ "threadId": "thread-main", "website": "https://github.com", "username": "me", "secretFile": file.to_str().unwrap() });
        let created = dispatch(&deps, "vault_save_login", &args).await.unwrap();
        assert_eq!(created["saved"], true);
        assert_eq!(created["updated"], false);
        assert!(!file.exists(), "the secret file is deleted");
        assert!(!created.to_string().contains("first-secret"));

        // The same site and username updates the login instead of adding one.
        let file2 = secrets.join("two");
        std::fs::write(&file2, "second-secret").unwrap();
        let args2 = json!({ "threadId": "thread-main", "website": "https://github.com", "username": "me", "secretFile": file2.to_str().unwrap(), "apiToken": true });
        let updated = dispatch(&deps, "vault_save_login", &args2).await.unwrap();
        assert_eq!(updated["updated"], true);
        assert_eq!(updated["loginId"], created["loginId"]);
        assert_eq!(updated["apiToken"], true);
        assert_eq!(vault.list().await.unwrap().len(), 1);
        let id = created["loginId"].as_str().unwrap();
        assert_eq!(
            vault.get(id, true).await.unwrap()["password"],
            "second-secret"
        );
    }

    #[tokio::test]
    async fn a_token_file_is_private_and_only_for_api_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let tokens = dir.path().join("bot-secrets");
        let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let t3 = T3::new("http://127.0.0.1:1".into(), "t".into());
        let b = bots();
        let deps = Deps {
            t3: &t3,
            vault: &vault,
            bots: &b,
            secrets_dir: dir.path(),
            token_dir: &tokens,
        };
        let site = vault.create(&json!({ "type": "login", "website": "https://ha.local", "username": "", "password": "pw-login" })).await.unwrap();
        let token = vault.create(&json!({ "type": "login", "website": "https://ha.local", "username": "api", "password": "tok-xyz", "botCanRead": true })).await.unwrap();

        let refused = dispatch(
            &deps,
            "vault_secret_file",
            &json!({ "threadId": "thread-main", "loginId": site["id"] }),
        )
        .await
        .unwrap_err();
        assert!(!refused.contains("pw-login"));
        let out = dispatch(
            &deps,
            "vault_secret_file",
            &json!({ "threadId": "thread-main", "loginId": token["id"] }),
        )
        .await
        .unwrap();
        assert!(!out.to_string().contains("tok-xyz"));
        let file = PathBuf::from(out["file"].as_str().unwrap());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "tok-xyz");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn fill_refuses_without_a_bot_or_a_login() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let t3 = T3::new("http://127.0.0.1:1".into(), "t".into());
        let b = bots();
        let deps = Deps {
            t3: &t3,
            vault: &vault,
            bots: &b,
            secrets_dir: dir.path(),
            token_dir: dir.path(),
        };
        let err = dispatch(
            &deps,
            "vault_fill_password",
            &json!({ "threadId": "nobody", "loginId": "x" }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("isn't a Bot's main conversation"));
        let err = dispatch(
            &deps,
            "vault_fill_password",
            &json!({ "threadId": "thread-main", "loginId": "x" }),
        )
        .await
        .unwrap_err();
        assert_eq!(err, NO_LOGIN);
    }

    /// A T3 stand-in (`tests/mock-t3.ts`, run with bun). Skipped when bun isn't installed.
    struct MockT3 {
        child: std::process::Child,
        url: String,
    }

    impl Drop for MockT3 {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl MockT3 {
        async fn start() -> Option<Self> {
            let port = std::net::TcpListener::bind("127.0.0.1:0")
                .ok()?
                .local_addr()
                .ok()?
                .port();
            let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/mock-t3.ts");
            let child = std::process::Command::new("bun")
                .arg(script)
                .arg(port.to_string())
                .env("T3_TOKEN", "test-token")
                .spawn()
                .ok()?;
            let mut mock = Self {
                child,
                url: format!("http://127.0.0.1:{port}"),
            };
            for _ in 0..100 {
                if tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .is_ok()
                {
                    return Some(mock);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let _ = mock.child.kill();
            None
        }

        /// What the mock has typed into a tab.
        async fn typed(&self, tab: &str) -> String {
            let res = reqwest::Client::new()
                .get(format!("{}/mock/typed?tabId={tab}", self.url))
                .bearer_auth("test-token")
                .send()
                .await
                .unwrap();
            let body: Value = res.json().await.unwrap();
            body["text"].as_str().unwrap_or_default().to_string()
        }
    }

    #[tokio::test]
    async fn fills_a_password_through_the_browser_tab() {
        let Some(mock) = MockT3::start().await else {
            eprintln!("bun is not available: skipped the T3 round trip");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let item = vault
            .create(&json!({ "type": "login", "website": "https://github.com", "username": "me", "password": "s3cret-pass" }))
            .await
            .unwrap();
        let id = item["id"].as_str().unwrap();
        let t3 = T3::new(mock.url.clone(), "test-token".into());
        let b = bots();
        let deps = Deps {
            t3: &t3,
            vault: &vault,
            bots: &b,
            secrets_dir: dir.path(),
            token_dir: dir.path(),
        };

        // A lookalike tab is refused and nothing is typed into it.
        let refused = dispatch(
            &deps,
            "vault_fill_password",
            &json!({ "threadId": "thread-main", "loginId": id, "tabId": "t-evil" }),
        )
        .await
        .unwrap_err();
        assert!(refused.contains("Refused"));
        assert!(!refused.contains("s3cret-pass"));
        assert_eq!(mock.typed("t-evil").await, "");

        // A tab that another viewer controls is refused with the reason.
        let busy = dispatch(
            &deps,
            "vault_fill_password",
            &json!({ "threadId": "thread-main", "loginId": id, "tabId": "busy" }),
        )
        .await
        .unwrap_err();
        assert!(busy.contains("controlling this tab"));

        let ok = dispatch(
            &deps,
            "vault_fill_password",
            &json!({ "threadId": "thread-main", "loginId": id, "tabId": "t-github" }),
        )
        .await
        .unwrap();
        assert_eq!(
            ok,
            json!({ "filled": true, "field": "password", "site": "github.com", "tabId": "t-github" })
        );
        assert!(!ok.to_string().contains("s3cret-pass"));
        assert_eq!(mock.typed("t-github").await, "s3cret-pass");

        let user = dispatch(
            &deps,
            "vault_fill_username",
            &json!({ "threadId": "thread-main", "loginId": id, "tabId": "t-github" }),
        )
        .await
        .unwrap();
        assert_eq!(user["field"], "username");
        assert_eq!(mock.typed("t-github").await, "s3cret-passme");
    }
}
