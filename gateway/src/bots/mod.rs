//! Bots on disk: `bots/<slug>/.bot/bot.json` per Bot, the main-Bot
//! choice in `bots/.botapp/main-bot.json`, deleted Bots under `bots/.botapp/trash`.

use crate::storage::workspace::Workspace;
use crate::storage::LocalDisk;
use crate::{
    bots::agent_context::MemorySnapshot,
    platform::error::{ApiError, ApiResult},
    vault::now_iso,
};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

mod instructions;
mod lifecycle;
mod names;
mod prompt_policy;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use instructions::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use lifecycle::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use names::*;

#[derive(Debug)]
pub struct Bots {
    pub root: PathBuf,
    pub mcp_url: String,
    pub mcp_token: String,
    /// Same workspace as [`crate::api::state::AppState::workspace`] in the running gateway.
    ws: Arc<dyn Workspace>,
    settings_store: Arc<dyn crate::storage::stores::SettingsStore>,
    connector_store: Arc<dyn crate::storage::stores::ConnectorStore>,
    bot_store: Arc<dyn crate::storage::stores::BotStore>,
    lock: Mutex<()>,
    /// Slugs already warned because `CLAUDE.md` is a real file, not a symlink.
    claude_file_warned: std::sync::Mutex<HashSet<String>>,
}

impl Bots {
    pub fn new(root: PathBuf, mcp_url: String, mcp_token: String) -> Self {
        let ws = Arc::new(LocalDisk::new(root.clone()));
        Self::with_workspace(root, mcp_url, mcp_token, ws)
    }

    /// `ws` is the process workspace. Create and the memory tools share it.
    pub fn with_workspace(
        root: PathBuf,
        mcp_url: String,
        mcp_token: String,
        ws: Arc<dyn Workspace>,
    ) -> Self {
        let settings_store = Arc::new(
            crate::storage::adapters::filesystem::FilesystemSettingsStore::new(
                root.join(".botapp/settings.json"),
            ),
        );
        let connector_store = Arc::new(
            crate::storage::adapters::filesystem::FilesystemConnectorStore::new(Arc::clone(&ws)),
        );
        let bot_store = Arc::new(crate::storage::adapters::filesystem::FilesystemBotStore::new(
            root.clone(),
            false,
        ));
        Self {
            root,
            mcp_url,
            mcp_token,
            ws,
            settings_store,
            connector_store,
            bot_store,
            lock: Mutex::new(()),
            claude_file_warned: std::sync::Mutex::new(HashSet::new()),
        }
    }

    pub(crate) fn with_connector_store(
        mut self,
        store: Arc<dyn crate::storage::stores::ConnectorStore>,
    ) -> Self {
        self.connector_store = store;
        self
    }

    pub(crate) fn with_settings_store(
        mut self,
        store: Arc<dyn crate::storage::stores::SettingsStore>,
    ) -> Self {
        self.settings_store = store;
        self
    }

    pub(crate) fn with_local_overlay(self) -> Self {
        let store = Arc::new(
            crate::storage::adapters::filesystem::FilesystemBotStore::new(
                self.root.clone(),
                true,
            ),
        );
        self.with_bot_store(store)
    }

    pub(crate) fn with_bot_store(
        mut self,
        store: Arc<dyn crate::storage::stores::BotStore>,
    ) -> Self {
        self.bot_store = store;
        self
    }

    pub fn dir(&self, slug: &str) -> ApiResult<PathBuf> {
        if !crate::storage::stores::valid_slug(slug) {
            return Err(ApiError::bad_request("bad slug"));
        }
        Ok(self.root.join(slug))
    }

    async fn load(&self) -> ApiResult<Vec<Value>> {
        let mut bots = Vec::new();
        let mut slugs = self.bot_store.list_slugs().await?;
        slugs.sort();
        for name in slugs {
            let Some(meta) = self.bot_store.read_meta(&name).await? else {
                continue;
            };
            let mut bot = json!({
                "label": "", "shape": "ball", "color": hbots_design::tokens().avatar.default, "section": null, "pinned": false,
                "hidden": false, "primary": false, "notifications": true, "projectId": null, "mainThreadId": null,
                "role": "bot", "parentSlug": null, "hideFromSidebar": false, "prompt": "", "subagent": null,
            })
            .as_object()
            .cloned()
            .unwrap_or_default();
            bot.extend(obj(&meta));
            let shape = bot["shape"]
                .as_str()
                .and_then(shape_name)
                .unwrap_or_else(|| "ball".into());
            bot.insert("shape".into(), json!(shape));
            let display = meta["name"]
                .as_str()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(name.as_str())
                .to_string();
            bot.insert("name".into(), json!(display));
            if !bot.get("createdAt").is_some_and(Value::is_string) {
                bot.insert("createdAt".into(), json!(now_iso()));
            }
            bot.insert("slug".into(), json!(name));
            bot.insert(
                "workspaceRoot".into(),
                json!(self.root.join(&name).to_string_lossy()),
            );            bots.push(Value::Object(bot));
        }
        bots.sort_by(|a, b| {
            a["createdAt"]
                .as_str()
                .cmp(&b["createdAt"].as_str())
                .then(a["slug"].as_str().cmp(&b["slug"].as_str()))
        });
        Ok(bots)
    }

    async fn persist(&self, bot: &Value) -> ApiResult<()> {
        let mut m = obj(bot);
        let slug = m
            .remove("slug")
            .and_then(|s| s.as_str().map(String::from))
            .unwrap_or_default();
        m.remove("workspaceRoot");
        self.bot_store.write_meta(&slug, &Value::Object(m)).await?;
        Ok(())
    }

    /// Exactly one main Bot: the saved choice, else a legacy `primary`, else the first visible.
    async fn reconcile(&self) -> ApiResult<Vec<Value>> {
        let mut bots = self.load().await?;
        let selection = self.bot_store.read_selection().await?;
        let chosen = selection
            .as_ref()
            .and_then(|s| s["slug"].as_str().map(String::from));
        let main = chosen
            .as_ref()
            .filter(|c| {
                bots.iter()
                    .any(|b| b["slug"] == c.as_str() && !is_subagent(b))
            })
            .cloned()
            .or_else(|| {
                if selection.is_none() {
                    bots.iter()
                        .find(|b| !is_subagent(b) && b["hidden"] != true && b["primary"] == true)
                        .and_then(|b| b["slug"].as_str().map(String::from))
                } else {
                    None
                }
            })
            .or_else(|| {
                bots.iter()
                    .find(|b| !is_subagent(b) && b["hidden"] != true)
                    .or_else(|| bots.iter().find(|b| !is_subagent(b)))
                    .and_then(|b| b["slug"].as_str().map(String::from))
            });
        if chosen != main {
            self.bot_store
                .write_selection(main.as_deref())
                .await?;
        }
        for bot in bots.iter_mut() {
            let primary = main.as_deref() == bot["slug"].as_str();
            if bot["primary"] != primary || (primary && bot["hidden"] == true) {
                bot["primary"] = json!(primary);
                if primary {
                    bot["hidden"] = json!(false);
                }
                self.persist(bot).await?;
            }
        }
        Ok(bots)
    }

    pub async fn list(&self) -> ApiResult<Vec<Value>> {
        let _g = self.lock.lock().await;
        let _disk_guard = self.bot_store.acquire().await?;
        Ok(Self::show_all(self.reconcile().await?))
    }

    pub async fn get(&self, slug: &str) -> ApiResult<Option<Value>> {
        self.dir(slug)?;
        let _g = self.lock.lock().await;
        let _disk_guard = self.bot_store.acquire().await?;
        Ok(self
            .reconcile()
            .await?
            .into_iter()
            .find(|b| b["slug"] == slug)
            .map(Self::show))
    }

    pub async fn select(&self, slug: &str) -> ApiResult<Vec<Value>> {
        self.dir(slug)?;
        let _g = self.lock.lock().await;
        let _disk_guard = self.bot_store.acquire()
        .await?;
        let bots = self.reconcile().await?;
        let mut target = bots
            .into_iter()
            .find(|b| b["slug"] == slug)
            .ok_or_else(|| ApiError::not_found("Bot not found"))?;
        if is_subagent(&target) {
            return Err(ApiError::new(409, "A subagent can't be the main Bot."));
        }
        self.bot_store.write_selection(Some(slug)).await?;
        if target["hidden"] == true {
            target["hidden"] = json!(false);
            self.persist(&target).await?;
        }
        Ok(Self::show_all(self.reconcile().await?))
    }

    fn show(mut bot: Value) -> Value {
        helpers_policy::resolve_on_view(&mut bot);
        bot
    }

    fn show_all(bots: Vec<Value>) -> Vec<Value> {
        bots.into_iter().map(Self::show).collect()
    }
}

pub(crate) mod agent_context; // The managed block of AGENTS.md: prompt text, and later a memory snapshot.
#[allow(
    clippy::map_err_ignore,
    reason = "the client sees a fixed browser-stream failure, never the socket error or the bearer token"
)]
// `computer.rs` is intentionally left unchanged. `pub` in that private module is
// still meaningless to other crates; the allow stays until that file is edited.
#[allow(
    unreachable_pub,
    reason = "computer.rs is left unchanged; its pub items are not part of another crate's API"
)]
pub(crate) mod computer;
pub(crate) mod face_icon; // The Bot face written into the workspace for the thread list.
pub(crate) mod helpers_policy; // Lasting helpers and one-off child thread policy.
mod helpers_prompt; // That policy, written into the managed prompt.
pub(crate) mod history_tools; // Score, page, and the threads one Bot may search.
pub(crate) mod icons; // Site icons cached under HBOTS_HOME/icons.
pub(crate) mod image_tools; // image_show: copy a picture into this Bot's library/.
pub(crate) mod library; // Files and records through the workspace adapter.
pub(crate) mod lineage; // Subagents and delegated threads of one Bot.
pub(crate) mod mcp_identity; // Which Bot is calling the gateway MCP server.
pub(crate) mod memory; // Bot memory files through the workspace.
pub(crate) mod memory_files; // Markdown read and revision-safe save for the person.
pub(crate) mod memory_tools;
pub(crate) mod provider_switch; // A mid-chat provider change waits for confirmation.
pub(crate) mod provisioner;
pub(crate) mod roster; // What an inbox row needs: preview, working, unread.
pub(crate) mod runtime_mode; // Approval modes, least access first, and the pure cap.
pub(crate) mod subagent_tools;
pub(crate) mod suggestions; // New Bot starting points.
pub(crate) mod usage; // Local usage counts. Nothing leaves the machine. // The Bot's browser, watch first.
pub(crate) mod watch; // Phase 4: page and file watches, the poller, and the SSRF guard.
pub(crate) mod watch_tools; // The Bots' own watch_create, watch_list, watch_delete.
