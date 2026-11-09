//! Cloudflare Access (RS256). The team name and audience come from the environment; the
//! signing keys are fetched from the team's JWKS and cached. A token must match the audience,
//! the issuer and the expiry, and it must carry an email (optionally on an allow-list).
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Policy<'a> {
    pub audience: &'a str,
    pub issuer: &'a str,
    /// Empty means any email on a token that already passed the checks above.
    pub emails: &'a [String],
}

#[derive(Deserialize)]
struct EmailClaim {
    email: Option<String>,
}

/// `team` is either a Cloudflare team name or a full host.
pub fn issuer_for(team: &str) -> String {
    let host = if team.contains('.') {
        team.to_string()
    } else {
        format!("{team}.cloudflareaccess.com")
    };
    format!("https://{host}")
}

pub fn jwks_url(issuer: &str) -> String {
    format!("{issuer}/cdn-cgi/access/certs")
}

/// Check one JWT against an already-loaded JWKS document (`{"keys":[...]}`).
pub fn verify_token(token: &str, jwks: &Value, policy: &Policy<'_>) -> Result<String, String> {
    if policy.audience.is_empty() || policy.issuer.is_empty() {
        return Err("Cloudflare Access is not configured".into());
    }
    let header = decode_header(token).map_err(|_| "The access token is not a JWT.".to_string())?;
    if header.alg != Algorithm::RS256 {
        return Err("The access token must be signed with RS256.".into());
    }
    let keys = jwks["keys"].as_array().ok_or("The key set has no keys.")?;
    let key = match header.kid.as_deref() {
        Some(kid) => keys.iter().find(|k| k["kid"].as_str() == Some(kid)),
        None => keys.iter().find(|k| k["kty"].as_str() == Some("RSA")),
    }
    .ok_or("No signing key matches this token.")?;
    let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(key.clone())
        .map_err(|_| "The signing key is not a JWK.".to_string())?;
    let decoding =
        DecodingKey::from_jwk(&jwk).map_err(|_| "The signing key is not RSA.".to_string())?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[policy.audience]);
    validation.set_issuer(&[policy.issuer]);
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.leeway = 0;
    let data = decode::<EmailClaim>(token, &decoding, &validation)
        .map_err(|_| "The access token was rejected.".to_string())?;
    let email = data
        .claims
        .email
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err("The access token has no email.".into());
    }
    if !policy.emails.is_empty()
        && !policy
            .emails
            .iter()
            .any(|e| e.trim().eq_ignore_ascii_case(&email))
    {
        return Err("That email is not allowed in.".into());
    }
    Ok(email)
}

/// One JWKS document, refreshed after `ttl`.
pub struct JwksCache {
    url: String,
    ttl: Duration,
    client: reqwest::Client,
    slot: Mutex<Option<(Instant, Value)>>,
}

impl JwksCache {
    pub fn new(url: String, ttl: Duration) -> Self {
        Self {
            url,
            ttl,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            slot: Mutex::new(None),
        }
    }

    pub fn off() -> Self {
        Self::new(String::new(), Duration::from_secs(3600))
    }

    fn fresh(&self) -> Option<Value> {
        let slot = self.slot.lock().unwrap();
        slot.as_ref()
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, keys)| keys.clone())
    }

    pub async fn get(&self) -> Result<Value, String> {
        if self.url.is_empty() {
            return Err("Cloudflare Access is not configured".into());
        }
        if let Some(keys) = self.fresh() {
            return Ok(keys);
        }
        let body: Value = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|_| "The signing keys could not be fetched.".to_string())?
            .json()
            .await
            .map_err(|_| "The signing keys were not JSON.".to_string())?;
        *self.slot.lock().unwrap() = Some((Instant::now(), body.clone()));
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;
    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    struct Mint<'a> {
        iss: &'a str,
        aud: &'a str,
        exp: u64,
        email: &'a str,
    }

    struct Key {
        pem: String,
        jwk: Value,
    }

    fn key(kid: &str) -> Key {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("rsa");
        let pem = private
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
            .to_string();
        let n = URL_SAFE_NO_PAD.encode(private.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(private.e().to_bytes_be());
        Key {
            pem,
            jwk: json!({ "kty": "RSA", "kid": kid, "alg": "RS256", "use": "sig", "n": n, "e": e }),
        }
    }

    fn sign(key: &Key, kid: &str, claims: &Mint<'_>) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.into());
        encode(
            &header,
            claims,
            &EncodingKey::from_rsa_pem(key.pem.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn policy<'a>(emails: &'a [String]) -> Policy<'a> {
        Policy {
            audience: "aud-1",
            issuer: "https://team.cloudflareaccess.com",
            emails,
        }
    }

    #[test]
    fn a_token_signed_by_the_jwks_is_accepted() {
        let key = key("k1");
        let jwks = json!({ "keys": [key.jwk] });
        let exp = jsonwebtoken::get_current_timestamp() + 3600;
        let token = sign(
            &key,
            "k1",
            &Mint {
                iss: "https://team.cloudflareaccess.com",
                aud: "aud-1",
                exp,
                email: "Ada@Example.com",
            },
        );
        let email = verify_token(&token, &jwks, &policy(&[])).unwrap();
        assert_eq!(email, "ada@example.com");
    }

    fn claims<'a>(exp: u64, aud: &'a str, iss: &'a str, email: &'a str) -> Mint<'a> {
        Mint {
            iss,
            aud,
            exp,
            email,
        }
    }

    #[test]
    fn expiry_audience_issuer_email_and_the_wrong_key_are_refused() {
        let good = key("good");
        let other = key("other");
        let jwks = json!({ "keys": [good.jwk.clone(), other.jwk] });
        let now = jsonwebtoken::get_current_timestamp();
        let iss = "https://team.cloudflareaccess.com";
        let allowed = vec!["ada@example.com".to_string()];
        let ok = sign(
            &good,
            "good",
            &claims(now + 3600, "aud-1", iss, "ada@example.com"),
        );
        assert!(verify_token(&ok, &jwks, &policy(&allowed)).is_ok());
        let expired = sign(
            &good,
            "good",
            &claims(now - 3600, "aud-1", iss, "ada@example.com"),
        );
        assert!(verify_token(&expired, &jwks, &policy(&[])).is_err());
        let wrong_aud = sign(
            &good,
            "good",
            &claims(now + 3600, "other", iss, "ada@example.com"),
        );
        assert!(verify_token(&wrong_aud, &jwks, &policy(&[])).is_err());
        let wrong_iss = sign(
            &good,
            "good",
            &claims(
                now + 3600,
                "aud-1",
                "https://evil.example",
                "ada@example.com",
            ),
        );
        assert!(verify_token(&wrong_iss, &jwks, &policy(&[])).is_err());
        let stranger = sign(
            &good,
            "good",
            &claims(now + 3600, "aud-1", iss, "stranger@example.com"),
        );
        assert!(verify_token(&stranger, &jwks, &policy(&allowed)).is_err());
        // Signed by a key that is not the one named in the header.
        let swapped = sign(
            &other,
            "good",
            &claims(now + 3600, "aud-1", iss, "ada@example.com"),
        );
        assert!(verify_token(&swapped, &jwks, &policy(&[])).is_err());
        assert!(verify_token("not-a-jwt", &jwks, &policy(&[])).is_err());
    }

    #[test]
    fn the_key_cache_expires() {
        let cache = JwksCache::new("http://127.0.0.1/jwks".into(), Duration::ZERO);
        *cache.slot.lock().unwrap() = Some((Instant::now(), json!({ "keys": [] })));
        assert!(cache.fresh().is_none());
        let held = JwksCache::new("http://127.0.0.1/jwks".into(), Duration::from_secs(3600));
        *held.slot.lock().unwrap() = Some((Instant::now(), json!({ "keys": [] })));
        assert!(held.fresh().is_some());
    }

    #[test]
    fn team_names_become_an_issuer() {
        assert_eq!(issuer_for("hbots"), "https://hbots.cloudflareaccess.com");
        assert_eq!(
            jwks_url(&issuer_for("hbots")),
            "https://hbots.cloudflareaccess.com/cdn-cgi/access/certs"
        );
    }
}
