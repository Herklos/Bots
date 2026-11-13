//! Filesystem bots: `<root>/<slug>/.bot/bot.json`, the main-Bot choice in
//! `<root>/.botapp/main-bot.json`, and machine bindings in a `local.json`
//! overlay beside each profile when local overlays are enabled.
use super::bot_overlay;
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::stores::{valid_slug, BotStore};
use crate::storage::{read_json, write_json};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FilesystemBotStore {
    root: PathBuf,
    split_local: bool,
}

impl FilesystemBotStore {
    pub(crate) fn new(root: PathBuf, split_local: bool) -> Self {
        Self { root, split_local }
    }

    fn meta_file(&self, slug: &str) -> ApiResult<PathBuf> {
        if !valid_slug(slug) {
            return Err(ApiError::bad_request("bad slug"));
        }
        Ok(self.root.join(slug).join(".bot").join("bot.json"))
    }

    fn selection_file(&self) -> PathBuf {
        self.root.join(".botapp").join("main-bot.json")
    }
}

#[async_trait]
impl BotStore for FilesystemBotStore {
    async fn acquire(&self) -> std::io::Result<Option<std::fs::File>> {
        crate::storage::local_lock::acquire_if_active(
            self.root.join(".botapp/bots.lock"),
            self.split_local,
        )
        .await
    }

    async fn list_slugs(&self) -> ApiResult<Vec<String>> {
        let mut slugs = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(slugs),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().await?.is_dir() && valid_slug(&name) {
                slugs.push(name);
            }
        }
        Ok(slugs)
    }

    async fn read_meta(&self, slug: &str) -> ApiResult<Option<Value>> {
        let profile = self.meta_file(slug)?;
        // A missing or unreadable profile means there is no Bot to load here.
        let Some(meta) = read_json(&profile).await else {
            return Ok(None);
        };
        if self.split_local {
            Ok(Some(bot_overlay::load(&profile, meta).await?))
        } else {
            Ok(Some(bot_overlay::restore(&profile, meta).await?))
        }
    }

    async fn write_meta(&self, slug: &str, meta: &Value) -> ApiResult<()> {
        let profile = self.meta_file(slug)?;
        if self.split_local {
            bot_overlay::save(&profile, meta).await?;
        } else {
            write_json(&profile, meta).await?;
        }
        Ok(())
    }

    async fn read_selection(&self) -> ApiResult<Option<Value>> {
        match tokio::fs::read_to_string(self.selection_file()).await {
            Ok(text) => Some(
                #[allow(
                    clippy::map_err_ignore,
                    reason = "serde quotes the choice file, which names the main Bot"
                )]
                serde_json::from_str::<Value>(&text)
                    .map_err(|_| ApiError::new(500, "The main Bot choice file can't be read.")),
            )
            .transpose(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_selection(&self, slug: Option<&str>) -> ApiResult<()> {
        write_json(&self.selection_file(), &json!({ "slug": slug })).await?;
        Ok(())
    }
}
