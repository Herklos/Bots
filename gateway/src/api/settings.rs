use super::*;

pub(crate) async fn get_settings(State(s): S) -> Json<Value> {
    Json(s.settings().await)
}

pub(crate) async fn put_settings(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let patch = parse(&body)?;
    if !patch.is_object() {
        return Err(ApiError::bad_request("settings must be an object"));
    }
    let current = s.settings().await;
    let mut patch = patch;
    // The default model is checked like a Bot's, and stored as T3 stores a selection.
    if patch.get("defaultModel").is_some_and(|v| v.is_object()) {
        let chosen =
            checked_selection(&s, &patch["defaultModel"], &current["defaultModel"]).await?;
        patch["defaultModel"] = chosen;
    }
    if let Some(policy) = patch.get("groupAnswerPolicy") {
        if crate::chat::rooms::AnswerPolicy::parse(policy.as_str()).is_none() {
            return Err(ApiError::bad_request(
                "groupAnswerPolicy must be last, everyone, or chief",
            ));
        }
    }
    if let Some(mode) = patch.get("defaultRuntimeMode") {
        require_runtime_mode("defaultRuntimeMode", mode)?;
    }
    if let Some(mode) = patch.get("maxRuntimeMode") {
        require_runtime_mode("maxRuntimeMode", mode)?;
    }
    let mut next = storage::merge(&current, &patch);
    if let Some(beta) = patch.get("beta") {
        let beta = beta
            .as_object()
            .ok_or_else(|| ApiError::bad_request("beta must be an object"))?;
        let mut merged = current["beta"].clone();
        for (k, v) in beta {
            if k != "projects" {
                return Err(ApiError::bad_request(format!("Unknown beta feature: {k}")));
            }
            merged[k] = json!(v
                .as_bool()
                .ok_or_else(|| ApiError::bad_request(format!("beta.{k} must be true or false")))?);
        }
        next["beta"] = merged;
    }
    if let Some(notifications) = patch.get("notifications") {
        let notifications = notifications
            .as_object()
            .ok_or_else(|| ApiError::bad_request("notifications must be an object"))?;
        let mut merged = current["notifications"].clone();
        for (k, v) in notifications {
            match k.as_str() {
                "enabled" | "sound" => {
                    merged[k] = json!(v.as_bool().ok_or_else(|| {
                        ApiError::bad_request(format!("notifications.{k} must be true or false"))
                    })?);
                }
                "preview" => {
                    let preview = v
                        .as_str()
                        .filter(|p| matches!(*p, "full" | "hidden"))
                        .ok_or_else(|| {
                            ApiError::bad_request("notifications.preview must be full or hidden")
                        })?;
                    merged[k] = json!(preview);
                }
                "when" => {
                    let when = v
                        .as_str()
                        .filter(|w| matches!(*w, "any" | "background"))
                        .ok_or_else(|| {
                            ApiError::bad_request("notifications.when must be any or background")
                        })?;
                    merged[k] = json!(when);
                }
                _ => {
                    return Err(ApiError::bad_request(format!(
                        "Unknown notifications setting: {k}"
                    )));
                }
            }
        }
        next["notifications"] = merged;
    }
    // The default for a new Bot is stored at or below the cap.
    crate::bots::runtime_mode::apply_default_cap(&mut next);
    storage::write_json(&s.settings_path(), &next).await?;
    Ok(Json(next))
}

fn require_runtime_mode(key: &str, value: &Value) -> Result<(), ApiError> {
    if crate::bots::runtime_mode::RuntimeMode::parse(value.as_str()).is_some() {
        return Ok(());
    }
    Err(ApiError::bad_request(format!(
        "{key} must be approval-required, auto, or full-access"
    )))
}
