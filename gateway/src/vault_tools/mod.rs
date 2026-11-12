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

mod browser;
mod secrets;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use browser::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use secrets::*;

const NO_LOGIN: &str = "No login with that id. Check vault_list.";

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
