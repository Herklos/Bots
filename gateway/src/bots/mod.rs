//! Bots on disk: `bots/<slug>/.bot/bot.json` per Bot, the main-Bot
//! choice in `bots/.botapp/main-bot.json`, deleted Bots under `bots/.botapp/trash`.

use crate::storage::workspace::Workspace;
use crate::storage::LocalDisk;
use crate::{
    bots::agent_context::MemorySnapshot,
    platform::error::{ApiError, ApiResult},
    storage::{read_json, write_json},
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
        Self {
            root,
            mcp_url,
            mcp_token,
            ws,
            lock: Mutex::new(()),
            claude_file_warned: std::sync::Mutex::new(HashSet::new()),
        }
    }

    pub fn dir(&self, slug: &str) -> ApiResult<PathBuf> {
        if !valid_slug(slug) {
            return Err(ApiError::bad_request("bad slug"));
        }
        Ok(self.root.join(slug))
    }

    fn meta_file(&self, slug: &str) -> ApiResult<PathBuf> {
        Ok(self.dir(slug)?.join(".bot").join("bot.json"))
    }

    fn selection_file(&self) -> PathBuf {
        self.root.join(".botapp").join("main-bot.json")
    }

    async fn load(&self) -> ApiResult<Vec<Value>> {
        let mut bots = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(bots),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.file_type().await?.is_dir() || !valid_slug(&name) {
                continue;
            }
            let Some(meta) = read_json(&self.meta_file(&name)?).await else {
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
                .unwrap_or(&name)
                .to_string();
            bot.insert("name".into(), json!(display));
            if !bot.get("createdAt").is_some_and(Value::is_string) {
                bot.insert("createdAt".into(), json!(now_iso()));
            }
            bot.insert("slug".into(), json!(name));
            bot.insert(
                "workspaceRoot".into(),
                json!(self.root.join(&name).to_string_lossy()),
            );
            bots.push(Value::Object(bot));
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
        write_json(&self.meta_file(&slug)?, &Value::Object(m)).await?;
        Ok(())
    }

    /// Exactly one main Bot: the saved choice, else a legacy `primary`, else the first visible.
    async fn reconcile(&self) -> ApiResult<Vec<Value>> {
        let mut bots = self.load().await?;
        // A missing file is no choice yet; an unreadable one is an error, so this can't overwrite it.
        let selection = match tokio::fs::read_to_string(self.selection_file()).await {
            Ok(text) => Some(
                #[allow(
                    clippy::map_err_ignore,
                    reason = "serde quotes the choice file, which names the main Bot"
                )]
                serde_json::from_str::<Value>(&text)
                    .map_err(|_| ApiError::new(500, "The main Bot choice file can't be read."))?,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
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
            write_json(&self.selection_file(), &json!({ "slug": main })).await?;
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
        Ok(Self::show_all(self.reconcile().await?))
    }

    pub async fn get(&self, slug: &str) -> ApiResult<Option<Value>> {
        self.dir(slug)?;
        let _g = self.lock.lock().await;
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
        let bots = self.reconcile().await?;
        let mut target = bots
            .into_iter()
            .find(|b| b["slug"] == slug)
            .ok_or_else(|| ApiError::not_found("Bot not found"))?;
        if is_subagent(&target) {
            return Err(ApiError::new(409, "A subagent can't be the main Bot."));
        }
        write_json(&self.selection_file(), &json!({ "slug": slug })).await?;
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
