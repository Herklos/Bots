//! Cloudflare Access, ported from `server/access.mts`. A request that carries a Cloudflare Access
//! token (the `Cf-Access-Jwt-Assertion` header or the `CF_Authorization` cookie) for an allowed
//! email skips the app password. The token is an RS256 JWT: its signature is checked against the
//! team's public keys (JWKS, cached for an hour, refetched at most every 30 seconds for an unknown
//! key id), then its audience, issuer, expiry, not-before and email.
use crate::auth;
use axum::http::HeaderMap;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rsa::{BigUint, Pkcs1v15Sign, RsaPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const KEY_TTL: Duration = Duration::from_secs(60 * 60);
const FORCE_MIN_GAP: Duration = Duration::from_secs(30);
/// Seconds of clock skew allowed on `exp` and `nbf`.
const SKEW_SECS: i64 = 30;
const MAX_TOKEN_LEN: usize = 8192;

pub struct Access {
    issuer: String,
    certs_url: String,
    aud: String,
    emails: Vec<String>,
    keys: Mutex<Keys>,
    client: reqwest::Client,
}

#[derive(Default)]
struct Keys {
    by_kid: HashMap<String, RsaPublicKey>,
    fetched_at: Option<Instant>,
}

impl Access {
    /// Cloudflare Access from `CF_ACCESS_TEAM_DOMAIN`, `CF_ACCESS_AUD` and `CF_ACCESS_EMAILS`.
    /// Off unless all three are set.
    pub fn from_env(team_domain: &str, aud: &str, emails: &str) -> Option<Self> {
        let emails: Vec<String> = emails
            .split(',')
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
        if team_domain.trim().is_empty() || aud.trim().is_empty() || emails.is_empty() {
            return None;
        }
        let issuer = format!("https://{}", team_domain.trim());
        let certs_url = format!("{issuer}/cdn-cgi/access/certs");
        Some(Self::new(issuer, certs_url, aud.trim().to_string(), emails))
    }

    pub fn new(issuer: String, certs_url: String, aud: String, emails: Vec<String>) -> Self {
        Self {
            issuer,
            certs_url,
            aud,
            emails: emails.into_iter().map(|e| e.to_lowercase()).collect(),
            keys: Mutex::new(Keys::default()),
            client: reqwest::Client::new(),
        }
    }

    /// The allowed emails, lowercased, as Settings lists them.
    pub fn emails(&self) -> &[String] {
        &self.emails
    }

    /// The verified, lowercased email of a valid token for an allowed user, else `None`.
    pub async fn verify(&self, token: &str) -> Option<String> {
        if token.len() > MAX_TOKEN_LEN {
            return None;
        }
        let parts: Vec<&str> = token.split('.').collect();
        let [head, body, sig] = parts.as_slice() else {
            return None;
        };
        let header = decode_json(head)?;
        let claims = decode_json(body)?;
        if header["alg"] != "RS256" {
            return None;
        }
        let kid = header["kid"].as_str().filter(|k| !k.is_empty())?;
        let key = match self.key_for(kid).await {
            Ok(Some(key)) => key,
            Ok(None) => return None,
            Err(e) => {
                tracing::error!("[access] could not load certs: {e:#}");
                return None;
            }
        };
        let signature = URL_SAFE_NO_PAD.decode(sig).ok()?;
        let digest = Sha256::digest(format!("{head}.{body}").as_bytes());
        key.verify(sha256_scheme(), &digest, &signature).ok()?;
        self.check_claims(&claims, now_secs())
    }

    /// The claims checks that run after the signature: audience, issuer, expiry, not-before, email.
    pub fn check_claims(&self, claims: &Value, now: i64) -> Option<String> {
        let aud_ok = match &claims["aud"] {
            Value::Array(list) => list.iter().any(|a| a.as_str() == Some(self.aud.as_str())),
            other => other.as_str() == Some(self.aud.as_str()),
        };
        if !aud_ok || claims["iss"].as_str() != Some(self.issuer.as_str()) {
            return None;
        }
        let exp = claims["exp"].as_i64()?;
        if exp + SKEW_SECS < now {
            return None;
        }
        if let Some(nbf) = claims["nbf"].as_i64() {
            if nbf - SKEW_SECS > now {
                return None;
            }
        }
        let email = claims["email"].as_str()?.trim().to_lowercase();
        self.emails.contains(&email).then_some(email)
    }

    async fn key_for(&self, kid: &str) -> anyhow::Result<Option<RsaPublicKey>> {
        self.load(false).await?;
        if !self.has_key(kid) {
            self.load(true).await?;
        }
        Ok(self.keys.lock().unwrap().by_kid.get(kid).cloned())
    }

    fn has_key(&self, kid: &str) -> bool {
        self.keys.lock().unwrap().by_kid.contains_key(kid)
    }

    /// Fetch the team's keys unless they are fresh. A forced load (an unknown key id) waits at least
    /// 30 seconds since the last fetch, so a stream of bad tokens can't hammer the certs endpoint.
    async fn load(&self, force: bool) -> anyhow::Result<()> {
        {
            let keys = self.keys.lock().unwrap();
            let age = keys.fetched_at.map(|t| t.elapsed());
            if !force && !keys.by_kid.is_empty() && age.is_some_and(|a| a < KEY_TTL) {
                return Ok(());
            }
            if force && age.is_some_and(|a| a < FORCE_MIN_GAP) {
                return Ok(());
            }
        }
        let res = self
            .client
            .get(&self.certs_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        if !res.status().is_success() {
            anyhow::bail!("Access certs {}", res.status().as_u16());
        }
        let body: Value = res.json().await?;
        let next = parse_jwks(&body);
        let mut keys = self.keys.lock().unwrap();
        keys.by_kid = next;
        keys.fetched_at = Some(Instant::now());
        Ok(())
    }
}

/// PKCS#1 v1.5 with SHA-256: the DigestInfo prefix, written out so the `sha2` OID feature isn't needed.
fn sha256_scheme() -> Pkcs1v15Sign {
    const DIGEST_INFO_SHA256: &[u8] = &[
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];
    Pkcs1v15Sign {
        hash_len: Some(32),
        prefix: DIGEST_INFO_SHA256.into(),
    }
}

/// RSA public keys from a JWKS document, by key id. A key that doesn't parse is skipped.
pub(crate) fn parse_jwks(body: &Value) -> HashMap<String, RsaPublicKey> {
    let mut out = HashMap::new();
    for jwk in body["keys"].as_array().into_iter().flatten() {
        let (Some(kid), Some(n), Some(e)) =
            (jwk["kid"].as_str(), jwk["n"].as_str(), jwk["e"].as_str())
        else {
            continue;
        };
        let (Ok(n), Ok(e)) = (URL_SAFE_NO_PAD.decode(n), URL_SAFE_NO_PAD.decode(e)) else {
            continue;
        };
        if let Ok(key) = RsaPublicKey::new(BigUint::from_bytes_be(&n), BigUint::from_bytes_be(&e)) {
            out.insert(kid.to_string(), key);
        }
    }
    out
}

fn decode_json(part: &str) -> Option<Value> {
    let bytes = URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The Access token of a request: its `Cf-Access-Jwt-Assertion` header, else the `CF_Authorization` cookie.
pub(crate) fn token_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cf-access-jwt-assertion")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(String::from)
        .or_else(|| auth::cookie(headers, "CF_Authorization").filter(|v| !v.is_empty()))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderValue, routing::get, Json, Router};
    use rsa::traits::PublicKeyParts;

    const TEAM: &str = "example.cloudflareaccess.com";
    const AUD: &str = "test-aud";

    /// Two RSA private keys made by Node (`gateway/tests/fixtures/t4`): the signing key and another,
    /// for forgeries. Loaded from their parts, since generating keys in a debug build is too slow.
    fn keys() -> (rsa::RsaPrivateKey, rsa::RsaPrivateKey) {
        let load = |text: &str| {
            let v: Value = serde_json::from_str(text).unwrap();
            let num = |k: &str| {
                BigUint::from_bytes_be(&URL_SAFE_NO_PAD.decode(v[k].as_str().unwrap()).unwrap())
            };
            rsa::RsaPrivateKey::from_components(
                num("n"),
                num("e"),
                num("d"),
                vec![num("p"), num("q")],
            )
            .unwrap()
        };
        (
            load(include_str!("../tests/fixtures/t4/signing-key.jwk.json")),
            load(include_str!("../tests/fixtures/t4/other-key.jwk.json")),
        )
    }

    fn jwks() -> Value {
        let public = keys().0.to_public_key();
        serde_json::json!({ "keys": [{
            "kid": "k1", "kty": "RSA", "alg": "RS256",
            "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        }] })
    }

    /// A fake JWKS endpoint on a real local port, so the fetch and cache run for real.
    async fn fake_certs() -> String {
        let app = Router::new().route("/certs", get(|| async { Json(jwks()) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/certs")
    }

    fn b64(v: &Value) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(v).unwrap())
    }

    fn token(claims: Value, kid: &str, alg: &str, key: &rsa::RsaPrivateKey) -> String {
        let head = b64(&serde_json::json!({ "alg": alg, "kid": kid, "typ": "JWT" }));
        let mut body = serde_json::json!({
            "aud": [AUD], "iss": format!("https://{TEAM}"), "email": "me@example.com",
            "exp": now_secs() + 600, "iat": now_secs(), "nbf": now_secs(),
        });
        for (k, v) in claims.as_object().unwrap() {
            body[k] = v.clone();
        }
        let body = b64(&body);
        let digest = Sha256::digest(format!("{head}.{body}").as_bytes());
        let sig = key.sign(sha256_scheme(), &digest).unwrap();
        format!("{head}.{body}.{}", URL_SAFE_NO_PAD.encode(sig))
    }

    fn verifier(certs: String) -> Access {
        Access::new(
            format!("https://{TEAM}"),
            certs,
            AUD.into(),
            vec!["Me@example.com".into()],
        )
    }

    #[tokio::test]
    async fn accepts_a_valid_token_for_an_allowed_email() {
        let access = verifier(fake_certs().await);
        let t = token(serde_json::json!({}), "k1", "RS256", &keys().0);
        assert_eq!(access.verify(&t).await.as_deref(), Some("me@example.com"));
    }

    #[tokio::test]
    async fn rejects_other_emails_audiences_issuers_and_expired_tokens() {
        let access = verifier(fake_certs().await);
        let k = &keys().0;
        for claims in [
            serde_json::json!({ "email": "someone@else.com" }),
            serde_json::json!({ "aud": ["other-aud"] }),
            serde_json::json!({ "iss": "https://evil.cloudflareaccess.com" }),
            serde_json::json!({ "exp": now_secs() - 3600 }),
        ] {
            assert_eq!(
                access
                    .verify(&token(claims.clone(), "k1", "RS256", k))
                    .await,
                None,
                "{claims}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_forged_signatures_unknown_keys_and_alg_none() {
        let access = verifier(fake_certs().await);
        let forged = token(serde_json::json!({}), "k1", "RS256", &keys().1);
        assert_eq!(access.verify(&forged).await, None);
        let unknown = token(serde_json::json!({}), "nope", "RS256", &keys().0);
        assert_eq!(access.verify(&unknown).await, None);
        let good = token(serde_json::json!({}), "k1", "RS256", &keys().0);
        let [head, body, _] = good.split('.').collect::<Vec<_>>()[..] else {
            panic!()
        };
        let none = format!(
            "{}.{body}.",
            b64(&serde_json::json!({ "alg": "none", "kid": "k1" }))
        );
        assert_eq!(access.verify(&none).await, None);
        assert_eq!(access.verify(&format!("{head}.{body}.")).await, None);
        assert_eq!(access.verify("garbage").await, None);
    }

    #[tokio::test]
    async fn an_unknown_key_id_refetches_only_after_the_gap() {
        let access = verifier(fake_certs().await);
        assert!(access
            .verify(&token(serde_json::json!({}), "k1", "RS256", &keys().0))
            .await
            .is_some());
        let before = access.keys.lock().unwrap().fetched_at;
        let _ = access
            .verify(&token(serde_json::json!({}), "other", "RS256", &keys().0))
            .await;
        assert_eq!(
            access.keys.lock().unwrap().fetched_at,
            before,
            "a refetch within 30 seconds is skipped"
        );
    }

    /// A token and JWKS made by Node's crypto (`gateway/tests/fixtures/t4`), verified here.
    #[tokio::test]
    async fn a_node_made_token_verifies_against_a_node_made_jwks() {
        let jwks: Value =
            serde_json::from_str(include_str!("../tests/fixtures/t4/jwks.json")).unwrap();
        let keys = parse_jwks(&jwks);
        assert!(keys.contains_key("node-k1"));
        let node_token = include_str!("../tests/fixtures/t4/token.txt")
            .trim()
            .to_string();
        let server = Router::new().route(
            "/certs",
            get(move || {
                let jwks = jwks.clone();
                async move { Json(jwks) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
        let access = Access::new(
            "https://fixture.cloudflareaccess.com".into(),
            format!("http://{addr}/certs"),
            "fixture-aud".into(),
            vec!["me@example.com".into()],
        );
        assert_eq!(
            access.verify(&node_token).await.as_deref(),
            Some("me@example.com")
        );
        assert_eq!(
            access.verify(&format!("{node_token}x")).await,
            None,
            "a changed signature is refused"
        );
    }

    #[test]
    fn is_disabled_without_configuration() {
        assert!(Access::from_env("", AUD, "a@b.c").is_none());
        assert!(Access::from_env(TEAM, "", "a@b.c").is_none());
        assert!(Access::from_env(TEAM, AUD, " , ").is_none());
        let on = Access::from_env(TEAM, AUD, " A@b.c ,").unwrap();
        assert_eq!(on.certs_url, format!("https://{TEAM}/cdn-cgi/access/certs"));
    }

    #[test]
    fn the_token_comes_from_the_header_or_the_cookie() {
        let mut h = HeaderMap::new();
        assert_eq!(token_from(&h), None);
        h.insert(
            "cookie",
            HeaderValue::from_static("a=1; CF_Authorization=cookie.tok.x"),
        );
        assert_eq!(token_from(&h).as_deref(), Some("cookie.tok.x"));
        h.insert(
            "cf-access-jwt-assertion",
            HeaderValue::from_static("header.tok.x"),
        );
        assert_eq!(token_from(&h).as_deref(), Some("header.tok.x"));
    }

    #[test]
    fn claims_allow_thirty_seconds_of_skew() {
        let a = verifier(String::new());
        let ok = serde_json::json!({ "aud": AUD, "iss": format!("https://{TEAM}"), "email": "ME@example.com", "exp": now_secs() - 20, "nbf": now_secs() + 20 });
        assert_eq!(
            a.check_claims(&ok, now_secs()).as_deref(),
            Some("me@example.com")
        );
        let late = serde_json::json!({ "aud": AUD, "iss": format!("https://{TEAM}"), "email": "me@example.com", "exp": now_secs() - 31 });
        assert_eq!(a.check_claims(&late, now_secs()), None);
    }
}
