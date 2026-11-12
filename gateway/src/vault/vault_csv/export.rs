use super::*;

/// RFC 4180 rows joined with CRLF, with a trailing CRLF.
pub(crate) fn to_csv(rows: &[Vec<String>]) -> String {
    let lines: Vec<String> = rows
        .iter()
        .map(|r| r.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","))
        .collect();
    let mut out = lines.join("\r\n");
    out.push_str("\r\n");
    out
}

/// What a login would hold once saved; None where the Vault would refuse it.
pub(crate) struct LocalLogin {
    pub(crate) websites: Vec<String>,
    pub(crate) website: String,
    pub(crate) title: String,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) hints: String,
}

pub(crate) fn local_login(input: &Map<String, Value>) -> Option<LocalLogin> {
    let websites = unique_nonempty(
        input_values(input, "websites", "website")
            .iter()
            .map(normalize_website)
            .collect(),
    );
    let website = websites.first().cloned().unwrap_or_default();
    let title_raw = clean_value(input.get("title")).trim().to_string();
    if website.is_empty() && title_raw.is_empty() {
        return None;
    }
    let title = if !title_raw.is_empty() {
        title_raw
    } else {
        let domain = site_domain(&website);
        if domain.is_empty() {
            website.clone()
        } else {
            domain
        }
    };
    Some(LocalLogin {
        websites,
        website,
        title,
        username: clean_value(input.get("username")).trim().to_string(),
        password: clean_value(input.get("password")),
        hints: clean_value(input.get("hints")).trim().to_string(),
    })
}

/// A contact as it would be saved, shaped like `contact_signature` reads a saved one.
pub(crate) fn local_contact(input: &Map<String, Value>) -> Value {
    let mut m = Map::new();
    for f in CONTACT_KEY_FIELDS {
        m.insert(f.into(), json!(clean_value(input.get(f)).trim()));
    }
    let kind = match input.get("kind") {
        Some(k) => k
            .as_str()
            .filter(|k| ["home", "work", "other"].contains(k))
            .unwrap_or("other"),
        None => "home",
    };
    m.insert("kind".into(), json!(kind));
    let values = |plural: &str, field: &str| {
        unique_nonempty(
            input_values(input, plural, field)
                .iter()
                .map(|v| clean_value(Some(v)).trim().to_string())
                .collect(),
        )
    };
    m.insert("emails".into(), json!(values("emails", "email")));
    m.insert("phones".into(), json!(values("phones", "phone")));
    Value::Object(m)
}

/// The fields a contact import compares; a contact with the same signature is already saved.
pub(crate) fn contact_signature(item: &Value) -> Value {
    let mut m = Map::new();
    for f in CONTACT_KEY_FIELDS.iter().chain(["kind"].iter()) {
        m.insert((*f).into(), json!(text(item, f)));
    }
    m.insert("emails".into(), json!(repeated_values(item, "email")));
    m.insert("phones".into(), json!(repeated_values(item, "phone")));
    Value::Object(m)
}

// ---------- export (vault.mts exportCsv) ----------

/// The CSV for logins (with passwords) or contacts. Repeated values get extra JSON columns
/// only when some item has more than one.
pub(crate) async fn export_csv(vault: &Vault, kind: &str) -> ApiResult<String> {
    let all = vault.list().await?;
    if kind == "contact" {
        let contacts: Vec<&Value> = all.iter().filter(|i| i["type"] == "contact").collect();
        let repeated = contacts.iter().any(|c| {
            repeated_values(c, "email").len() > 1 || repeated_values(c, "phone").len() > 1
        });
        let mut header: Vec<String> = CONTACT_HEADER.iter().map(|s| s.to_string()).collect();
        if repeated {
            header.extend(["emails".into(), "phones".into()]);
        }
        let mut rows = vec![header];
        for c in contacts {
            let label = text(c, "label");
            let mut row = vec![
                if label.is_empty() {
                    text(c, "title")
                } else {
                    label
                },
                text(c, "name"),
                text(c, "email"),
                text(c, "phone"),
                text(c, "street"),
                text(c, "apt"),
                text(c, "city"),
                text(c, "state"),
                text(c, "zip"),
                text(c, "country"),
                text(c, "kind"),
                text(c, "hints"),
            ];
            if repeated {
                row.push(serde_json::to_string(&repeated_values(c, "email")).unwrap_or_default());
                row.push(serde_json::to_string(&repeated_values(c, "phone")).unwrap_or_default());
            }
            rows.push(row);
        }
        return Ok(to_csv(&rows));
    }
    // The list is redacted; logins need their passwords.
    let mut logins = Vec::new();
    for item in all.iter().filter(|i| i["type"] == "login") {
        logins.push(vault.get(&text(item, "id"), true).await?);
    }
    let repeated = logins
        .iter()
        .any(|l| repeated_values(l, "website").len() > 1);
    let mut header: Vec<String> = LOGIN_HEADER.iter().map(|s| s.to_string()).collect();
    if repeated {
        header.push("websites".into());
    }
    let mut rows = vec![header];
    for l in &logins {
        let mut row = vec![
            text(l, "title"),
            text(l, "website"),
            text(l, "username"),
            text(l, "password"),
            text(l, "hints"),
        ];
        if repeated {
            row.push(serde_json::to_string(&repeated_values(l, "website")).unwrap_or_default());
        }
        rows.push(row);
    }
    Ok(to_csv(&rows))
}

/// GET /api/vault/export.csv[?type=contact]: the file carries passwords, so it is not cached.
pub(crate) async fn export(
    State(s): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let contact = q.get("type").map(String::as_str) == Some("contact");
    let kind = if contact { "contact" } else { "login" };
    let csv = export_csv(&s.vault, kind).await?;
    tracing::info!("vault: exported {kind}s as CSV");
    let name = if contact {
        "bot-vault-contacts.csv"
    } else {
        "bot-vault-logins.csv"
    };
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        csv,
    )
        .into_response())
}
