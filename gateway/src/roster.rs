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

const SHELL: &str = "orchestration.subscribeShell";
const KEEP: Duration = Duration::from_millis(1500);

/// What the shell snapshot says about one thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The thread's `updatedAt`: the stamp its preview is cached under.
    pub stamp: String,
    pub working: bool,
}

/// The shell snapshot, read at most once every 1.5 seconds however many clients poll.
#[derive(Default)]
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
    let client = s.t3.rpc().await?;
    let item = client
        .read_until(SHELL, json!({}), |item| {
            item.get("kind").and_then(Value::as_str) == Some("snapshot")
        })
        .await?;
    Ok(facts_in(item.get("snapshot").unwrap_or(&Value::Null)))
}

pub fn facts_in(snapshot: &Value) -> HashMap<String, Fact> {
    snapshot
        .get("threads")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let id = t["id"].as_str()?.to_string();
            let working = crate::release::thread_is_busy(t) || t["activeRunId"].is_string();
            Some((
                id,
                Fact {
                    stamp: t["updatedAt"].as_str().unwrap_or("").to_string(),
                    working,
                },
            ))
        })
        .collect()
}

/// A Bot has something new when its latest message is the Bot's own, and newer than the last
/// time the person had its chat open. A hidden Bot never shows one.
pub fn is_unread(bot: &Value, preview: &Value) -> bool {
    if bot["hidden"] == true || preview.is_null() {
        return false;
    }
    let from_bot = preview["role"] == "assistant" && preview["kind"] != "sent";
    let at = preview["at"].as_str().unwrap_or("");
    let seen = bot["lastSeenAt"].as_str().unwrap_or("");
    from_bot && !at.is_empty() && at > seen
}

/// `bots` with `preview`, `working` and `unread` added.
pub fn enrich(bots: Vec<Value>, facts: &HashMap<String, Fact>, previews: &Value) -> Vec<Value> {
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
pub async fn list(s: &AppState, bots: Vec<Value>) -> Vec<Value> {
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
    let rpc = s.t3.rpc().await.ok();
    let previews = s
        .threads
        .previews(Some(&Value::Object(stamps)), &names, |thread| {
            let rpc = rpc.clone();
            async move {
                let rpc = rpc.ok_or_else(|| anyhow::anyhow!("T3 unavailable"))?;
                rpc.call(
                    "orchestration.getThreadProjection",
                    json!({ "threadId": thread }),
                )
                .await
            }
        })
        .await
        .unwrap_or_else(|_| json!({}));
    enrich(bots, &facts, &previews)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let f: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/t3/shell-thread.json"))
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
        let facts = facts_in(&json!({ "threads": [idle.clone(), live] }));
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
