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

pub const MANAGED_START: &str = "<!-- bot:managed:start -->";
pub const MANAGED_END: &str = "<!-- bot:managed:end -->";

pub fn valid_slug(slug: &str) -> bool {
    let b = slug.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// "Ada Lovelace" → "ada-lovelace".
pub fn slugify(name: &str) -> String {
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

pub struct Bots {
    pub root: PathBuf,
    pub mcp_url: String,
    pub mcp_token: String,
    lock: Mutex<()>,
}

fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
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
                "label": "", "shape": "sphere", "color": hbots_design::tokens().avatar.default, "section": null, "pinned": false,
                "hidden": false, "primary": false, "notifications": true, "projectId": null, "mainThreadId": null,
            })
            .as_object()
            .cloned()
            .unwrap_or_default();
            bot.extend(obj(&meta));
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
            .filter(|c| bots.iter().any(|b| b["slug"] == c.as_str()))
            .cloned()
            .or_else(|| {
                if selection.is_none() {
                    bots.iter()
                        .find(|b| b["hidden"] != true && b["primary"] == true)
                        .and_then(|b| b["slug"].as_str().map(String::from))
                } else {
                    None
                }
            })
            .or_else(|| {
                bots.iter()
                    .find(|b| b["hidden"] != true)
                    .or(bots.first())
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
            for key in ["primary", "hidden"] {
                if patch.get(key).is_some_and(|v| !v.is_boolean()) {
                    return Err(ApiError::bad_request(format!("{key} must be a boolean")));
                }
            }
            let is_main = current.as_ref().is_some_and(|c| c["primary"] == true);
            if is_main && (patch["hidden"] == true || patch["primary"] == false) {
                return Err(ApiError::new(409, "Choose another main Bot first."));
            }
            let mut next = current.as_ref().map(obj).unwrap_or_default();
            let mut clean = obj(patch);
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
        if current["primary"] == true && bots.len() > 1 {
            return Err(ApiError::new(
                409,
                "Choose another main Bot before deleting this Bot.",
            ));
        }
        let trash = self.root.join(".botapp").join("trash");
        tokio::fs::create_dir_all(&trash).await?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        tokio::fs::rename(&dir, trash.join(format!("{slug}-{stamp}"))).await?;
        self.reconcile().await
    }

    /// A new Bot folder: memory, library and `bot.json`. The provisioner then gives it a T3
    /// project and main thread.
    pub async fn create(&self, input: &Value) -> ApiResult<Value> {
        let name = input["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| ApiError::bad_request("A Bot needs a name."))?;
        let base = slugify(name);
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
        let text = |k: &str, d: &str| json!(input[k].as_str().unwrap_or(d));
        let bot = json!({
            "name": name, "label": text("label", ""), "shape": text("shape", "sphere"), "color": text("color", &hbots_design::tokens().avatar.default),
            "section": input["section"].as_str(), "pinned": input["pinned"] == true, "hidden": false,
            "primary": input["primary"] == true, "notifications": true, "projectId": null, "mainThreadId": null,
            "model": input.get("model").cloned().unwrap_or(Value::Null),
            "brief": input["brief"].as_str(),
            "createdAt": now_iso(),
        });
        self.save(&slug, &bot, true).await
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
        tokio::fs::write(
            &agents,
            merge_managed(&existing, &render_agents_md(bot, &self.root)),
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
pub fn render_agents_md(bot: &Value, root: &Path) -> String {
    let name = bot["name"].as_str().unwrap_or("Bot");
    let label = bot["label"]
        .as_str()
        .filter(|l| !l.is_empty())
        .map(|l| format!(" ({l})"))
        .unwrap_or_default();
    let shared = root.join(".botapp").join("shared-memory.md");
    format!(
        "{MANAGED_START}\n# {name}{label}\n\n\
You are {name}, a personal Bot. You run as a long-lived T3 Code thread; the person talks to you from the Bots app on their phone, desktop or browser.\n\n\
## Memory\n\n- Your notes: `memory/MEMORY.md`. Keep durable facts there, short and current.\n- Your work log: `memory/log.md`.\n- Facts shared by every Bot: `{}`.\n- Files you keep for the person: `library/`.\n\n\
## Tools\n\nThe `bot` MCP server gives you the Vault (`vault_list`) and the Projects board (`projects_*`). Never read the vault files on disk.\n\n\
## Asking first\n\nAsk before anything that leaves this machine or can't be undone.\n{MANAGED_END}\n",
        shared.display()
    )
}

pub fn merge_managed(existing: &str, block: &str) -> String {
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
}
