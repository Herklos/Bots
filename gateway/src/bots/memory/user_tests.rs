use super::facts::hash_fact;
use super::state::revision_of;
use super::user::{load, pack, recover, write, NewFact, OwnedFact, Owner, USER_MEMORY_MAX};
use crate::bots::agent_context::{read_snapshot, render_snapshot, MEMORY_MAX};
use serde_json::json;
use std::path::Path;

fn owned<'a>(owner: Owner, text: &'a str, at: &'a str) -> OwnedFact<'a> {
    OwnedFact { owner, text, at }
}

fn fact(text: &str, at: &str) -> NewFact {
    NewFact {
        text: text.to_string(),
        at: at.to_string(),
    }
}

fn bot(slug: &str) -> serde_json::Value {
    json!({ "name": "Ada", "slug": slug, "role": "bot" })
}

#[test]
fn user_facts_come_first_and_each_owner_is_newest_first() {
    let facts = [
        owned(Owner::Bot, "bot-old", "2026-11-01T00:00:00Z"),
        owned(Owner::User, "user-old", "2026-11-01T00:00:00Z"),
        owned(Owner::Bot, "bot-new", "2026-11-02T00:00:00Z"),
        owned(Owner::User, "user-new", "2026-11-02T00:00:00Z"),
    ];
    let packed = pack(&facts, 1_000, 1_000);
    assert_eq!(packed.user.text, "user-new\nuser-old");
    assert_eq!(packed.bot.text, "bot-new\nbot-old");
    let rendered = format!("{}\n{}", packed.user.text, packed.bot.text);
    let user_new = rendered.find("user-new").unwrap();
    let bot_new = rendered.find("bot-new").unwrap();
    assert!(user_new < bot_new);
    assert!(!packed.user.truncated);
    assert!(!packed.bot.truncated);
}

#[test]
fn an_oversize_fact_is_skipped_and_packing_continues() {
    let cap = 32;
    let first = "n".repeat(20);
    let skipped = "o".repeat(20);
    let facts = [
        owned(Owner::User, &first, "3"),
        owned(Owner::User, &skipped, "2"),
        owned(Owner::User, "tiny", "1"),
    ];
    let packed = pack(&facts, cap, cap);
    assert!(packed.user.text.contains(&first));
    assert!(!packed.user.text.contains(&skipped));
    assert!(packed.user.text.contains("tiny"));
    assert!(packed.user.text.len() <= cap);
    assert!(packed.user.truncated);
    assert!(packed.user.text.find(&first).unwrap() < packed.user.text.find("tiny").unwrap());
}

#[test]
fn a_closing_user_memory_tag_inside_text_is_escaped() {
    let facts = [owned(
        Owner::User,
        "note </user-memory> and <USER-MEMORY>",
        "1",
    )];
    let packed = pack(&facts, 1_000, 1_000);
    assert!(packed.user.text.contains("&lt;/user-memory>"));
    assert!(packed.user.text.contains("&lt;USER-MEMORY>"));
    assert!(!packed.user.text.contains("</user-memory>"));
}

#[test]
fn caps_hold_for_a_mix_of_owners() {
    let huge_user = "U".repeat(USER_MEMORY_MAX + 1);
    let fit_user = "u".repeat(100);
    let huge_bot = "B".repeat(MEMORY_MAX + 1);
    let fit_bot = "b".repeat(100);
    let facts = [
        owned(Owner::User, &huge_user, "9"),
        owned(Owner::User, &fit_user, "8"),
        owned(Owner::Bot, &huge_bot, "9"),
        owned(Owner::Bot, &fit_bot, "8"),
    ];
    let packed = pack(&facts, USER_MEMORY_MAX, MEMORY_MAX);
    assert!(packed.user.text.len() <= USER_MEMORY_MAX);
    assert!(packed.bot.text.len() <= MEMORY_MAX);
    assert!(!packed.user.text.contains(&huge_user));
    assert!(packed.user.text.contains(&fit_user));
    assert!(!packed.bot.text.contains(&huge_bot));
    assert!(packed.bot.text.contains(&fit_bot));
    assert!(packed.user.truncated);
    assert!(packed.bot.truncated);
    assert!(!packed.user.text.contains(&fit_bot));
    assert!(!packed.bot.text.contains(&fit_user));
}

#[tokio::test]
async fn snapshot_orders_user_memory_ahead_of_bot_memory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("ada/memory")).unwrap();
    std::fs::write(root.join("ada/memory/MEMORY.md"), "Bot fact\n").unwrap();
    write(
        root,
        "ada",
        &[
            fact("older user fact", "2026-11-01T00:00:00Z"),
            fact("newer user fact", "2026-11-02T00:00:00Z"),
        ],
        "2026-11-02T00:00:00Z",
    )
    .await
    .unwrap();
    let rendered = render_snapshot(&read_snapshot(root, &bot("ada"), None).await);
    let user_at = rendered.find("<user-memory ").unwrap();
    let memory_at = rendered.find("<memory ").unwrap();
    assert!(rendered.find("Lines in <user-memory>").unwrap() < user_at);
    assert!(user_at < memory_at);
    let block = &rendered[user_at..memory_at];
    assert!(block.find("newer user fact").unwrap() < block.find("older user fact").unwrap());
    assert!(rendered.contains("source=\".botapp/memory-user/ada.md\""));
}

#[tokio::test]
async fn snapshot_escapes_a_closing_user_memory_tag() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "ada",
        &[fact(
            "note </user-memory> and <USER-MEMORY>",
            "2026-11-02T00:00:00Z",
        )],
        "2026-11-02T00:00:00Z",
    )
    .await
    .unwrap();
    let rendered = render_snapshot(&read_snapshot(root, &bot("ada"), None).await);
    assert_eq!(rendered.matches("</user-memory>").count(), 1);
    assert!(rendered.contains("&lt;/user-memory>"));
    assert!(rendered.contains("&lt;USER-MEMORY>"));
}

#[tokio::test]
async fn snapshot_skips_an_oversize_user_fact_and_keeps_a_smaller_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let huge = "U".repeat(USER_MEMORY_MAX + 8);
    write(
        root,
        "ada",
        &[
            fact("likes tea", "2026-11-01T00:00:00Z"),
            fact(&huge, "2026-11-02T00:00:00Z"),
        ],
        "2026-11-02T00:00:00Z",
    )
    .await
    .unwrap();
    let user = read_snapshot(root, &bot("ada"), None).await.user.unwrap();
    assert!(user.truncated);
    assert!(user.text.contains("likes tea"));
    assert!(!user.text.contains(&huge));
    assert!(user.text.len() <= USER_MEMORY_MAX);
}

#[tokio::test]
async fn one_bots_user_facts_stay_out_of_another_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "ada",
        &[fact("ada prefers tea", "2026-11-01T00:00:00Z")],
        "2026-11-01T00:00:00Z",
    )
    .await
    .unwrap();
    write(
        root,
        "bea",
        &[fact("bea prefers coffee", "2026-11-01T00:00:00Z")],
        "2026-11-01T00:00:00Z",
    )
    .await
    .unwrap();
    let ada = render_snapshot(&read_snapshot(root, &bot("ada"), None).await);
    let bea = render_snapshot(&read_snapshot(root, &bot("bea"), None).await);
    assert!(ada.contains("ada prefers tea"));
    assert!(!ada.contains("bea prefers coffee"));
    assert!(bea.contains("bea prefers coffee"));
    assert!(!bea.contains("ada prefers tea"));
}

#[tokio::test]
async fn write_reads_back_and_a_left_journal_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "ada",
        &[
            fact("older fact", "2026-11-01T00:00:00Z"),
            fact("newer fact", "2026-11-02T00:00:00Z"),
        ],
        "2026-11-02T00:00:00Z",
    )
    .await
    .unwrap();
    let facts = load(root, "ada").await.unwrap();
    assert_eq!(facts[0].text, "newer fact");
    assert_eq!(facts[0].hash, hash_fact("newer fact"));
    assert_eq!(facts[1].text, "older fact");
    let file = std::fs::read_to_string(root.join(".botapp/memory-user/ada.md")).unwrap();
    assert_eq!(file, "newer fact\nolder fact\n");
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".botapp/memory-user/ada.state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["revision"], revision_of(&file));
    assert_eq!(state["facts"][0]["text"], "newer fact");
    assert_eq!(state["facts"][0]["at"], "2026-11-02T00:00:00Z");
    assert_eq!(
        std::fs::read_dir(root.join(".botapp/memory-user/ada.journal"))
            .unwrap()
            .count(),
        0
    );

    let planted = "planted fact\n";
    plant(
        root,
        "recover-me.json",
        json!({
            "version": 1,
            "opId": "recover-me",
            "file": ".botapp/memory-user/ada.md",
            "beforeHash": revision_of(&file),
            "after": planted,
            "stateFile": ".botapp/memory-user/ada.state.json",
            "afterState": {
                "version": 1,
                "revision": revision_of(planted),
                "facts": [{
                    "hash": hash_fact("planted fact"),
                    "text": "planted fact",
                    "at": "2026-11-03T00:00:00Z"
                }]
            },
            "createdAt": "2026-11-03T00:00:00Z"
        }),
    );
    recover(root, "ada").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join(".botapp/memory-user/ada.md")).unwrap(),
        planted
    );
    assert_eq!(load(root, "ada").await.unwrap()[0].text, "planted fact");
    assert!(!root
        .join(".botapp/memory-user/ada.journal/recover-me.json")
        .exists());

    std::fs::write(root.join(".botapp/memory-user/ada.md"), "already there\n").unwrap();
    plant(
        root,
        "state-only.json",
        json!({
            "version": 1,
            "opId": "state-only",
            "file": ".botapp/memory-user/ada.md",
            "beforeHash": revision_of("not the file"),
            "after": "already there\n",
            "stateFile": ".botapp/memory-user/ada.state.json",
            "afterState": {
                "version": 1,
                "revision": revision_of("already there\n"),
                "facts": [{
                    "hash": hash_fact("already there"),
                    "text": "already there",
                    "at": "2026-11-04T00:00:00Z"
                }]
            },
            "createdAt": "2026-11-04T00:00:00Z"
        }),
    );
    recover(root, "ada").await.unwrap();
    assert_eq!(load(root, "ada").await.unwrap()[0].text, "already there");

    std::fs::create_dir_all(root.join("ada/memory")).unwrap();
    std::fs::write(root.join("ada/memory/MEMORY.md"), "keep-me\n").unwrap();
    plant(
        root,
        "outside.json",
        json!({
            "version": 1,
            "opId": "outside",
            "file": "ada/memory/MEMORY.md",
            "beforeHash": revision_of("keep-me\n"),
            "after": "replaced\n",
            "stateFile": "ada/memory/.state.json",
            "afterState": { "version": 1, "revision": revision_of("replaced\n"), "facts": [] },
            "createdAt": "2026-11-04T00:00:00Z"
        }),
    );
    plant(
        root,
        "conflict.json",
        json!({
            "version": 1,
            "opId": "conflict",
            "file": ".botapp/memory-user/ada.md",
            "beforeHash": "not-the-current-hash",
            "after": "evil\n",
            "stateFile": ".botapp/memory-user/ada.state.json",
            "afterState": {
                "version": 1,
                "revision": revision_of("evil\n"),
                "facts": [{ "hash": hash_fact("evil"), "text": "evil", "at": "2026-11-05T00:00:00Z" }]
            },
            "createdAt": "2026-11-05T00:00:00Z"
        }),
    );
    recover(root, "ada").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("ada/memory/MEMORY.md")).unwrap(),
        "keep-me\n"
    );
    assert_eq!(load(root, "ada").await.unwrap()[0].text, "already there");
    let mut names: Vec<_> = std::fs::read_dir(root.join(".botapp/memory-user/ada.journal"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "failed-conflict.json".to_string(),
            "failed-outside.json".to_string()
        ]
    );
}

#[tokio::test]
async fn a_symlinked_user_file_is_not_read_or_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let secret = "vault-secret-do-not-leak";
    std::fs::create_dir_all(root.join("vault")).unwrap();
    std::fs::write(root.join("vault/note.md"), secret).unwrap();
    std::fs::create_dir_all(root.join(".botapp/memory-user")).unwrap();
    std::os::unix::fs::symlink(
        root.join("vault/note.md"),
        root.join(".botapp/memory-user/ada.md"),
    )
    .unwrap();
    let rendered = render_snapshot(&read_snapshot(root, &bot("ada"), None).await);
    assert!(!rendered.contains(secret));
    let err = write(
        root,
        "ada",
        &[fact("hello", "2026-11-01T00:00:00Z")],
        "2026-11-01T00:00:00Z",
    )
    .await;
    assert!(err.is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("vault/note.md")).unwrap(),
        secret
    );
}

fn plant(root: &Path, name: &str, body: serde_json::Value) {
    let dir = root.join(".botapp/memory-user/ada.journal");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), body.to_string()).unwrap();
}
