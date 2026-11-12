use super::*;

impl Rooms {
    #[allow(
        clippy::too_many_arguments,
        reason = "answerPolicy and chief are optional fields on the same create call"
    )]
    pub async fn create(
        &self,
        title: &str,
        description: &str,
        color: &str,
        shape: &str,
        slugs: &[String],
        bots: &[Value],
        answer_policy: Option<&str>,
        chief: Option<&str>,
    ) -> ApiResult<Value> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ApiError::bad_request("Name the group."));
        }
        let slugs = distinct(slugs);
        if slugs.len() < 2 {
            return Err(ApiError::bad_request("A group needs two Bots."));
        }
        for slug in &slugs {
            if !crate::bots::valid_slug(slug) {
                return Err(ApiError::bad_request("Pick Bots that already exist."));
            }
        }
        // `bots` is the list `Bots::list` already loaded. A subagent is not a second store.
        refuse_subagent(&slugs, bots)?;
        let answer_policy = answer_policy
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(policy) = answer_policy {
            if !matches!(policy, "default" | "last" | "everyone" | "chief") {
                return Err(ApiError::bad_request(
                    "answerPolicy must be default, last, everyone, or chief",
                ));
            }
        }
        let chief = chief.map(str::trim).filter(|value| !value.is_empty());
        if let Some(slug) = chief {
            if !slugs.iter().any(|member| member == slug) {
                return Err(ApiError::bad_request("chief must be a member"));
            }
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let mut room = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "title": title,
            "description": description.trim(),
            "color": clean_color(color),
            "shape": crate::bots::shape_name(shape).unwrap_or_else(|| "ball".into()),
            "members": slugs,
            "messages": [],
            "anchors": {},
            "threads": {},
            "hops": 0,
            "notifications": true,
        });
        if let Some(policy) = answer_policy {
            room["answerPolicy"] = json!(policy);
        }
        if let Some(slug) = chief {
            room["chief"] = json!(slug);
        }
        push_room(&mut doc, room.clone())?;
        write_doc(&self.path, &doc).await?;
        Ok(room)
    }

    /// Changes the fields present in `patch`. Messages stay.
    /// `bots` is the list `Bots::list` already loaded, used when `members` is replaced.
    pub async fn update(&self, room_id: &str, patch: &Value, bots: &[Value]) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        let replacement = if let Some(members) = patch.get("members") {
            let list: Vec<String> = members
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let list = distinct(&list);
            if list.len() < 2 {
                return Err(ApiError::bad_request("A group needs two Bots."));
            }
            if list.iter().any(|slug| !crate::bots::valid_slug(slug)) {
                return Err(ApiError::bad_request("Pick Bots that already exist."));
            }
            // A list that includes a subagent is refused before the file is written.
            refuse_subagent(&list, bots)?;
            Some(list)
        } else {
            None
        };
        if let Some(title) = patch.get("title") {
            let title = title.as_str().unwrap_or("").trim();
            if title.is_empty() {
                return Err(ApiError::bad_request("Name the group."));
            }
            room["title"] = json!(title);
        }
        if let Some(description) = patch.get("description") {
            room["description"] = json!(description.as_str().unwrap_or("").trim());
        }
        if let Some(color) = patch.get("color") {
            room["color"] = json!(clean_color(color.as_str().unwrap_or("")));
        }
        if let Some(shape) = patch.get("shape") {
            room["shape"] = json!(crate::bots::shape_name(shape.as_str().unwrap_or(""))
                .unwrap_or_else(|| "ball".into()));
        }
        if let Some(list) = replacement {
            room["members"] = json!(list);
            let chief = room["chief"].as_str().unwrap_or("");
            if !chief.is_empty() && !list.iter().any(|slug| slug == chief) {
                room.as_object_mut().map(|object| object.remove("chief"));
            }
        }
        if let Some(value) = patch.get("answerPolicy") {
            let policy = value.as_str().unwrap_or("");
            if !matches!(policy, "default" | "last" | "everyone" | "chief") {
                return Err(ApiError::bad_request(
                    "answerPolicy must be default, last, everyone, or chief",
                ));
            }
            room["answerPolicy"] = json!(policy);
        }
        if let Some(value) = patch.get("chief") {
            let chief = value.as_str().unwrap_or("").trim();
            if chief.is_empty() {
                room.as_object_mut().map(|object| object.remove("chief"));
            } else if member_slugs(room).iter().any(|slug| slug == chief) {
                room["chief"] = json!(chief);
            } else {
                return Err(ApiError::bad_request("chief must be a member"));
            }
        }
        if let Some(value) = patch.get("notifications") {
            let on = value
                .as_bool()
                .ok_or_else(|| ApiError::bad_request("notifications must be a boolean"))?;
            room["notifications"] = json!(on);
        }
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok(stored)
    }

    /// Remembers the thread made for one member of the group.
    pub async fn set_thread(&self, room_id: &str, slug: &str, thread: &str) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        if !room["threads"].is_object() {
            room["threads"] = json!({});
        }
        room["threads"][slug] = json!(thread);
        write_doc(&self.path, &doc).await
    }

    /// Nothing stored, or the Bot's one-to-one thread, is not a thread for this group.
    pub fn group_thread_missing(stored: &str, main_thread_id: &str) -> bool {
        stored.is_empty() || stored == main_thread_id
    }

    /// Marks the room seen now. A later Bot message is unread again.
    pub async fn seen(&self, room_id: &str) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        room["lastSeenAt"] = json!(crate::vault::now_iso());
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok(stored)
    }

    pub async fn delete(&self, room_id: &str) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let list = doc
            .get_mut("rooms")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
        let before = list.len();
        list.retain(|room| room["id"].as_str() != Some(room_id));
        if list.len() == before {
            return Err(ApiError::not_found("No such group"));
        }
        write_doc(&self.path, &doc).await
    }
}
