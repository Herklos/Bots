//! Connected apps, recorded in `bots/.botapp/apps.json`, and the executor URL.
//!
//! The gateway stores which apps are connected and where an executor is
//! (`HBOTS_EXECUTOR_URL`, returned as `executorUrl` on `GET /api/config`). It does not
//! speak the executor protocol: that lives in the Node server checkout, which is not
//! on this machine.

use crate::error::{ApiError, ApiResult};
use crate::{parse, require_json, AppState};
use axum::{body::Bytes, extract::State, http::HeaderMap, Json};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use url::Url;

const MAX_APPS: usize = 40;

pub struct Apps {
    path: PathBuf,
}

impl Apps {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn list(&self) -> ApiResult<Value> {
        match crate::store::read_json(&self.path).await {
            Some(doc) if doc.is_object() && doc["apps"].is_array() => Ok(doc),
            Some(_) => Err(ApiError::new(500, "apps.json is not a list of apps")),
            None => Ok(json!({ "version": 1, "apps": [] })),
        }
    }

    pub async fn replace(&self, body: &Value) -> ApiResult<Value> {
        let doc = normalize(body)?;
        crate::store::write_json(&self.path, &doc).await?;
        Ok(doc)
    }
}

pub async fn get_apps(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    Ok(Json(s.apps.list().await?))
}

pub async fn put_apps(
    State(s): State<Arc<AppState>>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(s.apps.replace(&parse(&body)?).await?))
}

/// `https://host/...` with no userinfo, or empty when the value is blank or not a URL.
pub fn executor_url(raw: &str) -> String {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return String::new();
    }
    match Url::parse(raw) {
        Ok(url)
            if matches!(url.scheme(), "https" | "http")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none() =>
        {
            raw.to_string()
        }
        _ => String::new(),
    }
}

pub fn normalize(body: &Value) -> ApiResult<Value> {
    let list = body["apps"]
        .as_array()
        .ok_or_else(|| ApiError::bad_request("apps must be a list"))?;
    if list.len() > MAX_APPS {
        return Err(ApiError::bad_request(format!("at most {MAX_APPS} apps")));
    }
    let mut apps = Vec::with_capacity(list.len());
    let mut seen = std::collections::HashSet::new();
    for (index, item) in list.iter().enumerate() {
        let id = item["id"].as_str().unwrap_or("").trim();
        if !crate::bots::valid_slug(id) {
            return Err(ApiError::bad_request(format!(
                "app {} needs an id of letters, digits and dashes",
                index + 1
            )));
        }
        if !seen.insert(id.to_string()) {
            return Err(ApiError::bad_request(format!("app id {id} is repeated")));
        }
        let name = item["name"].as_str().unwrap_or("").trim();
        if name.is_empty() || name.len() > 80 || name.chars().any(|c| c.is_control()) {
            return Err(ApiError::bad_request(format!(
                "app {} needs a name",
                index + 1
            )));
        }
        let url = app_url(item["url"].as_str().unwrap_or("")).map_err(|_| {
            ApiError::bad_request(format!("app {} needs an https address", index + 1))
        })?;
        apps.push(json!({ "id": id, "name": name, "url": url }));
    }
    Ok(json!({ "version": 1, "apps": apps }))
}

fn app_url(raw: &str) -> Result<String, ()> {
    let raw = raw.trim().trim_end_matches('/');
    let url = Url::parse(raw).map_err(|_| ())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(());
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executor_url_must_be_http() {
        assert_eq!(
            executor_url(" https://exec.example/hbots/ "),
            "https://exec.example/hbots"
        );
        assert_eq!(executor_url("http://127.0.0.1:9"), "http://127.0.0.1:9");
        assert!(executor_url("").is_empty());
        assert!(executor_url("not a url").is_empty());
        assert!(executor_url("ftp://files.example/x").is_empty());
        assert!(executor_url("https://user:pw@exec.example").is_empty());
    }

    #[test]
    fn a_list_is_cleaned_and_a_bad_one_is_refused() {
        let doc = normalize(&json!({
            "apps": [
                { "id": "mail", "name": " Mail ", "url": "https://mail.example/app/" },
            ]
        }))
        .unwrap();
        assert_eq!(doc["apps"][0]["url"], "https://mail.example/app");
        assert_eq!(doc["apps"][0]["name"], "Mail");
        assert!(normalize(
            &json!({ "apps": [{ "id": "mail", "name": "Mail", "url": "http://mail.example" }] })
        )
        .is_err());
        assert!(normalize(&json!({
            "apps": [
                { "id": "mail", "name": "Mail", "url": "https://mail.example" },
                { "id": "mail", "name": "Again", "url": "https://other.example" },
            ]
        }))
        .is_err());
    }

    #[tokio::test]
    async fn a_saved_list_reads_back_and_a_broken_file_is_not_a_list() {
        let dir = tempfile::tempdir().unwrap();
        let apps = Apps::new(dir.path().join("apps.json"));
        assert_eq!(
            apps.list().await.unwrap()["apps"].as_array().unwrap().len(),
            0
        );
        apps.replace(&json!({
            "apps": [{ "id": "docs", "name": "Docs", "url": "https://docs.example" }]
        }))
        .await
        .unwrap();
        assert_eq!(apps.list().await.unwrap()["apps"][0]["id"], "docs");
        tokio::fs::write(dir.path().join("apps.json"), "{\"nope\":true}")
            .await
            .unwrap();
        assert!(apps.list().await.is_err());
    }
}
