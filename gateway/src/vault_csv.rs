//! Vault CSV import and export. The gateway accepts raw CSV, so a thin client need not parse it.
//!
//! Import and export use the Vault's public API (`list`, `get`, `create`, `update`). The Vault file
//! is rewritten once per saved item.
use crate::error::{ApiError, ApiResult};
use crate::vault::{site_domain, Vault};
use crate::{parse, require_json, AppState};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, sync::Arc};

const MAX_ITEMS: usize = 10_000;
const MAX_FIELD: usize = 4000;
const MAX_REPEATED: usize = 20;
const LOGIN_HEADER: [&str; 5] = ["name", "url", "username", "password", "note"];
const CONTACT_HEADER: [&str; 12] = [
    "label", "name", "email", "phone", "street", "apt", "city", "state", "zip", "country", "kind",
    "note",
];
/// Contact fields compared when deciding whether an imported contact is already saved.
const CONTACT_KEY_FIELDS: [&str; 9] = [
    "label", "name", "street", "apt", "city", "state", "zip", "country", "hints",
];

// ---------- parsing (vaultCsv.ts) ----------

/// RFC 4180 CSV: quoted fields, doubled quotes, CRLF or LF, a leading BOM. Blank rows are dropped.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let s = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == ',' {
            row.push(std::mem::take(&mut field));
        } else if ch == '\n' || ch == '\r' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else {
            field.push(ch);
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    rows
}

/// Which exporter wrote a file, as the import dialog names it.
fn detect(h: &[String]) -> &'static str {
    let has = |k: &str| h.iter().any(|x| x == k);
    if has("street") || (has("kind") && has("label") && !has("password")) {
        return "Bot contact info";
    }
    if has("login_uri") || has("login_password") {
        return "Bitwarden";
    }
    if has("httprealm") || has("formactionorigin") || has("timepasswordchanged") {
        return "Firefox";
    }
    if has("grouping") || (has("extra") && has("url")) {
        return "LastPass";
    }
    if has("otpauth") && (has("favorite") || has("archived") || has("tags")) {
        return "1Password";
    }
    if has("otpauth") && has("title") {
        return "Safari or Apple Passwords";
    }
    if has("title") && (has("website") || has("url")) && has("password") {
        return "1Password";
    }
    if has("name") && has("url") && has("password") {
        return "Chrome or Edge";
    }
    "CSV"
}

#[derive(Debug)]
pub struct CsvImport {
    pub format: &'static str,
    pub items: Vec<Value>,
    pub skipped: usize,
}

/// The header names each login field may carry, in the order they are tried.
const TITLE_COLS: &[&str] = &["name", "title"];
const URL_COLS: &[&str] = &[
    "url",
    "login_uri",
    "website",
    "urls",
    "login url",
    "web site",
    "uri",
    "location",
];
const USERNAME_COLS: &[&str] = &[
    "username",
    "login_username",
    "user name",
    "login",
    "login name",
    "email",
    "user",
];
const PASSWORD_COLS: &[&str] = &["password", "login_password"];
const NOTE_COLS: &[&str] = &["note", "notes", "extra", "comments", "comment"];

/// The first cell among `names` (in header order) that has text; untrimmed, like the browser parser.
fn col(header: &[String], row: &[String], names: &[&str]) -> String {
    for n in names {
        if let Some(i) = header.iter().position(|h| h == n) {
            if let Some(v) = row.get(i) {
                if !v.trim().is_empty() {
                    return v.clone();
                }
            }
        }
    }
    String::new()
}

/// A repeatable column: absent, malformed (a null in Node; the row is skipped), or a list of values.
enum Repeated {
    Absent,
    Invalid,
    List(Vec<String>),
}

/// Bot exports write JSON arrays only for repeated values; other exporters have no such column.
fn repeated_column(header: &[String], row: &[String], name: &str) -> Repeated {
    let raw = col(header, row, &[name]);
    if raw.is_empty() {
        return Repeated::Absent;
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Array(a))
            if a.len() <= MAX_REPEATED
                && a.iter().all(|v| {
                    v.as_str()
                        .is_some_and(|s| s.encode_utf16().count() <= MAX_FIELD)
                }) =>
        {
            Repeated::List(
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
            )
        }
        _ => Repeated::Invalid,
    }
}

fn hostname(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
        })
        .unwrap_or_default()
}

/// LastPass marks secure notes with a `http://sn` URL.
fn is_lastpass_note(url: &str) -> bool {
    matches!(
        url.to_lowercase().as_str(),
        "http://sn" | "http://sn/" | "https://sn" | "https://sn/"
    )
}

/// Parses a password manager's export (or a Bot contact export) into Vault items.
pub fn parse_vault_csv(text: &str) -> CsvImport {
    let rows = parse_csv(text);
    if rows.len() < 2 {
        return CsvImport {
            format: "CSV",
            items: Vec::new(),
            skipped: 0,
        };
    }
    let header: Vec<String> = rows[0].iter().map(|c| c.trim().to_lowercase()).collect();
    let format = detect(&header);
    let mut items = Vec::new();
    let mut skipped = 0;
    for row in &rows[1..] {
        let websites = repeated_column(&header, row, "websites");
        let emails = repeated_column(&header, row, "emails");
        let phones = repeated_column(&header, row, "phones");
        if [&websites, &emails, &phones]
            .iter()
            .any(|r| matches!(r, Repeated::Invalid))
        {
            skipped += 1;
            continue;
        }
        if format == "Bot contact info" {
            let get = |k: &str| col(&header, row, &[k]).trim().to_string();
            let kind = get("kind").to_lowercase();
            let note = get("note");
            let mut c = json!({
                "type": "contact",
                "label": get("label"),
                "name": get("name"),
                "email": get("email"),
                "phone": get("phone"),
                "street": get("street"),
                "apt": get("apt"),
                "city": get("city"),
                "state": get("state"),
                "zip": get("zip"),
                "country": get("country"),
                "kind": if ["home", "work", "other"].contains(&kind.as_str()) { kind } else { "home".into() },
                "hints": if note.is_empty() { get("hints") } else { note },
            });
            if let Repeated::List(v) = &emails {
                c["emails"] = json!(v);
            }
            if let Repeated::List(v) = &phones {
                c["phones"] = json!(v);
            }
            let present = ["label", "name", "email", "phone", "street"]
                .iter()
                .any(|k| c[*k].as_str().is_some_and(|s| !s.is_empty()));
            if present {
                items.push(c);
            } else {
                skipped += 1;
            }
            continue;
        }
        // Bitwarden exports notes and cards too; LastPass marks secure notes with http://sn.
        let kind = col(&header, row, &["type"]).trim().to_lowercase();
        if format == "Bitwarden" && !kind.is_empty() && kind != "login" {
            skipped += 1;
            continue;
        }
        let raw_url = col(&header, row, URL_COLS).trim().to_string();
        if format == "LastPass" && is_lastpass_note(&raw_url) {
            skipped += 1;
            continue;
        }
        // Several URIs in one field (Bitwarden, 1Password): keep the first.
        let url = raw_url
            .split(|c: char| c == ',' || c.is_whitespace())
            .find(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        let password = col(&header, row, PASSWORD_COLS);
        let username = col(&header, row, USERNAME_COLS).trim().to_string();
        let mut title = col(&header, row, TITLE_COLS).trim().to_string();
        if title.is_empty() {
            title = hostname(&url);
        }
        if password.is_empty() && username.is_empty() {
            skipped += 1;
            continue;
        }
        if url.is_empty() && title.is_empty() {
            skipped += 1;
            continue;
        }
        let mut item = json!({
            "type": "login",
            "title": title,
            "website": url,
            "username": username,
            "password": password,
            "hints": col(&header, row, NOTE_COLS).trim(),
        });
        if let Repeated::List(v) = &websites {
            item["websites"] = json!(v);
        }
        items.push(item);
    }
    CsvImport {
        format,
        items,
        skipped,
    }
}

// ---------- writing (vault.mts toCsv) ----------

fn csv_cell(v: &str) -> String {
    let needs_quotes = v.contains(['"', ',', '\r', '\n'])
        || v.starts_with(char::is_whitespace)
        || v.ends_with(char::is_whitespace);
    if needs_quotes {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

/// RFC 4180 rows joined with CRLF, with a trailing CRLF.
pub fn to_csv(rows: &[Vec<String>]) -> String {
    let lines: Vec<String> = rows
        .iter()
        .map(|r| r.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","))
        .collect();
    let mut out = lines.join("\r\n");
    out.push_str("\r\n");
    out
}

// ---------- shared item helpers ----------

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// A stored value as text: null or missing is empty, anything else is its JSON text.
fn clean_value(v: Option<&Value>) -> String {
    let s = match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    s.chars().take(MAX_FIELD).collect()
}

/// `website` / `email` / `phone` values: the plural list if sent, else the single value.
fn repeated_values(item: &Value, field: &str) -> Vec<String> {
    if let Some(Value::Array(a)) = item.get(format!("{field}s")) {
        return a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    let single = text(item, field);
    if single.is_empty() {
        Vec::new()
    } else {
        vec![single]
    }
}

/// Text values as the Vault stores them: trimmed, non-empty, first occurrence kept.
fn unique_nonempty(values: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in values {
        if !v.is_empty() && !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Whether the Vault would accept a repeated field sent as `v`.
fn list_valid(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Array(a)) => {
            a.len() <= MAX_REPEATED
                && a.iter()
                    .all(|x| x.as_str().is_some_and(|s| s.len() <= MAX_FIELD))
        }
        Some(_) => false,
    }
}

/// The values a repeated field would hold once saved: the plural list, else the single value.
fn input_values(input: &Map<String, Value>, plural: &str, field: &str) -> Vec<Value> {
    if let Some(Value::Array(a)) = input.get(plural) {
        return a.clone();
    }
    input.get(field).cloned().into_iter().collect()
}

fn normalize_website(v: &Value) -> String {
    let s = clean_value(Some(v)).trim().to_string();
    if s.is_empty() || s.contains("://") || site_domain(&s).is_empty() {
        s
    } else {
        format!("https://{s}")
    }
}

/// What a login would hold once saved; None where the Vault would refuse it.
struct LocalLogin {
    websites: Vec<String>,
    website: String,
    title: String,
    username: String,
    password: String,
    hints: String,
}

fn local_login(input: &Map<String, Value>) -> Option<LocalLogin> {
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
fn local_contact(input: &Map<String, Value>) -> Value {
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
fn contact_signature(item: &Value) -> Value {
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
pub async fn export_csv(vault: &Vault, kind: &str) -> ApiResult<String> {
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

// ---------- import (vault.mts importItems) ----------

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

impl ImportCounts {
    fn to_json(&self) -> Value {
        json!({ "added": self.added, "updated": self.updated, "skipped": self.skipped })
    }
}

enum Outcome {
    Added,
    Updated,
    Skipped,
}

/// Adds imported items. A login with the same domain and username as a saved one updates it
/// (password and hints), and an identical one is skipped. A contact identical to a saved one is
/// skipped. An item the Vault would refuse counts as skipped.
pub async fn import_items(vault: &Vault, list: &Value) -> ApiResult<ImportCounts> {
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

async fn import_one(vault: &Vault, raw: &Value) -> ApiResult<Outcome> {
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
pub async fn import(
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
pub async fn import_csv(
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

/// GET /api/vault/export.csv[?type=contact]: the file carries passwords, so it is not cached.
pub async fn export(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str) -> Vec<Vec<String>> {
        parse_csv(text)
    }

    fn s(v: &str) -> String {
        v.to_string()
    }

    #[test]
    fn parses_quotes_crlf_and_bom() {
        assert_eq!(
            rows("\u{feff}a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,2,3\r\n"),
            vec![
                vec![s("a"), s("b,c"), s("say \"hi\"")],
                vec![s("1"), s("2"), s("3")],
            ]
        );
    }

    #[test]
    fn keeps_newlines_inside_quotes_and_drops_blank_rows() {
        assert_eq!(
            rows("x,y\n\n\"line one\nline two\",z"),
            vec![vec![s("x"), s("y")], vec![s("line one\nline two"), s("z")]]
        );
        // A trailing row without a newline still counts; a row of spaces does not.
        assert_eq!(rows("a\n   \nb"), vec![vec![s("a")], vec![s("b")]]);
    }

    #[test]
    fn keeps_empty_trailing_fields() {
        assert_eq!(rows("a,,\r\n"), vec![vec![s("a"), s(""), s("")]]);
    }

    #[test]
    fn writes_quoted_cells_only_when_needed() {
        let out = to_csv(&[vec![
            s("plain"),
            s("has,comma"),
            s("has \"quote\""),
            s("line\r\nbreak"),
            s(" leading"),
            s("trailing "),
            s(""),
        ]]);
        assert_eq!(
            out,
            "plain,\"has,comma\",\"has \"\"quote\"\"\",\"line\r\nbreak\",\" leading\",\"trailing \",\r\n"
        );
        assert_eq!(to_csv(&[]), "\r\n");
    }

    #[test]
    fn chrome_fixture() {
        let r = parse_vault_csv(
            "name,url,username,password,note\r\nGitHub,https://github.com/login,octo,hunter2,\"two, lines\"\r\n",
        );
        assert_eq!(r.format, "Chrome or Edge");
        assert_eq!(r.skipped, 0);
        assert_eq!(
            r.items,
            vec![
                json!({"type":"login","title":"GitHub","website":"https://github.com/login","username":"octo","password":"hunter2","hints":"two, lines"})
            ]
        );
    }

    #[test]
    fn one_password_fixture() {
        let r = parse_vault_csv(
            "Title,Website,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\nExample,https://example.com,ann,pw1,,false,false,,remember me\n",
        );
        assert_eq!(r.format, "1Password");
        assert_eq!(r.items[0]["title"], "Example");
        assert_eq!(r.items[0]["website"], "https://example.com");
        assert_eq!(r.items[0]["hints"], "remember me");
    }

    #[test]
    fn safari_fixture() {
        let r = parse_vault_csv(
            "Title,URL,Username,Password,Notes,OTPAuth\nBank,https://bank.example,bob,pw2,,\n",
        );
        assert_eq!(r.format, "Safari or Apple Passwords");
        assert_eq!(r.items[0]["username"], "bob");
        assert_eq!(r.items[0]["password"], "pw2");
    }

    #[test]
    fn firefox_fixture_takes_the_title_from_the_host() {
        let r = parse_vault_csv(
            "url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,timePasswordChanged\nhttps://www.forum.example/x,cy,pw3,,https://forum.example,{1},0,0,0\n",
        );
        assert_eq!(r.format, "Firefox");
        assert_eq!(r.items[0]["title"], "forum.example");
        assert_eq!(r.items[0]["website"], "https://www.forum.example/x");
    }

    #[test]
    fn bitwarden_fixture_skips_notes() {
        let r = parse_vault_csv(
            "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
             ,0,login,Mail,mailnote,,0,\"https://mail.example, https://webmail.example\",dee,pw4,\n\
             ,0,note,Secret note,body,,0,,,,\n",
        );
        assert_eq!(r.format, "Bitwarden");
        assert_eq!(r.skipped, 1);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0]["website"], "https://mail.example");
        assert_eq!(r.items[0]["hints"], "mailnote");
    }

    #[test]
    fn lastpass_fixture_skips_secure_notes() {
        let r = parse_vault_csv(
            "url,username,password,totp,extra,name,grouping,fav\n\
             http://sn,,,,secret stuff,Note,,0\n\
             https://shop.example,eve,pw5,,shop note,Shop,Store,0\n",
        );
        assert_eq!(r.format, "LastPass");
        assert_eq!(r.skipped, 1);
        assert_eq!(r.items[0]["title"], "Shop");
        assert_eq!(r.items[0]["hints"], "shop note");
    }

    #[test]
    fn bot_contact_fixture_keeps_repeated_values() {
        let r = parse_vault_csv(concat!(
            "kind,label,name,email,phone,street,apt,city,state,zip,country,note,emails,phones\r\n",
            "home,Home,Ada,ada@example.com,555,1 Main St,,Springfield,IL,62701,US,door code 1234,",
            "\"[\"\"ada@example.com\"\",\"\"ada@home.example\"\"]\",\"[\"\"555\"\"]\"\r\n",
            "work,Broken,,,,,,,,,,,\"[1]\",\r\n",
        ));
        assert_eq!(r.format, "Bot contact info");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.skipped, 1);
        let c = &r.items[0];
        assert_eq!(c["type"], "contact");
        assert_eq!(c["emails"], json!(["ada@example.com", "ada@home.example"]));
        assert_eq!(c["phones"], json!(["555"]));
        assert_eq!(c["hints"], "door code 1234");
        assert_eq!(c["kind"], "home");
    }

    #[test]
    fn login_websites_keep_their_order_and_malformed_arrays_are_skipped() {
        let r = parse_vault_csv(concat!(
            "name,url,username,password,note,websites\n",
            "Sample,https://first.invalid,user,synthetic,,\"[\"\"https://first.invalid\"\",\"\"https://second.invalid\"\"]\"\n",
            "Broken,https://x.invalid,user,synthetic,,broken\n",
        ));
        assert_eq!(r.skipped, 1);
        assert_eq!(
            r.items[0]["websites"],
            json!(["https://first.invalid", "https://second.invalid"])
        );
    }

    #[test]
    fn rows_without_a_login_or_a_site_are_skipped() {
        let r = parse_vault_csv("name,url,username,password\nNo login,,,\n,,me,\n");
        assert_eq!(r.items.len(), 0);
        assert_eq!(r.skipped, 2);
        assert_eq!(parse_vault_csv("name,url\n").items.len(), 0);
    }

    fn tmp_vault(dir: &tempfile::TempDir) -> Vault {
        Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"))
    }

    #[tokio::test]
    async fn import_merges_by_domain_and_username() {
        let dir = tempfile::tempdir().unwrap();
        let v = tmp_vault(&dir);
        v.create(&json!({"type":"login","website":"https://github.com","username":"octo","password":"old"}))
            .await
            .unwrap();
        let counts = import_items(
            &v,
            &json!([
                {"website":"https://www.github.com/login","username":"octo","password":"new"},
                {"website":"https://github.com","username":"octo","password":"new"},
                {"website":"https://gitlab.com","username":"octo","password":"x"},
                {"type":"contact","name":"Ada","email":"ada@example.com"},
                {"title":""},
            ]),
        )
        .await
        .unwrap();
        assert_eq!(
            counts,
            ImportCounts {
                added: 2,
                updated: 1,
                skipped: 2
            }
        );
        let github = v
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|i| i["domain"] == "github.com")
            .unwrap();
        let full = v.get(github["id"].as_str().unwrap(), true).await.unwrap();
        assert_eq!(full["password"], "new");
    }

    #[tokio::test]
    async fn import_rejects_a_list_that_is_not_a_list() {
        let dir = tempfile::tempdir().unwrap();
        let err = import_items(&tmp_vault(&dir), &json!({"a": 1}))
            .await
            .unwrap_err();
        assert_eq!(err.status, 400);
    }

    #[tokio::test]
    async fn import_skips_a_contact_already_saved() {
        let dir = tempfile::tempdir().unwrap();
        let v = tmp_vault(&dir);
        let contact = json!({"type":"contact","label":"Home","name":"Ada","emails":["ada@example.com","a@b.example"],"kind":"home"});
        assert_eq!(
            import_items(&v, &json!([contact.clone()]))
                .await
                .unwrap()
                .added,
            1
        );
        assert_eq!(
            import_items(&v, &json!([contact])).await.unwrap().skipped,
            1
        );
    }

    #[tokio::test]
    async fn export_then_import_gives_the_same_items() {
        let dir_a = tempfile::tempdir().unwrap();
        let a = tmp_vault(&dir_a);
        a.create(&json!({"type":"login","title":"Mail","websites":["https://mail.example","https://webmail.example"],"username":"dee","password":" pass, \"quoted\"\r\n","hints":"two\nlines"}))
            .await
            .unwrap();
        a.create(&json!({"type":"login","website":"https://shop.example","username":"eve","password":"pw5"}))
            .await
            .unwrap();
        a.create(&json!({"type":"contact","label":"Home","name":"Ada","email":"ada@example.com","phones":["555","556"],"kind":"work","street":"1 Main St, Apt 2"}))
            .await
            .unwrap();

        let logins = export_csv(&a, "login").await.unwrap();
        let contacts = export_csv(&a, "contact").await.unwrap();

        let dir_b = tempfile::tempdir().unwrap();
        let b = tmp_vault(&dir_b);
        let parsed_logins = parse_vault_csv(&logins);
        assert_eq!(parsed_logins.format, "Chrome or Edge");
        assert_eq!(parsed_logins.skipped, 0);
        let parsed_contacts = parse_vault_csv(&contacts);
        assert_eq!(parsed_contacts.format, "Bot contact info");
        assert_eq!(parsed_contacts.skipped, 0);
        import_items(&b, &Value::Array(parsed_logins.items))
            .await
            .unwrap();
        import_items(&b, &Value::Array(parsed_contacts.items))
            .await
            .unwrap();

        assert_eq!(export_csv(&b, "login").await.unwrap(), logins);
        assert_eq!(export_csv(&b, "contact").await.unwrap(), contacts);
        // The password survived the round trip, quotes, comma, CRLF and all.
        let reread = parse_vault_csv(&logins);
        assert_eq!(reread.items[0]["password"], " pass, \"quoted\"\r\n");
    }

    #[tokio::test]
    async fn exports_passwords_only_in_the_login_file() {
        let dir = tempfile::tempdir().unwrap();
        let v = tmp_vault(&dir);
        v.create(&json!({"type":"login","website":"https://a.example","username":"u","password":"hidden-pass"}))
            .await
            .unwrap();
        assert!(export_csv(&v, "login")
            .await
            .unwrap()
            .contains("hidden-pass"));
        assert!(!export_csv(&v, "contact")
            .await
            .unwrap()
            .contains("hidden-pass"));
    }
}
