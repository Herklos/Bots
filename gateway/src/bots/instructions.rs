use super::*;

pub(crate) const MANAGED_START: &str = "<!-- bot:managed:start -->";

pub(crate) const MANAGED_END: &str = "<!-- bot:managed:end -->";

/// Missing is an empty list. A file that is not JSON is an error, so the caller
/// does not rewrite `.mcp.json` and drop the connectors it could not read.
pub(crate) async fn read_connector_list(path: &Path) -> ApiResult<Vec<Value>> {
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

/// The brief names T3 Code because that is the harness wired today. Other backends may be added.
/// `parent` is the parent Bot record when this Bot is a subagent. `children` are that parent's
/// subagents. A parent's prompt is its `prompt` field.
pub(crate) fn render_agents_md(
    bot: &Value,
    root: &Path,
    parent: Option<&Value>,
    children: &[&Value],
    memory: &MemorySnapshot,
    rules: &Value,
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
    let goal = bot["goal"]
        .as_str()
        .map(|text| crate::bots::agent_context::prompt_section("Goal", text))
        .unwrap_or_default();
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
        crate::bots::agent_context::prompt_section(
            "Your prompt",
            bot["subagent"]["prompt"].as_str().unwrap_or(""),
        )
    } else {
        String::new()
    };
    let inherited = if child && bot["subagent"]["inheritParentPrompt"] == true {
        crate::bots::agent_context::prompt_section(
            "Parent prompt",
            parent.and_then(|p| p["prompt"].as_str()).unwrap_or(""),
        )
    } else {
        String::new()
    };
    let snapshot = crate::bots::agent_context::render_snapshot(memory);
    let child_threads = crate::bots::helpers_prompt::child_threads_section_for(bot);
    let instructions = if child {
        String::new()
    } else {
        crate::bots::agent_context::prompt_section(
            "Instructions",
            bot["prompt"].as_str().unwrap_or(""),
        )
    };
    let subagents = crate::bots::helpers_prompt::subagents_section(bot, children);
    let independent = crate::bots::prompt_policy::stored_independent(bot);
    let other_bots = crate::bots::prompt_policy::other_bots_blurb(independent);
    let policy = crate::bots::prompt_policy::policy_section(independent, rules);
    format!(
        "{MANAGED_START}\n# {name}{label}\n\n\
{intro}\n{goal}\n\
## Memory\n\n- Your notes: `memory/MEMORY.md`. Keep durable facts there, short and current.\n- Your work log: `memory/log.md`.\n{own_folders}- Facts shared by every Bot: {}.\n- Files you keep for the person: `library/`.\n- To show the person a picture, save it as PNG, JPEG, GIF or WebP under `library/`, call `image_show`, and put the Markdown it returns in your reply.\n{snapshot}{instructions}{own_prompt}{inherited}{child_threads}\n\
## Tools\n\nThe `bot` MCP server gives you the Vault (`vault_list`) and the Projects board (`projects_*`). Never read the vault files on disk.\n\n\
## Routines\n\nTo run something on a schedule, use your `schedule_task` tool. Weekdays count from 0 for Sunday to 6 for Saturday, and times are local. The person sees your routines in the app but can't change them there, so say what you set up.\n\n## Computer\n\nThe person can open or close your browser from the Computer panel. A message framed `[Computer]` means open a page (resume where you left off, or https://www.google.com) or close your browser tabs with your browser tools, without asking, and stay quiet in chat.\n\n## Other Bots\n\n{other_bots}{subagents}\n## Writing\n\nAvoid Markdown tables. Prefer bullet lists.\n\n{policy}{MANAGED_END}\n",
        shared.display()
    )
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

impl Bots {
    /// `.mcp.json`, `.codex/config.toml`, `.claude/settings.json` and the managed block of
    /// `AGENTS.md` (with `CLAUDE.md` linked to it), so Claude and Codex find the Bot's tools.
    /// Enabled connectors in `.bot/connectors.json` are kept. A broken file is left unread
    /// and these harness files are not rewritten, so a save cannot drop them.
    pub async fn write_instructions(&self, bot: &Value) -> ApiResult<()> {
        let settings = read_json(&self.root.join(".botapp").join("settings.json"))
            .await
            .unwrap_or_else(|| json!({}));
        self.write_instructions_with(bot, &settings).await
    }

    /// `settings.rules` is rendered only for an Independent Bot.
    async fn write_instructions_with(&self, bot: &Value, settings: &Value) -> ApiResult<()> {
        let slug = bot["slug"].as_str().unwrap_or_default();
        let dir = self.dir(slug)?;
        let connectors = read_connector_list(&dir.join(".bot").join("connectors.json")).await?;
        let mcp_path = dir.join(".mcp.json");
        let existing = read_json(&mcp_path).await.unwrap_or_else(|| json!({}));
        let mcp =
            crate::bots::library::merge_mcp(&existing, &connectors, &self.mcp_url, &self.mcp_token);
        crate::storage::write_atomic(
            &mcp_path,
            (serde_json::to_string_pretty(&mcp).unwrap_or_default() + "\n").as_bytes(),
            Some(0o600),
        )
        .await?;
        let toml = crate::bots::library::codex_toml(&mcp);
        crate::storage::write_atomic(
            &dir.join(".codex").join("config.toml"),
            toml.as_bytes(),
            Some(0o600),
        )
        .await?;
        #[cfg(unix)]
        crate::bots::library::write_agents_hook_script(
            &dir.join(".bot").join("hooks").join("agents-md.sh"),
        )
        .await?;
        let claude_settings = dir.join(".claude").join("settings.json");
        let cs_existing = read_json(&claude_settings)
            .await
            .unwrap_or_else(|| json!({}));
        let cs = crate::bots::library::claude_settings(&cs_existing, &connectors);
        write_json(&claude_settings, &cs).await?;

        let agents = dir.join("AGENTS.md");
        let existing_agents = tokio::fs::read_to_string(&agents).await.unwrap_or_default();
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
        let memory = crate::bots::agent_context::read_snapshot(&self.root, bot, parent).await;
        let rules = settings.get("rules").unwrap_or(&Value::Null);
        let block = render_agents_md(bot, &self.root, parent, &children, &memory, rules);
        let merged = merge_managed(&existing_agents, &block);
        if merged != existing_agents {
            crate::storage::write_atomic(&agents, merged.as_bytes(), None).await?;
        }
        #[cfg(unix)]
        self.merge_claude_md(&dir, slug, &block).await?;
        Ok(())
    }

    /// Load this Bot and rewrite its managed block, then the parent's list or the children.
    /// `write_instructions` skips the file when the merged text is unchanged.
    pub async fn refresh_agents(&self, slug: &str) -> ApiResult<()> {
        let settings = read_json(&self.root.join(".botapp").join("settings.json"))
            .await
            .unwrap_or_else(|| json!({}));
        let Some(bot) = self.get(slug).await? else {
            return Ok(());
        };
        self.write_instructions_with(&bot, &settings).await?;
        if is_subagent(&bot) {
            if let Some(parent_slug) = parent_slug_of(&bot) {
                if let Some(parent) = self.get(&parent_slug).await? {
                    self.write_instructions_with(&parent, &settings).await?;
                }
            }
        } else {
            let family = self.load().await?;
            let children: Vec<Value> = family
                .into_iter()
                .filter(|other| parent_slug_of(other).as_deref() == Some(slug))
                .collect();
            for child in children {
                self.write_instructions_with(&child, &settings).await?;
            }
        }
        Ok(())
    }

    /// `CLAUDE.md` is a symlink to `AGENTS.md` when it is absent. A regular file keeps its
    /// own text and receives the same managed block, with one warning per Bot.
    #[cfg(unix)]
    pub(crate) async fn merge_claude_md(
        &self,
        dir: &Path,
        slug: &str,
        block: &str,
    ) -> ApiResult<()> {
        let claude = dir.join("CLAUDE.md");
        match tokio::fs::symlink_metadata(&claude).await {
            Err(_) => {
                let _ = tokio::fs::symlink("AGENTS.md", &claude).await;
                Ok(())
            }
            Ok(meta) if meta.file_type().is_symlink() => Ok(()),
            Ok(meta) if meta.file_type().is_file() => {
                self.warn_claude_file_once(slug);
                let existing = tokio::fs::read_to_string(&claude).await.unwrap_or_default();
                let merged = merge_managed(&existing, block);
                if merged != existing {
                    crate::storage::write_atomic(&claude, merged.as_bytes(), None).await?;
                }
                Ok(())
            }
            Ok(_) => Ok(()),
        }
    }

    #[cfg(unix)]
    pub(crate) fn warn_claude_file_once(&self, slug: &str) {
        let mut seen = self
            .claude_file_warned
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if seen.insert(slug.to_string()) {
            tracing::warn!(
                "CLAUDE.md for {slug} is a regular file; merging the managed block into it"
            );
        }
    }
}
