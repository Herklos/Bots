//! This machine's `m_device` object and the list of the others. One object per gateway
//! on one machine, `m_slug` is the engine's device id. The heartbeat re-creates the
//! object if it is gone and never returns an error to the caller: the loop logs it.

use super::super::client::{AnytypeClient, AnytypeError};
use super::outbox::{self, Op};
use super::payload;
use super::unix_ms;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Peer {
    pub slug: String,
    pub hostname: String,
    pub version: String,
    pub status: String,
    pub last_seen: i64,
}

pub(super) async fn register(
    client: &AnytypeClient,
    device_id: &str,
    hostname: &str,
    version: &str,
) -> Result<(), AnytypeError> {
    let props = json!({
        "m_slug": device_id,
        "m_hostname": hostname,
        "m_gateway_version": version,
        "m_status": "online",
        "m_last_seen": unix_ms(),
    });
    if let Some(id) = own_id(client, device_id).await? {
        let logical = outbox::idem_key(device_id, "m_device", Op::Update, &props.to_string());
        client
            .patch_object(
                &id,
                &payload::patch_body(props),
                None,
                &payload::wire_key(&logical, None),
            )
            .await?;
    } else {
        let logical = outbox::idem_key(device_id, "m_device", Op::Create, &props.to_string());
        let body = json!({
            "type": "m_device",
            "name": device_id,
            "properties": props,
        });
        client
            .create_object(&body, &payload::wire_key(&logical, None))
            .await?;
    }
    Ok(())
}

pub(super) async fn peers(
    client: &AnytypeClient,
    device_id: &str,
) -> Result<Vec<Peer>, AnytypeError> {
    let rows = client
        .search(&json!({
            "type": "m_device",
            "fields": ["m_slug", "m_hostname", "m_gateway_version", "m_status", "m_last_seen"],
        }))
        .await?;
    let mut peers = Vec::new();
    for row in rows {
        let slug = text_at(&row, "m_slug").unwrap_or_default();
        if slug.is_empty() || slug == device_id {
            continue;
        }
        peers.push(Peer {
            slug,
            hostname: text_at(&row, "m_hostname").unwrap_or_default(),
            version: text_at(&row, "m_gateway_version").unwrap_or_default(),
            status: text_at(&row, "m_status").unwrap_or_default(),
            last_seen: number_at(&row, "m_last_seen").unwrap_or(0),
        });
    }
    peers.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(peers)
}

async fn own_id(client: &AnytypeClient, device_id: &str) -> Result<Option<String>, AnytypeError> {
    let rows = client
        .search(&json!({
            "type": "m_device",
            "fields": ["name", "m_slug"],
        }))
        .await?;
    Ok(rows
        .iter()
        .filter(|row| text_at(row, "m_slug").as_deref() == Some(device_id))
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .min())
}

/// Search rows nest properties under `properties`, the same shape `remote` reads.
fn text_at(row: &Value, field: &str) -> Option<String> {
    row.get("properties")
        .or(Some(row))
        .and_then(|props| props.get(field))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn number_at(row: &Value, field: &str) -> Option<i64> {
    row.get("properties")
        .or(Some(row))
        .and_then(|props| props.get(field))
        .and_then(number_ms)
}

/// `m_last_seen` is a millisecond number. A whole number arrives as an int; the
/// app sends a float once in a while, and rounding loses no part that matters.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a rounded ms stamp is the exact value"
)]
fn number_ms(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|n| n.round() as i64))
}
