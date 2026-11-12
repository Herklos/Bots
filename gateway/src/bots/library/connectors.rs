use super::*;

pub(crate) fn validate_connector_name(name: &str) -> ApiResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 32
        && name != "bot"
        && name != "executor"
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "A connector name is a short token, and not bot or executor.",
        ))
    }
}

pub(crate) fn validate_connector_url(url: &str) -> ApiResult<()> {
    match url::Url::parse(url) {
        Ok(u)
            if matches!(u.scheme(), "http" | "https") && !u.host_str().unwrap_or("").is_empty() =>
        {
            Ok(())
        }
        _ => Err(ApiError::bad_request(
            "A connector URL starts with http:// or https://.",
        )),
    }
}

/// `bot` stays. An enabled connector is written. A disabled one is removed.
/// A secret never appears in a response; it is only placed in the file the harness reads.
pub(crate) fn merge_mcp(
    existing: &Value,
    connectors: &[Value],
    bot_url: &str,
    bot_token: &str,
) -> Value {
    let mut mcp = existing.clone();
    if !mcp.is_object() {
        mcp = json!({});
    }
    let mut servers = mcp["mcpServers"].as_object().cloned().unwrap_or_default();
    servers.insert(
        "bot".into(),
        json!({
            "type": "http",
            "url": bot_url,
            "headers": { "Authorization": format!("Bearer {bot_token}") },
        }),
    );
    for c in connectors {
        let Some(name) = c["name"].as_str() else {
            continue;
        };
        if name == "bot" || name == "executor" {
            continue;
        }
        if c["enabled"] == true {
            let mut row = json!({ "type": "http", "url": c["url"] });
            if let Some(secret) = c["secret"].as_str().filter(|s| !s.is_empty()) {
                row["headers"] = json!({ "Authorization": format!("Bearer {secret}") });
            }
            servers.insert(name.to_string(), row);
        } else {
            servers.remove(name);
        }
    }
    mcp["mcpServers"] = json!(servers);
    mcp
}

pub(crate) fn claude_settings(existing: &Value, connectors: &[Value]) -> Value {
    let mut cs = if existing.is_object() {
        existing.clone()
    } else {
        json!({})
    };
    cs["enableAllProjectMcpServers"] = json!(true);
    let disabled: Vec<&str> = connectors
        .iter()
        .filter(|c| c["enabled"] != true)
        .filter_map(|c| c["name"].as_str())
        .filter(|n| *n != "bot")
        .collect();
    cs["disabledMcpjsonServers"] = json!(disabled);
    install_agents_md_hooks(&mut cs);
    cs
}

pub(crate) fn codex_toml(mcp: &Value) -> String {
    let mut out =
        String::from("# Written by the Bot gateway. bot = the Vault and other Bot tools.\n");
    if let Some(servers) = mcp["mcpServers"].as_object() {
        let mut names: Vec<_> = servers.keys().cloned().collect();
        names.sort();
        for name in names {
            let s = &servers[&name];
            let Some(url) = s["url"].as_str() else {
                continue;
            };
            out.push_str(&format!("[mcp_servers.{name}]\nurl = {url:?}\n"));
            if let Some(auth) = s["headers"]["Authorization"].as_str() {
                out.push_str(&format!(
                    "http_headers = {{ \"Authorization\" = {auth:?} }}\n"
                ));
            }
            out.push_str("tool_timeout_sec = 960\n");
        }
    }
    out
}

impl Library {
    pub async fn public_connectors(&self, slug: &str) -> ApiResult<Vec<Value>> {
        Self::require_slug(slug)?;
        let mut rows = vec![json!({
            "name": "bot",
            "url": self.mcp_url,
            "enabled": true,
            "locked": true,
            "hasSecret": true,
        })];
        for c in self.read_connectors(slug).await? {
            rows.push(json!({
                "name": c["name"],
                "url": c["url"],
                "enabled": c["enabled"] == true,
                "locked": false,
                "hasSecret": c["secret"].as_str().is_some_and(|s| !s.is_empty()),
            }));
        }
        Ok(rows)
    }

    pub(crate) async fn read_connectors(&self, slug: &str) -> ApiResult<Vec<Value>> {
        match self.ws.read(&Self::connectors_path(slug)).await {
            Ok(bytes) => {
                #[allow(
                    clippy::map_err_ignore,
                    reason = "serde quotes connectors.json, which can hold a connector secret"
                )]
                let doc: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::new(500, "connectors.json is not valid JSON"))?;
                Ok(doc["connectors"].as_array().cloned().unwrap_or_default())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save_connector(&self, slug: &str, input: &Value) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        let name = input["name"].as_str().unwrap_or("").trim();
        validate_connector_name(name)?;
        let url = input["url"].as_str().unwrap_or("").trim();
        validate_connector_url(url)?;
        let enabled = input["enabled"] != false;
        let secret = input["secret"].as_str().unwrap_or("").trim().to_string();
        let mut all = self.read_connectors(slug).await?;
        if let Some(found) = all.iter_mut().find(|c| c["name"] == name) {
            let kept = found["secret"].as_str().unwrap_or("").to_string();
            found["url"] = json!(url);
            found["enabled"] = json!(enabled);
            if !secret.is_empty() {
                found["secret"] = json!(secret);
            } else if !kept.is_empty() {
                found["secret"] = json!(kept);
            }
        } else {
            all.push(json!({ "name": name, "url": url, "enabled": enabled, "secret": secret }));
        }
        self.write_connectors(slug, &all).await?;
        self.rewrite_harness(slug, &all).await?;
        Ok(json!({ "ok": true }))
    }

    pub async fn delete_connector(&self, slug: &str, name: &str) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        if name == "bot" {
            return Err(ApiError::bad_request("The vault connector stays on."));
        }
        let mut all = self.read_connectors(slug).await?;
        let before = all.len();
        all.retain(|c| c["name"] != name);
        if all.len() == before {
            return Err(ApiError::not_found("no such connector"));
        }
        self.write_connectors(slug, &all).await?;
        // The deleted name is not in the saved list. Pass it as off so the
        // harness files drop it, and so a later session cannot keep it alive.
        let mut for_harness = all.clone();
        for_harness.push(json!({ "name": name, "enabled": false }));
        self.rewrite_harness(slug, &for_harness).await?;
        Ok(json!({ "ok": true }))
    }

    pub(crate) async fn write_connectors(&self, slug: &str, connectors: &[Value]) -> ApiResult<()> {
        let text =
            serde_json::to_string_pretty(&json!({ "connectors": connectors })).unwrap() + "\n";
        self.ws
            .write(&Self::connectors_path(slug), text.as_bytes(), Some(0o600))
            .await?;
        Ok(())
    }

    pub(crate) async fn rewrite_harness(&self, slug: &str, connectors: &[Value]) -> ApiResult<()> {
        let mcp_rel = format!("{slug}/.mcp.json");
        let existing = match self.ws.read(&mcp_rel).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        let claude_rel = format!("{slug}/.claude/settings.json");
        let claude_existing = match self.ws.read(&claude_rel).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        let mcp = merge_mcp(&existing, connectors, &self.mcp_url, &self.mcp_token);
        let claude = claude_settings(&claude_existing, connectors);
        #[cfg(unix)]
        {
            let hook_rel = format!("{slug}/.bot/hooks/agents-md.sh");
            let current = self.ws.read(&hook_rel).await.unwrap_or_default();
            if current != AGENTS_MD_SCRIPT.as_bytes() {
                self.ws
                    .write(&hook_rel, AGENTS_MD_SCRIPT.as_bytes(), Some(0o755))
                    .await?;
            }
        }
        let toml = codex_toml(&mcp);
        let mcp_text = serde_json::to_string_pretty(&mcp).unwrap_or_else(|_| "{}".into()) + "\n";
        let claude_text =
            serde_json::to_string_pretty(&claude).unwrap_or_else(|_| "{}".into()) + "\n";
        self.ws
            .write(&mcp_rel, mcp_text.as_bytes(), Some(0o600))
            .await?;
        self.ws
            .write(
                &format!("{slug}/.codex/config.toml"),
                toml.as_bytes(),
                Some(0o600),
            )
            .await?;
        self.ws
            .write(&claude_rel, claude_text.as_bytes(), None)
            .await?;
        Ok(())
    }
}
