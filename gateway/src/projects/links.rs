use super::*;

/// `{type: "bot", name, slug}` for a Bot, otherwise the user.
#[allow(
    dead_code,
    reason = "board rows build the actor object inline, so this helper has no caller yet"
)]
pub(crate) fn actor(input: Option<&Value>) -> Value {
    match input {
        Some(a) if a["type"] == "bot" => {
            let mut out = json!({ "type": "bot" });
            if let Some(name) = a["name"].as_str() {
                out["name"] = json!(name);
            }
            if let Some(slug) = a["slug"].as_str() {
                out["slug"] = json!(slug);
            }
            out
        }
        _ => json!({ "type": "user" }),
    }
}

/// "You" for the user, otherwise the Bot's name.
pub(crate) fn actor_label(actor: &Value) -> String {
    if actor.is_null() || actor["type"] == "user" {
        return "You".into();
    }
    actor["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| actor["slug"].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("A Bot")
        .to_string()
}

/// One link, checked by its type. Any link may carry a label.
pub(crate) fn clean_link(v: &Value) -> ApiResult<Value> {
    let obj = v
        .as_object()
        .ok_or_else(|| invalid("Each link must be an object with a type."))?;
    let get = |k: &str| obj.get(k).unwrap_or(&Value::Null);
    let mut out = Map::new();
    match obj.get("type").and_then(Value::as_str).unwrap_or("") {
        "thread" => {
            let thread = oneline(get("threadId"), "threadId", 1024)?
                .ok_or_else(|| invalid("A thread link needs threadId."))?;
            out.insert("type".into(), json!("thread"));
            out.insert("threadId".into(), json!(thread));
            if let Some(env) = env_id(get("environmentId"))? {
                out.insert("environmentId".into(), json!(env));
            }
        }
        "bot" => {
            let slug = oneline(get("slug"), "slug", 63)?
                .filter(|s| valid_slug(s))
                .ok_or_else(|| invalid("A Bot link needs the Bot's slug."))?;
            out.insert("type".into(), json!("bot"));
            out.insert("slug".into(), json!(slug));
        }
        "item" => {
            let id =
                oneline(get("id"), "id", 40)?.ok_or_else(|| invalid("An item link needs id."))?;
            out.insert("type".into(), json!("item"));
            out.insert("id".into(), json!(id));
        }
        "url" => {
            let url = clean_url(get("url"), "link url")?
                .ok_or_else(|| invalid("A url link needs url."))?;
            out.insert("type".into(), json!("url"));
            out.insert("url".into(), json!(url));
        }
        "t3project" => {
            let project = oneline(get("projectId"), "projectId", 200)?
                .ok_or_else(|| invalid("A t3project link needs projectId."))?;
            out.insert("type".into(), json!("t3project"));
            out.insert("projectId".into(), json!(project));
            if let Some(env) = env_id(get("environmentId"))? {
                out.insert("environmentId".into(), json!(env));
            }
        }
        _ => {
            return Err(invalid(format!(
                "Link type must be one of {}.",
                LINK_TYPES.join(", ")
            )))
        }
    }
    if let Some(label) = oneline(get("label"), "link label", 120)? {
        out.insert("label".into(), json!(label));
    }
    Ok(Value::Object(out))
}

/// The identity of a link: two links with the same key are the same link.
pub(crate) fn link_key(link: &Value) -> String {
    let target = ["threadId", "slug", "id", "url", "projectId"]
        .iter()
        .find_map(|k| link.get(*k).filter(|v| !v.is_null()))
        .cloned()
        .unwrap_or(Value::Null);
    let env = link
        .get("environmentId")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or(Value::Null);
    json!([link["type"], target, env]).to_string()
}

/// Cleans a list of links. A repeated link collapses into one (the later one wins).
pub(crate) fn clean_links(list: &Value) -> ApiResult<Vec<Value>> {
    let arr = list
        .as_array()
        .ok_or_else(|| invalid("links must be a list."))?;
    let mut out: Vec<(String, Value)> = Vec::new();
    for item in arr {
        let link = clean_link(item)?;
        let key = link_key(&link);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = link,
            None => out.push((key, link)),
        }
    }
    if out.len() > 40 {
        return Err(invalid("Use at most 40 links per item."));
    }
    Ok(out.into_iter().map(|(_, v)| v).collect())
}

pub(crate) fn current_links(item: &Map<String, Value>) -> Vec<Value> {
    item.get("links")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn valid_ref_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// An item id, or `ref:<name>` for an item created earlier in the same batch. Null means "none".
pub(crate) fn resolve_ref(
    raw: &Value,
    refs: &HashMap<String, String>,
    items: &Map<String, Value>,
    field: &str,
) -> ApiResult<Option<String>> {
    if raw.is_null() {
        return Ok(None);
    }
    let value = raw
        .as_str()
        .ok_or_else(|| invalid(format!("{field} must be an item id.")))?;
    let id = match value.strip_prefix("ref:") {
        Some(name) if valid_ref_name(name) => refs.get(name).cloned(),
        _ => Some(value.to_string()),
    };
    let id = id.filter(|i| !i.is_empty()).ok_or_else(|| {
        invalid(format!(
            "{field} \"{value}\" doesn't name an item in this batch."
        ))
    })?;
    if !items.contains_key(&id) {
        return Err(invalid(format!("{field} \"{value}\" isn't on the board.")));
    }
    Ok(Some(id))
}

pub(crate) fn assert_no_cycle(items: &Map<String, Value>, id: &str, parent: &str) -> ApiResult<()> {
    let mut at = Some(parent.to_string());
    let mut steps = 0;
    while let Some(current) = at {
        if current == id || steps > 50 {
            return Err(invalid("An item can't be inside itself."));
        }
        at = items
            .get(&current)
            .and_then(|i| i["parentId"].as_str())
            .map(String::from);
        steps += 1;
    }
    Ok(())
}

pub(crate) fn items_mut(state: &mut Value) -> ApiResult<&mut Map<String, Value>> {
    state["items"]
        .as_object_mut()
        .ok_or_else(|| ApiError::new(500, "The Projects board is damaged."))
}
