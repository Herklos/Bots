use super::super::apply::{apply_snapshot, Apply};
use super::super::notice::NoticeFile;
use super::super::store::{ReviewFile, ReviewState, ReviewStore};
use super::super::watch::{next_backoff, BACKOFF_MAX, BACKOFF_MIN, HEARTBEAT};
use super::super::{record, resolve, Choice};
use super::support::{bot, snap, task, Fake, PROMPT};
use super::{hold, sealed, NOW};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct Desk {
    dir: tempfile::TempDir,
    file: ReviewFile,
    notices: NoticeFile,
    fake: Fake,
    applied: BTreeSet<String>,
}

impl Desk {
    async fn sealed() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let botapp = dir.path().join(".botapp");
        let file = ReviewFile::new(&botapp);
        file.save(&sealed(&[])).await.unwrap();
        Self {
            dir,
            file,
            notices: NoticeFile::new(&botapp),
            fake: Fake::new(Vec::new(), Arc::new(AtomicBool::new(false))),
            applied: BTreeSet::new(),
        }
    }

    async fn apply(&mut self, bots: &[Value], doc: &Value) -> Result<(), String> {
        apply_snapshot(
            &mut Apply {
                routines: &self.fake,
                bots,
                file: &self.file,
                notices: &self.notices,
                now: NOW,
                applied: &mut self.applied,
            },
            doc,
        )
        .await
    }

    async fn store(&self) -> ReviewStore {
        self.file.load().await.unwrap()
    }

    fn notes(&self) -> String {
        let path = self.dir.path().join(".botapp").join("routine-notices.json");
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

#[test]
fn backoff_starts_at_one_second_and_caps_at_thirty() {
    assert_eq!(BACKOFF_MIN, Duration::from_secs(1));
    assert_eq!(BACKOFF_MAX, Duration::from_secs(30));
    assert_eq!(HEARTBEAT, Duration::from_secs(60));
    let mut delay = BACKOFF_MIN;
    let mut seen = vec![delay];
    for _ in 0..6 {
        delay = next_backoff(delay, BACKOFF_MAX);
        seen.push(delay);
    }
    let expect: Vec<_> = [1u64, 2, 4, 8, 16, 30, 30]
        .into_iter()
        .map(Duration::from_secs)
        .collect();
    assert_eq!(seen, expect);
}

#[tokio::test]
async fn the_first_snapshot_grandfathers_and_a_later_one_is_held() {
    let mut desk = Desk::sealed().await;
    // Replace the sealed store with an empty one, as a first start.
    desk.file.save(&ReviewStore::empty()).await.unwrap();
    let old = task("old", 3_600_000);
    desk.apply(&[bot("ask")], &snap(vec![old.clone()]))
        .await
        .unwrap();
    assert!(desk.fake.writes().is_empty());
    assert!(desk.store().await.grandfathered.contains("old"));
    assert!(desk.store().await.items.is_empty());

    let fresh = task("new", 3_600_000);
    desk.apply(&[bot("ask")], &snap(vec![old, fresh]))
        .await
        .unwrap();
    assert_eq!(desk.fake.writes().len(), 1);
    assert_eq!(desk.fake.writes()[0]["id"], "new");
    assert_eq!(desk.fake.writes()[0]["enabled"], false);
    assert!(!desk.store().await.items.contains_key("old"));
    assert_eq!(desk.store().await.items["new"].state, ReviewState::Pending);

    // A restart keeps the pending row and does not flag the grandfathered one again.
    desk.applied.clear();
    desk.apply(
        &[bot("ask")],
        &snap(vec![task("old", 3_600_000), task("new", 3_600_000)]),
    )
    .await
    .unwrap();
    assert_eq!(desk.fake.writes().len(), 1);
    assert_eq!(desk.notices.load().await.unwrap().len(), 1);
    assert!(!desk.notes().contains(PROMPT));
    assert!(!desk.store().await.items.contains_key("old"));
}

#[tokio::test]
async fn a_repeated_snapshot_disables_a_routine_once() {
    let mut desk = Desk::sealed().await;
    let doc = snap(vec![task("t", 3_600_000)]);
    desk.apply(&[bot("ask")], &doc).await.unwrap();
    desk.apply(&[bot("ask")], &doc).await.unwrap();
    assert_eq!(
        desk.fake.writes(),
        vec![json!({ "id": "t", "enabled": false })]
    );
    assert_eq!(desk.notices.load().await.unwrap().len(), 1);
    let text = &desk.notices.load().await.unwrap()[0].text;
    assert_eq!(
        text,
        "Ada wants to run Morning digest (Every hour). It is waiting for your OK."
    );
    assert!(!text.contains(PROMPT));
}

#[tokio::test]
async fn a_failed_disable_is_tried_again_and_recorded_once() {
    let mut desk = Desk::sealed().await;
    desk.fake.fail_enabled.store(1, Ordering::SeqCst);
    let doc = snap(vec![task("t", 3_600_000)]);
    let err = desk.apply(&[bot("ask")], &doc).await.unwrap_err();
    assert!(err.contains("could not update"));
    assert!(desk.store().await.items.is_empty());
    assert!(desk.notices.load().await.unwrap().is_empty());
    desk.apply(&[bot("ask")], &doc).await.unwrap();
    assert_eq!(desk.fake.attempts.load(Ordering::SeqCst), 2);
    assert_eq!(desk.fake.writes().len(), 1);
    assert_eq!(desk.store().await.items["t"].state, ReviewState::Pending);
    assert_eq!(desk.notices.load().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_fast_interval_is_raised_without_sending_environment_id() {
    let mut desk = Desk::sealed().await;
    let doc = snap(vec![task("t", 60_000)]);
    desk.apply(&[bot("ask")], &doc).await.unwrap();
    desk.apply(&[bot("ask")], &doc).await.unwrap();
    assert_eq!(desk.fake.writes().len(), 1);
    let body = &desk.fake.writes()[0];
    assert_eq!(body["schedule"]["everyMs"], 15 * 60 * 1000);
    assert_eq!(body["enabled"], false);
    assert_eq!(body["requireExisting"], true);
    assert!(body.get("environmentId").is_none());
    assert_eq!(body["commandId"], "hbots-routine-review:hold:t:900000:");
    // The second identical snapshot is a revise of the same floor, not a second write.
    let raw = serde_json::to_string(body).unwrap();
    assert!(!raw.contains("catchUpPolicy"));
    assert!(!raw.contains("skipIfOpen"));
    let note = &desk.notices.load().await.unwrap()[0].text;
    assert!(note.contains("every 15 minutes"));
    assert!(!note.contains(PROMPT));
    assert!(!desk.notes().contains(PROMPT));
}

#[tokio::test]
async fn a_once_routine_is_disabled_and_catch_up_is_not_invented() {
    let mut desk = Desk::sealed().await;
    let mut once = task("once", 3_600_000);
    once["schedule"] = json!({ "type": "once", "runAt": "2026-11-12T10:00:05Z" });
    desk.apply(&[bot("ask")], &snap(vec![once])).await.unwrap();
    assert_eq!(
        desk.fake.writes(),
        vec![json!({ "id": "once", "enabled": false })]
    );
    let raw = serde_json::to_string(&desk.fake.writes()[0]).unwrap();
    assert!(!raw.contains("catchUpPolicy"));
    assert!(!raw.contains("skipIfOpen"));
    assert!(!raw.contains("environmentId"));
}

#[tokio::test]
async fn a_person_a_trusted_bot_and_an_unbound_task_are_left_alone() {
    let mut desk = Desk::sealed().await;
    let mut person = task("mine", 60_000);
    person["createdBy"] = json!("user");
    person["creationSource"] = json!("web");
    let mut other = task("stray", 60_000);
    other["threadId"] = json!("thread-other");
    other["projectId"] = json!("project-other");
    desk.apply(&[bot("ask")], &snap(vec![person, other.clone()]))
        .await
        .unwrap();
    desk.apply(&[bot("trust")], &snap(vec![task("t", 60_000)]))
        .await
        .unwrap();
    assert!(desk.fake.writes().is_empty());
    assert!(desk.notices.load().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_extra_pending_routine_is_deleted_and_the_notice_says_why() {
    let mut desk = Desk::sealed().await;
    let mut store = sealed(&[]);
    for index in 0..5 {
        record(&mut store, &format!("p{index}"), "ada", &hold(), NOW);
    }
    desk.file.save(&store).await.unwrap();
    desk.apply(&[bot("ask")], &snap(vec![task("extra", 3_600_000)]))
        .await
        .unwrap();
    assert_eq!(desk.fake.writes(), vec![json!({ "id": "extra" })]);
    assert_eq!(
        desk.store().await.items["extra"].state,
        ReviewState::Rejected
    );
    let text = &desk.notices.load().await.unwrap()[0].text;
    assert!(text.contains("already waiting for your OK"));
    assert!(!text.contains(PROMPT));
}

#[tokio::test]
async fn three_distinct_failures_pause_once_and_a_repeat_does_not() {
    let mut desk = Desk::sealed().await;
    let mut store = sealed(&[]);
    record(&mut store, "t", "ada", &hold(), NOW);
    assert_eq!(
        resolve(&mut store, "t", Choice::Approve, NOW),
        super::super::Resolution::Enable
    );
    desk.file.save(&store).await.unwrap();
    for count in 1..=3 {
        let mut failed = task("t", 3_600_000);
        failed["lastRunStatus"] = json!("failed");
        failed["runCount"] = json!(count);
        failed["lastRunAt"] = json!(format!("2026-11-12T10:0{count}:00Z"));
        desk.apply(&[bot("ask")], &snap(vec![failed]))
            .await
            .unwrap();
    }
    assert_eq!(desk.store().await.failure_count("t"), 3);
    assert_eq!(
        desk.fake.writes(),
        vec![json!({ "id": "t", "enabled": false })]
    );
    let mut again = task("t", 3_600_000);
    again["lastRunStatus"] = json!("failed");
    again["runCount"] = json!(3);
    again["lastRunAt"] = json!("2026-11-12T10:03:00Z");
    desk.apply(&[bot("ask")], &snap(vec![again])).await.unwrap();
    assert_eq!(desk.fake.writes().len(), 1);
    assert_eq!(desk.store().await.failure_count("t"), 3);
    let text = &desk.notices.load().await.unwrap()[0].text;
    assert!(text.contains("paused after repeated failures"));
    assert!(!text.contains(PROMPT));
}

#[tokio::test]
async fn a_damaged_store_is_not_overwritten() {
    let mut desk = Desk::sealed().await;
    let path = desk.dir.path().join(".botapp").join("routine-review.json");
    std::fs::write(&path, "{").unwrap();
    let err = desk
        .apply(&[bot("ask")], &snap(vec![task("t", 3_600_000)]))
        .await
        .unwrap_err();
    assert!(err.contains("not valid"));
    assert!(desk.fake.writes().is_empty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "{");
}
