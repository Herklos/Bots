use super::*;
use crate::bots::runtime_mode::{is_stored_runtime_mode, runtime_mode_rejected};

impl Bots {
    pub async fn save(&self, slug: &str, patch: &Value, create: bool) -> ApiResult<Value> {
        self.dir(slug)?;
        let saved = {
            let _g = self.lock.lock().await;
            let _disk_guard = self.bot_store.acquire()
            .await?;
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
            if let Some(prompt) = patch.get("prompt") {
                let Some(text) = prompt.as_str() else {
                    return Err(ApiError::bad_request("prompt must be a string"));
                };
                crate::bots::agent_context::ensure_prompt_limit(text)?;
            }
            if let Some(mode) = patch.get("runtimeMode") {
                if !mode.as_str().is_some_and(is_stored_runtime_mode) {
                    return Err(ApiError::bad_request(runtime_mode_rejected("runtimeMode")));
                }
            }
            if let Some(mode) = patch.get("code") {
                crate::chat::code::require_mode(mode).map_err(ApiError::bad_request)?;
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
            clean.remove("environment");
            let mut policy = Map::new();
            for key in ["subagent", "subagents", "childThreads"] {
                if let Some(value) = clean.remove(key) {
                    policy.insert(key.to_string(), value);
                }
            }
            next.extend(clean);
            next.insert("slug".into(), json!(slug));
            next.insert("primary".into(), json!(is_main));
            next.insert("updatedAt".into(), json!(now_iso()));
            if wants_main {
                next.insert("hidden".into(), json!(false));
            }
            super::helpers_policy::merge_policy(&mut next, &policy)?;
            if create {
                super::helpers_policy::ensure_can_add(
                    &bots,
                    &json!({
                        "role": next.get("role"),
                        "hideFromSidebar": next.get("hideFromSidebar"),
                        "parentSlug": next.get("parentSlug"),
                    }),
                )?;
            }
            self.persist(&Value::Object(next)).await?;
            if wants_main {
                self.bot_store.write_selection(Some(slug)).await?;
            }
            let mut saved = self
                .reconcile()
                .await?
                .into_iter()
                .find(|b| b["slug"] == slug)
                .ok_or_else(|| ApiError::not_found("Bot not found"))?;
            super::helpers_policy::resolve_on_view(&mut saved);
            saved
        };
        let slug = saved["slug"].as_str().unwrap_or("").to_string();
        self.refresh_agents(&slug).await?;
        Ok(saved)
    }

    pub async fn remove(&self, slug: &str) -> ApiResult<Vec<Value>> {
        let dir = self.dir(slug)?;
        let _g = self.lock.lock().await;
        let _disk_guard = self.bot_store.acquire()
        .await?;
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
        let remaining = Self::show_all(self.reconcile().await?);
        drop(_disk_guard);
        drop(_g);
        if let Some(parent) = parent {
            self.write_instructions(&parent).await?;
        }
        Ok(remaining)
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
        crate::bots::agent_context::ensure_prompt_limit(&prompt)?;
        let runtime_mode = match input.get("runtimeMode") {
            None | Some(Value::Null) => None,
            Some(Value::String(mode)) if is_stored_runtime_mode(mode) => Some(mode.clone()),
            Some(_) => {
                return Err(ApiError::bad_request(runtime_mode_rejected("runtimeMode")));
            }
        };
        let code_mode = match input.get("code") {
            None | Some(Value::Null) => None,
            Some(value) => {
                Some(crate::chat::code::require_mode(value).map_err(ApiError::bad_request)?)
            }
        };
        let goal = match input.get("goal") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => {
                let text: String = text.trim().chars().take(2000).collect();
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
            Some(_) => return Err(ApiError::bad_request("goal must be a string")),
        };
        let pending_routines = match input.get("pendingRoutines") {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) if items.is_empty() => None,
            Some(Value::Array(items)) => {
                for item in items {
                    crate::chat::routines::validate_pending_routine(item)
                        .map_err(ApiError::bad_request)?;
                }
                Some(Value::Array(items.clone()))
            }
            Some(_) => return Err(ApiError::bad_request("pendingRoutines must be a list")),
        };
        let child_plan = if let Some(parent) = &parent {
            let existing = {
                let _g = self.lock.lock().await;
                let _disk_guard = self.bot_store.acquire()
                .await?;
                self.load().await?
            };
            Some(super::helpers_policy::plan_child(input, parent, &existing)?)
        } else {
            None
        };
        let base = slugify(&name);
        let (mut slug, mut n) = (base.clone(), 2);
        let store = crate::bots::memory::MemoryStore::new(std::sync::Arc::clone(&self.ws));
        while store.exists(&slug).await.unwrap_or(false) {
            slug = format!("{base}-{n}");
            n += 1;
        }
        self.dir(&slug)?;
        store.bootstrap(&slug, &name).await?;
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
        if let Some(mode) = runtime_mode {
            bot["runtimeMode"] = json!(mode);
        }
        if let Some(mode) = code_mode {
            bot["code"] = json!(mode.as_str());
        }
        if let Some(goal) = goal {
            bot["goal"] = json!(goal);
        }
        if let Some(pending) = pending_routines {
            bot["pendingRoutines"] = pending;
        }
        if let Some(plan) = child_plan {
            super::helpers_policy::apply_plan(&mut bot, plan);
        }
        self.save(&slug, &bot, true).await
    }

    pub(crate) async fn parent_for_create(&self, input: &Value) -> ApiResult<Option<Value>> {
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

    pub(crate) async fn fresh_name(
        &self,
        input: &Value,
        parent: Option<&Value>,
    ) -> ApiResult<String> {
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
}
