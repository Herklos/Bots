//! What a row in the inbox needs beyond `bot.json`: the latest message, whether the Bot is
//! working, and whether something new is waiting. T3 Code is the harness wired today; one
//! shared read of its shell snapshot gives every Bot's thread at once.
use crate::AppState;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const KEEP: Duration = Duration::from_millis(1500);

/// What the shell snapshot says about one thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The thread's `updatedAt`: the stamp its preview is cached under.
    pub stamp: String,
    pub working: bool,
}

/// The shell snapshot, read at most once every 1.5 seconds however many clients poll.
#[derive(Debug, Default)]
pub struct Roster {
    cached: Mutex<Option<(Instant, HashMap<String, Fact>)>>,
}

impl Roster {
    /// Every thread's fact, or nothing when T3 can't be read. A list of Bots never fails on this.
    pub async fn facts(&self, s: &AppState) -> HashMap<String, Fact> {
        if let Some((at, facts)) = self.cached.lock().ok().and_then(|g| g.clone()) {
            if at.elapsed() < KEEP {
                return facts;
            }
        }
        let facts = read(s).await.unwrap_or_default();
        if let Ok(mut slot) = self.cached.lock() {
            *slot = Some((Instant::now(), facts.clone()));
        }
        facts
    }
}

async fn read(s: &AppState) -> anyhow::Result<HashMap<String, Fact>> {
    let shell = s
        .harness
        .transcript()
        .map_err(|err| anyhow::anyhow!(err.to_string()))?
        .activity()
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    Ok(shell
        .facts()
        .into_iter()
        .map(|(id, (stamp, working))| (id, Fact { stamp, working }))
        .collect())
}

/// A Bot has something new when its latest message is the Bot's own, and newer than the last
/// time the person had its chat open. A hidden Bot never shows one.
pub(crate) fn is_unread(bot: &Value, preview: &Value) -> bool {
    if bot["hidden"] == true || preview.is_null() {
        return false;
    }
    let from_bot = preview["role"] == "assistant" && preview["kind"] != "sent";
    let at = preview["at"].as_str().unwrap_or("");
    let seen = bot["lastSeenAt"].as_str().unwrap_or("");
    from_bot && !at.is_empty() && at > seen
}

/// `bots` with `preview`, `working` and `unread` added.
pub(crate) fn enrich(
    bots: Vec<Value>,
    facts: &HashMap<String, Fact>,
    previews: &Value,
) -> Vec<Value> {
    bots.into_iter()
        .map(|mut bot| {
            let thread = bot["mainThreadId"].as_str().unwrap_or("").to_string();
            let preview = previews.get(&thread).cloned().unwrap_or(Value::Null);
            let working = facts.get(&thread).is_some_and(|f| f.working);
            let unread = is_unread(&bot, &preview);
            bot["preview"] = preview;
            bot["working"] = json!(working);
            bot["unread"] = json!(unread);
            bot
        })
        .collect()
}

/// The list the inbox shows. Bots that predate `lastSeenAt` are marked seen now, once, so an
/// old conversation doesn't start out unread.
pub(crate) async fn list(s: &AppState, bots: Vec<Value>) -> Vec<Value> {
    let mut bots = bots;
    for bot in bots.iter_mut() {
        if bot["lastSeenAt"].as_str().is_none() {
            let now = crate::vault::now_iso();
            if let Some(slug) = bot["slug"].as_str() {
                let _ = s
                    .bots
                    .save(slug, &json!({ "lastSeenAt": now }), false)
                    .await;
            }
            bot["lastSeenAt"] = json!(now);
        }
    }
    let facts = s.roster.facts(s).await;
    let names: HashMap<String, String> = bots
        .iter()
        .filter_map(|b| {
            Some((
                b["mainThreadId"].as_str()?.to_string(),
                b["name"].as_str()?.to_string(),
            ))
        })
        .collect();
    let stamps: Map<String, Value> = bots
        .iter()
        .filter_map(|b| {
            let thread = b["mainThreadId"].as_str()?;
            let fact = facts.get(thread)?;
            Some((thread.to_string(), json!(fact.stamp)))
        })
        .collect();
    let harness = s.harness.clone();
    let preview_names = names.clone();
    let previews = s
        .threads
        .previews(Some(&Value::Object(stamps)), &names, |thread| {
            let harness = harness.clone();
            let preview_names = preview_names.clone();
            async move {
                harness
                    .previews()
                    .map_err(|err| anyhow::anyhow!(err.to_string()))?
                    .inbox(&thread, &preview_names)
                    .await
                    .map_err(|err| anyhow::anyhow!(err.to_string()))
            }
        })
        .await
        .unwrap_or_else(|_| json!({}));
    let mut rows = enrich(bots, &facts, &previews);
    // A hidden Bot's mark is the message id, not the roster timestamp.
    // Remember a new id without moving last-read. Opening the chat does that.
    for bot in rows.iter_mut() {
        if bot["hidden"] != true {
            continue;
        }
        let thread = bot["mainThreadId"].as_str().unwrap_or("");
        let preview_id = bot["preview"]["id"].as_str().unwrap_or("");
        let meta = if thread.is_empty() {
            json!({})
        } else {
            s.threads.get(Some(thread)).await.unwrap_or(json!({}))
        };
        let stored = meta["lastRead"].as_str().unwrap_or("");
        let known = meta["latest"].as_str().unwrap_or("");
        if !thread.is_empty() && !preview_id.is_empty() && preview_id != known {
            if let Ok(seen) = s
                .threads
                .mark_read(Some(thread), Some(preview_id), false, true)
                .await
            {
                bot["unread"] = seen["unread"].clone();
                continue;
            }
        }
        let latest = if !preview_id.is_empty() {
            preview_id
        } else {
            known
        };
        bot["unread"] = json!(crate::chat::thread_meta::inbox_unread(
            true, false, stored, latest
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let f: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/t3/shell-thread.json"))
                .unwrap();
        f["thread"].clone()
    }

    #[test]
    fn a_thread_with_an_active_run_is_working_and_its_update_time_is_the_stamp() {
        let mut idle = fixture();
        let mut live = fixture();
        live["id"] = json!("busy");
        live["activeRunId"] = json!("run-1");
        idle["activeRunId"] = Value::Null;
        let facts =
            crate::harness::t3::transcript::facts_in(&json!({ "threads": [idle.clone(), live] }));
        let id = idle["id"].as_str().unwrap();
        assert!(!facts[id].working);
        assert_eq!(facts[id].stamp, idle["updatedAt"].as_str().unwrap());
        assert!(facts["busy"].working);
    }

    #[test]
    fn only_a_newer_message_from_the_bot_is_unread() {
        let bot = json!({ "lastSeenAt": "2026-11-10T10:00:00.000Z" });
        let newer =
            json!({ "kind": "text", "role": "assistant", "at": "2026-11-10T11:00:00.000Z" });
        let older =
            json!({ "kind": "text", "role": "assistant", "at": "2026-11-10T09:00:00.000Z" });
        let mine = json!({ "kind": "text", "role": "user", "at": "2026-11-10T11:00:00.000Z" });
        let sent = json!({ "kind": "sent", "role": "assistant", "at": "2026-11-10T11:00:00.000Z" });
        assert!(is_unread(&bot, &newer));
        assert!(!is_unread(&bot, &older));
        assert!(!is_unread(&bot, &mine));
        assert!(!is_unread(&bot, &sent));
        assert!(!is_unread(&bot, &Value::Null));
        let hidden = json!({ "lastSeenAt": "", "hidden": true });
        assert!(!is_unread(&hidden, &newer));
    }

    #[test]
    fn rows_get_a_preview_a_working_flag_and_an_unread_flag() {
        let bots = vec![
            json!({ "slug": "ada", "mainThreadId": "t1", "lastSeenAt": "2026-11-10T10:00:00.000Z" }),
        ];
        let facts = HashMap::from([(
            "t1".to_string(),
            Fact {
                stamp: "s".into(),
                working: true,
            },
        )]);
        let previews = json!({ "t1": { "kind": "text", "role": "assistant", "text": "Done.", "at": "2026-11-10T11:00:00.000Z" } });
        let out = enrich(bots, &facts, &previews);
        assert_eq!(out[0]["preview"]["text"], "Done.");
        assert_eq!(out[0]["working"], true);
        assert_eq!(out[0]["unread"], true);
        let none = enrich(vec![json!({ "slug": "x" })], &HashMap::new(), &json!({}));
        assert_eq!(none[0]["preview"], Value::Null);
        assert_eq!(none[0]["working"], false);
    }
}
