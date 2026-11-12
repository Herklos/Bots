use super::*;

// ---------- queries shared by the Bots' tools and the page ----------

pub(crate) fn by_order(a: &Value, b: &Value) -> Ordering {
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

pub(crate) fn is_archived(item: &Value) -> bool {
    item["archived"] == true
}

pub(crate) fn is_open_task(item: &Value) -> bool {
    item["kind"] == "task" && item["status"] != "done" && !is_archived(item)
}

pub(crate) fn item_link(id: &str, app_url: &str) -> String {
    let encoded: String = id
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("{}/?item={encoded}", app_url.trim_end_matches('/'))
}

/// The compact view agents get: ids, titles and placement, without bodies.
pub(crate) fn brief(item: &Value, app_url: &str) -> Value {
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
pub(crate) fn overview(items: &[Value], activity: &[Value], app_url: &str) -> Value {
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
            .map_or(-1, |x| i64::try_from(x).unwrap_or(i64::MAX))
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
pub(crate) fn find(items: &[Value], query: &Value, app_url: &str) -> Value {
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
    let limit = limit.clamp(1.0, 200.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "search limit is clamped to 1..=200 before the fractional part is dropped"
    )]
    let limit = limit as usize;
    let total = matches.len();
    let items: Vec<Value> = matches
        .into_iter()
        .take(limit)
        .map(|i| brief(i, app_url))
        .collect();
    json!({ "total": total, "items": items })
}

/// One item in full: its children, what is on it (for a machine), its parent and what links to it.
pub(crate) fn detail(items: &[Value], id: &str, app_url: &str) -> Option<Value> {
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
