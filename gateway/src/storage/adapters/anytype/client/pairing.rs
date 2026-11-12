//! Pairing. Anytype Desktop shows an approval dialog, then a 4 digit code.

use super::http::{legacy_client, legacy_json, legacy_unreachable, API_VERSION, APP_NAME};
use crate::platform::error::ApiResult;
use serde_json::{json, Value};

/// Starts pairing. Anytype Desktop then shows an approval dialog.
pub(crate) async fn start_pairing(host: &str) -> ApiResult<String> {
    let response = legacy_client()?
        .post(format!("{host}/v2/auth/challenges"))
        .header("Anytype-Version", API_VERSION)
        .json(&json!({ "app_name": APP_NAME }))
        .send()
        .await
        .map_err(legacy_unreachable)?;
    let body = legacy_json(response, "the pairing request").await?;
    body["challenge_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            crate::platform::error::ApiError::new(
                502,
                "Anytype did not return a pairing challenge.",
            )
        })
}

/// Exchanges the 4 digit code for a key. Returns `(key, grant)`.
pub(crate) async fn confirm_pairing(
    host: &str,
    challenge_id: &str,
    code: &str,
) -> ApiResult<(String, Value)> {
    let response = legacy_client()?
        .post(format!("{host}/v2/auth/api_keys"))
        .header("Anytype-Version", API_VERSION)
        .json(&json!({ "challenge_id": challenge_id, "code": code }))
        .send()
        .await
        .map_err(legacy_unreachable)?;
    let body = legacy_json(response, "that code").await?;
    let key = body["api_key"]
        .as_str()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| crate::platform::error::ApiError::new(502, "Anytype did not return a key."))?
        .to_string();
    Ok((key, body["grant"].clone()))
}
