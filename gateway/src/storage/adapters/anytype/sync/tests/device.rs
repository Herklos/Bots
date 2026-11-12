//! The hb_device heartbeat against the mock: register twice is one object, and the
//! peers list names the other devices without this one.

use super::support::World;
use crate::storage::adapters::anytype::client::AnytypeClient;

#[tokio::test]
async fn registering_twice_publishes_one_device_and_lists_peers() {
    let world = World::start("device-heartbeat").await;
    world.mock.seed_named("hb_device", "hb_device");
    let client = AnytypeClient::new(&world.rig.url, world.rig.device.clone(), "space").unwrap();

    super::super::device::register(&client, "device-heartbeat", "mac.example", "0.4.0")
        .await
        .unwrap();
    super::super::device::register(&client, "device-heartbeat", "mac.example", "0.4.0")
        .await
        .unwrap();

    let rows = client
        .search(&serde_json::json!({
            "type": "hb_device",
            "fields": ["id", "hb_slug", "hb_hostname", "hb_last_seen"],
        }))
        .await
        .unwrap();
    let own: Vec<&serde_json::Value> = rows
        .iter()
        .filter(|row| row["properties"]["hb_slug"].as_str() == Some("device-heartbeat"))
        .collect();
    assert_eq!(own.len(), 1, "register twice must not duplicate: {rows:?}");
    assert_eq!(own[0]["properties"]["hb_hostname"], "mac.example");
    assert!(own[0]["properties"]["hb_last_seen"].as_i64().unwrap_or(0) > 0);

    // Another gateway on another machine: a peer, never this device.
    client
        .create_object(
            &serde_json::json!({
                "type": "hb_device",
                "name": "other-device",
                "properties": {
                    "hb_slug": "other-device",
                    "hb_hostname": "second.example",
                    "hb_gateway_version": "0.4.0",
                    "hb_status": "online",
                    "hb_last_seen": 1_000,
                },
            }),
            "device-heartbeat-peer",
        )
        .await
        .unwrap();
    let peers = super::super::device::peers(&client, "device-heartbeat")
        .await
        .unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].slug, "other-device");
    assert_eq!(peers[0].hostname, "second.example");
    assert_eq!(peers[0].last_seen, 1_000);
}
