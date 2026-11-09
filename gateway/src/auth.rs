//! Sign-in, ported from `server/gateway.mts`: an HMAC-signed `bot_session` cookie in the same
//! `v1.<exp>.<sig>` format (so sessions survive switching between the two gateways), a login
//! lockout after five failures, and the same-origin check for anything that changes state.
use axum::http::{HeaderMap, Method};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

pub const COOKIE: &str = "bot_session";
const SESSION_MS: u128 = 30 * 24 * 3600 * 1000;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

pub struct Auth {
    secret: Mutex<String>,
    secret_path: PathBuf,
    password: String,
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
    allowed_origins: Vec<String>,
}

pub fn safe_equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl Auth {
    pub fn load(
        app_dir: &std::path::Path,
        password: String,
        allowed_origins: Vec<String>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(app_dir)?;
        let secret_path = app_dir.join("session-secret");
        let secret = match std::fs::read_to_string(&secret_path) {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => {
                let s = hex::encode(rand::random::<[u8; 32]>());
                write_private(&secret_path, &s)?;
                s
            }
        };
        Ok(Self {
            secret: Mutex::new(secret),
            secret_path,
            password,
            attempts: Mutex::default(),
            allowed_origins,
        })
    }

    pub fn password_required(&self) -> bool {
        !self.password.is_empty()
    }

    pub fn password_matches(&self, given: &str) -> bool {
        !self.password_required() || safe_equal(given, &self.password)
    }

    fn sign(&self, value: &str) -> String {
        let secret = self.secret.lock().unwrap().clone();
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key");
        mac.update(value.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    pub fn make_session(&self) -> String {
        let v = format!("v1.{}", now_ms() + SESSION_MS);
        format!("{v}.{}", self.sign(&v))
    }

    pub fn valid_session(&self, raw: &str) -> bool {
        let Some(i) = raw.rfind('.') else {
            return false;
        };
        let (v, sig) = (&raw[..i], &raw[i + 1..]);
        if !safe_equal(sig, &self.sign(v)) {
            return false;
        }
        v.split('.')
            .nth(1)
            .and_then(|e| e.parse::<u128>().ok())
            .is_some_and(|exp| exp > now_ms())
    }

    /// A new password signs everyone out.
    pub fn rotate(&self) -> std::io::Result<()> {
        let s = hex::encode(rand::random::<[u8; 32]>());
        write_private(&self.secret_path, &s)?;
        *self.secret.lock().unwrap() = s;
        Ok(())
    }

    pub fn authed(&self, headers: &HeaderMap) -> bool {
        if !self.password_required() {
            // Same as hbots without a password or Cloudflare Access: an open, local-only setup.
            return true;
        }
        cookie(headers, COOKIE).is_some_and(|c| self.valid_session(&c))
    }

    pub fn login_blocked(&self, ip: &str) -> bool {
        let attempts = self.attempts.lock().unwrap();
        attempts
            .get(ip)
            .is_some_and(|(n, last)| *n >= 5 && last.elapsed() < LOCKOUT)
    }

    pub fn login_failed(&self, ip: &str) {
        let mut attempts = self.attempts.lock().unwrap();
        let entry = attempts
            .entry(ip.to_string())
            .or_insert((0, Instant::now()));
        if entry.1.elapsed() > LOCKOUT {
            entry.0 = 0;
        }
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    pub fn login_ok(&self, ip: &str) {
        self.attempts.lock().unwrap().remove(ip);
    }

    /// Browsers attach cookies to cross-site requests and don't apply the same-origin policy to
    /// WebSockets, so a change must also come from this app's own pages.
    pub fn same_origin(&self, headers: &HeaderMap) -> bool {
        let Some(origin) = headers.get("origin").and_then(|o| o.to_str().ok()) else {
            return headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
                != Some("cross-site");
        };
        if self.allowed_origins.iter().any(|o| o == origin) {
            return true;
        }
        let Ok(parsed) = url::Url::parse(origin) else {
            return false;
        };
        if matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
            return true;
        }
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let origin_host = match parsed.port() {
            Some(p) => format!("{}:{p}", parsed.host_str().unwrap_or("")),
            None => parsed.host_str().unwrap_or("").to_string(),
        };
        origin_host == host
    }
}

pub fn needs_origin_check(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD)
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    raw.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

pub fn session_cookie(value: &str, https: bool) -> String {
    let secure = if https { "; Secure" } else { "" };
    format!(
        "{COOKIE}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{secure}",
        30 * 24 * 3600
    )
}

pub fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(password: &str) -> Auth {
        let dir = tempfile::tempdir().unwrap();
        Auth::load(dir.keep().as_path(), password.into(), vec![]).unwrap()
    }

    #[test]
    fn sessions_round_trip_and_rotate() {
        let a = auth("pw");
        let s = a.make_session();
        assert!(a.valid_session(&s));
        assert!(!a.valid_session(&(s.clone() + "0")));
        a.rotate().unwrap();
        assert!(!a.valid_session(&s));
    }

    #[test]
    fn lockout_after_five_failures() {
        let a = auth("pw");
        for _ in 0..5 {
            a.login_failed("1.2.3.4");
        }
        assert!(a.login_blocked("1.2.3.4"));
        assert!(!a.login_blocked("5.6.7.8"));
    }

    #[test]
    fn origin_rules() {
        let a = auth("");
        let mut h = HeaderMap::new();
        h.insert("host", "bot.example.com".parse().unwrap());
        assert!(a.same_origin(&h));
        h.insert("origin", "https://bot.example.com".parse().unwrap());
        assert!(a.same_origin(&h));
        h.insert("origin", "https://evil.example".parse().unwrap());
        assert!(!a.same_origin(&h));
    }
}
