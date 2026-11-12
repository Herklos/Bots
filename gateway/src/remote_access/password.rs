use super::*;

pub(crate) const MIN_PASSWORD: usize = 8;

// ---------- errors ----------

/// A failed request: a status and a message that is safe to show, plus a code the app can act on.
#[derive(Debug)]
pub struct AccessError {
    pub status: u16,
    pub code: Option<&'static str>,
    pub message: String,
}

impl AccessError {
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: 409,
            code: None,
            message: message.into(),
        }
    }
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: 409,
            code: Some(code),
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            code: None,
            message: message.into(),
        }
    }
    pub fn internal(e: impl std::fmt::Display) -> Self {
        Self {
            status: 500,
            code: None,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for AccessError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.message });
        if let Some(code) = self.code {
            body["code"] = json!(code);
        }
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (
            status,
            [(header::X_CONTENT_TYPE_OPTIONS, "nosniff")],
            Json(body),
        )
            .into_response()
    }
}

// ---------- access.json ----------

/// A saved password: its scrypt hash and salt, base64.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredPassword {
    pub salt: String,
    pub hash: String,
    pub set_at: String,
}

// ---------- passwords ----------

/// scrypt with Node's defaults (N=16384, r=8, p=1),.
pub(crate) fn scrypt_bytes(password: &[u8], salt: &[u8], len: usize) -> anyhow::Result<Vec<u8>> {
    // The scrypt crate's error types don't implement std::error::Error without its std feature.
    let params =
        scrypt::Params::new(14, 8, 1, len).map_err(|e| anyhow::anyhow!("scrypt params: {e:?}"))?;
    let mut out = vec![0u8; len];
    scrypt::scrypt(password, salt, &params, &mut out)
        .map_err(|e| anyhow::anyhow!("scrypt: {e:?}"))?;
    Ok(out)
}

pub(crate) async fn hash_password(password: String) -> anyhow::Result<StoredPassword> {
    tokio::task::spawn_blocking(move || {
        let salt: [u8; 16] = rand::random();
        let hash = scrypt_bytes(password.as_bytes(), &salt, 32)?;
        Ok(StoredPassword {
            salt: B64.encode(salt),
            hash: B64.encode(hash),
            set_at: iso_now(),
        })
    })
    .await?
}

pub(crate) async fn verify_password(password: String, stored: StoredPassword) -> bool {
    tokio::task::spawn_blocking(move || {
        let (Ok(salt), Ok(expected)) = (B64.decode(&stored.salt), B64.decode(&stored.hash)) else {
            return false;
        };
        if salt.is_empty() || expected.is_empty() {
            return false;
        }
        scrypt_bytes(password.as_bytes(), &salt, expected.len())
            .map(|actual| bool::from(actual.ct_eq(&expected)))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}
