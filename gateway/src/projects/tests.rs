use super::*;

fn bot() -> Value {
    json!({ "type": "bot", "name": "Ops", "slug": "ops" })
}

fn board() -> (tempfile::TempDir, PathBuf, Projects) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("projects.json");
    let p = Projects::new(file.clone());
    (dir, file, p)
}

fn msg(e: ApiError) -> String {
    e.message
}

async fn refuses(p: &Projects, items: Vec<Value>, needle: &str) {
    let err = p.save(&items, bot()).await.unwrap_err();
    assert_eq!(err.status, 400, "{}", err.message);
    assert!(
        err.message.contains(needle),
        "{:?} lacks {needle:?}",
        err.message
    );
}

#[tokio::test]
async fn a_project_and_its_todos_save_in_one_batch_with_refs_and_defaults() {
    let (_d, file, p) = board();
    let mut rx = p.events.subscribe();
    let saved = p
        .save(
            &[
                json!({ "ref": "nas", "kind": "machine", "title": "NAS" }),
                json!({ "ref": "tb", "kind": "project", "title": "hbots", "area": "Personal" }),
                json!({ "kind": "task", "title": "Ship the projects page", "parentId": "ref:tb", "fields": { "priority": "high", "due": "2026-11-05" } }),
                json!({ "kind": "service", "title": "bot-gateway", "machineId": "ref:nas", "parentId": "ref:tb", "fields": { "port": "3780" },
                        "addLinks": [{ "type": "thread", "threadId": "t-1" }, { "type": "thread", "threadId": "t-1" }] }),
            ],
            bot(),
        )
        .await
        .unwrap();
    let items = saved["items"].as_array().unwrap();
    let (nas, tb, task, svc) = (&items[0], &items[1], &items[2], &items[3]);
    assert_eq!(nas["status"], "unknown");
    assert_eq!(tb["status"], "active");
    assert_eq!(tb["area"], "personal");
    assert_eq!(task["status"], "todo");
    assert_eq!(task["parentId"], tb["id"]);
    assert_eq!(svc["machineId"], nas["id"]);
    assert_eq!(svc["fields"]["port"], 3780);
    assert_eq!(
        svc["links"].as_array().unwrap().len(),
        1,
        "duplicate links collapse"
    );
    assert_eq!(task["createdBy"], bot());
    assert_eq!(nas["id"].as_str().unwrap().len(), 8);
    let event = rx.recv().await.unwrap();
    assert_eq!(event["version"], 1);
    assert_eq!(event["activity"].as_array().unwrap().len(), 4);
    assert_eq!(event["activity"][0]["actor"]["name"], "Ops");
    let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(on_disk["items"].as_object().unwrap().len(), 4);
    assert_eq!(on_disk["version"], 1);
}

#[tokio::test]
async fn patches_change_only_what_they_name_null_clears_and_done_todos_record_when() {
    let (_d, _f, p) = board();
    let task = p
        .save(
            &[json!({ "kind": "task", "title": "Renew domain", "body": "Before the 9th", "fields": { "tags": ["Admin", "admin", "web"] } })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap()["items"][0]
        .clone();
    assert_eq!(task["fields"]["tags"], json!(["admin", "web"]));
    let id = task["id"].as_str().unwrap().to_string();
    let after = p
        .save(&[json!({ "id": id, "status": "done", "fields": { "tags": null, "url": "https://example.com/renew" } })], bot())
        .await
        .unwrap()["items"][0]
        .clone();
    assert_eq!(after["title"], "Renew domain");
    assert_eq!(after["body"], "Before the 9th");
    assert!(after["fields"].get("tags").is_none());
    assert_eq!(after["fields"]["url"], "https://example.com/renew");
    assert!(after["doneAt"].is_string());
    assert_eq!(after["updatedBy"], bot());
    let activity = p.activity().await.unwrap();
    assert_eq!(activity.last().unwrap()["action"], "completed");
    let reopened = p
        .save(
            &[json!({ "id": id, "status": "todo" })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap()["items"][0]
        .clone();
    assert!(reopened.get("doneAt").is_none());
    let unchanged = p
        .save(
            &[json!({ "id": id, "status": "todo" })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap();
    assert_eq!(unchanged["unchanged"], true);
}

#[tokio::test]
async fn bad_input_is_refused_with_a_plain_reason_and_the_board_is_unchanged() {
    let (_d, _f, p) = board();
    let project = p
        .save(
            &[json!({ "kind": "project", "title": "Home" })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap()["items"][0]
        .clone();
    let pid = project["id"].as_str().unwrap().to_string();
    refuses(&p, vec![json!({ "kind": "task" })], "needs a title").await;
    refuses(
        &p,
        vec![json!({ "kind": "spaceship", "title": "x" })],
        "kind must be",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "kind": "task", "title": "x", "status": "active" })],
        "status is one of todo",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "kind": "task", "title": "x", "fields": { "colour": "red" } })],
        "Unknown field \"colour\"",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "kind": "task", "title": "x", "fields": { "url": "javascript:alert(1)" } })],
        "http",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "kind": "task", "title": "x", "fields": { "due": "tomorrow" } })],
        "date like",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "kind": "task", "title": "x", "parentId": "ref:nope" })],
        "doesn't name an item",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "kind": "service", "title": "x", "machineId": pid })],
        "must point at a machine",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "id": "missing1", "title": "x" })],
        "no item \"missing1\"",
    )
    .await;
    refuses(
        &p,
        vec![
            json!({ "kind": "task", "title": "ok" }),
            json!({ "kind": "task" }),
        ],
        "needs a title",
    )
    .await;
    refuses(
        &p,
        vec![json!({ "id": pid, "parentId": pid })],
        "its own parent",
    )
    .await;
    let child = p
        .save(
            &[json!({ "kind": "project", "title": "Garden", "parentId": pid })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap()["items"][0]
        .clone();
    let cid = child["id"].as_str().unwrap().to_string();
    refuses(
        &p,
        vec![json!({ "id": pid, "parentId": cid })],
        "inside itself",
    )
    .await;
    assert_eq!(
        p.items().await.unwrap().len(),
        2,
        "the half-valid batch saved nothing"
    );
    assert!(p
        .items()
        .await
        .unwrap()
        .iter()
        .find(|i| i["id"] == pid)
        .unwrap()
        .get("parentId")
        .is_none());
}

#[tokio::test]
async fn deleting_moves_children_up_and_drops_links_to_the_deleted_item() {
    let (_d, _f, p) = board();
    let saved = p
        .save(
            &[
                json!({ "ref": "a", "kind": "project", "title": "Work" }),
                json!({ "ref": "b", "kind": "project", "title": "Launch", "parentId": "ref:a" }),
                json!({ "ref": "m", "kind": "machine", "title": "Air" }),
                json!({ "kind": "task", "title": "Write post", "parentId": "ref:b", "addLinks": [{ "type": "item", "id": "ref:m" }] }),
                json!({ "kind": "repo", "title": "hbots", "machineId": "ref:m" }),
            ],
            json!({ "type": "user" }),
        )
        .await
        .unwrap();
    let items = saved["items"].as_array().unwrap().clone();
    let mut rx = p.events.subscribe();
    let launch = items[1]["id"].as_str().unwrap().to_string();
    let air = items[2]["id"].as_str().unwrap().to_string();
    p.delete(&[launch.clone(), air.clone()], bot())
        .await
        .unwrap();
    let left = p.items().await.unwrap();
    let task = left.iter().find(|i| i["title"] == "Write post").unwrap();
    assert_eq!(task["parentId"], items[0]["id"]);
    assert_eq!(task["links"], json!([]));
    assert!(left
        .iter()
        .find(|i| i["title"] == "hbots")
        .unwrap()
        .get("machineId")
        .is_none());
    let _ = rx.try_recv();
}

#[tokio::test]
async fn saves_from_several_tasks_all_land() {
    let (_d, _f, p) = board();
    let p = std::sync::Arc::new(p);
    let handles: Vec<_> = (0..20)
        .map(|i| {
            let p = p.clone();
            tokio::spawn(async move {
                p.save(
                    &[json!({ "kind": "task", "title": format!("Task {i}") })],
                    json!({ "type": "user" }),
                )
                .await
                .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(p.items().await.unwrap().len(), 20);
    assert_eq!(p.version().await, 20);
}

#[tokio::test]
async fn a_fresh_store_reads_what_an_earlier_one_wrote() {
    let (_d, file, p) = board();
    p.save(
        &[json!({ "kind": "note", "title": "Wifi is on the fridge" })],
        json!({ "type": "user" }),
    )
    .await
    .unwrap();
    let again = Projects::new(file);
    assert_eq!(
        again.items().await.unwrap()[0]["title"],
        "Wifi is on the fridge"
    );
}

#[tokio::test]
async fn the_board_is_refused_rather_than_overwritten_when_its_file_is_corrupt() {
    let (_d, file, p) = board();
    std::fs::write(&file, "{ not json").unwrap();
    let err = p
        .save(
            &[json!({ "kind": "note", "title": "x" })],
            json!({ "type": "user" }),
        )
        .await
        .unwrap_err();
    assert_eq!(msg(err), "The Projects board file can't be read.");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
}

#[tokio::test]
async fn overview_find_and_detail_give_agents_ids_links_and_placement() {
    let (_d, _f, p) = board();
    let saved = p
        .save(
            &[
                json!({ "ref": "p", "kind": "project", "title": "hbots", "area": "personal" }),
                json!({ "ref": "m", "kind": "machine", "title": "NAS", "status": "online" }),
                json!({ "kind": "task", "title": "Low thing", "parentId": "ref:p", "fields": { "priority": "low" } }),
                json!({ "kind": "task", "title": "Urgent thing", "parentId": "ref:p", "fields": { "priority": "high" } }),
                json!({ "kind": "task", "title": "Finished", "parentId": "ref:p", "status": "done" }),
                json!({ "kind": "service", "title": "bot-gateway", "machineId": "ref:m", "addLinks": [{ "type": "thread", "threadId": "thread-9" }] }),
                json!({ "kind": "repo", "title": "hbots-source", "machineId": "ref:m", "fields": { "path": "/mnt/apps/t3/projects/hbots-source" } }),
            ],
            bot(),
        )
        .await
        .unwrap();
    let items = saved["items"].as_array().unwrap().clone();
    let all = p.items().await.unwrap();
    let activity = p.activity().await.unwrap();
    let app = "https://bot.example";
    let view = overview(&all, &activity, app);
    assert_eq!(view["projects"][0]["openTasks"], 2);
    let titles: Vec<&str> = view["openTasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Urgent thing", "Low thing"]);
    assert_eq!(view["openTasks"][0]["project"], "hbots");
    assert_eq!(view["machines"][0]["services"][0]["title"], "bot-gateway");
    assert_eq!(view["machines"][0]["repos"][0]["title"], "hbots-source");
    assert_eq!(view["page"], "https://bot.example/?page=projects");
    assert_eq!(view["recent"][0]["who"], "Ops");
    assert!(view["openTasks"][0]["link"]
        .as_str()
        .unwrap()
        .starts_with("https://bot.example/?item="));
    let found = |q: Value| -> Vec<String> {
        find(&all, &q, app)["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(found(json!({ "search": "source mnt" })), ["hbots-source"]);
    let mut open = found(json!({ "kind": "task", "open": true }));
    open.sort();
    assert_eq!(open, ["Low thing", "Urgent thing"]);
    assert_eq!(found(json!({ "threadId": "thread-9" })), ["bot-gateway"]);
    let machine = detail(&all, items[1]["id"].as_str().unwrap(), app).unwrap();
    let mut on: Vec<String> = machine["onThisMachine"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_string())
        .collect();
    on.sort();
    assert_eq!(on, ["bot-gateway", "hbots-source"]);
    let project = detail(&all, items[0]["id"].as_str().unwrap(), app).unwrap();
    assert_eq!(project["children"].as_array().unwrap().len(), 3);
}

#[test]
fn cleaners_refuse_bad_values_and_fold_good_ones() {
    assert_eq!(
        clean_tags(&json!(["Web", " web ", "", "API"])).unwrap(),
        Some(json!(["web", "api"]))
    );
    assert!(clean_tags(&json!((0..13).map(|i| format!("t{i}")).collect::<Vec<_>>())).is_err());
    assert_eq!(
        clean_url(&json!(" https://example.com/a "), "url")
            .unwrap()
            .as_deref(),
        Some("https://example.com/a")
    );
    assert!(clean_url(&json!("example.com"), "url").is_err());
    assert!(clean_url(&json!("ftp://example.com"), "url").is_err());
    assert!(valid_date("2028-02-29"));
    assert!(!valid_date("2027-02-29"));
    assert!(!valid_date("2026-13-01"));
    assert!(!valid_date("２026-01-01"));
    assert_eq!(clean_port(&json!("3780")).unwrap(), Some(json!(3780)));
    assert!(clean_port(&json!(70000)).is_err());
    assert!(clean_port(&json!(80.5)).is_err());
    assert_eq!(clean_priority(&json!("")).unwrap(), None);
    assert!(clean_priority(&json!("urgent")).is_err());
}

#[test]
fn text_and_one_line_fold_crlf_and_collapse_space() {
    assert_eq!(
        oneline(&json!("  a \r\n  b\tc  "), "t", 50)
            .unwrap()
            .as_deref(),
        Some("a b c")
    );
    assert_eq!(
        text(&json!("a\r\nb"), "t", 50).unwrap().as_deref(),
        Some("a\nb")
    );
    assert!(text(&json!("abc"), "t", 2).is_err());
    assert_eq!(text(&json!("   "), "t", 2).unwrap(), None);
    assert!(text(&json!(3), "t", 2).is_err());
}

#[test]
fn link_keys_ignore_labels_and_keep_environments_apart() {
    let a = clean_link(&json!({ "type": "thread", "threadId": "t", "label": "one" })).unwrap();
    let b = clean_link(&json!({ "type": "thread", "threadId": "t", "label": "two" })).unwrap();
    let c =
        clean_link(&json!({ "type": "thread", "threadId": "t", "environmentId": "e1" })).unwrap();
    assert_eq!(link_key(&a), link_key(&b));
    assert_ne!(link_key(&a), link_key(&c));
    assert!(clean_link(&json!({ "type": "bot", "slug": "Bad Slug" })).is_err());
    assert!(clean_link(&json!({ "type": "nope" })).is_err());
}

/// A projects.json from `tests/fixtures/node-projects.json`.
#[tokio::test]
async fn a_board_written_by_hbots_loads_and_resaves_without_losing_fields() {
    let fixture = include_str!("../../tests/fixtures/node-projects.json");
    let node: Value = serde_json::from_str(fixture).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("projects.json");
    std::fs::write(&file, fixture).unwrap();
    let p = Projects::new(file.clone());
    let board = p.try_board().await.unwrap();
    assert_eq!(board["version"], node["version"]);
    assert_eq!(
        board["items"].as_array().unwrap().len(),
        node["items"].as_object().unwrap().len()
    );
    let task_id = node["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, i)| i["title"] == "Ship the projects page")
        .map(|(id, _)| id.clone())
        .unwrap();
    p.save(
        &[json!({ "id": task_id, "title": "Changed from Rust" })],
        bot(),
    )
    .await
    .unwrap();
    let resaved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(resaved["version"], node["version"].as_u64().unwrap() + 1);
    assert_eq!(
        resaved["seq"].as_u64(),
        Some(node["seq"].as_u64().unwrap() + 1)
    );
    assert_eq!(resaved["items"][&task_id]["title"], "Changed from Rust");
    for (id, item) in node["items"].as_object().unwrap() {
        if *id == task_id {
            continue;
        }
        assert_eq!(&resaved["items"][id], item, "item {id} changed on re-save");
    }
    assert_eq!(
        resaved["activity"].as_array().unwrap().len(),
        node["activity"].as_array().unwrap().len() + 1
    );
}
