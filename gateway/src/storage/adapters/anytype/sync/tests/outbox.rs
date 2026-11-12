use super::super::outbox::{idem_key, Item, Op, Outbox, OutboxError, Push, REFUSE_AT, WARN_AT};
use super::super::sha256_hex;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn sample(id: &str, sha: &str) -> Item {
    Item::new(id, "ada/memory/MEMORY.md", Op::Create, sha, "device")
}

#[test]
fn idempotency_key_is_stable() {
    let path = "ada/memory/MEMORY.md";
    let key = idem_key("device", path, Op::Create, "abc");
    assert_eq!(key, sha256_hex(b"device|ada/memory/MEMORY.md|create|abc"));
    assert_eq!(key, idem_key("device", path, Op::Create, "abc"));
    assert_ne!(key, idem_key("device", path, Op::Create, "abd"));
    assert_eq!(
        idem_key("device", path, Op::Update, "abc"),
        sha256_hex(b"device|ada/memory/MEMORY.md|update|abc")
    );
    assert_eq!(
        idem_key("device", path, Op::Tombstone, "abc"),
        sha256_hex(b"device|ada/memory/MEMORY.md|tombstone|abc")
    );
}

#[tokio::test]
async fn replay_keeps_one_item_through_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("outbox.jsonl");
    let mut item = sample("id-1", "abc");
    item.attempts = 2;
    item.next_at = 50;
    let mut outbox = Outbox::default();
    assert_eq!(outbox.push(item).unwrap(), Push::Queued { warn: false });
    outbox.save(&path).await.unwrap();
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("\"op\":\"create\""));
    assert!(saved.ends_with('\n'));

    let mut loaded = Outbox::load(&path).unwrap();
    assert_eq!(loaded.items().len(), 1);
    assert_eq!(loaded.push(sample("id-2", "abc")).unwrap(), Push::Replay);
    assert_eq!(loaded.items().len(), 1);
    assert_eq!(loaded.items()[0].id, "id-1");
    assert_eq!(loaded.items()[0].attempts, 2);
    assert_eq!(loaded.items()[0].next_at, 50);
    assert_eq!(
        loaded.items()[0].idem_key,
        sha256_hex(b"device|ada/memory/MEMORY.md|create|abc")
    );

    assert!(loaded.ack("id-1"));
    loaded.save(&path).await.unwrap();
    assert!(Outbox::load(&path).unwrap().items().is_empty());
}

#[test]
fn duplicate_lines_collapse_on_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("outbox.jsonl");
    let line = serde_json::to_string(&sample("id-1", "abc")).unwrap();
    let second = serde_json::to_string(&sample("id-2", "abc")).unwrap();
    std::fs::write(&path, format!("{line}\n{second}\n")).unwrap();
    let loaded = Outbox::load(&path).unwrap();
    assert_eq!(loaded.items().len(), 1);
    assert_eq!(loaded.items()[0].id, "id-1");
}

#[test]
fn a_corrupt_line_is_not_an_empty_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("outbox.jsonl");
    std::fs::write(&path, "{}\nnot-json\n").unwrap();
    assert!(matches!(Outbox::load(&path), Err(OutboxError::Corrupt(_))));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\nnot-json\n");
}

#[test]
fn caps_at_ten_thousand_and_warns_at_one_thousand() {
    let mut outbox = Outbox::default();
    for n in 0..REFUSE_AT {
        let push = outbox
            .push(sample(&format!("id-{n}"), &format!("sha-{n}")))
            .unwrap();
        assert_eq!(
            push,
            Push::Queued {
                warn: n + 1 >= WARN_AT
            }
        );
    }
    assert!(matches!(
        outbox.push(sample("overflow", "sha-overflow")),
        Err(OutboxError::Full)
    ));
    assert_eq!(outbox.items().len(), REFUSE_AT);
    assert_eq!(outbox.push(sample("again", "sha-0")).unwrap(), Push::Replay);
    assert_eq!(outbox.items().len(), REFUSE_AT);
}
