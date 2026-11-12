use super::*;

pub(crate) const MAX_ITEMS: usize = 10_000;

pub(crate) const MAX_FIELD: usize = 4000;

pub(crate) const MAX_REPEATED: usize = 20;

pub(crate) const LOGIN_HEADER: [&str; 5] = ["name", "url", "username", "password", "note"];

pub(crate) const CONTACT_HEADER: [&str; 12] = [
    "label", "name", "email", "phone", "street", "apt", "city", "state", "zip", "country", "kind",
    "note",
];

/// Contact fields compared when deciding whether an imported contact is already saved.
pub(crate) const CONTACT_KEY_FIELDS: [&str; 9] = [
    "label", "name", "street", "apt", "city", "state", "zip", "country", "hints",
];

// ---------- parsing (vaultCsv.ts) ----------

/// RFC 4180 CSV: quoted fields, doubled quotes, CRLF or LF, a leading BOM. Blank rows are dropped.
pub(crate) fn parse_csv(text: &str) -> Vec<Vec<String>> {
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
pub(crate) fn detect(h: &[String]) -> &'static str {
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
pub(crate) struct CsvImport {
    pub format: &'static str,
    pub items: Vec<Value>,
    pub skipped: usize,
}

/// The header names each login field may carry, in the order they are tried.
pub(crate) const TITLE_COLS: &[&str] = &["name", "title"];

pub(crate) const URL_COLS: &[&str] = &[
    "url",
    "login_uri",
    "website",
    "urls",
    "login url",
    "web site",
    "uri",
    "location",
];

pub(crate) const USERNAME_COLS: &[&str] = &[
    "username",
    "login_username",
    "user name",
    "login",
    "login name",
    "email",
    "user",
];

pub(crate) const PASSWORD_COLS: &[&str] = &["password", "login_password"];

pub(crate) const NOTE_COLS: &[&str] = &["note", "notes", "extra", "comments", "comment"];

/// The first cell among `names` (in header order) that has text; untrimmed, like the browser parser.
pub(crate) fn col(header: &[String], row: &[String], names: &[&str]) -> String {
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
pub(crate) enum Repeated {
    Absent,
    Invalid,
    List(Vec<String>),
}

/// Bot exports write JSON arrays only for repeated values; other exporters have no such column.
pub(crate) fn repeated_column(header: &[String], row: &[String], name: &str) -> Repeated {
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

pub(crate) fn hostname(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
        })
        .unwrap_or_default()
}

/// LastPass marks secure notes with a `http://sn` URL.
pub(crate) fn is_lastpass_note(url: &str) -> bool {
    matches!(
        url.to_lowercase().as_str(),
        "http://sn" | "http://sn/" | "https://sn" | "https://sn/"
    )
}

/// Parses a password manager's export (or a Bot contact export) into Vault items.
pub(crate) fn parse_vault_csv(text: &str) -> CsvImport {
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

pub(crate) fn csv_cell(v: &str) -> String {
    let needs_quotes = v.contains(['"', ',', '\r', '\n'])
        || v.starts_with(char::is_whitespace)
        || v.ends_with(char::is_whitespace);
    if needs_quotes {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

// ---------- shared item helpers ----------

pub(crate) fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// A stored value as text: null or missing is empty, anything else is its JSON text.
pub(crate) fn clean_value(v: Option<&Value>) -> String {
    let s = match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    s.chars().take(MAX_FIELD).collect()
}

/// `website` / `email` / `phone` values: the plural list if sent, else the single value.
pub(crate) fn repeated_values(item: &Value, field: &str) -> Vec<String> {
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
pub(crate) fn unique_nonempty(values: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in values {
        if !v.is_empty() && !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Whether the Vault would accept a repeated field sent as `v`.
pub(crate) fn list_valid(v: Option<&Value>) -> bool {
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
pub(crate) fn input_values(input: &Map<String, Value>, plural: &str, field: &str) -> Vec<Value> {
    if let Some(Value::Array(a)) = input.get(plural) {
        return a.clone();
    }
    input.get(field).cloned().into_iter().collect()
}

pub(crate) fn normalize_website(v: &Value) -> String {
    let s = clean_value(Some(v)).trim().to_string();
    if s.is_empty() || s.contains("://") || site_domain(&s).is_empty() {
        s
    } else {
        format!("https://{s}")
    }
}
