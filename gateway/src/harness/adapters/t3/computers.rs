//! This computer, and the links T3 Code already has.
//!
//! `peerLinks.list` stays in this adapter. A link is stored only. Nothing here
//! places a Bot, and no call sends `environmentId`.

use serde_json::{json, Value};

use super::T3;
use crate::harness::{without_urls, ComputerCatalog, ComputerLink, HarnessError};

const LIST: &str = "peerLinks.list";

/// A down probe or a failed list is an offline catalog, not an error.
pub(super) async fn list(t3: &T3) -> Result<ComputerCatalog, HarnessError> {
    let probe = match t3.get_json("/.well-known/t3/environment").await {
        Ok(body) => body,
        Err(_) => return Ok(ComputerCatalog::offline()),
    };
    let listed = match t3.rpc().await {
        Ok(rpc) => rpc.call(LIST, json!({})).await.ok(),
        Err(_) => None,
    };
    Ok(catalog_of(Some(&probe), listed.as_ref()))
}

/// `probe` is `GET /.well-known/t3/environment`. `listed` is the `peerLinks.list` value.
/// Either missing means the list could not be read.
pub(crate) fn catalog_of(probe: Option<&Value>, listed: Option<&Value>) -> ComputerCatalog {
    let (Some(probe), Some(listed)) = (probe, listed) else {
        return ComputerCatalog::offline();
    };
    // `platform.machine` is part of the probe. This slice does not show it.
    let _machine = probe.pointer("/platform/machine").and_then(Value::as_str);
    let broker_id = probe["environmentId"].as_str().unwrap_or("").to_string();
    ComputerCatalog {
        ok: true,
        local_label: probe["label"].as_str().unwrap_or("").to_string(),
        local_version: version_of(probe),
        local_laptop: machine_is_laptop(machine_name(probe)),
        links: links_of(listed, &broker_id),
        broker_id,
    }
}

fn links_of(body: &Value, broker_id: &str) -> Vec<ComputerLink> {
    body["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|link| link_of(link, broker_id))
        .collect()
}

fn link_of(link: &Value, broker_id: &str) -> Option<ComputerLink> {
    let id = link["environmentId"].as_str().unwrap_or("");
    if id.is_empty() || (!broker_id.is_empty() && id == broker_id) {
        return None;
    }
    // A missing status is not the empty string the client uses for a saved id that left the list.
    let status = link["status"]
        .as_str()
        .filter(|status| !status.is_empty())
        .unwrap_or("unreachable")
        .to_string();
    let access = link["access"].as_str().unwrap_or("").to_string();
    let label = link["label"]
        .as_str()
        .filter(|label| !label.is_empty())
        .unwrap_or(id)
        .to_string();
    let selectable =
        status == "reachable" && access != "read-only" && pin_charset(id) && id != broker_id;
    Some(ComputerLink {
        id: id.to_string(),
        label,
        status,
        access,
        selectable,
        last_seen_at: field(link, "lastReachedAt"),
        last_error: field(link, "lastError"),
        laptop: laptop_of(link),
        version: version_of(link),
    })
}

fn field(value: &Value, key: &str) -> String {
    without_urls(value.get(key).and_then(Value::as_str).unwrap_or(""))
}

fn version_of(value: &Value) -> String {
    for key in ["serverVersion", "t3Version", "version"] {
        let text = field(value, key);
        if !text.is_empty() {
            return text;
        }
    }
    String::new()
}

fn laptop_of(value: &Value) -> bool {
    value.get("laptop").and_then(Value::as_bool) == Some(true)
        || machine_is_laptop(machine_name(value))
}

fn machine_name(value: &Value) -> &str {
    value
        .pointer("/platform/machine")
        .or_else(|| value.get("machine"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// A laptop that cannot be reached is asleep. A desktop that cannot be reached is unreachable.
fn machine_is_laptop(machine: &str) -> bool {
    let lower = machine.to_ascii_lowercase();
    lower.contains("laptop") || lower.contains("macbook") || lower.contains("notebook")
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

    fn sample() -> Value {
        json!({
            "links": [
                {
                    "environmentId": "srv-west-1",
                    "label": "Build server",
                    "urls": ["https://build.example"],
                    "access": "approval-required",
                    "status": "reachable"
                },
                {
                    "environmentId": "srv-down",
                    "label": "Down",
                    "urls": ["https://down.example"],
                    "access": "full-access",
                    "status": "unreachable"
                },
                {
                    "environmentId": "srv-ro",
                    "label": "Read",
                    "urls": ["https://ro.example"],
                    "access": "read-only",
                    "status": "reachable"
                },
                {
                    "environmentId": "srv-old",
                    "label": "Old",
                    "urls": [],
                    "access": "auto",
                    "status": "expired"
                },
                {
                    "environmentId": "bad id",
                    "label": "Bad",
                    "urls": [],
                    "access": "auto",
                    "status": "reachable"
                },
                {
                    "environmentId": "15f077d9-b158-435e-96f1-ed7709f11cbd",
                    "label": "This broker",
                    "urls": ["https://self.example"],
                    "access": "full-access",
                    "status": "reachable"
                },
                {
                    "environmentId": "srv-nolabel",
                    "urls": ["https://nolabel.example"],
                    "access": "auto",
                    "status": "reachable"
                }
            ]
        })
    }

    #[test]
    fn a_link_list_drops_urls_and_never_emits_the_broker() {
        let probe = json!({
            "label": "Computer",
            "environmentId": "15f077d9-b158-435e-96f1-ed7709f11cbd",
            "platform": { "machine": "arm64" }
        });
        let body = sample();
        assert!(body["links"][0]["urls"]
            .as_array()
            .unwrap()
            .contains(&json!("https://build.example")));
        let catalog = catalog_of(Some(&probe), Some(&body));
        assert!(catalog.ok);
        assert_eq!(catalog.local_label, "Computer");
        assert_eq!(catalog.broker_id, "15f077d9-b158-435e-96f1-ed7709f11cbd");
        let ids: Vec<_> = catalog.links.iter().map(|link| link.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "srv-west-1",
                "srv-down",
                "srv-ro",
                "srv-old",
                "bad id",
                "srv-nolabel"
            ]
        );
        let by = |id: &str| catalog.links.iter().find(|link| link.id == id).unwrap();
        assert!(by("srv-west-1").selectable);
        assert!(!by("srv-down").selectable);
        assert!(!by("srv-ro").selectable);
        assert!(!by("srv-old").selectable);
        assert!(!by("bad id").selectable);
        assert!(by("srv-nolabel").selectable);
        assert_eq!(by("srv-nolabel").label, "srv-nolabel");
        let public = catalog
            .links
            .iter()
            .map(|link| {
                json!({
                    "id": link.id,
                    "label": link.label,
                    "status": link.status,
                    "access": link.access,
                    "selectable": link.selectable,
                })
            })
            .collect::<Vec<_>>();
        let text = serde_json::to_string(&public).unwrap();
        assert!(!text.contains("urls"));
        assert!(!text.contains("https://"));
        assert!(!text.contains("15f077d9-b158-435e-96f1-ed7709f11cbd"));
    }

    #[test]
    fn a_failed_probe_or_list_is_offline() {
        let probe = json!({ "label": "Computer", "environmentId": "broker" });
        let offline = catalog_of(None, Some(&json!({ "links": [] })));
        assert!(!offline.ok);
        assert!(offline.links.is_empty());
        assert!(offline.broker_id.is_empty());
        assert!(offline.local_label.is_empty());
        let listed_down = catalog_of(Some(&probe), None);
        assert!(!listed_down.ok);
        assert!(listed_down.links.is_empty());
        assert!(listed_down.local_label.is_empty());
        assert!(listed_down.local_version.is_empty());
        assert!(!listed_down.local_laptop);
        assert!(listed_down.broker_id.is_empty());
    }

    #[test]
    fn a_link_keeps_status_facts_and_drops_urls() {
        let probe = json!({
            "label": "Studio",
            "environmentId": "15f077d9-b158-435e-96f1-ed7709f11cbd",
            "serverVersion": "0.1.9-nightly",
            "platform": { "machine": "mac-mini" }
        });
        let listed = json!({
            "links": [
                {
                    "environmentId": "srv-sleep",
                    "label": "Travel laptop",
                    "urls": ["https://sleep.example/wake"],
                    "access": "auto",
                    "status": "unreachable",
                    "laptop": true,
                    "lastReachedAt": "2026-11-01T00:00:00Z",
                    "lastError": "timed out https://sleep.example/health",
                    "serverVersion": "0.1.8"
                },
                {
                    "environmentId": "srv-book",
                    "label": "Book",
                    "urls": ["https://book.example"],
                    "access": "full-access",
                    "status": "unreachable",
                    "machine": "MacBookPro"
                },
                {
                    "environmentId": "srv-west-1",
                    "label": "Build server",
                    "urls": ["https://build.example"],
                    "access": "approval-required",
                    "status": "reachable",
                    "lastError": "https://build.example/only"
                }
            ]
        });
        let catalog = catalog_of(Some(&probe), Some(&listed));
        assert!(catalog.ok);
        assert_eq!(catalog.local_label, "Studio");
        assert_eq!(catalog.local_version, "0.1.9-nightly");
        assert!(!catalog.local_laptop);
        let sleep = catalog
            .links
            .iter()
            .find(|link| link.id == "srv-sleep")
            .unwrap();
        assert!(sleep.laptop);
        assert_eq!(sleep.status, "unreachable");
        assert_eq!(sleep.last_seen_at, "2026-11-01T00:00:00Z");
        assert_eq!(sleep.last_error, "timed out");
        assert_eq!(sleep.version, "0.1.8");
        let book = catalog
            .links
            .iter()
            .find(|link| link.id == "srv-book")
            .unwrap();
        assert!(book.laptop);
        let build = catalog
            .links
            .iter()
            .find(|link| link.id == "srv-west-1")
            .unwrap();
        assert!(build.last_error.is_empty());
        let text = format!("{catalog:?}");
        assert!(!text.contains("https://"));
        assert!(!text.contains("urls"));
    }
}
