//! Where a Bot is saved to run. A linked computer is stored only.
//! This build does not send `environmentId` on project, thread, dispatch, or preview calls.

use crate::bots::{obj, Bots};
use crate::harness::{ComputerCatalog, ComputerLink};
use crate::platform::error::{ApiError, ApiResult};
use crate::vault::now_iso;
use serde_json::{json, Value};

/// `""` is this computer. A pinned id is returned even when the charset is bad, so the menu can show it.
pub(crate) fn selected_id(bot: &Value) -> String {
    let Some(env) = bot.get("environment").and_then(Value::as_object) else {
        return String::new();
    };
    if env.get("mode").and_then(Value::as_str) != Some("pinned") {
        return String::new();
    }
    env.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or("")
        .to_string()
}

/// The public place object. A down catalog is still 200, with `ok: false` and the saved id.
pub(crate) fn place_view(bot: &Value, catalog: &ComputerCatalog) -> Value {
    let selected = selected_id(bot);
    let waiting = !selected.is_empty();
    if !catalog.ok {
        return json!({
            "ok": false,
            "selected": selected,
            "localLabel": "",
            "links": [],
            "waiting": waiting,
            "error": "environment_offline",
        });
    }
    let links = catalog.links.iter().map(link_json).collect::<Vec<_>>();
    json!({
        "ok": true,
        "selected": selected,
        "localLabel": catalog.local_label,
        "links": links,
        "waiting": waiting,
        "error": "",
    })
}

fn link_json(link: &ComputerLink) -> Value {
    json!({
        "id": link.id,
        "label": link.label,
        "status": link.status,
        "access": link.access,
        "selectable": link.selectable,
    })
}

impl Bots {
    /// `id` empty writes `{mode:local}`. A link writes `{mode:pinned,id}` when that link can be chosen.
    pub(crate) async fn set_environment(
        &self,
        slug: &str,
        id: &str,
        catalog: &ComputerCatalog,
    ) -> ApiResult<Value> {
        self.dir(slug)?;
        let _guard = self.lock.lock().await;
        let _disk_guard = self.bot_store.acquire()
        .await?;
        let bots = self.reconcile().await?;
        let current = bots
            .iter()
            .find(|bot| bot["slug"] == slug)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Bot not found"))?;
        if !id.is_empty() {
            reject_pin(id, catalog)?;
        }
        let mut next = obj(&current);
        next.insert(
            "environment".into(),
            if id.is_empty() {
                json!({ "mode": "local" })
            } else {
                json!({ "mode": "pinned", "id": id })
            },
        );
        next.insert("updatedAt".into(), json!(now_iso()));
        self.persist(&Value::Object(next)).await?;
        let fresh = self
            .reconcile()
            .await?
            .into_iter()
            .find(|bot| bot["slug"] == slug)
            .ok_or_else(|| ApiError::not_found("Bot not found"))?;
        Ok(place_view(&fresh, catalog))
    }
}

fn reject_pin(id: &str, catalog: &ComputerCatalog) -> ApiResult<()> {
    if !catalog.ok {
        return Err(ApiError::new(409, "That computer is not available."));
    }
    if !pin_charset(id) || (!catalog.broker_id.is_empty() && id == catalog.broker_id) {
        return Err(ApiError::bad_request("That computer id is not valid."));
    }
    let selectable = catalog
        .links
        .iter()
        .any(|link| link.id == id && link.selectable);
    if !selectable {
        return Err(ApiError::bad_request("That computer is not available."));
    }
    Ok(())
}

/// One line, 1 to 128 characters, ASCII letters, digits, `_`, `-`. Whitespace is rejected.
fn pin_charset(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::Bots;

    fn link(id: &str, selectable: bool) -> ComputerLink {
        ComputerLink {
            id: id.into(),
            label: id.into(),
            status: if selectable {
                "reachable"
            } else {
                "unreachable"
            }
            .into(),
            access: "approval-required".into(),
            selectable,
            last_seen_at: String::new(),
            last_error: String::new(),
            laptop: false,
            version: String::new(),
        }
    }

    fn catalog() -> ComputerCatalog {
        ComputerCatalog {
            ok: true,
            local_label: "Computer".into(),
            local_version: String::new(),
            local_laptop: false,
            broker_id: "15f077d9-b158-435e-96f1-ed7709f11cbd".into(),
            links: vec![link("srv-west-1", true), link("srv-ro", false)],
        }
    }

    fn environment_bytes(file: &str) -> String {
        let key = "\"environment\": ";
        let start = file.find(key).expect("environment");
        let rest = &file[start + key.len()..];
        let mut depth = 0;
        for (index, ch) in rest.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return rest[..=index].to_string();
                    }
                }
                _ => {}
            }
        }
        panic!("environment object did not close");
    }

    #[test]
    fn an_absent_or_local_choice_reads_as_this_computer() {
        assert_eq!(selected_id(&json!({})), "");
        assert_eq!(selected_id(&json!({ "environment": "local" })), "");
        assert_eq!(
            selected_id(&json!({ "environment": { "mode": "local" } })),
            ""
        );
        assert_eq!(
            selected_id(&json!({ "environment": { "mode": "inherit" } })),
            ""
        );
        assert_eq!(
            selected_id(&json!({ "environment": { "mode": "pinned" } })),
            ""
        );
        assert_eq!(
            selected_id(&json!({ "environment": { "mode": "pinned", "id": "srv-west-1" } })),
            "srv-west-1"
        );
        assert_eq!(
            selected_id(&json!({ "environment": { "mode": "pinned", "id": "bad id" } })),
            "bad id"
        );
    }

    #[tokio::test]
    async fn saving_a_name_leaves_the_environment_bytes_alone() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let slug = ada["slug"].as_str().unwrap();
        assert_eq!(selected_id(&ada), "");
        let local = bots.set_environment(slug, "", &catalog()).await.unwrap();
        assert_eq!(local["selected"], "");
        assert_eq!(local["waiting"], false);
        let pinned = bots
            .set_environment(slug, "srv-west-1", &catalog())
            .await
            .unwrap();
        assert_eq!(pinned["selected"], "srv-west-1");
        assert_eq!(pinned["waiting"], true);
        let path = bots.dir(slug).unwrap().join(".bot").join("bot.json");
        let before = tokio::fs::read_to_string(&path).await.unwrap();
        let saved = environment_bytes(&before);
        assert!(saved.contains("\"mode\": \"pinned\""));
        assert!(saved.contains("\"id\": \"srv-west-1\""));
        bots.save(slug, &json!({ "name": "Ada Lovelace" }), false)
            .await
            .unwrap();
        let after = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(environment_bytes(&after), saved);
        bots.save(
            slug,
            &json!({ "name": "Ada", "environment": { "mode": "local" } }),
            false,
        )
        .await
        .unwrap();
        let kept = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(environment_bytes(&kept), saved);
    }

    #[tokio::test]
    async fn a_bad_pin_is_refused_and_an_offline_catalog_only_accepts_this_computer() {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        let slug = bots.create(&json!({ "name": "Bea" })).await.unwrap()["slug"]
            .as_str()
            .unwrap()
            .to_string();
        let catalog = catalog();
        for id in [
            "15f077d9-b158-435e-96f1-ed7709f11cbd",
            "bad id",
            "has space",
        ] {
            let err = bots.set_environment(&slug, id, &catalog).await.unwrap_err();
            assert_eq!(err.status, 400);
            assert_eq!(err.message, "That computer id is not valid.");
        }
        for id in ["srv-ro", "missing"] {
            let err = bots.set_environment(&slug, id, &catalog).await.unwrap_err();
            assert_eq!(err.status, 400);
            assert_eq!(err.message, "That computer is not available.");
        }
        let offline = ComputerCatalog::offline();
        let err = bots
            .set_environment(&slug, "bad id", &offline)
            .await
            .unwrap_err();
        assert_eq!(err.status, 409);
        assert_eq!(err.message, "That computer is not available.");
        let local = bots.set_environment(&slug, "", &offline).await.unwrap();
        assert_eq!(local["ok"], false);
        assert_eq!(local["selected"], "");
        assert_eq!(local["error"], "environment_offline");
        let file =
            tokio::fs::read_to_string(bots.dir(&slug).unwrap().join(".bot").join("bot.json"))
                .await
                .unwrap();
        assert!(file.contains("\"mode\": \"local\""));
        assert!(!file.contains("\"mode\": \"pinned\""));
    }
}
