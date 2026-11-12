//! What the key reaches, and whether the chosen space is private.

use super::http::{legacy_client, legacy_json, legacy_unreachable, API_VERSION};
use crate::platform::error::ApiResult;
use serde_json::{json, Value};

async fn get(host: &str, key: &str, path: &str) -> ApiResult<Value> {
    let response = legacy_client()?
        .get(format!("{host}{path}"))
        .header("Anytype-Version", API_VERSION)
        .bearer_auth(key)
        .send()
        .await
        .map_err(legacy_unreachable)?;
    legacy_json(response, "the request").await
}

/// What the key reaches and whether the chosen space is private. No secrets in the result.
pub(crate) async fn status(host: &str, key: &str, space_name: &str) -> ApiResult<Value> {
    let who = get(host, key, "/v2/auth/whoami?ids=full").await?;
    let spaces = get(host, key, "/v2/spaces?ids=full").await?;
    let names: Vec<Value> = spaces["data"]
        .as_array()
        .map(|rows| rows.iter().map(|row| row["name"].clone()).collect())
        .unwrap_or_default();
    let chosen = spaces["data"].as_array().and_then(|rows| {
        rows.iter()
            .find(|row| row["name"].as_str() == Some(space_name))
    });
    let (space_found, shared) = match chosen {
        Some(row) => {
            let id = row["id"].as_str().unwrap_or_default();
            let members = get(host, key, &format!("/v2/spaces/{id}/members")).await?;
            let count = members["data"].as_array().map(Vec::len).unwrap_or(0);
            (true, Some(count > 1))
        }
        None => (false, None),
    };
    Ok(json!({
        "reachable": true,
        "api": who["api"]["version"],
        "keyName": who["key"]["name"],
        "scoped": who["grant"]["scoped"],
        "restricted": who["grant"]["restricted"],
        "permission": who["grant"]["permission"],
        "spaces": names,
        "hasNotGrantedSpaces": spaces["has_not_granted_spaces"],
        "space": space_name,
        "spaceFound": space_found,
        "shared": shared,
    }))
}
