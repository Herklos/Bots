use super::*;

// ---------- import (vault.mts importItems) ----------

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportCounts {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

impl ImportCounts {
    pub(crate) fn to_json(&self) -> Value {
        json!({ "added": self.added, "updated": self.updated, "skipped": self.skipped })
    }
}

pub(crate) enum Outcome {
    Added,
    Updated,
    Skipped,
}

/// Adds imported items. A login with the same domain and username as a saved one updates it
/// (password and hints), and an identical one is skipped. A contact identical to a saved one is
/// skipped. An item the Vault would refuse counts as skipped.
pub(crate) async fn import_items(vault: &Vault, list: &Value) -> ApiResult<ImportCounts> {
    let list = list
        .as_array()
        .ok_or_else(|| ApiError::bad_request("items must be a list"))?;
    let mut counts = ImportCounts::default();
    for raw in list.iter().take(MAX_ITEMS) {
        match import_one(vault, raw).await? {
            Outcome::Added => counts.added += 1,
            Outcome::Updated => counts.updated += 1,
            Outcome::Skipped => counts.skipped += 1,
        }
    }
    Ok(counts)
}

pub(crate) async fn import_one(vault: &Vault, raw: &Value) -> ApiResult<Outcome> {
    let Some(obj) = raw.as_object() else {
        return Ok(Outcome::Skipped);
    };
    let is_contact = obj.get("type").and_then(Value::as_str) == Some("contact");
    let mut input = obj.clone();
    input.insert(
        "type".into(),
        json!(if is_contact { "contact" } else { "login" }),
    );
    if is_contact {
        if !list_valid(obj.get("emails")) || !list_valid(obj.get("phones")) {
            return Ok(Outcome::Skipped);
        }
        let key = contact_signature(&local_contact(obj));
        let duplicate = vault
            .list()
            .await?
            .iter()
            .any(|x| x["type"] == "contact" && contact_signature(x) == key);
        if duplicate {
            return Ok(Outcome::Skipped);
        }
        vault.create(&Value::Object(input)).await?;
        return Ok(Outcome::Added);
    }
    if !list_valid(obj.get("websites")) {
        return Ok(Outcome::Skipped);
    }
    let Some(item) = local_login(obj) else {
        return Ok(Outcome::Skipped);
    };
    let domain = site_domain(&item.website);
    let saved = vault.list().await?;
    let found = saved.iter().find(|x| {
        if x["type"] != "login" || text(x, "username") != item.username {
            return false;
        }
        let same_site = if domain.is_empty() {
            site_domain(&text(x, "website")).is_empty()
        } else {
            repeated_values(x, "website")
                .iter()
                .any(|site| site_domain(site) == domain)
        };
        same_site && (!domain.is_empty() || text(x, "title") == item.title)
    });
    let Some(existing) = found else {
        vault.create(&Value::Object(input)).await?;
        return Ok(Outcome::Added);
    };
    let id = text(existing, "id");
    let prev = vault.get(&id, true).await?;
    let unchanged = text(&prev, "password") == item.password
        && (item.hints.is_empty() || text(&prev, "hints") == item.hints)
        && (!obj.contains_key("websites") || repeated_values(&prev, "website") == item.websites);
    if unchanged {
        return Ok(Outcome::Skipped);
    }
    // Only what was sent changes: a blank password or hint keeps the saved one.
    let mut patch = Map::new();
    if !item.password.is_empty() {
        patch.insert("password".into(), json!(item.password));
    }
    if !item.hints.is_empty() {
        patch.insert("hints".into(), json!(item.hints));
    }
    if let Some(websites) = obj.get("websites") {
        patch.insert("websites".into(), websites.clone());
    }
    vault.update(&id, &Value::Object(patch)).await?;
    Ok(Outcome::Updated)
}

// ---------- routes ----------

/// POST /api/vault/import: `{items}` as JSON.
pub(crate) async fn import(
    State(s): State<Arc<AppState>>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let counts = import_items(&s.vault, &body["items"]).await?;
    tracing::info!(
        "vault: imported {} added, {} updated, {} skipped",
        counts.added,
        counts.updated,
        counts.skipped
    );
    Ok(Json(counts.to_json()))
}

/// POST /api/vault/import-csv: the raw CSV text, sent as `text/csv`. A cross-site form can't send
/// that type, so the request needs the same guard as the JSON writes.
pub(crate) async fn import_csv(
    State(s): State<Arc<AppState>>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let ct = h
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ct.starts_with("text/csv") {
        return Err(ApiError::new(415, "content-type must be text/csv"));
    }
    #[allow(
        clippy::map_err_ignore,
        reason = "a UTF-8 error quotes the CSV, which is a vault export of secrets"
    )]
    let text = std::str::from_utf8(&body)
        .map_err(|_| ApiError::bad_request("the CSV must be UTF-8 text"))?;
    let parsed = parse_vault_csv(text);
    let counts = import_items(&s.vault, &Value::Array(parsed.items)).await?;
    tracing::info!(
        "vault: imported CSV as {} ({} added, {} updated, {} skipped)",
        parsed.format,
        counts.added,
        counts.updated,
        parsed.skipped + counts.skipped
    );
    Ok(Json(json!({
        "format": parsed.format,
        "added": counts.added,
        "updated": counts.updated,
        "skipped": parsed.skipped + counts.skipped,
    })))
}
