//! Read-only webhook views. The list row never carries a token or a URL.
//! Detail replaces authorization, cookie, and the signature header with `[hidden]`.

use super::webhook;
use crate::harness::Routines;
use serde_json::{json, Map, Value};

const LIST_CAP: usize = 50;
const BODY_CAP: usize = 64 * 1024;
const HIDDEN: &str = "[hidden]";

pub(crate) fn hook_view(
    task: &Value,
    settings: &Value,
    remote: &crate::network::remote_access::RemoteAccess,
) -> Value {
    let made = webhook::webhook_url_for(task, settings, remote);
    json!({
        "url": made.url,
        "source": made.source,
        "stable": made.stable,
    })
}

/// Prefer the rotate reply's path. A following list can still show the old one.
pub(crate) fn task_after_rotate(fresh: Value, rotated: &Value) -> Value {
    if rotated["task"]["schedule"]["type"] == "webhook"
        && rotated["task"]["webhook"]["path"]
            .as_str()
            .is_some_and(|path| !path.is_empty())
    {
        return rotated["task"].clone();
    }
    let Some(path) = rotated["webhook"]["path"]
        .as_str()
        .filter(|path| !path.is_empty())
    else {
        return fresh;
    };
    let mut task = fresh;
    task["webhook"]["path"] = json!(path);
    task
}

pub(crate) fn delivery_list(value: &Value) -> Value {
    let rows = newest(rows_of(value));
    json!({
        "deliveries": rows.iter().map(summary).collect::<Vec<_>>(),
    })
}

pub(crate) fn delivery_detail(task: &Value, value: &Value) -> Value {
    let row = if value["delivery"].is_object() {
        &value["delivery"]
    } else {
        value
    };
    let (body, cut) = cap_body(row["body"].as_str().unwrap_or(""));
    let (prompt, _) = cap_body(row["renderedPrompt"].as_str().unwrap_or(""));
    let mut out = summary(row);
    out["query"] = json!(text_of(&row["query"]));
    out["headers"] = redact_headers(&row["headers"], &signature_header(task));
    out["body"] = json!(body);
    out["bodyTruncated"] = json!(row["bodyTruncated"] == true || cut);
    out["renderedPrompt"] = json!(prompt);
    out
}

/// Newest outcome for the routines chip. A failed read leaves the chip empty.
pub(crate) async fn attach_outcomes(routines: &dyn Routines, rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if row["kind"] != "webhook" {
            continue;
        }
        let Some(id) = row["id"].as_str() else {
            continue;
        };
        let Ok(listed) = routines.list_deliveries(id).await else {
            continue;
        };
        let Some(outcome) = newest_outcome(&listed) else {
            continue;
        };
        if let Some(hook) = row.get_mut("hook").and_then(Value::as_object_mut) {
            hook.insert("lastOutcome".into(), json!(outcome));
        }
    }
}

fn newest_outcome(value: &Value) -> Option<String> {
    let rows = newest(rows_of(value));
    let outcome = rows.first()?.get("outcome")?.as_str()?.to_string();
    if outcome.is_empty() {
        None
    } else {
        Some(outcome)
    }
}

fn rows_of(value: &Value) -> Vec<Value> {
    value["deliveries"]
        .as_array()
        .or_else(|| value.as_array())
        .cloned()
        .unwrap_or_default()
}

fn newest(rows: Vec<Value>) -> Vec<Value> {
    let mut indexed: Vec<(usize, Value)> = rows.into_iter().enumerate().collect();
    indexed.sort_by(|(left_i, left), (right_i, right)| {
        let left_at = left["receivedAt"].as_str().unwrap_or("");
        let right_at = right["receivedAt"].as_str().unwrap_or("");
        right_at.cmp(left_at).then(left_i.cmp(right_i))
    });
    indexed
        .into_iter()
        .map(|(_, row)| row)
        .take(LIST_CAP)
        .collect()
}

fn summary(row: &Value) -> Value {
    json!({
        "id": row["id"].as_str().unwrap_or(""),
        "receivedAt": row["receivedAt"].as_str().unwrap_or(""),
        "method": row["method"].as_str().unwrap_or(""),
        "contentType": row["contentType"].as_str().unwrap_or(""),
        "bodyBytes": row["bodyBytes"].as_u64().unwrap_or(0),
        "outcome": row["outcome"].as_str().unwrap_or(""),
        "signatureVerified": row["signatureVerified"] == true,
        "missingFields": strings_of(&row["missingFields"]),
        "error": text_of(&row["error"]),
    })
}

fn strings_of(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(|text| json!(text)))
        .collect()
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn signature_header(task: &Value) -> String {
    task["schedule"]["signature"]["header"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn redact_headers(headers: &Value, signature: &str) -> Value {
    let Some(obj) = headers.as_object() else {
        return json!({});
    };
    let mut out = Map::new();
    for (key, value) in obj {
        let name = key.to_ascii_lowercase();
        let hidden = name == "authorization"
            || name == "cookie"
            || (!signature.is_empty() && name == signature);
        let text = if hidden {
            HIDDEN.to_string()
        } else {
            text_of(value)
        };
        out.insert(key.clone(), json!(text));
    }
    Value::Object(out)
}

fn cap_body(body: &str) -> (String, bool) {
    if body.len() <= BODY_CAP {
        return (body.to_string(), false);
    }
    let mut end = BODY_CAP;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    (body[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Value {
        json!({
            "schedule": { "type": "webhook", "signature": { "header": "X-Hub-Signature-256" } }
        })
    }

    #[test]
    fn the_list_keeps_fifty_and_drops_the_body() {
        let mut rows = Vec::new();
        for n in 0..51 {
            rows.push(json!({
                "id": format!("d{n}"),
                "receivedAt": format!("2026-11-12T20:{n:02}:00.000Z"),
                "method": "POST",
                "bodyBytes": 4,
                "outcome": "accepted",
                "body": "PAYLOAD-LEAK",
                "url": "https://secret.example/tok",
                "headers": { "authorization": "Bearer leak" },
                "renderedPrompt": "prompt",
            }));
        }
        let listed = delivery_list(&json!({ "deliveries": rows }));
        let kept = listed["deliveries"].as_array().unwrap();
        assert_eq!(kept.len(), 50);
        assert_eq!(kept[0]["id"], "d50");
        assert_eq!(kept.last().unwrap()["id"], "d1");
        let text = listed.to_string();
        assert!(!text.contains("PAYLOAD-LEAK"));
        assert!(!text.contains("secret.example"));
        assert!(!text.contains("Bearer"));
        assert!(kept[0].get("body").is_none());
        assert!(kept[0].get("headers").is_none());
        assert!(kept[0].get("url").is_none());
    }

    #[test]
    fn detail_hides_auth_cookie_and_the_signature() {
        let raw = json!({
            "delivery": {
                "id": "delivery:1",
                "receivedAt": "2026-11-12T20:21:16.707Z",
                "method": "POST",
                "contentType": "application/json",
                "bodyBytes": 16,
                "outcome": "accepted",
                "signatureVerified": false,
                "missingFields": ["body.id"],
                "error": null,
                "query": "x=1",
                "headers": {
                    "authorization": "[redacted]",
                    "cookie": "[redacted]",
                    "x-hub-signature-256": "sha256=abc",
                    "x-custom-header": "FooBar"
                },
                "body": "{\"w0\":\"headers\"}",
                "bodyTruncated": false,
                "renderedPrompt": "A webhook arrived."
            }
        });
        let detail = delivery_detail(&task(), &raw);
        assert_eq!(detail["headers"]["authorization"], "[hidden]");
        assert_eq!(detail["headers"]["cookie"], "[hidden]");
        assert_eq!(detail["headers"]["x-hub-signature-256"], "[hidden]");
        assert_eq!(detail["headers"]["x-custom-header"], "FooBar");
        assert_eq!(detail["renderedPrompt"], "A webhook arrived.");
        assert_eq!(detail["body"], "{\"w0\":\"headers\"}");
        assert_eq!(detail["bodyTruncated"], false);
        assert_eq!(detail["query"], "x=1");
        assert!(!detail.to_string().contains("sha256=abc"));
        let huge = "é".repeat(BODY_CAP);
        let cut = delivery_detail(
            &task(),
            &json!({ "body": format!("{huge}more"), "bodyTruncated": false, "headers": {} }),
        );
        assert_eq!(cut["bodyTruncated"], true);
        assert!(cut["body"].as_str().unwrap().len() <= BODY_CAP);
    }
}
