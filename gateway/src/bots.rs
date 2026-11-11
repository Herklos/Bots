//! Bots on disk: `bots/<slug>/.bot/bot.json` per Bot, the main-Bot
//! choice in `bots/.botapp/main-bot.json`, deleted Bots under `bots/.botapp/trash`.
use crate::{
    error::{ApiError, ApiResult},
    store::{read_json, write_json},
    vault::now_iso,
};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

pub(crate) const MANAGED_START: &str = "<!-- bot:managed:start -->";
pub(crate) const MANAGED_END: &str = "<!-- bot:managed:end -->";

/// The shape names a Bot may have, from the design crate. The old default, `sphere`, reads as the ball.
pub(crate) fn shape_name(raw: &str) -> Option<String> {
    let shapes = hbots_design::tokens().face.shape_names();
    if raw == "sphere" {
        return shapes.first().map(|s| s.to_string());
    }
    shapes.contains(&raw).then(|| raw.to_string())
}

/// Bots pinned as tiles above the inbox, besides the main Bot.
pub(crate) const MAX_PINNED: usize = 2;

pub(crate) fn valid_slug(slug: &str) -> bool {
    let b = slug.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// "Ada Lovelace" → "ada-lovelace".
pub(crate) fn slugify(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let lowered: String = name
        .to_lowercase()
        .nfkd()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || c.is_whitespace())
        .filter(char::is_ascii)
        .collect();
    let mut out = String::new();
    for c in lowered.trim().chars() {
        let c = if c.is_whitespace() || c == '_' {
            '-'
        } else {
            c
        };
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    let out: String = out.chars().take(48).collect();
    if out.is_empty() {
        "bot".into()
    } else {
        out
    }
}

#[derive(Debug)]
pub struct Bots {
    pub root: PathBuf,
    pub mcp_url: String,
    pub mcp_token: String,
    lock: Mutex<()>,
}

fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

pub(crate) fn is_subagent(bot: &Value) -> bool {
    bot["role"] == "subagent" || bot["hideFromSidebar"] == true
}

fn parent_slug_of(bot: &Value) -> Option<String> {
    match bot.get("parentSlug") {
        Some(Value::String(slug)) => {
            let slug = slug.trim();
            if slug.is_empty() {
                None
            } else {
                Some(slug.to_string())
            }
        }
        _ => None,
    }
}

fn parent_slug_value(value: &Value) -> ApiResult<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    let slug = value
        .as_str()
        .ok_or_else(|| ApiError::bad_request("parentSlug must be a string"))?;
    Ok(Some(slug.trim().to_string()).filter(|s| !s.is_empty()))
}

fn check_subagent_keys(fields: &Map<String, Value>) -> ApiResult<()> {
    if fields.get("prompt").is_some_and(|v| !v.is_string()) {
        return Err(ApiError::bad_request("subagent.prompt must be a string"));
    }
    if fields
        .get("inheritParentPrompt")
        .is_some_and(|v| !v.is_boolean())
    {
        return Err(ApiError::bad_request(
            "subagent.inheritParentPrompt must be a boolean",
        ));
    }
    Ok(())
}

/// Defaults, then the sent object, so a later unknown key is kept.
fn child_subagent(input: &Value) -> ApiResult<Value> {
    let mut base = Map::new();
    base.insert("prompt".into(), json!(""));
    base.insert("inheritParentPrompt".into(), json!(false));
    if let Some(raw) = input.get("subagent").filter(|v| !v.is_null()) {
        let fields = raw
            .as_object()
            .ok_or_else(|| ApiError::bad_request("subagent must be an object"))?;
        check_subagent_keys(fields)?;
        base.extend(fields.clone());
    }
    Ok(Value::Object(base))
}

impl Bots {
    pub fn new(root: PathBuf, mcp_url: String, mcp_token: String) -> Self {
        Self {
            root,
            mcp_url,
            mcp_token,
            lock: Mutex::new(()),
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
        self.reconcile().await
    }

    pub async fn get(&self, slug: &str) -> ApiResult<Option<Value>> {
        self.dir(slug)?;
        let _g = self.lock.lock().await;
        Ok(self
            .reconcile()
            .await?
            .into_iter()
            .find(|b| b["slug"] == slug))
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
        self.reconcile().await
    }

    pub async fn save(&self, slug: &str, patch: &Value, create: bool) -> ApiResult<Value> {
        self.dir(slug)?;
        let saved = {
            let _g = self.lock.lock().await;
            let bots = self.reconcile().await?;
            let current = bots.iter().find(|b| b["slug"] == slug).cloned();
            if current.is_none() && !create {
                return Err(ApiError::not_found("Bot not found"));
            }
            for key in [
                "primary",
                "hidden",
                "pinned",
                "hideFromSidebar",
                "notifications",
            ] {
                if patch.get(key).is_some_and(|v| !v.is_boolean()) {
                    return Err(ApiError::bad_request(format!("{key} must be a boolean")));
                }
            }
            if patch.get("prompt").is_some_and(|v| !v.is_string()) {
                return Err(ApiError::bad_request("prompt must be a string"));
            }
            if let Some(mode) = patch.get("runtimeMode") {
                if !matches!(
                    mode.as_str(),
                    Some("approval-required" | "auto" | "full-access")
                ) {
                    return Err(ApiError::bad_request(
                        "runtimeMode must be approval-required, auto, or full-access",
                    ));
                }
            }
            if let Some(subagent) = patch.get("subagent") {
                if subagent.is_null() {
                    if current.as_ref().is_some_and(is_subagent) {
                        return Err(ApiError::bad_request("subagent must be an object"));
                    }
                } else {
                    let fields = subagent
                        .as_object()
                        .ok_or_else(|| ApiError::bad_request("subagent must be an object"))?;
                    check_subagent_keys(fields)?;
                }
            }
            if !create {
                let child = current.as_ref().is_some_and(is_subagent);
                if child {
                    if patch.get("pinned").is_some() {
                        return Err(ApiError::new(409, "A subagent can't be pinned."));
                    }
                    if patch.get("hidden").is_some() {
                        return Err(ApiError::new(409, "A subagent can't be hidden."));
                    }
                    if patch.get("primary").is_some() {
                        return Err(ApiError::new(409, "A subagent can't be the main Bot."));
                    }
                    if patch["hideFromSidebar"] == false {
                        return Err(ApiError::new(409, "A subagent stays out of the inbox."));
                    }
                } else if patch["hideFromSidebar"] == true {
                    return Err(ApiError::new(
                        409,
                        "Only a subagent is hidden from the sidebar.",
                    ));
                }
                if let Some(role) = patch.get("role") {
                    let role = role
                        .as_str()
                        .ok_or_else(|| ApiError::bad_request("role must be a string"))?;
                    let current_role = current
                        .as_ref()
                        .and_then(|c| c["role"].as_str())
                        .unwrap_or("bot");
                    if role != current_role {
                        return Err(ApiError::new(409, "A Bot's role is chosen once."));
                    }
                }
                if let Some(next_parent) = patch.get("parentSlug") {
                    let next_parent = parent_slug_value(next_parent)?;
                    let current_parent = current.as_ref().and_then(parent_slug_of);
                    if next_parent != current_parent {
                        return Err(ApiError::new(409, "A subagent's parent is chosen once."));
                    }
                }
            }
            if let Some(shape) = patch.get("shape") {
                if shape.as_str().and_then(shape_name).is_none() {
                    return Err(ApiError::bad_request("Pick a shape from the list."));
                }
            }
            if patch["pinned"] == true
                && current.as_ref().is_none_or(|c| c["pinned"] != true)
                && bots
                    .iter()
                    .filter(|b| b["pinned"] == true && b["primary"] != true && b["slug"] != slug)
                    .count()
                    >= MAX_PINNED
            {
                return Err(ApiError::new(
                    409,
                    "Two Bots can be pinned. Unpin one first.",
                ));
            }
            let is_main = current.as_ref().is_some_and(|c| c["primary"] == true);
            if is_main && (patch["hidden"] == true || patch["primary"] == false) {
                return Err(ApiError::new(409, "Choose another main Bot first."));
            }
            let mut next = current.as_ref().map(obj).unwrap_or_default();
            let mut clean = obj(patch);
            if let Some(shape) = patch["shape"].as_str().and_then(shape_name) {
                clean.insert("shape".into(), json!(shape));
            }
            let wants_main = clean.remove("primary") == Some(Value::Bool(true));
            clean.remove("slug");
            clean.remove("workspaceRoot");
            next.extend(clean);
            next.insert("slug".into(), json!(slug));
            next.insert("primary".into(), json!(is_main));
            next.insert("updatedAt".into(), json!(now_iso()));
            if wants_main {
                next.insert("hidden".into(), json!(false));
            }
            self.persist(&Value::Object(next)).await?;
            if wants_main {
                write_json(&self.selection_file(), &json!({ "slug": slug })).await?;
            }
            self.reconcile()
                .await?
                .into_iter()
                .find(|b| b["slug"] == slug)
                .ok_or_else(|| ApiError::not_found("Bot not found"))?
        };
        self.write_instructions(&saved).await?;
        // A child's name is part of the parent's list, so the same rewrite runs for the parent.
        if is_subagent(&saved) {
            if let Some(parent_slug) = parent_slug_of(&saved) {
                if let Some(parent) = self.get(&parent_slug).await? {
                    self.write_instructions(&parent).await?;
                }
            }
        }
        Ok(saved)
    }

    pub async fn remove(&self, slug: &str) -> ApiResult<Vec<Value>> {
        let dir = self.dir(slug)?;
        let _g = self.lock.lock().await;
        let bots = self.reconcile().await?;
        let current = bots
            .iter()
            .find(|b| b["slug"] == slug)
            .ok_or_else(|| ApiError::not_found("Bot not found"))?;
        if bots
            .iter()
            .any(|b| parent_slug_of(b).as_deref() == Some(slug))
        {
            return Err(ApiError::new(
                409,
                "Delete this Bot's subagents before deleting this Bot.",
            ));
        }
        if current["primary"] == true && bots.len() > 1 {
            return Err(ApiError::new(
                409,
                "Choose another main Bot before deleting this Bot.",
            ));
        }
        let parent = if is_subagent(current) {
            parent_slug_of(current).and_then(|slug| {
                bots.iter()
                    .find(|b| b["slug"].as_str() == Some(slug.as_str()))
                    .cloned()
            })
        } else {
            None
        };
        let trash = self.root.join(".botapp").join("trash");
        tokio::fs::create_dir_all(&trash).await?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        tokio::fs::rename(&dir, trash.join(format!("{slug}-{stamp}"))).await?;
        if let Some(parent) = parent {
            self.write_instructions(&parent).await?;
        }
        self.reconcile().await
    }

    /// A new Bot folder: memory, library, skills and `bot.json`. A `parentSlug` makes a
    /// subagent of that Bot. The provisioner then gives it a T3 project and main thread.
    pub async fn create(&self, input: &Value) -> ApiResult<Value> {
        if input
            .get("shape")
            .is_some_and(|v| !v.is_null() && v.as_str().and_then(shape_name).is_none())
        {
            return Err(ApiError::bad_request("Pick a shape from the list."));
        }
        let parent = self.parent_for_create(input).await?;
        if parent.is_none() && input["hideFromSidebar"] == true {
            return Err(ApiError::new(
                409,
                "Only a subagent is hidden from the sidebar.",
            ));
        }
        let name = self.fresh_name(input, parent.as_ref()).await?;
        let prompt = match input.get("prompt") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(text)) => text.clone(),
            Some(_) => return Err(ApiError::bad_request("prompt must be a string")),
        };
        let base = slugify(&name);
        let (mut slug, mut n) = (base.clone(), 2);
        while tokio::fs::try_exists(self.root.join(&slug))
            .await
            .unwrap_or(false)
        {
            slug = format!("{base}-{n}");
            n += 1;
        }
        let dir = self.dir(&slug)?;
        tokio::fs::create_dir_all(dir.join("memory")).await?;
        tokio::fs::create_dir_all(dir.join("library")).await?;
        tokio::fs::create_dir_all(dir.join("skills")).await?;
        write_if_missing(
            &dir.join("memory").join("MEMORY.md"),
            &format!("# {name} memory\n\n"),
        )
        .await?;
        write_if_missing(&dir.join("memory").join("log.md"), "# Work log\n\n").await?;
        write_if_missing(
            &self.root.join(".botapp").join("shared-memory.md"),
            "# Shared facts (all Bots)\n\n",
        )
        .await?;
        let default_color = hbots_design::tokens().avatar.default.as_str();
        let shape = if let Some(shape) = input["shape"].as_str().and_then(shape_name) {
            shape
        } else if let Some(parent) = &parent {
            parent["shape"]
                .as_str()
                .and_then(shape_name)
                .unwrap_or_else(|| "ball".into())
        } else {
            "ball".into()
        };
        let color = if let Some(color) = input.get("color").filter(|v| !v.is_null()) {
            color.as_str().unwrap_or(default_color).to_string()
        } else if let Some(parent) = &parent {
            parent["color"]
                .as_str()
                .unwrap_or(default_color)
                .to_string()
        } else {
            default_color.to_string()
        };
        let (role, parent_slug, hide_from_sidebar, pinned, primary, subagent) =
            if let Some(parent) = &parent {
                (
                    "subagent",
                    json!(parent["slug"].as_str().unwrap_or_default()),
                    true,
                    false,
                    false,
                    child_subagent(input)?,
                )
            } else {
                (
                    "bot",
                    Value::Null,
                    false,
                    input["pinned"] == true,
                    input["primary"] == true,
                    Value::Null,
                )
            };
        let text = |k: &str, d: &str| json!(input[k].as_str().unwrap_or(d));
        let mut bot = json!({
            "name": name, "label": text("label", ""), "shape": shape, "color": color,
            "section": input["section"].as_str(), "pinned": pinned, "hidden": false,
            "primary": primary, "notifications": true, "projectId": null, "mainThreadId": null,
            "model": input.get("model").cloned().unwrap_or(Value::Null),
            "brief": input["brief"].as_str(),
            "firstMessage": input["firstMessage"].as_str().map(str::trim).filter(|m| !m.is_empty()).map(|m| m.chars().take(2000).collect::<String>()),
            "createdAt": now_iso(),
            "lastSeenAt": now_iso(),
            "role": role, "parentSlug": parent_slug, "hideFromSidebar": hide_from_sidebar,
            "prompt": prompt, "subagent": subagent,
        });
        if let Some(created_by) = input["createdBy"]
            .as_str()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            bot["createdBy"] = json!(created_by);
        }
        self.save(&slug, &bot, true).await
    }

    async fn parent_for_create(&self, input: &Value) -> ApiResult<Option<Value>> {
        let Some(raw) = input.get("parentSlug") else {
            return Ok(None);
        };
        if raw.is_null() {
            return Ok(None);
        }
        let Some(slug) = parent_slug_value(raw)? else {
            return Ok(None);
        };
        let parent = self
            .load()
            .await?
            .into_iter()
            .find(|b| b["slug"] == slug)
            .ok_or_else(|| ApiError::bad_request("That parent Bot does not exist."))?;
        if is_subagent(&parent) {
            return Err(ApiError::new(409, "A subagent can't have a subagent."));
        }
        Ok(Some(parent))
    }

    async fn fresh_name(&self, input: &Value, parent: Option<&Value>) -> ApiResult<String> {
        if let Some(name) = input["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            return Ok(name.to_string());
        }
        let Some(parent) = parent else {
            return Err(ApiError::bad_request("A Bot needs a name."));
        };
        let parent_name = parent["name"].as_str().unwrap_or("Bot");
        for n in 1..=10_000 {
            let name = format!("{parent_name} junior {n}");
            if !tokio::fs::try_exists(self.root.join(slugify(&name)))
                .await
                .unwrap_or(false)
            {
                return Ok(name);
            }
        }
        Err(ApiError::bad_request("A Bot needs a name."))
    }

    /// `.mcp.json`, `.codex/config.toml`, `.claude/settings.json` and the managed block of
    /// `AGENTS.md` (with `CLAUDE.md` linked to it), so Claude and Codex find the Bot's tools.
    /// Enabled connectors in `.bot/connectors.json` are kept. A broken file is left unread
    /// and these harness files are not rewritten, so a save cannot drop them.
    pub async fn write_instructions(&self, bot: &Value) -> ApiResult<()> {
        let slug = bot["slug"].as_str().unwrap_or_default();
        let dir = self.dir(slug)?;
        let connectors = read_connector_list(&dir.join(".bot").join("connectors.json")).await?;
        let mcp_path = dir.join(".mcp.json");
        let existing = read_json(&mcp_path).await.unwrap_or_else(|| json!({}));
        let mcp = crate::library::merge_mcp(&existing, &connectors, &self.mcp_url, &self.mcp_token);
        crate::store::write_atomic(
            &mcp_path,
            (serde_json::to_string_pretty(&mcp).unwrap_or_default() + "\n").as_bytes(),
            Some(0o600),
        )
        .await?;
        let toml = crate::library::codex_toml(&mcp);
        crate::store::write_atomic(
            &dir.join(".codex").join("config.toml"),
            toml.as_bytes(),
            Some(0o600),
        )
        .await?;
        let claude_settings = dir.join(".claude").join("settings.json");
        let cs_existing = read_json(&claude_settings)
            .await
            .unwrap_or_else(|| json!({}));
        let cs = crate::library::claude_settings(&cs_existing, &connectors);
        write_json(&claude_settings, &cs).await?;

        let agents = dir.join("AGENTS.md");
        let existing = tokio::fs::read_to_string(&agents).await.unwrap_or_default();
        let family = self.load().await?;
        let parent = parent_slug_of(bot).and_then(|slug| {
            family
                .iter()
                .find(|other| other["slug"].as_str() == Some(slug.as_str()))
        });
        let children: Vec<&Value> = if is_subagent(bot) {
            Vec::new()
        } else {
            let mut children: Vec<&Value> = family
                .iter()
                .filter(|other| parent_slug_of(other).as_deref() == Some(slug))
                .collect();
            children.sort_by(|a, b| {
                a["slug"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(b["slug"].as_str().unwrap_or_default())
            });
            children
        };
        tokio::fs::write(
            &agents,
            merge_managed(
                &existing,
                &render_agents_md(bot, &self.root, parent, &children),
            ),
        )
        .await?;
        #[cfg(unix)]
        {
            let claude = dir.join("CLAUDE.md");
            if tokio::fs::symlink_metadata(&claude).await.is_err() {
                let _ = tokio::fs::symlink("AGENTS.md", &claude).await;
            }
        }
        Ok(())
    }
}

/// Missing is an empty list. A file that is not JSON is an error, so the caller
/// does not rewrite `.mcp.json` and drop the connectors it could not read.
async fn read_connector_list(path: &Path) -> ApiResult<Vec<Value>> {
    match tokio::fs::read_to_string(path).await {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
        Ok(text) if text.trim().is_empty() => Ok(Vec::new()),
        Ok(text) => {
            #[allow(
                clippy::map_err_ignore,
                reason = "serde quotes connectors.json, which can hold a connector secret"
            )]
            let doc: Value = serde_json::from_str(&text)
                .map_err(|_| ApiError::new(500, "connectors.json is not valid JSON"))?;
            Ok(doc["connectors"].as_array().cloned().unwrap_or_default())
        }
    }
}

async fn write_if_missing(path: &Path, text: &str) -> std::io::Result<()> {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        if let Some(d) = path.parent() {
            tokio::fs::create_dir_all(d).await?;
        }
        tokio::fs::write(path, text).await?;
    }
    Ok(())
}

/// The brief names T3 Code because that is the harness wired today. Other backends may be added.
/// `parent` is the parent Bot record when this Bot is a subagent. `children` are that parent's
/// subagents. A parent's prompt is its `prompt` field.
pub(crate) fn render_agents_md(
    bot: &Value,
    root: &Path,
    parent: Option<&Value>,
    children: &[&Value],
) -> String {
    let name = bot["name"].as_str().unwrap_or("Bot");
    let label = bot["label"]
        .as_str()
        .filter(|l| !l.is_empty())
        .map(|l| format!(" ({l})"))
        .unwrap_or_default();
    let shared = root.join(".botapp").join("shared-memory.md");
    let child = is_subagent(bot);
    let parent_name = parent
        .and_then(|p| p["name"].as_str())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| parent_slug_of(bot))
        .unwrap_or_else(|| "its parent".into());
    let intro = if child {
        format!("You are {name}, a subagent of {parent_name}. {parent_name} assigns your work.")
    } else {
        format!(
            "You are {name}, a personal Bot. You run as a long-lived T3 Code thread; the person talks to you from the Bots app on their phone, desktop or browser."
        )
    };
    let own_folders = if child {
        "- Your own notes folder: `memory/`.\n- Your own skills folder: `skills/`.\n"
    } else {
        ""
    };
    let own_prompt = if child {
        let prompt = bot["subagent"]["prompt"].as_str().unwrap_or("").trim();
        if prompt.is_empty() {
            String::new()
        } else {
            format!("\n## Your prompt\n\n{}\n", plain_prompt(prompt))
        }
    } else {
        String::new()
    };
    let inherited = if child && bot["subagent"]["inheritParentPrompt"] == true {
        let prompt = plain_prompt(parent.and_then(|p| p["prompt"].as_str()).unwrap_or(""));
        let prompt = prompt.trim();
        if prompt.is_empty() {
            String::new()
        } else {
            format!("\n## Parent prompt\n\n{prompt}\n")
        }
    } else {
        String::new()
    };
    let subagents = if child {
        String::new()
    } else {
        let mut lines = String::from("\n## Subagents\n\n");
        if children.is_empty() {
            lines.push_str("You have no lasting helpers yet.\n");
        } else {
            for child in children {
                let child_name = child["name"].as_str().unwrap_or("Bot");
                let child_slug = child["slug"].as_str().unwrap_or("");
                lines.push_str(&format!("- {child_name} (`{child_slug}`)\n"));
            }
            lines.push('\n');
        }
        lines.push_str(
            "To list lasting helpers, use `subagent_list`. Create one with `subagent_create` (its own folder, project, and main thread). Work that ends with this turn stays a T3 child thread, and is not a lasting Subagent.\n",
        );
        lines
    };
    format!(
        "{MANAGED_START}\n# {name}{label}\n\n\
{intro}\n\n\
## Memory\n\n- Your notes: `memory/MEMORY.md`. Keep durable facts there, short and current.\n- Your work log: `memory/log.md`.\n{own_folders}- Facts shared by every Bot: {}.\n- Files you keep for the person: `library/`.\n{own_prompt}{inherited}\n\
## Tools\n\nThe `bot` MCP server gives you the Vault (`vault_list`) and the Projects board (`projects_*`). Never read the vault files on disk.\n\n\
## Routines\n\nTo run something on a schedule, use your `schedule_task` tool. Weekdays count from 0 for Sunday to 6 for Saturday, and times are local. The person sees your routines in the app but can't change them there, so say what you set up.\n\n## Computer\n\nThe person can open or close your browser from the Computer panel. A message framed `[Computer]` means open a page (resume where you left off, or https://www.google.com) or close your browser tabs with your browser tools, without asking, and stay quiet in chat.\n\n## Other Bots\n\nTo message another Bot, use `t3_thread_send`. Say who you are and what you need.\n{subagents}\n## Writing\n\nAvoid Markdown tables. Prefer bullet lists.\n\n## Asking first\n\nAsk before anything that leaves this machine or can't be undone.\n{MANAGED_END}\n",
        shared.display()
    )
}

fn plain_prompt(text: &str) -> String {
    text.replace(MANAGED_START, "").replace(MANAGED_END, "")
}

pub(crate) fn merge_managed(existing: &str, block: &str) -> String {
    if existing.is_empty() {
        return block.to_string();
    }
    match (existing.find(MANAGED_START), existing.find(MANAGED_END)) {
        (Some(s), Some(e)) if e > s => format!(
            "{}{}{}",
            &existing[..s],
            block.trim_end(),
            &existing[e + MANAGED_END.len()..]
        ),
        _ => format!("{block}\n{existing}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Ada  Lovelace"), "ada-lovelace");
        assert_eq!(slugify("Élodie_B"), "elodie-b");
        assert_eq!(slugify("!!!"), "bot");
        assert!(valid_slug("ada-2") && !valid_slug("-ada") && !valid_slug("../x"));
    }

    #[test]
    fn shapes_come_from_the_design_crate_and_sphere_reads_as_the_ball() {
        assert_eq!(shape_name("hex").as_deref(), Some("hex"));
        assert_eq!(shape_name("sphere").as_deref(), Some("ball"));
        assert_eq!(shape_name("blob"), None);
        assert_eq!(shape_name(""), None);
    }

    #[tokio::test]
    async fn a_bot_takes_a_shape_and_refuses_an_unknown_one() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots
            .create(&json!({ "name": "Ada", "shape": "cloud" }))
            .await
            .unwrap();
        assert_eq!(ada["shape"], "cloud");
        let plain = bots.create(&json!({ "name": "Bob" })).await.unwrap();
        assert_eq!(plain["shape"], "ball");
        assert!(bots
            .create(&json!({ "name": "Cy", "shape": "blob" }))
            .await
            .is_err());
        let slug = ada["slug"].as_str().unwrap();
        let saved = bots
            .save(slug, &json!({ "shape": "sphere" }), false)
            .await
            .unwrap();
        assert_eq!(saved["shape"], "ball");
        assert!(bots
            .save(slug, &json!({ "shape": "blob" }), false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn only_two_bots_besides_the_main_one_can_be_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let mut slugs = Vec::new();
        for name in ["Main", "One", "Two", "Three"] {
            slugs.push(
                bots.create(&json!({ "name": name })).await.unwrap()["slug"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
        }
        bots.select(&slugs[0]).await.unwrap();
        assert!(bots
            .save(&slugs[1], &json!({ "pinned": true }), false)
            .await
            .is_ok());
        assert!(bots
            .save(&slugs[2], &json!({ "pinned": true }), false)
            .await
            .is_ok());
        let third = bots
            .save(&slugs[3], &json!({ "pinned": true }), false)
            .await
            .unwrap_err();
        assert_eq!(third.status, 409);
        // Pinning one already pinned is fine, and unpinning makes room.
        assert!(bots
            .save(&slugs[1], &json!({ "pinned": true }), false)
            .await
            .is_ok());
        bots.save(&slugs[1], &json!({ "pinned": false }), false)
            .await
            .unwrap();
        assert!(bots
            .save(&slugs[3], &json!({ "pinned": true }), false)
            .await
            .is_ok());
        assert!(bots
            .save(&slugs[0], &json!({ "pinned": "yes" }), false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn notifications_must_be_a_boolean_and_a_subagent_can_store_false() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let slug = ada["slug"].as_str().unwrap();
        let bad = bots
            .save(slug, &json!({ "notifications": "no" }), false)
            .await
            .unwrap_err();
        assert_eq!(status_of(bad), 400);
        assert_eq!(
            bots.get(slug).await.unwrap().unwrap()["notifications"],
            true
        );
        let saved = bots
            .save(slug, &json!({ "notifications": false }), false)
            .await
            .unwrap();
        assert_eq!(saved["notifications"], false);

        let junior = bots.create(&json!({ "parentSlug": slug })).await.unwrap();
        let child = junior["slug"].as_str().unwrap();
        let muted = bots
            .save(child, &json!({ "notifications": false }), false)
            .await
            .unwrap();
        assert_eq!(muted["role"], "subagent");
        assert_eq!(muted["notifications"], false);
        let child_bad = bots
            .save(child, &json!({ "notifications": "off" }), false)
            .await
            .unwrap_err();
        assert_eq!(status_of(child_bad), 400);
    }

    #[tokio::test]
    async fn a_new_bot_starts_seen_and_remembers_its_first_message() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots
            .create(&json!({ "name": "Ada", "firstMessage": "  Plan my day.  " }))
            .await
            .unwrap();
        assert!(ada["lastSeenAt"].as_str().is_some_and(|t| !t.is_empty()));
        assert_eq!(ada["firstMessage"], "Plan my day.");
    }

    #[test]
    fn the_managed_block_tells_a_bot_about_routines_and_other_bots() {
        let text = render_agents_md(
            &json!({ "name": "Ada" }),
            std::path::Path::new("/bots"),
            None,
            &[],
        );
        assert!(text.contains("schedule_task") && text.contains("t3_thread_send"));
        assert!(text.contains("Avoid Markdown tables. Prefer bullet lists."));
        assert!(text.contains("## Computer") && text.contains("[Computer]"));
        assert!(text.contains("https://www.google.com"));
        assert!(text.contains("Sunday") && text.starts_with(MANAGED_START));
    }

    #[test]
    fn managed_block_keeps_user_text() {
        let first = merge_managed("", "<!-- bot:managed:start -->A<!-- bot:managed:end -->\n");
        let user = format!("{first}\nMy own notes\n");
        let next = merge_managed(
            &user,
            "<!-- bot:managed:start -->B<!-- bot:managed:end -->\n",
        );
        assert!(next.contains('B') && !next.contains(">A<") && next.contains("My own notes"));
    }

    #[tokio::test]
    async fn instructions_keep_enabled_connectors() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(
            dir.path().to_path_buf(),
            "http://127.0.0.1:3780/mcp/bot".into(),
            "tok".into(),
        );
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let slug = ada["slug"].as_str().unwrap();
        let folder = dir.path().join(slug);
        let connectors = json!({
            "connectors": [
                { "name": "notes", "url": "http://127.0.0.1:9/mcp", "enabled": true, "secret": "s3cret" },
                { "name": "off", "url": "https://example.com/mcp", "enabled": false, "secret": "hidden" }
            ]
        });
        tokio::fs::create_dir_all(folder.join(".bot"))
            .await
            .unwrap();
        tokio::fs::write(
            folder.join(".bot").join("connectors.json"),
            serde_json::to_vec(&connectors).unwrap(),
        )
        .await
        .unwrap();
        bots.write_instructions(&ada).await.unwrap();
        let mcp: Value = serde_json::from_str(
            &tokio::fs::read_to_string(folder.join(".mcp.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(mcp["mcpServers"]["bot"]["url"]
            .as_str()
            .unwrap()
            .contains("/mcp/bot"));
        assert_eq!(mcp["mcpServers"]["notes"]["url"], "http://127.0.0.1:9/mcp");
        assert!(mcp["mcpServers"]["notes"]["headers"]["Authorization"]
            .as_str()
            .unwrap()
            .contains("s3cret"));
        assert!(mcp["mcpServers"].get("off").is_none());
        assert!(!mcp.to_string().contains("hidden"));
        let toml = tokio::fs::read_to_string(folder.join(".codex").join("config.toml"))
            .await
            .unwrap();
        assert!(toml.contains("[mcp_servers.bot]"));
        assert!(toml.contains("[mcp_servers.notes]"));
        assert!(!toml.contains("[mcp_servers.off]"));
        assert!(!toml.contains("hidden"));
        let cs: Value = serde_json::from_str(
            &tokio::fs::read_to_string(folder.join(".claude").join("settings.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(cs["enableAllProjectMcpServers"], true);
        let disabled = cs["disabledMcpjsonServers"].as_array().unwrap();
        assert!(disabled.iter().any(|n| n == "off"));
        assert!(!disabled.iter().any(|n| n == "notes" || n == "bot"));
        assert!(!cs.to_string().contains("hidden"));
    }

    #[tokio::test]
    async fn a_broken_connector_file_is_left_unread() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(
            dir.path().to_path_buf(),
            "http://127.0.0.1:3780/mcp/bot".into(),
            "tok".into(),
        );
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let slug = ada["slug"].as_str().unwrap();
        let folder = dir.path().join(slug);
        let before = tokio::fs::read_to_string(folder.join(".mcp.json"))
            .await
            .unwrap();
        tokio::fs::write(folder.join(".bot").join("connectors.json"), "{")
            .await
            .unwrap();
        assert!(bots.write_instructions(&ada).await.is_err());
        let after = tokio::fs::read_to_string(folder.join(".mcp.json"))
            .await
            .unwrap();
        assert_eq!(before, after);
        assert!(after.contains("\"bot\""));
    }

    #[tokio::test]
    async fn create_select_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(
            dir.path().to_path_buf(),
            "http://127.0.0.1:3780/mcp/bot".into(),
            "tok".into(),
        );
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        assert_eq!(ada["primary"], true, "the first Bot is the main Bot");
        let bob = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        assert_eq!(bob["slug"], "ada-2");
        assert!(
            bots.remove("ada").await.is_err(),
            "the main Bot can't be deleted while others exist"
        );
        let list = bots.select("ada-2").await.unwrap();
        assert_eq!(list.iter().filter(|b| b["primary"] == true).count(), 1);
        bots.remove("ada").await.unwrap();
        assert_eq!(bots.list().await.unwrap().len(), 1);
        let agents = std::fs::read_to_string(dir.path().join("ada-2/AGENTS.md")).unwrap();
        assert!(agents.starts_with(MANAGED_START));
        let mcp: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("ada-2/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            mcp["mcpServers"]["bot"]["headers"]["Authorization"],
            "Bearer tok"
        );
    }

    #[tokio::test]
    async fn an_unreadable_main_bot_choice_is_an_error_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(
            dir.path().to_path_buf(),
            "http://127.0.0.1:3780/mcp/bot".into(),
            "tok".into(),
        );
        bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let choice = dir.path().join(".botapp/main-bot.json");
        std::fs::write(&choice, "{not json").unwrap();
        assert!(bots.list().await.is_err());
        assert_eq!(std::fs::read_to_string(&choice).unwrap(), "{not json");
    }

    fn status_of(err: ApiError) -> u16 {
        err.status.as_u16()
    }

    #[tokio::test]
    async fn a_bot_record_defaults_role_and_keeps_a_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots
            .create(&json!({ "name": "Ada", "prompt": "Be brief." }))
            .await
            .unwrap();
        assert_eq!(ada["role"], "bot");
        assert!(ada["parentSlug"].is_null());
        assert_eq!(ada["hideFromSidebar"], false);
        assert!(ada["subagent"].is_null());
        assert_eq!(ada["prompt"], "Be brief.");
        assert!(dir.path().join("ada/skills").is_dir());
        assert_eq!(
            std::fs::read_dir(dir.path().join("ada/skills"))
                .unwrap()
                .count(),
            0
        );
        let saved = bots
            .save("ada", &json!({ "prompt": "Be shorter." }), false)
            .await
            .unwrap();
        assert_eq!(saved["prompt"], "Be shorter.");
        assert!(bots
            .save("ada", &json!({ "hideFromSidebar": true }), false)
            .await
            .is_err_and(|e| status_of(e) == 409));
        assert!(bots.create(&json!({})).await.is_err());
    }

    #[tokio::test]
    async fn a_child_is_a_bot_with_a_parent_and_one_level() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots
            .create(&json!({ "name": "Ada", "shape": "cloud", "color": "#3366cc" }))
            .await
            .unwrap();
        let junior = bots
            .create(&json!({
                "parentSlug": "ada",
                "pinned": true,
                "primary": true,
                "subagent": { "prompt": "Look things up.", "inheritParentPrompt": true, "toolAllow": ["read"] }
            }))
            .await
            .unwrap();
        assert_eq!(junior["name"], "Ada junior 1");
        assert_eq!(junior["slug"], "ada-junior-1");
        assert_eq!(junior["role"], "subagent");
        assert_eq!(junior["parentSlug"], "ada");
        assert_eq!(junior["hideFromSidebar"], true);
        assert_eq!(junior["hidden"], false);
        assert_eq!(junior["pinned"], false);
        assert_eq!(junior["primary"], false);
        assert_eq!(junior["shape"], "cloud");
        assert_eq!(junior["color"], "#3366cc");
        assert_eq!(junior["subagent"]["prompt"], "Look things up.");
        assert_eq!(junior["subagent"]["inheritParentPrompt"], true);
        assert_eq!(junior["subagent"]["toolAllow"], json!(["read"]));
        assert!(dir.path().join("ada-junior-1/skills").is_dir());
        assert_eq!(ada["primary"], true);

        let named = bots
            .create(&json!({ "name": "Research", "parentSlug": "ada", "shape": "hex", "color": "#111111" }))
            .await
            .unwrap();
        assert_eq!(named["name"], "Research");
        assert_eq!(named["slug"], "research");
        assert_eq!(named["shape"], "hex");
        assert_eq!(named["color"], "#111111");

        let second = bots.create(&json!({ "parentSlug": "ada" })).await.unwrap();
        assert_eq!(second["name"], "Ada junior 2");
        let taken = bots
            .create(&json!({ "name": "Ada junior 1", "parentSlug": "ada" }))
            .await
            .unwrap();
        assert_eq!(taken["slug"], "ada-junior-1-2");

        let nested = bots
            .create(&json!({ "name": "Nope", "parentSlug": "ada-junior-1" }))
            .await
            .unwrap_err();
        assert_eq!(status_of(nested), 409);
        let missing = bots
            .create(&json!({ "name": "Nope", "parentSlug": "missing" }))
            .await
            .unwrap_err();
        assert_eq!(status_of(missing), 400);

        let replaced = bots
            .save(
                "ada-junior-1",
                &json!({ "subagent": { "prompt": "New.", "cap": 3 } }),
                false,
            )
            .await
            .unwrap();
        assert_eq!(replaced["subagent"]["prompt"], "New.");
        assert_eq!(replaced["subagent"]["cap"], 3);
        assert!(replaced["subagent"].get("toolAllow").is_none());
        assert!(replaced["subagent"].get("inheritParentPrompt").is_none());

        for patch in [
            json!({ "pinned": true }),
            json!({ "hidden": true }),
            json!({ "primary": true }),
            json!({ "hideFromSidebar": false }),
            json!({ "parentSlug": "research" }),
            json!({ "role": "bot" }),
        ] {
            let err = bots.save("ada-junior-1", &patch, false).await.unwrap_err();
            assert_eq!(status_of(err), 409, "{patch}");
        }
        let same = bots
            .save(
                "ada-junior-1",
                &json!({ "role": "subagent", "parentSlug": "ada", "hideFromSidebar": true, "name": "Scout" }),
                false,
            )
            .await
            .unwrap();
        assert_eq!(same["name"], "Scout");
        assert_eq!(same["parentSlug"], "ada");

        assert_eq!(
            status_of(bots.select("ada-junior-1").await.unwrap_err()),
            409
        );
        assert_eq!(status_of(bots.remove("ada").await.unwrap_err()), 409);
        bots.remove("ada-junior-1").await.unwrap();
        bots.remove("research").await.unwrap();
        bots.remove("ada-junior-2").await.unwrap();
        bots.remove(taken["slug"].as_str().unwrap()).await.unwrap();
        assert!(bots.remove("ada").await.is_ok());
    }

    #[tokio::test]
    async fn reconcile_does_not_make_a_child_the_main_bot() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        bots.create(&json!({ "name": "Ada" })).await.unwrap();
        bots.create(&json!({ "parentSlug": "ada" })).await.unwrap();
        std::fs::write(
            dir.path().join(".botapp/main-bot.json"),
            r#"{"slug":"ada-junior-1"}"#,
        )
        .unwrap();
        let list = bots.list().await.unwrap();
        let main = list.iter().find(|b| b["primary"] == true).unwrap();
        assert_eq!(main["slug"], "ada");
        assert_eq!(
            list.iter().find(|b| b["slug"] == "ada-junior-1").unwrap()["primary"],
            false
        );
    }

    #[test]
    fn a_child_block_names_its_parent_and_its_own_folders() {
        let parent = json!({ "name": "Ada", "slug": "ada", "prompt": "Be brief." });
        let child = json!({
            "name": "Scout",
            "slug": "scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "prompt": "Look things up.", "inheritParentPrompt": false }
        });
        let text = render_agents_md(&child, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(text.starts_with(MANAGED_START) && text.contains(MANAGED_END));
        assert!(text.contains("a subagent of Ada"));
        assert!(text.contains("Ada assigns your work"));
        assert!(text.contains("`memory/`") && text.contains("`skills/`"));
        assert!(!text.contains("Be brief."));
    }

    #[test]
    fn a_child_block_includes_the_parent_prompt_field_when_asked() {
        let parent = json!({
            "name": "Ada",
            "slug": "ada",
            "prompt": "Be brief. This is the prompt field."
        });
        let child = json!({
            "name": "Scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "inheritParentPrompt": true, "prompt": "Look things up." }
        });
        let text = render_agents_md(&child, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(text.contains("Be brief. This is the prompt field."));
        let quiet = json!({
            "name": "Scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "inheritParentPrompt": false }
        });
        let without = render_agents_md(&quiet, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(!without.contains("Be brief."));
    }

    #[test]
    fn a_parent_block_lists_its_children_and_names_subagent_tools() {
        let ada = json!({ "name": "Ada", "slug": "ada", "role": "bot" });
        let scout = json!({ "name": "Scout", "slug": "scout", "role": "subagent" });
        let lookup = json!({ "name": "Lookup", "slug": "lookup", "role": "subagent" });
        let text = render_agents_md(
            &ada,
            std::path::Path::new("/bots"),
            None,
            &[&scout, &lookup],
        );
        assert!(text.contains("- Scout (`scout`)"));
        assert!(text.contains("- Lookup (`lookup`)"));
        assert!(text.contains("`subagent_list`") && text.contains("`subagent_create`"));
        assert!(text.contains("T3 child thread"));
        assert!(!text.contains("subagent_send"));
        let alone = render_agents_md(&ada, std::path::Path::new("/bots"), None, &[]);
        assert!(alone.contains("no lasting helpers yet"));
        assert!(alone.contains("`subagent_list`") && alone.contains("`subagent_create`"));
        assert!(alone.contains("T3 child thread"));
    }

    #[tokio::test]
    async fn saving_a_child_refreshes_the_parent_managed_block() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
            .await
            .unwrap();
        let agents = dir.path().join("ada/AGENTS.md");
        let mut text = std::fs::read_to_string(&agents).unwrap();
        text.push_str("\nMy own notes\n");
        std::fs::write(&agents, &text).unwrap();
        bots.create(&json!({
            "parentSlug": "ada",
            "name": "Scout",
            "subagent": { "prompt": "Look things up.", "inheritParentPrompt": true }
        }))
        .await
        .unwrap();
        let parent_md = std::fs::read_to_string(&agents).unwrap();
        assert!(parent_md.contains("- Scout (`scout`)"));
        assert!(parent_md.contains("`subagent_list`") && parent_md.contains("`subagent_create`"));
        assert!(!parent_md.contains("subagent_send"));
        assert!(parent_md.contains("My own notes"));
        let child_md = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
        assert!(child_md.contains("a subagent of Ada"));
        assert!(child_md.contains("Be brief."));
        assert!(child_md.contains("`skills/`"));
        bots.save("scout", &json!({ "name": "Finder" }), false)
            .await
            .unwrap();
        let parent_md = std::fs::read_to_string(&agents).unwrap();
        assert!(parent_md.contains("- Finder (`scout`)"));
        assert!(!parent_md.contains("Scout"));
        assert!(parent_md.contains("My own notes"));
        let outside = parent_md.split(MANAGED_END).nth(1).unwrap_or("");
        assert!(outside.contains("My own notes"));
        assert!(!outside.contains("Finder"));
    }

    #[test]
    fn a_child_block_includes_its_own_prompt_and_omits_an_empty_one() {
        let parent = json!({ "name": "Ada", "slug": "ada", "prompt": "Be brief." });
        let child = json!({
            "name": "Scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "prompt": "  Look things up.  ", "inheritParentPrompt": true }
        });
        let text = render_agents_md(&child, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(text.contains("## Your prompt\n\nLook things up."));
        assert!(text.contains("## Parent prompt\n\nBe brief."));
        let empty = json!({
            "name": "Scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "prompt": "   ", "inheritParentPrompt": false }
        });
        let without = render_agents_md(&empty, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(!without.contains("## Your prompt"));
        assert!(!without.contains("Be brief."));
    }

    #[test]
    fn a_child_block_names_its_parent_and_does_not_name_subagent_send() {
        let parent = json!({ "name": "Ada", "slug": "ada" });
        let child = json!({
            "name": "Scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "prompt": "" }
        });
        let text = render_agents_md(&child, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(text.contains("a subagent of Ada"));
        assert!(!text.contains("subagent_send"));
    }

    #[test]
    fn an_empty_inherited_parent_prompt_adds_nothing() {
        let parent = json!({ "name": "Ada", "slug": "ada", "prompt": "   " });
        let child = json!({
            "name": "Scout",
            "role": "subagent",
            "parentSlug": "ada",
            "subagent": { "prompt": "Look things up.", "inheritParentPrompt": true }
        });
        let text = render_agents_md(&child, std::path::Path::new("/bots"), Some(&parent), &[]);
        assert!(text.contains("## Your prompt\n\nLook things up."));
        assert!(!text.contains("## Parent prompt"));
    }

    #[tokio::test]
    async fn created_by_round_trips_when_the_create_sets_it() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        assert!(ada.get("createdBy").is_none());
        let junior = bots
            .create(&json!({
                "parentSlug": "ada",
                "name": "Scout",
                "brief": "Find it.",
                "createdBy": "Ada"
            }))
            .await
            .unwrap();
        assert_eq!(junior["createdBy"], "Ada");
        assert_eq!(junior["brief"], "Find it.");
        let loaded = bots.get("scout").await.unwrap().unwrap();
        assert_eq!(loaded["createdBy"], "Ada");
        assert_eq!(loaded["brief"], "Find it.");
    }

    #[tokio::test]
    async fn deleting_a_child_refreshes_the_parent_managed_block() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let agents = dir.path().join("ada/AGENTS.md");
        let mut text = std::fs::read_to_string(&agents).unwrap();
        text.push_str("\nMy own notes\n");
        std::fs::write(&agents, &text).unwrap();
        bots.create(&json!({ "parentSlug": "ada", "name": "Scout" }))
            .await
            .unwrap();
        bots.create(&json!({ "parentSlug": "ada", "name": "Lookup" }))
            .await
            .unwrap();
        bots.remove("scout").await.unwrap();
        let parent_md = std::fs::read_to_string(&agents).unwrap();
        assert!(parent_md.contains("- Lookup (`lookup`)"));
        assert!(!parent_md.contains("Scout"));
        assert!(!parent_md.contains("`scout`"));
        let outside = parent_md.split(MANAGED_END).nth(1).unwrap_or("");
        assert!(outside.contains("My own notes"));
        assert!(!outside.contains("Lookup"));
    }
}
