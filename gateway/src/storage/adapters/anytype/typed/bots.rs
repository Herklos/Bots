//! The anytype Bot store: shared profile fields live on `m_bot` objects keyed
//! by `m_slug`, machine bindings live in a complement sidecar on this device,
//! and a legacy filesystem profile is adopted once by pushing it. The main-Bot
//! choice rides on the shared `m_settings` object as `m_main_bot`.

use super::mapper;
use super::TypedCtx;
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::stores::{valid_slug, BotStore};
use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct AnytypeBotStore {
    root: PathBuf,
    local_dir: PathBuf,
    ctx: Arc<TypedCtx>,
}

impl AnytypeBotStore {
    pub(crate) fn new(root: PathBuf, data_root: PathBuf, ctx: Arc<TypedCtx>) -> Self {
        Self {
            root,
            local_dir: data_root.join("bot-local"),
            ctx,
        }
    }

    fn sidecar_file(&self, slug: &str) -> ApiResult<PathBuf> {
        if !valid_slug(slug) {
            return Err(ApiError::bad_request("bad slug"));
        }
        Ok(self.local_dir.join(format!("{slug}.json")))
    }

    async fn write_complement(&self, slug: &str, meta: &Value) -> ApiResult<()> {
        let file = self.sidecar_file(slug)?;
        let complement = mapper::bot_complement(meta);
        if complement.is_empty() {
            let _ = tokio::fs::remove_file(&file).await;
            return Ok(());
        }
        crate::storage::write_json(&file, &Value::Object(complement)).await?;
        Ok(())
    }

    async fn read_complement(&self, slug: &str) -> ApiResult<Map<String, Value>> {
        let file = self.sidecar_file(slug)?;
        match crate::storage::read_json(&file).await {
            Some(Value::Object(map)) => Ok(map),
            Some(_) => Err(ApiError::new(500, "Bot local settings file is not valid")),
            None => Ok(Map::new()),
        }
    }

    fn profile(&self, slug: &str, props: &Value) -> Value {
        let shared = Value::Object(mapper::bot_meta(props));
        let mut profile = json!({ "slug": slug });
        if let (Some(profile), Some(shared)) = (profile.as_object_mut(), shared.as_object()) {
            for (key, value) in shared {
                profile.insert(key.clone(), value.clone());
            }
        }
        profile
    }

    /// A fresh read: merge sidecar first, remote shared fields on top, then
    /// anything still pending in the outbox.
    async fn merged_meta(&self, slug: &str) -> ApiResult<Option<Value>> {
        let complement = self.read_complement(slug).await?;
        match self.bot_object(slug).await? {
            Some(object) => {
                let props = object.get("properties").cloned().unwrap_or_else(|| json!({}));
                let mut profile = self.profile(slug, &props);
                if let Some(profile) = profile.as_object_mut() {
                    for (key, value) in complement {
                        if !profile.contains_key(&key) {
                            profile.insert(key, value);
                        }
                    }
                }
                let pending = self.ctx.pending_properties(mapper::BOT_TYPE, slug);
                if let Some(pending) = pending {
                    let shared = Value::Object(mapper::bot_meta(&Value::Object(pending)));
                    if let (Some(profile), Some(shared)) =
                        (profile.as_object_mut(), shared.as_object())
                    {
                        for (key, value) in shared {
                            profile.insert(key.clone(), value.clone());
                        }
                    }
                }
                Ok(Some(profile))
            }
            None => self.adopt_or_none(slug).await,
        }
    }

    async fn bot_object(&self, slug: &str) -> ApiResult<Option<Value>> {
        let Some(object_id) = self
            .ctx
            .find_object(mapper::BOT_TYPE, slug)
            .await
            .map_err(anytype_error)?
        else {
            return Ok(None);
        };
        let object = self.ctx.get_object(&object_id).await.map_err(anytype_error)?;
        Ok(Some(object))
    }
}

fn anytype_error(error: crate::storage::adapters::anytype::AnytypeError) -> ApiError {
    ApiError::new(
        502,
        format!("The Anytype space is not answering: {}", super::error_note(&error)),
    )
}

#[async_trait]
impl BotStore for AnytypeBotStore {
    /// Remote profiles are read fresh per call; a durable distributed lock is
    /// the home-device work still ahead of this store.
    async fn acquire(&self) -> std::io::Result<Option<std::fs::File>> {
        Ok(None)
    }

    async fn list_slugs(&self) -> ApiResult<Vec<String>> {
        let mut slugs = std::collections::BTreeSet::new();
        let rows = self
            .ctx
            .search_type(mapper::BOT_TYPE, &["m_slug"])
            .await
            .map_err(anytype_error)?;
        for row in &rows {
            if let Some(slug) = mapper::properties_at(row)
                .and_then(|props| props.get("m_slug"))
                .and_then(Value::as_str)
                .filter(|slug| !slug.is_empty())
            {
                slugs.insert(slug.to_string());
            }
        }
        // Legacy filesystem profiles still here are adopted on read.
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(slugs.into_iter().collect());
            }
            Err(error) => return Err(error.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().await?.is_dir() && valid_slug(&name) {
                slugs.insert(name);
            }
        }
        Ok(slugs.into_iter().collect())
    }

    async fn read_meta(&self, slug: &str) -> ApiResult<Option<Value>> {
        self.sidecar_file(slug)?;
        self.merged_meta(slug).await
    }

    async fn write_meta(&self, slug: &str, meta: &Value) -> ApiResult<()> {
        self.sidecar_file(slug)?;
        self.write_complement(slug, meta).await?;
        let mut props = mapper::bot_props(meta);
        props.insert("m_slug".into(), json!(slug));
        self.ctx.submit(mapper::BOT_TYPE, slug, &Value::Object(props)).await;
        Ok(())
    }

    async fn read_selection(&self) -> ApiResult<Option<Value>> {
        let Some(object) = self.ctx.settings_object().await.map_err(anytype_error)? else {
            return Ok(None);
        };
        let reference = object
            .get("properties")
            .and_then(|props| props.get(mapper::MAIN_BOT))
            .cloned()
            .unwrap_or(Value::Null);
        let Some(object_id) = reference_id(&reference) else {
            return Ok(None);
        };
        let rows = self
            .ctx
            .search_type(mapper::BOT_TYPE, &["m_slug"])
            .await
            .map_err(anytype_error)?;
        Ok(rows
            .iter()
            .filter(|row| row["id"].as_str() == Some(object_id))
            .find_map(|row| {
                mapper::properties_at(row)
                    .and_then(|props| props.get("m_slug"))
                    .and_then(Value::as_str)
                    .map(|slug| json!({ "slug": slug }))
            })
            .filter(|choice| match choice["slug"].as_str() {
                Some(slug) => !slug.is_empty(),
                None => false,
            }))
    }

    async fn write_selection(&self, slug: Option<&str>) -> ApiResult<()> {
        let reference = match slug {
            None => Value::Null,
            Some(slug) => {
                let Some(object_id) = self
                    .ctx
                    .find_object(mapper::BOT_TYPE, slug)
                    .await
                    .map_err(anytype_error)?
                else {
                    return Err(ApiError::new(
                        409,
                        "That Bot is not in the Anytype space yet; try again after it syncs.",
                    ));
                };
                json!([object_id])
            }
        };
        let props = json!({ mapper::MAIN_BOT: reference });
        self.ctx
            .submit(mapper::SETTINGS_TYPE, mapper::SETTINGS_NATURAL, &props)
            .await;
        Ok(())
    }
}

impl AnytypeBotStore {
    /// No remote object for this slug: serve a legacy filesystem profile and
    /// push it once, or serve only the pending write when that is all there is.
    async fn adopt_or_none(&self, slug: &str) -> ApiResult<Option<Value>> {
        let profile = self.root.join(slug).join(".bot").join("bot.json");
        if let Some(mut meta) = crate::storage::read_json(&profile).await {
            meta["slug"] = json!(slug);
            let named = meta
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty());
            if !named {
                meta["name"] = json!(slug);
            }
            self.write_complement(slug, &meta).await?;
            let mut props = mapper::bot_props(&meta);
            props.insert("m_slug".into(), json!(slug));
            self.ctx.submit(mapper::BOT_TYPE, slug, &Value::Object(props)).await;
            return Ok(Some(meta));
        }
        let Some(pending) = self.ctx.pending_properties(mapper::BOT_TYPE, slug) else {
            return Ok(None);
        };
        Ok(Some(self.profile(slug, &Value::Object(pending))))
    }
}

fn reference_id(reference: &Value) -> Option<&str> {
    match reference {
        Value::Array(items) => items.first().and_then(Value::as_str),
        Value::String(id) => Some(id),
        _ => None,
    }
}
