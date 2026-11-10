//! The Projects board, reading and writing `bots/.botapp/projects.json`
//! (`{version, seq, items: {id: Item}, activity: []}`):
//! per-kind statuses, field cleaners, link cleaning and de-duplication, `ref:` batches,
//! archive/restore/complete activity, `order`, children moving up on delete, and the
//! query views the Bots use (`overview`, `find`, `detail`). A save is all or nothing, and
//! every change is stamped with who made it and broadcast to open pages.
//!
//! Items stay as JSON objects, so fields this module does not know about survive a re-save.
use crate::{
    error::{ApiError, ApiResult},
    store::write_atomic,
    vault::now_iso,
};
use serde_json::{json, Map, Value};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use tokio::sync::{broadcast, Mutex};

pub const KINDS: &[&str] = &[
    "project", "task", "machine", "service", "repo", "note", "link",
];
pub const LINK_TYPES: &[&str] = &["thread", "bot", "item", "url", "t3project"];
pub const PRIORITIES: &[&str] = &["high", "normal", "low"];
pub const FIELD_NAMES: &[&str] = &[
    "url", "path", "host", "port", "branch", "due", "priority", "owner", "tags",
];
pub const MAX_BATCH: usize = 100;
const MAX_ITEMS: usize = 5000;
const MAX_ACTIVITY: usize = 300;
const BOARD_TAIL: usize = 60;
const TRACKED: &[&str] = &[
    "kind",
    "title",
    "status",
    "area",
    "body",
    "parentId",
    "machineId",
    "fields",
    "links",
    "pinned",
    "order",
    "archived",
];
const ID_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// The statuses a kind may have. Note and link have none.
pub fn statuses(kind: &str) -> &'static [&'static str] {
    match kind {
        "project" => &["active", "paused", "idea", "done"],
        "task" => &["todo", "doing", "waiting", "done"],
        "machine" => &["online", "offline", "unknown"],
        "service" => &["running", "stopped", "failing", "unknown"],
        "repo" => &["active", "archived"],
        _ => &[],
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(message)
}

fn unreadable() -> ApiError {
    ApiError::new(500, "The Projects board file can't be read.")
}

fn kind_error() -> ApiError {
    invalid(format!("kind must be one of {}.", KINDS.join(", ")))
}

/// `{type: "bot", name, slug}` for a Bot, otherwise the user.
pub fn actor(input: Option<&Value>) -> Value {
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
pub fn actor_label(actor: &Value) -> String {
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

fn quoted(v: &Value) -> String {
    format!(
        "\"{}\"",
        v.as_str().map_or_else(|| v.to_string(), String::from)
    )
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn as_list(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(list) => list.clone(),
        other => vec![other.clone()],
    }
}

/// Text as JavaScript's cleaners see it: CRLF folded, trimmed, bounded; empty means absent.
fn text(v: &Value, field: &str, max: usize) -> ApiResult<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => {
            let t = s.replace("\r\n", "\n");
            let t = t.trim();
            if t.chars().count() > max {
                return Err(invalid(format!("{field} is longer than {max} characters.")));
            }
            Ok((!t.is_empty()).then(|| t.to_string()))
        }
        _ => Err(invalid(format!("{field} must be text."))),
    }
}

/// One line: like `text`, with runs of whitespace collapsed.
fn oneline(v: &Value, field: &str, max: usize) -> ApiResult<Option<String>> {
    Ok(text(v, field, max)?.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")))
}

fn clean_text(v: &Value, field: &str, max: usize) -> ApiResult<Option<Value>> {
    Ok(oneline(v, field, max)?.map(Value::String))
}

fn clean_url(v: &Value, field: &str) -> ApiResult<Option<String>> {
    let Some(s) = oneline(v, field, 2000)? else {
        return Ok(None);
    };
    let url = url::Url::parse(&s).map_err(|_| {
        invalid(format!(
            "{field} must be a full address, like https://example.com."
        ))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(format!(
            "{field} must start with http:// or https://."
        )));
    }
    Ok(Some(url.to_string()))
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 {
        return false;
    }
    let shape_ok = b.iter().enumerate().all(|(i, c)| match i {
        4 | 7 => *c == b'-',
        _ => c.is_ascii_digit(),
    });
    if !shape_ok {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        s[0..4].parse::<u32>(),
        s[5..7].parse::<u32>(),
        s[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

fn clean_date(v: &Value) -> ApiResult<Option<Value>> {
    let Some(s) = oneline(v, "due", 10)? else {
        return Ok(None);
    };
    if !valid_date(&s) {
        return Err(invalid("due must be a date like 2026-11-03."));
    }
    Ok(Some(json!(s)))
}

fn clean_tags(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() {
        return Ok(None);
    }
    let list = v
        .as_array()
        .ok_or_else(|| invalid("tags must be a list of words."))?;
    let mut tags: Vec<String> = Vec::new();
    for raw in list {
        if raw.is_null() {
            continue;
        }
        let Some(tag) = oneline(raw, "tag", 32)?.map(|t| t.to_lowercase()) else {
            continue;
        };
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    if tags.len() > 12 {
        return Err(invalid("Use at most 12 tags."));
    }
    Ok((!tags.is_empty()).then(|| json!(tags)))
}

fn clean_port(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() || v.as_str() == Some("") {
        return Ok(None);
    }
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    };
    match n {
        Some(n) if n.fract() == 0.0 && (1.0..=65535.0).contains(&n) => Ok(Some(json!(n as u64))),
        _ => Err(invalid("port must be a number from 1 to 65535.")),
    }
}

fn clean_priority(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() || v.as_str() == Some("") {
        return Ok(None);
    }
    match v.as_str().filter(|p| PRIORITIES.contains(p)) {
        Some(p) => Ok(Some(json!(p))),
        None => Err(invalid(format!(
            "priority must be one of {}.",
            PRIORITIES.join(", ")
        ))),
    }
}

/// Merge `input` into the item's existing fields. Unknown names are refused; null removes one.
fn clean_fields(input: &Value, previous: Map<String, Value>) -> ApiResult<Map<String, Value>> {
    if input.is_null() {
        return Ok(Map::new());
    }
    let obj = input
        .as_object()
        .ok_or_else(|| invalid("fields must be an object."))?;
    let mut next = previous;
    for (key, value) in obj {
        let cleaned = match key.as_str() {
            "url" => clean_url(value, "url")?.map(Value::String),
            "path" => clean_text(value, "path", 500)?,
            "host" => clean_text(value, "host", 200)?,
            "port" => clean_port(value)?,
            "branch" => clean_text(value, "branch", 200)?,
            "due" => clean_date(value)?,
            "priority" => clean_priority(value)?,
            "owner" => clean_text(value, "owner", 60)?,
            "tags" => clean_tags(value)?,
            _ => {
                return Err(invalid(format!(
                    "Unknown field \"{key}\". Use {}, or put it in body.",
                    FIELD_NAMES.join(", ")
                )))
            }
        };
        match cleaned {
            Some(v) => {
                next.insert(key.clone(), v);
            }
            None => {
                next.remove(key);
            }
        }
    }
    Ok(next)
}

fn env_id(v: &Value) -> ApiResult<Option<String>> {
    let Some(env) = oneline(v, "environmentId", 128)? else {
        return Ok(None);
    };
    let ok = env.len() <= 128
        && env
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok {
        return Err(invalid("environmentId looks wrong."));
    }
    Ok(Some(env))
}

fn valid_slug(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// One link, checked by its type. Any link may carry a label.
fn clean_link(v: &Value) -> ApiResult<Value> {
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
fn link_key(link: &Value) -> String {
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
fn clean_links(list: &Value) -> ApiResult<Vec<Value>> {
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

fn current_links(item: &Map<String, Value>) -> Vec<Value> {
    item.get("links")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn same_value(a: Option<&Value>, b: Option<&Value>) -> bool {
    a.unwrap_or(&Value::Null) == b.unwrap_or(&Value::Null)
}

fn valid_ref_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// An item id, or `ref:<name>` for an item created earlier in the same batch. Null means "none".
fn resolve_ref(
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

fn assert_no_cycle(items: &Map<String, Value>, id: &str, parent: &str) -> ApiResult<()> {
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

fn items_mut(state: &mut Value) -> ApiResult<&mut Map<String, Value>> {
    state["items"]
        .as_object_mut()
        .ok_or_else(|| ApiError::new(500, "The Projects board is damaged."))
}

fn new_id(taken: &HashSet<String>) -> String {
    loop {
        let id: String = (0..8)
            .map(|_| ID_ALPHABET[rand::random::<u8>() as usize % ID_ALPHABET.len()] as char)
            .collect();
        if !taken.contains(&id) {
            return id;
        }
    }
}

/// Creates or changes items inside `next`, the board as it would be after the save.
/// Returns the saved items and their activity entries, or None when nothing changed.
fn apply_save(
    next: &mut Value,
    inputs: &[Value],
    who: &Value,
    stamp: &str,
) -> ApiResult<Option<(Vec<Value>, Vec<Value>)>> {
    if inputs.is_empty() {
        return Err(invalid("Give at least one item."));
    }
    if inputs.len() > MAX_BATCH {
        return Err(invalid(format!(
            "Save at most {MAX_BATCH} items at a time."
        )));
    }
    let before: Map<String, Value> = items_mut(next)?.clone();
    let mut seq = next["seq"].as_u64().unwrap_or(0);
    let items = items_mut(next)?;
    let mut refs: HashMap<String, String> = HashMap::new();
    let mut taken: HashSet<String> = items.keys().cloned().collect();
    let mut plan: Vec<(&Map<String, Value>, String, bool)> = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let obj = input
            .as_object()
            .ok_or_else(|| invalid(format!("Item {} must be an object.", index + 1)))?;
        if let Some(raw) = obj.get("id") {
            let id = raw
                .as_str()
                .filter(|id| before.contains_key(*id))
                .ok_or_else(|| {
                    invalid(format!(
                        "There's no item {}. Leave id out to create one.",
                        quoted(raw)
                    ))
                })?;
            plan.push((obj, id.to_string(), false));
        } else {
            let id = new_id(&taken);
            taken.insert(id.clone());
            if let Some(raw) = obj.get("ref") {
                let name = raw.as_str().filter(|r| valid_ref_name(r)).ok_or_else(|| {
                    invalid("ref must be a short name of letters, digits, - or _.")
                })?;
                if refs.contains_key(name) {
                    return Err(invalid(format!("ref \"{name}\" is used twice.")));
                }
                refs.insert(name.to_string(), id.clone());
            }
            plan.push((obj, id, true));
        }
    }
    let creating = plan.iter().filter(|p| p.2).count();
    if before.len() + creating > MAX_ITEMS {
        return Err(invalid(format!(
            "The board holds at most {MAX_ITEMS} items. Archive or delete some first."
        )));
    }
    // Every new item gets its id first, so items later in the batch can point at it.
    for (obj, id, created) in &plan {
        if *created {
            let kind = obj
                .get("kind")
                .and_then(Value::as_str)
                .filter(|k| KINDS.contains(k))
                .unwrap_or("task");
            items.insert(
                id.clone(),
                json!({
                    "id": id, "kind": kind, "title": "(untitled)", "fields": {}, "links": [],
                    "pinned": false, "archived": false, "createdAt": stamp, "createdBy": who,
                    "updatedAt": stamp, "updatedBy": who,
                }),
            );
        }
    }
    let mut saved = Vec::new();
    let mut entries = Vec::new();
    for (obj, id, created) in plan {
        let prev = before.get(&id);
        let mut m = items
            .get(&id)
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| ApiError::new(500, "The Projects board is damaged."))?;

        if created
            && !obj
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|k| KINDS.contains(&k))
        {
            return Err(kind_error());
        }
        if !created {
            if let Some(raw) = obj.get("kind") {
                if raw.as_str() != m.get("kind").and_then(Value::as_str) {
                    let k = raw
                        .as_str()
                        .filter(|k| KINDS.contains(k))
                        .ok_or_else(kind_error)?;
                    m.insert("kind".into(), json!(k));
                }
            }
        }
        let kind = m["kind"].as_str().unwrap_or_default().to_string();

        if created || obj.contains_key("title") {
            let title = oneline(obj.get("title").unwrap_or(&Value::Null), "title", 200)?
                .ok_or_else(|| invalid("Every item needs a title."))?;
            m.insert("title".into(), json!(title));
        }

        let list = statuses(&kind);
        if let Some(raw) = obj.get("status") {
            if raw.is_null() || raw.as_str() == Some("") {
                m.remove("status");
            } else {
                let s = raw.as_str().filter(|s| list.contains(s)).ok_or_else(|| {
                    invalid(if list.is_empty() {
                        format!("A {kind} has no status.")
                    } else {
                        format!("A {kind}'s status is one of {}.", list.join(", "))
                    })
                })?;
                m.insert("status".into(), json!(s));
            }
        }
        if m.get("status")
            .and_then(Value::as_str)
            .is_some_and(|s| !list.contains(&s))
        {
            m.remove("status");
        }
        if !m.contains_key("status") && !list.is_empty() {
            let idx = if kind == "machine" || kind == "service" {
                list.len() - 1
            } else {
                0
            };
            m.insert("status".into(), json!(list[idx]));
        }

        if let Some(raw) = obj.get("area") {
            match oneline(raw, "area", 40)? {
                Some(a) => {
                    m.insert("area".into(), json!(a.to_lowercase()));
                }
                None => {
                    m.remove("area");
                }
            }
        }
        if let Some(raw) = obj.get("body") {
            match text(raw, "body", 20000)? {
                Some(b) => {
                    m.insert("body".into(), json!(b));
                }
                None => {
                    m.remove("body");
                }
            }
        }

        for key in ["parentId", "machineId"] {
            let Some(raw) = obj.get(key) else { continue };
            let target = resolve_ref(raw, &refs, items, key)?;
            if target.as_deref() == Some(id.as_str()) {
                let what = if key == "parentId" {
                    "parent"
                } else {
                    "machine"
                };
                return Err(invalid(format!("An item can't be its own {what}.")));
            }
            match &target {
                Some(t) => {
                    if key == "machineId"
                        && items.get(t).and_then(|i| i["kind"].as_str()) != Some("machine")
                    {
                        return Err(invalid("machineId must point at a machine."));
                    }
                    m.insert(key.into(), json!(t));
                }
                None => {
                    m.remove(key);
                }
            }
        }
        if let Some(parent) = m.get("parentId").and_then(Value::as_str).map(String::from) {
            assert_no_cycle(items, &id, &parent)?;
        }

        if let Some(raw) = obj.get("fields") {
            let previous = m
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            m.insert("fields".into(), Value::Object(clean_fields(raw, previous)?));
        }
        if let Some(raw) = obj.get("links") {
            let links = if raw.is_null() {
                Vec::new()
            } else {
                clean_links(raw)?
            };
            m.insert("links".into(), json!(links));
        }
        if let Some(raw) = obj.get("addLinks") {
            let mut list = current_links(&m);
            list.extend(as_list(raw));
            m.insert("links".into(), json!(clean_links(&Value::Array(list))?));
        }
        if let Some(raw) = obj.get("removeLinks") {
            let drop: HashSet<String> = clean_links(&Value::Array(as_list(raw)))?
                .iter()
                .map(link_key)
                .collect();
            let kept: Vec<Value> = current_links(&m)
                .into_iter()
                .filter(|l| !drop.contains(&link_key(l)))
                .collect();
            m.insert("links".into(), json!(kept));
        }
        let mut links = current_links(&m);
        for link in links.iter_mut() {
            if link["type"] == "item" {
                let raw = link["id"].clone();
                let resolved = resolve_ref(&raw, &refs, items, "link id")?.unwrap_or_default();
                link["id"] = json!(resolved);
            }
        }
        m.insert("links".into(), json!(links));

        if let Some(raw) = obj.get("pinned") {
            m.insert("pinned".into(), json!(truthy(raw)));
        }
        if let Some(raw) = obj.get("order") {
            if raw.is_null() {
                m.remove("order");
            } else if raw.is_number() {
                m.insert("order".into(), raw.clone());
            } else {
                return Err(invalid("order must be a number."));
            }
        }
        if let Some(raw) = obj.get("archived") {
            m.insert("archived".into(), json!(truthy(raw)));
        }

        let keys: Vec<&str> = TRACKED
            .iter()
            .copied()
            .filter(|k| created || !same_value(prev.and_then(|p| p.get(*k)), m.get(*k)))
            .collect();
        if !created && keys.is_empty() {
            continue;
        }

        let status = m
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let was_done = prev.and_then(|p| p["status"].as_str()) == Some("done");
        if kind == "task" && status == "done" && (created || !was_done) {
            m.insert("doneAt".into(), json!(stamp));
        }
        if status != "done" {
            m.remove("doneAt");
        }
        m.insert("updatedAt".into(), json!(stamp));
        m.insert("updatedBy".into(), who.clone());

        let action = if created {
            "created"
        } else if keys.contains(&"archived") {
            if m["archived"] == true {
                "archived"
            } else {
                "restored"
            }
        } else if kind == "task" && keys.contains(&"status") && status == "done" {
            "completed"
        } else {
            "updated"
        };
        seq += 1;
        let changed: Vec<&str> = if created { Vec::new() } else { keys };
        entries.push(json!({
            "seq": seq, "at": stamp, "actor": who, "itemId": id, "kind": kind,
            "title": m["title"], "action": action, "changed": changed,
        }));
        let value = Value::Object(m);
        items.insert(id.clone(), value.clone());
        saved.push(value);
    }
    next["seq"] = json!(seq);
    if saved.is_empty() {
        return Ok(None);
    }
    Ok(Some((saved, entries)))
}

fn parent_of(items: &Map<String, Value>, id: &str) -> Option<String> {
    items
        .get(id)
        .and_then(|i| i["parentId"].as_str())
        .map(String::from)
}

/// Deletes items inside `next`. Children move up to the nearest surviving ancestor.
/// Returns (removed ids, items that changed, activity entries).
fn apply_remove(
    next: &mut Value,
    ids: &[String],
    who: &Value,
    stamp: &str,
) -> ApiResult<(Vec<String>, Vec<Value>, Vec<Value>)> {
    if ids.is_empty() {
        return Err(invalid("Give at least one id."));
    }
    let mut seq = next["seq"].as_u64().unwrap_or(0);
    let items = items_mut(next)?;
    let mut gone: Vec<String> = Vec::new();
    for id in ids {
        if items.contains_key(id) && !gone.contains(id) {
            gone.push(id.clone());
        }
    }
    if gone.is_empty() {
        return Err(invalid("None of those items are on the board."));
    }
    let gone_set: HashSet<String> = gone.iter().cloned().collect();
    let mut moved = Vec::new();
    let keys: Vec<String> = items.keys().cloned().collect();
    for key in keys {
        if gone_set.contains(&key) {
            continue;
        }
        let mut item = items[&key].clone();
        let mut changed = false;
        if let Some(parent) = item["parentId"].as_str().map(String::from) {
            if gone_set.contains(&parent) {
                let mut up = parent_of(items, &parent);
                loop {
                    match up.clone() {
                        Some(p) if gone_set.contains(&p) => up = parent_of(items, &p),
                        _ => break,
                    }
                }
                match up {
                    Some(p) => item["parentId"] = json!(p),
                    None => {
                        item.as_object_mut().map(|m| m.remove("parentId"));
                    }
                }
                changed = true;
            }
        }
        if item["machineId"]
            .as_str()
            .is_some_and(|m| gone_set.contains(m))
        {
            item.as_object_mut().map(|m| m.remove("machineId"));
            changed = true;
        }
        if let Some(links) = item["links"].as_array() {
            let kept: Vec<Value> = links
                .iter()
                .filter(|l| {
                    !(l["type"] == "item"
                        && l["id"].as_str().is_some_and(|id| gone_set.contains(id)))
                })
                .cloned()
                .collect();
            if kept.len() != links.len() {
                item["links"] = json!(kept);
                changed = true;
            }
        }
        if changed {
            item["updatedAt"] = json!(stamp);
            moved.push(item.clone());
            items.insert(key, item);
        }
    }
    let mut entries = Vec::new();
    for id in &gone {
        seq += 1;
        let item = &items[id];
        entries.push(json!({
            "seq": seq, "at": stamp, "actor": who, "itemId": id, "kind": item["kind"],
            "title": item["title"], "action": "deleted", "changed": [],
        }));
    }
    for id in &gone {
        items.remove(id);
    }
    next["seq"] = json!(seq);
    Ok((gone, moved, entries))
}

/// Bumps the version and appends the entries to the activity log, keeping the last 300.
fn commit_entries(next: &mut Value, entries: &[Value]) {
    next["version"] = json!(next["version"].as_u64().unwrap_or(0) + 1);
    let mut activity: Vec<Value> = next["activity"].as_array().cloned().unwrap_or_default();
    activity.extend(entries.iter().cloned());
    let excess = activity.len().saturating_sub(MAX_ACTIVITY);
    next["activity"] = Value::Array(activity.split_off(excess));
}

pub struct Projects {
    file: PathBuf,
    state: Mutex<Option<Value>>,
    pub events: broadcast::Sender<Value>,
}

impl Projects {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            state: Mutex::new(None),
            events: broadcast::channel(64).0,
        }
    }

    /// Reads the file the first time. A missing file is an empty board; an unreadable one is an error,
    /// so a later save can't overwrite it.
    async fn load<'a>(&self, g: &'a mut Option<Value>) -> ApiResult<&'a mut Value> {
        if g.is_none() {
            let raw = match tokio::fs::read_to_string(&self.file).await {
                Ok(text) => serde_json::from_str::<Value>(&text).map_err(|_| unreadable())?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
                Err(_) => return Err(unreadable()),
            };
            *g = Some(json!({
                "version": raw["version"].as_u64().unwrap_or(0),
                "seq": raw["seq"].as_u64().unwrap_or(0),
                "items": if raw["items"].is_object() { raw["items"].clone() } else { json!({}) },
                "activity": if raw["activity"].is_array() { raw["activity"].clone() } else { json!([]) },
            }));
        }
        g.as_mut().ok_or_else(unreadable)
    }

    async fn persist(&self, next: &Value) -> ApiResult<()> {
        write_atomic(&self.file, next.to_string().as_bytes(), Some(0o600)).await?;
        Ok(())
    }

    /// The whole board: version, every item and the last 60 activity entries.
    pub async fn try_board(&self) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await?;
        let items: Vec<Value> = s["items"]
            .as_object()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        let activity = s["activity"].as_array().cloned().unwrap_or_default();
        let tail = activity[activity.len().saturating_sub(BOARD_TAIL)..].to_vec();
        Ok(json!({ "version": s["version"], "items": items, "activity": tail }))
    }

    /// Every item, for the query views.
    pub async fn items(&self) -> ApiResult<Vec<Value>> {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await?;
        Ok(s["items"]
            .as_object()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    /// The whole activity log, oldest first.
    pub async fn activity(&self) -> ApiResult<Vec<Value>> {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await?;
        Ok(s["activity"].as_array().cloned().unwrap_or_default())
    }

    pub async fn version(&self) -> u64 {
        let mut g = self.state.lock().await;
        self.load(&mut g)
            .await
            .map(|s| s["version"].as_u64().unwrap_or(0))
            .unwrap_or(0)
    }

    /// No id creates; an id changes only what is sent; `null` clears a field.
    pub async fn save(&self, inputs: &[Value], who: Value) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let state = self.load(&mut g).await?;
        let mut next = state.clone();
        let stamp = now_iso();
        let Some((saved, entries)) = apply_save(&mut next, inputs, &who, &stamp)? else {
            return Ok(json!({ "version": state["version"], "items": [], "unchanged": true }));
        };
        commit_entries(&mut next, &entries);
        self.persist(&next).await?;
        *state = next;
        let _ = self.events.send(json!({
            "version": state["version"], "items": saved, "removed": [], "activity": entries,
        }));
        Ok(json!({ "version": state["version"], "items": saved }))
    }

    /// Deletes items for good; their children move up a level.
    pub async fn delete(&self, ids: &[String], who: Value) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let state = self.load(&mut g).await?;
        let mut next = state.clone();
        let (gone, moved, entries) = apply_remove(&mut next, ids, &who, &now_iso())?;
        commit_entries(&mut next, &entries);
        self.persist(&next).await?;
        *state = next;
        let _ = self.events.send(json!({
            "version": state["version"], "items": moved, "removed": gone, "activity": entries,
        }));
        Ok(json!({ "version": state["version"], "removed": gone }))
    }
}

// ---------- queries shared by the Bots' tools and the page ----------

fn by_order(a: &Value, b: &Value) -> Ordering {
    let order = match (a["order"].as_f64(), b["order"].as_f64()) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    order.then_with(|| {
        a["createdAt"]
            .as_str()
            .unwrap_or("")
            .cmp(b["createdAt"].as_str().unwrap_or(""))
    })
}

fn is_archived(item: &Value) -> bool {
    item["archived"] == true
}

fn is_open_task(item: &Value) -> bool {
    item["kind"] == "task" && item["status"] != "done" && !is_archived(item)
}

pub fn item_link(id: &str, app_url: &str) -> String {
    let encoded: String = id
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("{}/?item={encoded}", app_url.trim_end_matches('/'))
}

/// The compact view agents get: ids, titles and placement, without bodies.
pub fn brief(item: &Value, app_url: &str) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), item["id"].clone());
    out.insert("kind".into(), item["kind"].clone());
    out.insert("title".into(), item["title"].clone());
    for key in ["status", "area", "parentId", "machineId"] {
        if let Some(v) = item[key].as_str().filter(|s| !s.is_empty()) {
            out.insert(key.into(), json!(v));
        }
    }
    if item["fields"].as_object().is_some_and(|f| !f.is_empty()) {
        out.insert("fields".into(), item["fields"].clone());
    }
    if item["pinned"] == true {
        out.insert("pinned".into(), json!(true));
    }
    if is_archived(item) {
        out.insert("archived".into(), json!(true));
    }
    if let Some(n) = item["links"].as_array().map(Vec::len).filter(|n| *n > 0) {
        out.insert("links".into(), json!(n));
    }
    let id = item["id"].as_str().unwrap_or_default();
    out.insert("link".into(), json!(item_link(id, app_url)));
    Value::Object(out)
}

/// The board at a glance: projects with open to-dos, open to-dos, machines with what runs on them,
/// and the latest changes.
pub fn overview(items: &[Value], activity: &[Value], app_url: &str) -> Value {
    let live: Vec<&Value> = items.iter().filter(|i| !is_archived(i)).collect();
    let title_of = |id: &str| {
        live.iter()
            .find(|i| i["id"] == id)
            .and_then(|i| i["title"].as_str())
            .map(String::from)
    };
    let mut projects: Vec<&Value> = live
        .iter()
        .copied()
        .filter(|i| i["kind"] == "project")
        .collect();
    projects.sort_by(|a, b| by_order(a, b));
    let projects: Vec<Value> = projects
        .into_iter()
        .map(|p| {
            let open = live
                .iter()
                .filter(|i| is_open_task(i) && i["parentId"] == p["id"])
                .count();
            let mut v = brief(p, app_url);
            v["openTasks"] = json!(open);
            v
        })
        .collect();

    let priority_rank = |i: &Value| {
        let p = i["fields"]["priority"].as_str().unwrap_or("normal");
        PRIORITIES
            .iter()
            .position(|x| *x == p)
            .map_or(-1, |x| x as i64)
    };
    let mut tasks: Vec<&Value> = live.iter().copied().filter(|i| is_open_task(i)).collect();
    tasks.sort_by(|a, b| {
        priority_rank(a)
            .cmp(&priority_rank(b))
            .then_with(|| {
                a["fields"]["due"]
                    .as_str()
                    .unwrap_or("9999")
                    .cmp(b["fields"]["due"].as_str().unwrap_or("9999"))
            })
            .then_with(|| by_order(a, b))
    });
    let open_tasks: Vec<Value> = tasks
        .into_iter()
        .take(60)
        .map(|t| {
            let mut v = brief(t, app_url);
            if let Some(project) = t["parentId"].as_str().and_then(title_of) {
                v["project"] = json!(project);
            }
            v
        })
        .collect();

    let mut machines: Vec<&Value> = live
        .iter()
        .copied()
        .filter(|i| i["kind"] == "machine")
        .collect();
    machines.sort_by(|a, b| by_order(a, b));
    let machines: Vec<Value> = machines
        .into_iter()
        .map(|m| {
            let under = |kind: &str| -> Vec<Value> {
                live.iter()
                    .filter(|i| i["kind"] == kind && i["machineId"] == m["id"])
                    .map(|i| brief(i, app_url))
                    .collect()
            };
            let mut v = brief(m, app_url);
            v["services"] = json!(under("service"));
            v["repos"] = json!(under("repo"));
            v
        })
        .collect();

    let unplaced: Vec<Value> = live
        .iter()
        .filter(|i| i["kind"] == "service" && i["machineId"].is_null())
        .map(|i| brief(i, app_url))
        .collect();
    let counts: Map<String, Value> = KINDS
        .iter()
        .map(|k| {
            (
                k.to_string(),
                json!(live.iter().filter(|i| i["kind"] == *k).count()),
            )
        })
        .collect();
    let recent: Vec<Value> = activity
        .iter()
        .rev()
        .take(12)
        .map(|e| {
            json!({
                "at": e["at"], "who": actor_label(&e["actor"]), "action": e["action"],
                "id": e["itemId"], "title": e["title"],
            })
        })
        .collect();

    let mut out = json!({
        "page": format!("{}/?page=projects", app_url.trim_end_matches('/')),
        "counts": counts,
        "projects": projects,
        "openTasks": open_tasks,
        "machines": machines,
        "recent": recent,
    });
    if !unplaced.is_empty() {
        out["servicesWithoutMachine"] = json!(unplaced);
    }
    out
}

/// Search with filters. Words must all appear in a title, note, area, tag, address or path.
pub fn find(items: &[Value], query: &Value, app_url: &str) -> Value {
    let text_of = |k: &str| {
        query
            .get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let search = text_of("search").unwrap_or("").trim().to_lowercase();
    let words: Vec<&str> = search.split_whitespace().collect();
    let kind = text_of("kind");
    let status = text_of("status");
    let area = text_of("area").map(str::to_lowercase);
    let parent = text_of("parentId");
    let machine = text_of("machineId");
    let thread = text_of("threadId");
    let include_archived = truthy(query.get("includeArchived").unwrap_or(&Value::Null));
    let open_only = truthy(query.get("open").unwrap_or(&Value::Null));

    let mut matches: Vec<&Value> = items
        .iter()
        .filter(|item| {
            if !include_archived && is_archived(item) {
                return false;
            }
            if kind.is_some_and(|k| item["kind"] != k) {
                return false;
            }
            if status.is_some_and(|s| item["status"] != s) {
                return false;
            }
            if open_only && item["kind"] == "task" && item["status"] == "done" {
                return false;
            }
            if area
                .as_deref()
                .is_some_and(|a| item["area"].as_str() != Some(a))
            {
                return false;
            }
            if parent.is_some_and(|p| item["parentId"] != p) {
                return false;
            }
            if machine.is_some_and(|m| item["machineId"] != m) {
                return false;
            }
            if thread.is_some_and(|t| {
                !item["links"].as_array().is_some_and(|ls| {
                    ls.iter()
                        .any(|l| l["type"] == "thread" && l["threadId"] == t)
                })
            }) {
                return false;
            }
            if !words.is_empty() {
                let mut hay: Vec<&str> = Vec::new();
                for key in ["title", "body", "area"] {
                    if let Some(s) = item[key].as_str() {
                        hay.push(s);
                    }
                }
                if let Some(tags) = item["fields"]["tags"].as_array() {
                    hay.extend(tags.iter().filter_map(Value::as_str));
                }
                for key in ["url", "path", "host"] {
                    if let Some(s) = item["fields"][key].as_str() {
                        hay.push(s);
                    }
                }
                let hay = hay.join(" ").to_lowercase();
                if !words.iter().all(|w| hay.contains(w)) {
                    return false;
                }
            }
            true
        })
        .collect();
    matches.sort_by(|a, b| by_order(a, b));
    let limit = match query.get("limit") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(50.0),
        Some(Value::String(s)) => s.parse::<f64>().unwrap_or(50.0),
        _ => 50.0,
    };
    let limit = limit.clamp(1.0, 200.0) as usize;
    let total = matches.len();
    let items: Vec<Value> = matches
        .into_iter()
        .take(limit)
        .map(|i| brief(i, app_url))
        .collect();
    json!({ "total": total, "items": items })
}

/// One item in full: its children, what is on it (for a machine), its parent and what links to it.
pub fn detail(items: &[Value], id: &str, app_url: &str) -> Option<Value> {
    let item = items.iter().find(|i| i["id"] == id)?;
    let mut children: Vec<&Value> = items
        .iter()
        .filter(|i| i["parentId"] == id && !is_archived(i))
        .collect();
    children.sort_by(|a, b| by_order(a, b));
    let mut out = item.clone();
    out["link"] = json!(item_link(id, app_url));
    if let Some(parent) = item["parentId"]
        .as_str()
        .and_then(|p| items.iter().find(|i| i["id"] == p))
    {
        out["parent"] =
            json!({ "id": parent["id"], "title": parent["title"], "kind": parent["kind"] });
    }
    out["children"] = json!(children
        .iter()
        .map(|c| brief(c, app_url))
        .collect::<Vec<_>>());
    if item["kind"] == "machine" {
        let on: Vec<Value> = items
            .iter()
            .filter(|i| i["machineId"] == id && !is_archived(i))
            .map(|i| brief(i, app_url))
            .collect();
        out["onThisMachine"] = json!(on);
    }
    let linked: Vec<Value> = items
        .iter()
        .filter(|c| {
            c["links"]
                .as_array()
                .is_some_and(|ls| ls.iter().any(|l| l["type"] == "item" && l["id"] == id))
        })
        .map(|c| json!({ "id": c["id"], "title": c["title"], "kind": c["kind"] }))
        .collect();
    out["linkedFrom"] = json!(linked);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot() -> Value {
        json!({ "type": "bot", "name": "Ops", "slug": "ops" })
    }

    fn board() -> (tempfile::TempDir, PathBuf, Projects) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("projects.json");
        let p = Projects::new(file.clone());
        (dir, file, p)
    }

    fn msg(e: ApiError) -> String {
        e.message
    }

    async fn refuses(p: &Projects, items: Vec<Value>, needle: &str) {
        let err = p.save(&items, bot()).await.unwrap_err();
        assert_eq!(err.status, 400, "{}", err.message);
        assert!(
            err.message.contains(needle),
            "{:?} lacks {needle:?}",
            err.message
        );
    }

    #[tokio::test]
    async fn a_project_and_its_todos_save_in_one_batch_with_refs_and_defaults() {
        let (_d, file, p) = board();
        let mut rx = p.events.subscribe();
        let saved = p
            .save(
                &[
                    json!({ "ref": "nas", "kind": "machine", "title": "NAS" }),
                    json!({ "ref": "tb", "kind": "project", "title": "hbots", "area": "Personal" }),
                    json!({ "kind": "task", "title": "Ship the projects page", "parentId": "ref:tb", "fields": { "priority": "high", "due": "2026-11-05" } }),
                    json!({ "kind": "service", "title": "bot-gateway", "machineId": "ref:nas", "parentId": "ref:tb", "fields": { "port": "3780" },
                            "addLinks": [{ "type": "thread", "threadId": "t-1" }, { "type": "thread", "threadId": "t-1" }] }),
                ],
                bot(),
            )
            .await
            .unwrap();
        let items = saved["items"].as_array().unwrap();
        let (nas, tb, task, svc) = (&items[0], &items[1], &items[2], &items[3]);
        assert_eq!(nas["status"], "unknown");
        assert_eq!(tb["status"], "active");
        assert_eq!(tb["area"], "personal");
        assert_eq!(task["status"], "todo");
        assert_eq!(task["parentId"], tb["id"]);
        assert_eq!(svc["machineId"], nas["id"]);
        assert_eq!(svc["fields"]["port"], 3780);
        assert_eq!(
            svc["links"].as_array().unwrap().len(),
            1,
            "duplicate links collapse"
        );
        assert_eq!(task["createdBy"], bot());
        assert_eq!(nas["id"].as_str().unwrap().len(), 8);
        let event = rx.recv().await.unwrap();
        assert_eq!(event["version"], 1);
        assert_eq!(event["activity"].as_array().unwrap().len(), 4);
        assert_eq!(event["activity"][0]["actor"]["name"], "Ops");
        let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(on_disk["items"].as_object().unwrap().len(), 4);
        assert_eq!(on_disk["version"], 1);
    }

    #[tokio::test]
    async fn patches_change_only_what_they_name_null_clears_and_done_todos_record_when() {
        let (_d, _f, p) = board();
        let task = p
            .save(
                &[json!({ "kind": "task", "title": "Renew domain", "body": "Before the 9th", "fields": { "tags": ["Admin", "admin", "web"] } })],
                json!({ "type": "user" }),
            )
            .await
            .unwrap()["items"][0]
            .clone();
        assert_eq!(task["fields"]["tags"], json!(["admin", "web"]));
        let id = task["id"].as_str().unwrap().to_string();
        let after = p
            .save(&[json!({ "id": id, "status": "done", "fields": { "tags": null, "url": "https://example.com/renew" } })], bot())
            .await
            .unwrap()["items"][0]
            .clone();
        assert_eq!(after["title"], "Renew domain");
        assert_eq!(after["body"], "Before the 9th");
        assert!(after["fields"].get("tags").is_none());
        assert_eq!(after["fields"]["url"], "https://example.com/renew");
        assert!(after["doneAt"].is_string());
        assert_eq!(after["updatedBy"], bot());
        let activity = p.activity().await.unwrap();
        assert_eq!(activity.last().unwrap()["action"], "completed");
        let reopened = p
            .save(
                &[json!({ "id": id, "status": "todo" })],
                json!({ "type": "user" }),
            )
            .await
            .unwrap()["items"][0]
            .clone();
        assert!(reopened.get("doneAt").is_none());
        let unchanged = p
            .save(
                &[json!({ "id": id, "status": "todo" })],
                json!({ "type": "user" }),
            )
            .await
            .unwrap();
        assert_eq!(unchanged["unchanged"], true);
    }

    #[tokio::test]
    async fn bad_input_is_refused_with_a_plain_reason_and_the_board_is_unchanged() {
        let (_d, _f, p) = board();
        let project = p
            .save(
                &[json!({ "kind": "project", "title": "Home" })],
                json!({ "type": "user" }),
            )
            .await
            .unwrap()["items"][0]
            .clone();
        let pid = project["id"].as_str().unwrap().to_string();
        refuses(&p, vec![json!({ "kind": "task" })], "needs a title").await;
        refuses(
            &p,
            vec![json!({ "kind": "spaceship", "title": "x" })],
            "kind must be",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "kind": "task", "title": "x", "status": "active" })],
            "status is one of todo",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "kind": "task", "title": "x", "fields": { "colour": "red" } })],
            "Unknown field \"colour\"",
        )
        .await;
        refuses(
            &p,
            vec![
                json!({ "kind": "task", "title": "x", "fields": { "url": "javascript:alert(1)" } }),
            ],
            "http",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "kind": "task", "title": "x", "fields": { "due": "tomorrow" } })],
            "date like",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "kind": "task", "title": "x", "parentId": "ref:nope" })],
            "doesn't name an item",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "kind": "service", "title": "x", "machineId": pid })],
            "must point at a machine",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "id": "missing1", "title": "x" })],
            "no item \"missing1\"",
        )
        .await;
        refuses(
            &p,
            vec![
                json!({ "kind": "task", "title": "ok" }),
                json!({ "kind": "task" }),
            ],
            "needs a title",
        )
        .await;
        refuses(
            &p,
            vec![json!({ "id": pid, "parentId": pid })],
            "its own parent",
        )
        .await;
        let child = p
            .save(
                &[json!({ "kind": "project", "title": "Garden", "parentId": pid })],
                json!({ "type": "user" }),
            )
            .await
            .unwrap()["items"][0]
            .clone();
        let cid = child["id"].as_str().unwrap().to_string();
        refuses(
            &p,
            vec![json!({ "id": pid, "parentId": cid })],
            "inside itself",
        )
        .await;
        assert_eq!(
            p.items().await.unwrap().len(),
            2,
            "the half-valid batch saved nothing"
        );
        assert!(p
            .items()
            .await
            .unwrap()
            .iter()
            .find(|i| i["id"] == pid)
            .unwrap()
            .get("parentId")
            .is_none());
    }

    #[tokio::test]
    async fn deleting_moves_children_up_and_drops_links_to_the_deleted_item() {
        let (_d, _f, p) = board();
        let saved = p
            .save(
                &[
                    json!({ "ref": "a", "kind": "project", "title": "Work" }),
                    json!({ "ref": "b", "kind": "project", "title": "Launch", "parentId": "ref:a" }),
                    json!({ "ref": "m", "kind": "machine", "title": "Air" }),
                    json!({ "kind": "task", "title": "Write post", "parentId": "ref:b", "addLinks": [{ "type": "item", "id": "ref:m" }] }),
                    json!({ "kind": "repo", "title": "hbots", "machineId": "ref:m" }),
                ],
                json!({ "type": "user" }),
            )
            .await
            .unwrap();
        let items = saved["items"].as_array().unwrap().clone();
        let mut rx = p.events.subscribe();
        let launch = items[1]["id"].as_str().unwrap().to_string();
        let air = items[2]["id"].as_str().unwrap().to_string();
        p.delete(&[launch.clone(), air.clone()], bot())
            .await
            .unwrap();
        let left = p.items().await.unwrap();
        let task = left.iter().find(|i| i["title"] == "Write post").unwrap();
        assert_eq!(task["parentId"], items[0]["id"]);
        assert_eq!(task["links"], json!([]));
        assert!(left
            .iter()
            .find(|i| i["title"] == "hbots")
            .unwrap()
            .get("machineId")
            .is_none());
        let _ = rx.try_recv();
    }

    #[tokio::test]
    async fn saves_from_several_tasks_all_land() {
        let (_d, _f, p) = board();
        let p = std::sync::Arc::new(p);
        let handles: Vec<_> = (0..20)
            .map(|i| {
                let p = p.clone();
                tokio::spawn(async move {
                    p.save(
                        &[json!({ "kind": "task", "title": format!("Task {i}") })],
                        json!({ "type": "user" }),
                    )
                    .await
                    .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(p.items().await.unwrap().len(), 20);
        assert_eq!(p.version().await, 20);
    }

    #[tokio::test]
    async fn a_fresh_store_reads_what_an_earlier_one_wrote() {
        let (_d, file, p) = board();
        p.save(
            &[json!({ "kind": "note", "title": "Wifi is on the fridge" })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap();
        let again = Projects::new(file);
        assert_eq!(
            again.items().await.unwrap()[0]["title"],
            "Wifi is on the fridge"
        );
    }

    #[tokio::test]
    async fn the_board_is_refused_rather_than_overwritten_when_its_file_is_corrupt() {
        let (_d, file, p) = board();
        std::fs::write(&file, "{ not json").unwrap();
        let err = p
            .save(
                &[json!({ "kind": "note", "title": "x" })],
                json!({ "type": "user" }),
            )
            .await
            .unwrap_err();
        assert_eq!(msg(err), "The Projects board file can't be read.");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
    }

    #[tokio::test]
    async fn overview_find_and_detail_give_agents_ids_links_and_placement() {
        let (_d, _f, p) = board();
        let saved = p
            .save(
                &[
                    json!({ "ref": "p", "kind": "project", "title": "hbots", "area": "personal" }),
                    json!({ "ref": "m", "kind": "machine", "title": "NAS", "status": "online" }),
                    json!({ "kind": "task", "title": "Low thing", "parentId": "ref:p", "fields": { "priority": "low" } }),
                    json!({ "kind": "task", "title": "Urgent thing", "parentId": "ref:p", "fields": { "priority": "high" } }),
                    json!({ "kind": "task", "title": "Finished", "parentId": "ref:p", "status": "done" }),
                    json!({ "kind": "service", "title": "bot-gateway", "machineId": "ref:m", "addLinks": [{ "type": "thread", "threadId": "thread-9" }] }),
                    json!({ "kind": "repo", "title": "hbots-source", "machineId": "ref:m", "fields": { "path": "/mnt/apps/t3/projects/hbots-source" } }),
                ],
                bot(),
            )
            .await
            .unwrap();
        let items = saved["items"].as_array().unwrap().clone();
        let all = p.items().await.unwrap();
        let activity = p.activity().await.unwrap();
        let app = "https://bot.example";
        let view = overview(&all, &activity, app);
        assert_eq!(view["projects"][0]["openTasks"], 2);
        let titles: Vec<&str> = view["openTasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["Urgent thing", "Low thing"]);
        assert_eq!(view["openTasks"][0]["project"], "hbots");
        assert_eq!(view["machines"][0]["services"][0]["title"], "bot-gateway");
        assert_eq!(view["machines"][0]["repos"][0]["title"], "hbots-source");
        assert_eq!(view["page"], "https://bot.example/?page=projects");
        assert_eq!(view["recent"][0]["who"], "Ops");
        assert!(view["openTasks"][0]["link"]
            .as_str()
            .unwrap()
            .starts_with("https://bot.example/?item="));
        let found = |q: Value| -> Vec<String> {
            find(&all, &q, app)["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["title"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(found(json!({ "search": "source mnt" })), ["hbots-source"]);
        let mut open = found(json!({ "kind": "task", "open": true }));
        open.sort();
        assert_eq!(open, ["Low thing", "Urgent thing"]);
        assert_eq!(found(json!({ "threadId": "thread-9" })), ["bot-gateway"]);
        let machine = detail(&all, items[1]["id"].as_str().unwrap(), app).unwrap();
        let mut on: Vec<String> = machine["onThisMachine"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_string())
            .collect();
        on.sort();
        assert_eq!(on, ["bot-gateway", "hbots-source"]);
        let project = detail(&all, items[0]["id"].as_str().unwrap(), app).unwrap();
        assert_eq!(project["children"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn cleaners_refuse_bad_values_and_fold_good_ones() {
        assert_eq!(
            clean_tags(&json!(["Web", " web ", "", "API"])).unwrap(),
            Some(json!(["web", "api"]))
        );
        assert!(clean_tags(&json!((0..13).map(|i| format!("t{i}")).collect::<Vec<_>>())).is_err());
        assert_eq!(
            clean_url(&json!(" https://example.com/a "), "url")
                .unwrap()
                .as_deref(),
            Some("https://example.com/a")
        );
        assert!(clean_url(&json!("example.com"), "url").is_err());
        assert!(clean_url(&json!("ftp://example.com"), "url").is_err());
        assert!(valid_date("2028-02-29"));
        assert!(!valid_date("2027-02-29"));
        assert!(!valid_date("2026-13-01"));
        assert!(!valid_date("２026-01-01"));
        assert_eq!(clean_port(&json!("3780")).unwrap(), Some(json!(3780)));
        assert!(clean_port(&json!(70000)).is_err());
        assert!(clean_port(&json!(80.5)).is_err());
        assert_eq!(clean_priority(&json!("")).unwrap(), None);
        assert!(clean_priority(&json!("urgent")).is_err());
    }

    #[test]
    fn text_and_one_line_fold_crlf_and_collapse_space() {
        assert_eq!(
            oneline(&json!("  a \r\n  b\tc  "), "t", 50)
                .unwrap()
                .as_deref(),
            Some("a b c")
        );
        assert_eq!(
            text(&json!("a\r\nb"), "t", 50).unwrap().as_deref(),
            Some("a\nb")
        );
        assert!(text(&json!("abc"), "t", 2).is_err());
        assert_eq!(text(&json!("   "), "t", 2).unwrap(), None);
        assert!(text(&json!(3), "t", 2).is_err());
    }

    #[test]
    fn link_keys_ignore_labels_and_keep_environments_apart() {
        let a = clean_link(&json!({ "type": "thread", "threadId": "t", "label": "one" })).unwrap();
        let b = clean_link(&json!({ "type": "thread", "threadId": "t", "label": "two" })).unwrap();
        let c = clean_link(&json!({ "type": "thread", "threadId": "t", "environmentId": "e1" }))
            .unwrap();
        assert_eq!(link_key(&a), link_key(&b));
        assert_ne!(link_key(&a), link_key(&c));
        assert!(clean_link(&json!({ "type": "bot", "slug": "Bad Slug" })).is_err());
        assert!(clean_link(&json!({ "type": "nope" })).is_err());
    }

    /// A projects.json from `tests/fixtures/node-projects.json`.
    #[tokio::test]
    async fn a_board_written_by_hbots_loads_and_resaves_without_losing_fields() {
        let fixture = include_str!("../tests/fixtures/node-projects.json");
        let node: Value = serde_json::from_str(fixture).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("projects.json");
        std::fs::write(&file, fixture).unwrap();
        let p = Projects::new(file.clone());
        let board = p.try_board().await.unwrap();
        assert_eq!(board["version"], node["version"]);
        assert_eq!(
            board["items"].as_array().unwrap().len(),
            node["items"].as_object().unwrap().len()
        );
        let task_id = node["items"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, i)| i["title"] == "Ship the projects page")
            .map(|(id, _)| id.clone())
            .unwrap();
        p.save(
            &[json!({ "id": task_id, "title": "Changed from Rust" })],
            bot(),
        )
        .await
        .unwrap();
        let resaved: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(resaved["version"], node["version"].as_u64().unwrap() + 1);
        assert_eq!(
            resaved["seq"].as_u64(),
            Some(node["seq"].as_u64().unwrap() + 1)
        );
        assert_eq!(resaved["items"][&task_id]["title"], "Changed from Rust");
        for (id, item) in node["items"].as_object().unwrap() {
            if *id == task_id {
                continue;
            }
            assert_eq!(&resaved["items"][id], item, "item {id} changed on re-save");
        }
        assert_eq!(
            resaved["activity"].as_array().unwrap().len(),
            node["activity"].as_array().unwrap().len() + 1
        );
    }
}
