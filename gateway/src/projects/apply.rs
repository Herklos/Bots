use super::*;

pub(crate) const MAX_ACTIVITY: usize = 300;

pub(crate) const BOARD_TAIL: usize = 60;

/// Creates or changes items inside `next`, the board as it would be after the save.
/// Returns the saved items and their activity entries, or None when nothing changed.
pub(crate) fn apply_save(
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

pub(crate) fn parent_of(items: &Map<String, Value>, id: &str) -> Option<String> {
    items
        .get(id)
        .and_then(|i| i["parentId"].as_str())
        .map(String::from)
}

/// Deletes items inside `next`. Children move up to the nearest surviving ancestor.
/// Returns (removed ids, items that changed, activity entries).
pub(crate) fn apply_remove(
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
pub(crate) fn commit_entries(next: &mut Value, entries: &[Value]) {
    next["version"] = json!(next["version"].as_u64().unwrap_or(0) + 1);
    let mut activity: Vec<Value> = next["activity"].as_array().cloned().unwrap_or_default();
    activity.extend(entries.iter().cloned());
    let excess = activity.len().saturating_sub(MAX_ACTIVITY);
    next["activity"] = Value::Array(activity.split_off(excess));
}
