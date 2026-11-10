//! A local room of Bots. This is not a T3 channel and not the Working on list.
//! The file is `groups.json` in the app directory: an id, a title, member slugs,
//! and the messages stored on this machine. A send is stored here, then dispatched
//! to each member's existing main thread. Naming exactly one member leaves the others quiet.

use crate::error::{ApiError, ApiResult};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Member {
    pub slug: String,
    pub title: String,
    pub thread_id: String,
    pub working: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub slug: String,
    pub thread_id: String,
}

pub struct Rooms {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Rooms {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub async fn list(&self) -> ApiResult<Vec<Value>> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(rooms_of(&doc))
    }

    pub async fn create(&self, title: &str, slugs: &[String]) -> ApiResult<Value> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ApiError::bad_request("Name the room."));
        }
        if slugs.len() < 2 {
            return Err(ApiError::bad_request("A room needs two Bots."));
        }
        for slug in slugs {
            if !crate::bots::valid_slug(slug) {
                return Err(ApiError::bad_request("Pick Bots that already exist."));
            }
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "title": title,
            "members": slugs,
            "messages": [],
        });
        push_room(&mut doc, room.clone())?;
        write_doc(&self.path, &doc).await?;
        Ok(room)
    }

    /// Stores one message on the room and returns who should hear it.
    /// Members with no main thread are not dispatched.
    pub async fn send(
        &self,
        room_id: &str,
        directory: &[Member],
        text: &str,
        id: &str,
        at: &str,
    ) -> ApiResult<(Value, Vec<Dispatch>)> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ApiError::bad_request("Write a message first."));
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        let members = resolve(member_slugs(room), directory);
        let dispatches = plan_dispatches(&members, text);
        push_message(
            room,
            &message(id, at, "user", "You", text),
        )?;
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok((stored, dispatches))
    }

    /// Appends one reply under that Bot's name. A repeated id is kept once.
    pub async fn reply(
        &self,
        room_id: &str,
        slug: &str,
        name: &str,
        text: &str,
        id: &str,
        at: &str,
    ) -> ApiResult<Value> {
        let text = text.trim();
        if text.is_empty() || slug.is_empty() {
            return Err(ApiError::bad_request("A reply needs a Bot and some text."));
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        if !has_message(room, id) {
            push_message(room, &message(id, at, slug, name, text))?;
        }
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok(stored)
    }

    pub async fn get(&self, room_id: &str) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        rooms_of(&doc)
            .into_iter()
            .find(|room| room["id"].as_str() == Some(room_id))
            .ok_or_else(|| ApiError::not_found("No such room"))
    }
}

/// The commands a room send dispatches. No quote field is added.
pub fn dispatch_commands(dispatches: &[Dispatch], text: &str) -> Vec<Value> {
    dispatches
        .iter()
        .map(|item| crate::chat::send_command(&item.thread_id, text, &[]))
        .collect()
}

/// One member when the text names exactly one slug or title. Otherwise everyone.
pub fn plan_dispatches(members: &[Member], text: &str) -> Vec<Dispatch> {
    let chosen: Vec<&Member> = match named_one(members, text) {
        Some(index) => vec![&members[index]],
        None => members.iter().collect(),
    };
    chosen
        .into_iter()
        .filter(|member| !member.thread_id.is_empty())
        .map(|member| Dispatch {
            slug: member.slug.clone(),
            thread_id: member.thread_id.clone(),
        })
        .collect()
}

pub fn present(room: &Value, directory: &[Member]) -> Value {
    let members = resolve(member_slugs(room), directory);
    json!({
        "id": room["id"],
        "title": room["title"],
        "members": members.iter().map(member_row).collect::<Vec<_>>(),
        "messages": room["messages"].as_array().cloned().unwrap_or_default(),
    })
}

pub fn member_row(member: &Member) -> Value {
    json!({
        "slug": member.slug,
        "name": member.title,
        "working": !member.thread_id.is_empty() && member.working,
        "ready": !member.thread_id.is_empty(),
        "state": face_state(member),
        "line": status_line(member),
    })
}

pub fn face_state(member: &Member) -> &'static str {
    if member.thread_id.is_empty() {
        "asleep"
    } else if member.working {
        "working"
    } else {
        "idle"
    }
}

pub fn status_line(member: &Member) -> &'static str {
    if member.thread_id.is_empty() {
        "Getting set up…"
    } else if member.working {
        "Working"
    } else {
        "Ready"
    }
}

/// Directory rows for the slugs stored on the room, in that order.
pub fn resolve(slugs: Vec<String>, directory: &[Member]) -> Vec<Member> {
    slugs
        .into_iter()
        .filter_map(|slug| directory.iter().find(|member| member.slug == slug).cloned())
        .collect()
}

fn named_one(members: &[Member], text: &str) -> Option<usize> {
    let mut hit: Option<usize> = None;
    for (index, member) in members.iter().enumerate() {
        if word_hit(text, &member.slug) || word_hit(text, &member.title) {
            if hit.is_some() {
                return None;
            }
            hit = Some(index);
        }
    }
    hit
}

fn word_hit(text: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == needle)
}

fn message(id: &str, at: &str, from: &str, name: &str, text: &str) -> Value {
    json!({ "id": id, "at": at, "from": from, "name": name, "text": text })
}

fn member_slugs(room: &Value) -> Vec<String> {
    room["members"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn has_message(room: &Value, id: &str) -> bool {
    room["messages"]
        .as_array()
        .is_some_and(|list| list.iter().any(|message| message["id"].as_str() == Some(id)))
}

fn push_message(room: &mut Value, message: &Value) -> ApiResult<()> {
    let list = room
        .as_object_mut()
        .and_then(|room| room.get_mut("messages"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    list.push(message.clone());
    Ok(())
}

fn rooms_of(doc: &Value) -> Vec<Value> {
    doc["rooms"].as_array().cloned().unwrap_or_default()
}

fn push_room(doc: &mut Value, room: Value) -> ApiResult<()> {
    let list = doc
        .as_object_mut()
        .and_then(|doc| doc.get_mut("rooms"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    list.push(room);
    Ok(())
}

fn room_mut<'a>(doc: &'a mut Value, room_id: &str) -> ApiResult<&'a mut Value> {
    doc.get_mut("rooms")
        .and_then(Value::as_array_mut)
        .and_then(|rooms| {
            rooms
                .iter_mut()
                .find(|room| room["id"].as_str() == Some(room_id))
        })
        .ok_or_else(|| ApiError::not_found("No such room"))
}

async fn read_doc(path: &std::path::Path) -> ApiResult<Value> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => {
            let doc: Value = serde_json::from_str(&text)
                .map_err(|_| ApiError::new(500, "groups.json is not valid"))?;
            if !doc["rooms"].is_array() {
                return Err(ApiError::new(500, "groups.json is not valid"));
            }
            Ok(doc)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({ "version": 1, "rooms": [] }))
        }
        Err(err) => Err(err.into()),
    }
}

async fn write_doc(path: &std::path::Path, doc: &Value) -> ApiResult<()> {
    crate::store::write_json(path, doc).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> Vec<Member> {
        vec![
            Member {
                slug: "ada".into(),
                title: "Ada".into(),
                thread_id: "thread-ada".into(),
                working: true,
            },
            Member {
                slug: "bea".into(),
                title: "Bea".into(),
                thread_id: "thread-bea".into(),
                working: false,
            },
        ]
    }

    #[tokio::test]
    async fn a_room_send_stores_one_message_and_fans_out_to_each_main_thread() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let created = rooms
            .create("Design", &["ada".into(), "bea".into()])
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        assert!(rooms.list().await.unwrap().is_empty() == false);

        let (stored, dispatches) = rooms
            .send(&id, &directory(), "hello both", "m1", "t1")
            .await
            .unwrap();
        assert_eq!(stored["messages"].as_array().unwrap().len(), 1);
        assert_eq!(stored["messages"][0]["text"], "hello both");
        assert_eq!(stored["messages"][0]["from"], "user");
        assert_eq!(dispatches.len(), 2);
        assert_eq!(dispatches[0].thread_id, "thread-ada");
        assert_eq!(dispatches[1].thread_id, "thread-bea");

        let commands = dispatch_commands(&dispatches, "hello both");
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0]["threadId"], "thread-ada");
        assert_eq!(commands[1]["threadId"], "thread-bea");
        assert_eq!(commands[0]["type"], "message.dispatch");
        assert!(commands[0].get("quote").is_none());
        assert_eq!(commands[0]["messageId"], commands[0]["commandId"]);
        assert_ne!(commands[0]["messageId"], commands[1]["messageId"]);

        let again = Rooms::new(dir.path().join("groups.json"));
        let loaded = again.get(&id).await.unwrap();
        assert_eq!(loaded["messages"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn naming_one_member_by_slug_or_title_dispatches_only_to_that_member() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = rooms
            .create("Design", &["ada".into(), "bea".into()])
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let (_, by_slug) = rooms
            .send(&id, &directory(), "ada please look", "m2", "t2")
            .await
            .unwrap();
        assert_eq!(by_slug.len(), 1);
        assert_eq!(by_slug[0].slug, "ada");
        assert_eq!(by_slug[0].thread_id, "thread-ada");

        let (stored, by_title) = rooms
            .send(&id, &directory(), "ask Bea about it", "m3", "t3")
            .await
            .unwrap();
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].slug, "bea");
        assert_eq!(stored["messages"].as_array().unwrap().len(), 2);

        let (_, both_named) = rooms
            .send(&id, &directory(), "Ada and Bea", "m4", "t4")
            .await
            .unwrap();
        assert_eq!(both_named.len(), 2);
    }

    #[tokio::test]
    async fn a_member_without_a_thread_stays_quiet_and_replies_keep_order() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = rooms
            .create("Design", &["ada".into(), "bea".into()])
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let members = vec![
            Member {
                slug: "ada".into(),
                title: "Ada".into(),
                thread_id: "thread-ada".into(),
                working: true,
            },
            Member {
                slug: "bea".into(),
                title: "Bea".into(),
                thread_id: String::new(),
                working: false,
            },
        ];
        let (_, dispatches) = rooms
            .send(&id, &members, "hello", "m1", "t1")
            .await
            .unwrap();
        assert_eq!(dispatches.len(), 1);
        assert_eq!(dispatches[0].slug, "ada");

        rooms
            .reply(&id, "ada", "Ada", "On it", "r1", "t2")
            .await
            .unwrap();
        rooms
            .reply(&id, "ada", "Ada", "On it", "r1", "t3")
            .await
            .unwrap();
        let stored = rooms
            .reply(&id, "bea", "Bea", "I am still starting", "r2", "t4")
            .await
            .unwrap();
        let messages = stored["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["name"], "Ada");
        assert_eq!(messages[1]["from"], "ada");
        assert_eq!(messages[2]["name"], "Bea");
        assert_eq!(messages[2]["text"], "I am still starting");

        let view = present(&stored, &members);
        assert_eq!(view["members"][0]["name"], "Ada");
        assert_eq!(view["members"][0]["line"], "Working");
        assert_eq!(view["members"][0]["state"], "working");
        assert_eq!(view["members"][1]["line"], "Getting set up…");
        assert_eq!(view["members"][1]["ready"], false);
        assert_eq!(view["members"][1]["state"], "asleep");
        assert_eq!(view["messages"].as_array().unwrap().len(), 3);
    }
}
