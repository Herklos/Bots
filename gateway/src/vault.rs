//! The Vault on disk:
//! `{"format":"bot-vault","version":1,"cipher":"aes-256-gcm","iv","tag","data"}` (base64),
//! AAD `bot-vault:v1`, a 32-byte raw key in its own file (mode 600), plaintext `{items, updatedAt}`.
//! Passwords never leave here except through `get(id, reveal = true)`.
use crate::error::{ApiError, ApiResult};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use tokio::sync::Mutex;

const FORMAT: &str = "bot-vault";
const AAD: &[u8] = b"bot-vault:v1";
const MAX_FIELD: usize = 4000;
const MAX_ITEMS: usize = 10_000;
const LOGIN_FIELDS: &[&str] = &["title", "username", "password", "hints", "botCanRead"];
const CONTACT_FIELDS: &[&str] = &[
    "title", "label", "name", "street", "apt", "city", "state", "zip", "country", "kind", "hints",
];

pub struct Vault {
    file: PathBuf,
    key_file: PathBuf,
    items: Mutex<Option<Vec<Value>>>,
}

fn now() -> String {
    // RFC 3339 in UTC, like JavaScript's toISOString.
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    iso(i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

pub fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms.rem_euclid(1000)
    )
}

pub fn now_iso() -> String {
    now()
}

/// The domain a login fills on: the website's hostname without a leading `www.`.
pub fn site_domain(website: &str) -> String {
    let s = website.trim();
    if s.is_empty() {
        return String::new();
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}")
    };
    match url::Url::parse(&with_scheme) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {
            let host = u.host_str().unwrap_or("").to_lowercase();
            let host = host.trim_end_matches('.');
            host.strip_prefix("www.").unwrap_or(host).to_string()
        }
        _ => String::new(),
    }
}

/// Whether a page URL is the login's site or one of its subdomains; lookalikes don't match.
/// Used by the `vault_fill_*` Bot tools (`vault_tools.rs`) before a password is typed.
pub fn matches_site(page_url: &str, website: &str) -> bool {
    let domain = site_domain(website);
    let Ok(page) = url::Url::parse(page_url) else {
        return false;
    };
    if domain.is_empty() || !matches!(page.scheme(), "http" | "https") {
        return false;
    }
    if page.scheme() == "http" && !website.trim().to_lowercase().starts_with("http://") {
        return false;
    }
    let host = page.host_str().unwrap_or("").to_lowercase();
    let host = host.trim_end_matches('.');
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn clean(v: &Value) -> String {
    let s = match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    s.chars().take(MAX_FIELD).collect()
}

fn normalize_website(v: &Value) -> String {
    let s = clean(v).trim().to_string();
    if s.is_empty() || s.contains("://") || site_domain(&s).is_empty() {
        s
    } else {
        format!("https://{s}")
    }
}

/// `website`/`websites`, `email`/`emails`, `phone`/`phones`: an ordered list, its first value mirrored.
fn normalize_repeated(
    input: &Map<String, Value>,
    out: &mut Map<String, Value>,
    field: &str,
) -> ApiResult<()> {
    let plural = format!("{field}s");
    if !input.contains_key(&plural) && !input.contains_key(field) {
        return Ok(());
    }
    let values: Vec<Value> = if let Some(list) = input.get(&plural) {
        let list = list.as_array().filter(|l| {
            l.len() <= 20
                && l.iter()
                    .all(|v| v.as_str().is_some_and(|s| s.len() <= MAX_FIELD))
        });
        list.cloned().ok_or_else(|| {
            ApiError::bad_request(format!("{plural} must contain at most 20 text values."))
        })?
    } else {
        let mut v = vec![input[field].clone()];
        if let Some(prev) = out.get(&plural).and_then(Value::as_array) {
            v.extend(prev.iter().skip(1).cloned());
        }
        v
    };
    let mut seen = Vec::<String>::new();
    for v in values {
        let s = if field == "website" {
            normalize_website(&v)
        } else {
            clean(&v).trim().to_string()
        };
        if !s.is_empty() && !seen.contains(&s) {
            seen.push(s);
        }
    }
    out.insert(
        field.into(),
        json!(seen.first().cloned().unwrap_or_default()),
    );
    out.insert(plural, json!(seen));
    Ok(())
}

fn normalize(input: &Value, existing: Option<&Value>) -> ApiResult<Value> {
    let input = input
        .as_object()
        .ok_or_else(|| ApiError::bad_request("expected an object"))?;
    let kind = existing
        .and_then(|e| e["type"].as_str())
        .or_else(|| input.get("type").and_then(Value::as_str))
        .unwrap_or("");
    if kind != "login" && kind != "contact" {
        return Err(ApiError::bad_request("type must be login or contact"));
    }
    let mut out = match existing {
        Some(e) => e.as_object().cloned().unwrap_or_default(),
        None => {
            let mut m = Map::new();
            m.insert("id".into(), json!(uuid::Uuid::new_v4().to_string()));
            m.insert("type".into(), json!(kind));
            m.insert("createdAt".into(), json!(now()));
            m
        }
    };
    for f in if kind == "login" {
        LOGIN_FIELDS
    } else {
        CONTACT_FIELDS
    } {
        let Some(v) = input.get(*f) else { continue };
        let value = match *f {
            "password" => json!(clean(v)),
            "botCanRead" => json!(v == &Value::Bool(true)),
            "kind" => json!(v
                .as_str()
                .filter(|k| matches!(*k, "home" | "work" | "other"))
                .unwrap_or("other")),
            _ => json!(clean(v).trim()),
        };
        out.insert((*f).into(), value);
    }
    if kind == "login" {
        normalize_repeated(input, &mut out, "website")?;
        let website = out
            .get("website")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let title = out
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if website.is_empty() && title.is_empty() {
            return Err(ApiError::bad_request("A login needs a website or a title."));
        }
        if title.is_empty() {
            let d = site_domain(&website);
            out.insert(
                "title".into(),
                json!(if d.is_empty() { website } else { d }),
            );
        }
    } else {
        normalize_repeated(input, &mut out, "email")?;
        normalize_repeated(input, &mut out, "phone")?;
        out.entry("kind").or_insert(json!("home"));
        if out
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
        {
            let pick = ["label", "name", "email"].iter().find_map(|k| {
                out.get(*k)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            });
            out.insert(
                "title".into(),
                json!(pick.unwrap_or_else(|| "Contact info".into())),
            );
        }
    }
    out.insert("updatedAt".into(), json!(now()));
    Ok(Value::Object(out))
}

/// What the app list and Bots may see: everything except the password.
pub fn redact(item: &Value) -> Value {
    if item["type"] != "login" {
        return item.clone();
    }
    let mut m = item.as_object().cloned().unwrap_or_default();
    let has = m
        .remove("password")
        .and_then(|p| p.as_str().map(|s| !s.is_empty()))
        .unwrap_or(false);
    let domain = site_domain(item["website"].as_str().unwrap_or(""));
    m.insert("domain".into(), json!(domain));
    m.insert("hasPassword".into(), json!(has));
    Value::Object(m)
}

impl Vault {
    pub fn new(file: PathBuf, key_file: PathBuf) -> Self {
        Self {
            file,
            key_file,
            items: Mutex::new(None),
        }
    }

    async fn key(&self, create: bool) -> ApiResult<[u8; 32]> {
        match tokio::fs::read(&self.key_file).await {
            #[allow(
                clippy::map_err_ignore,
                reason = "TryFrom's error is the key bytes themselves"
            )]
            Ok(k) => k.try_into().map_err(|_| ApiError::new(500, "The vault key file is not a 32-byte key.")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                let key: [u8; 32] = rand::random();
                crate::store::write_atomic(&self.key_file, &key, Some(0o600)).await?;
                Ok(key)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ApiError::new(500, "The vault key file is missing, so the vault can't be opened. Restore it (VAULT_KEY_FILE)."))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn encrypt(key: &[u8; 32], payload: &Value) -> Value {
        let cipher = Aes256Gcm::new(key.into());
        let iv: [u8; 12] = rand::random();
        let plain = serde_json::to_vec(payload).expect("JSON values serialize");
        let mut sealed = cipher
            .encrypt(
                Nonce::from_slice(&iv),
                Payload {
                    msg: &plain,
                    aad: AAD,
                },
            )
            .expect("AES-GCM encrypts");
        // aes-gcm appends the 16-byte tag; this envelope keeps the tag in its own field.
        let tag = sealed.split_off(sealed.len() - 16);
        json!({ "format": FORMAT, "version": 1, "cipher": "aes-256-gcm", "iv": B64.encode(iv), "tag": B64.encode(tag), "data": B64.encode(sealed) })
    }

    #[allow(
        clippy::map_err_ignore,
        reason = "a decrypt or parse failure can carry vault plaintext; the client only hears that the vault is damaged"
    )]
    pub fn decrypt(key: &[u8; 32], envelope: &Value) -> ApiResult<Value> {
        if envelope["format"] != FORMAT {
            return Err(ApiError::new(500, "This file is not a Bot vault."));
        }
        if envelope["version"] != 1 || envelope["cipher"] != "aes-256-gcm" {
            return Err(ApiError::new(
                500,
                format!("Unsupported vault version {}.", envelope["version"]),
            ));
        }
        let damaged = || {
            ApiError::new(500, "Couldn't decrypt the vault. The key file doesn't match the vault file, or the file is damaged.")
        };
        let field = |k: &str| {
            envelope[k]
                .as_str()
                .and_then(|s| B64.decode(s).ok())
                .ok_or_else(damaged)
        };
        let (iv, tag, mut data) = (field("iv")?, field("tag")?, field("data")?);
        if iv.len() != 12 {
            return Err(damaged());
        }
        data.extend_from_slice(&tag);
        let plain = Aes256Gcm::new(key.into())
            .decrypt(
                Nonce::from_slice(&iv),
                Payload {
                    msg: &data,
                    aad: AAD,
                },
            )
            .map_err(|_| damaged())?;
        serde_json::from_slice(&plain).map_err(|_| damaged())
    }

    async fn load<'a>(&self, guard: &'a mut Option<Vec<Value>>) -> ApiResult<&'a mut Vec<Value>> {
        if guard.is_none() {
            let items = match tokio::fs::read_to_string(&self.file).await {
                Ok(text) => {
                    #[allow(
                        clippy::map_err_ignore,
                        reason = "serde quotes the vault file, which is the encrypted secret store"
                    )]
                    let envelope: Value = serde_json::from_str(&text)
                        .map_err(|_| ApiError::new(500, "This file is not a Bot vault."))?;
                    let payload = Self::decrypt(&self.key(false).await?, &envelope)?;
                    payload["items"].as_array().cloned().unwrap_or_default()
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(e) => return Err(e.into()),
            };
            *guard = Some(items);
        }
        Ok(guard.as_mut().expect("just loaded"))
    }

    async fn save(&self, items: &[Value]) -> ApiResult<()> {
        // A key is only ever created together with a new vault file.
        let create = !tokio::fs::try_exists(&self.file).await.unwrap_or(false);
        let key = self.key(create).await?;
        let envelope = Self::encrypt(&key, &json!({ "items": items, "updatedAt": now() }));
        crate::store::write_atomic(&self.file, envelope.to_string().as_bytes(), Some(0o600))
            .await?;
        Ok(())
    }

    pub async fn list(&self) -> ApiResult<Vec<Value>> {
        let mut g = self.items.lock().await;
        Ok(self.load(&mut g).await?.iter().map(redact).collect())
    }

    pub async fn get(&self, id: &str, reveal: bool) -> ApiResult<Value> {
        let mut g = self.items.lock().await;
        let item = self
            .load(&mut g)
            .await?
            .iter()
            .find(|i| i["id"] == id)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Not found"))?;
        Ok(if reveal { item } else { redact(&item) })
    }

    pub async fn create(&self, input: &Value) -> ApiResult<Value> {
        let mut g = self.items.lock().await;
        let items = self.load(&mut g).await?;
        if items.len() >= MAX_ITEMS {
            return Err(ApiError::bad_request("The vault is full."));
        }
        let item = normalize(input, None)?;
        let mut next = items.clone();
        next.push(item.clone());
        self.save(&next).await?;
        *items = next;
        Ok(redact(&item))
    }

    pub async fn update(&self, id: &str, input: &Value) -> ApiResult<Value> {
        let mut g = self.items.lock().await;
        let items = self.load(&mut g).await?;
        let pos = items
            .iter()
            .position(|i| i["id"] == id)
            .ok_or_else(|| ApiError::not_found("Not found"))?;
        let mut patch = input.clone();
        // The password is kept unless one is sent.
        if patch
            .get("password")
            .is_some_and(|p| p.is_null() || p == "")
        {
            patch.as_object_mut().map(|m| m.remove("password"));
        }
        let item = normalize(&patch, Some(&items[pos]))?;
        let mut next = items.clone();
        next[pos] = item.clone();
        self.save(&next).await?;
        *items = next;
        Ok(redact(&item))
    }

    pub async fn delete(&self, id: &str) -> ApiResult<()> {
        let mut g = self.items.lock().await;
        let items = self.load(&mut g).await?;
        let next: Vec<Value> = items.iter().filter(|i| i["id"] != id).cloned().collect();
        if next.len() == items.len() {
            return Err(ApiError::not_found("Not found"));
        }
        self.save(&next).await?;
        *items = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips() {
        let key = [7u8; 32];
        let env = Vault::encrypt(&key, &json!({ "items": [{ "id": "a" }] }));
        assert_eq!(Vault::decrypt(&key, &env).unwrap()["items"][0]["id"], "a");
        assert!(Vault::decrypt(&[8u8; 32], &env).is_err());
    }

    #[test]
    fn decrypts_a_vault_written_by_node() {
        // Fixture ciphertext. Key is 32 bytes of 0x07. See tests/fixtures/node-vault.json.
        let envelope: Value =
            serde_json::from_str(include_str!("../tests/fixtures/node-vault.json")).unwrap();
        let payload = Vault::decrypt(&[7u8; 32], &envelope).unwrap();
        assert_eq!(payload["items"][0]["password"], "from-node");
    }

    #[test]
    fn site_matching_refuses_lookalikes() {
        assert!(matches_site("https://accounts.google.com/x", "google.com"));
        assert!(!matches_site("https://google.com.evil.io/", "google.com"));
        assert!(!matches_site("https://accounts-google.com/", "google.com"));
        assert!(!matches_site("http://google.com/", "google.com"));
        assert_eq!(site_domain("https://www.GitHub.com/login"), "github.com");
    }

    #[tokio::test]
    async fn create_redacts_and_reveal_returns_password() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let item = v.create(&json!({ "type": "login", "website": "github.com", "username": "me", "password": "s3cret" })).await.unwrap();
        assert!(item.get("password").is_none());
        assert_eq!(item["hasPassword"], true);
        assert_eq!(item["website"], "https://github.com");
        let id = item["id"].as_str().unwrap();
        assert_eq!(v.get(id, true).await.unwrap()["password"], "s3cret");
        v.update(id, &json!({ "username": "you", "password": "" }))
            .await
            .unwrap();
        // A fresh instance reads the file back with the same key.
        let again = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
        let got = again.get(id, true).await.unwrap();
        assert_eq!(
            (got["username"].as_str(), got["password"].as_str()),
            (Some("you"), Some("s3cret"))
        );
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
    }
}
