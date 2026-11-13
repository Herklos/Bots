use super::*;

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

pub(crate) use crate::storage::stores::valid_slug;

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

pub(crate) fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

pub(crate) fn is_subagent(bot: &Value) -> bool {
    bot["role"] == "subagent" || bot["hideFromSidebar"] == true
}

pub(crate) fn parent_slug_of(bot: &Value) -> Option<String> {
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

pub(crate) fn parent_slug_value(value: &Value) -> ApiResult<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    let slug = value
        .as_str()
        .ok_or_else(|| ApiError::bad_request("parentSlug must be a string"))?;
    Ok(Some(slug.trim().to_string()).filter(|s| !s.is_empty()))
}

pub(crate) fn check_subagent_keys(fields: &Map<String, Value>) -> ApiResult<()> {
    if let Some(prompt) = fields.get("prompt") {
        let Some(text) = prompt.as_str() else {
            return Err(ApiError::bad_request("subagent.prompt must be a string"));
        };
        crate::bots::agent_context::ensure_prompt_limit(text)?;
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
pub(crate) fn child_subagent(input: &Value) -> ApiResult<Value> {
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
