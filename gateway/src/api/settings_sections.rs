//! Sidebar sections stored at `settings.sections`.
//!
//! An empty array is today's sidebar. A bad value is rejected on write.
//! On read, a missing key stays an empty array and an invalid value becomes
//! one, so the rest of Settings still loads. A valid array is returned as stored.

use crate::platform::error::ApiError;
use serde_json::{json, Value};
use std::collections::HashSet;

const MAX_CUSTOM_SECTIONS: usize = 12;
const MAX_SECTION_ITEMS: usize = 200;
const MAX_DOCUMENT_ITEMS: usize = 500;
const MAX_NAME_CHARS: usize = 40;

/// The `sections` array to show. Missing and invalid both read back as `[]`.
pub(crate) fn sections_view(saved: &Value) -> Value {
    match saved.get("sections") {
        Some(sections) if check_sections(sections).is_ok() => sections.clone(),
        _ => json!([]),
    }
}

pub(crate) fn require_sections(value: &Value) -> Result<(), ApiError> {
    check_sections(value).map_err(ApiError::bad_request)
}

fn check_sections(value: &Value) -> Result<(), String> {
    let sections = value
        .as_array()
        .ok_or_else(|| "sections must be an array".to_string())?;
    let mut ids = HashSet::new();
    let mut custom = 0usize;
    let mut items = 0usize;
    for (index, section) in sections.iter().enumerate() {
        let (id, count) = check_section(index, section)?;
        let custom_id = is_custom_id(&id);
        if !ids.insert(id) {
            return Err(format!("sections[{index}].id is already used"));
        }
        if custom_id {
            custom += 1;
        }
        items += count;
    }
    if custom > MAX_CUSTOM_SECTIONS {
        return Err("sections has too many custom sections".to_string());
    }
    if items > MAX_DOCUMENT_ITEMS {
        return Err("sections has too many items".to_string());
    }
    Ok(())
}

fn check_section(index: usize, section: &Value) -> Result<(String, usize), String> {
    let obj = section
        .as_object()
        .ok_or_else(|| format!("sections[{index}] must be an object"))?;
    for key in obj.keys() {
        if !matches!(key.as_str(), "id" | "name" | "collapsed" | "sort" | "items") {
            return Err(format!("sections[{index}] has an unknown field"));
        }
    }
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_section_id(id))
        .ok_or_else(|| format!("sections[{index}].id is invalid"))?;
    check_name(index, id, obj.get("name"))?;
    if !obj.get("collapsed").is_some_and(Value::is_boolean) {
        return Err(format!("sections[{index}].collapsed must be true or false"));
    }
    check_sort(index, id, obj.get("sort"))?;
    let count = check_items(index, id, obj.get("items"))?;
    Ok((id.to_string(), count))
}

fn check_name(index: usize, id: &str, name: Option<&Value>) -> Result<(), String> {
    let name = name
        .and_then(Value::as_str)
        .ok_or_else(|| format!("sections[{index}].name is invalid"))?;
    if id == "chats" || id == "hidden" {
        return if name.is_empty() {
            Ok(())
        } else {
            Err(format!("sections[{index}].name is invalid"))
        };
    }
    if valid_custom_name(name) {
        Ok(())
    } else {
        Err(format!("sections[{index}].name is invalid"))
    }
}

/// A custom name is already trimmed, 1 to 40 characters, with no ASCII control
/// character. That includes U+001C through U+001F and U+007F.
fn valid_custom_name(name: &str) -> bool {
    if name != name.trim() {
        return false;
    }
    let mut count = 0usize;
    for c in name.chars() {
        if c.is_ascii_control() {
            return false;
        }
        count += 1;
        if count > MAX_NAME_CHARS {
            return false;
        }
    }
    count >= 1
}

fn check_sort(index: usize, id: &str, sort: Option<&Value>) -> Result<(), String> {
    let sort = sort
        .and_then(Value::as_str)
        .ok_or_else(|| format!("sections[{index}].sort must be recent or manual"))?;
    match sort {
        "manual" => Ok(()),
        "recent" if !is_custom_id(id) => Ok(()),
        "recent" => Err(format!("sections[{index}].sort must be manual")),
        _ => Err(format!("sections[{index}].sort must be recent or manual")),
    }
}

fn check_items(index: usize, id: &str, items: Option<&Value>) -> Result<usize, String> {
    let items = items
        .and_then(Value::as_array)
        .ok_or_else(|| format!("sections[{index}].items must be an array"))?;
    if id == "hidden" && !items.is_empty() {
        return Err(format!("sections[{index}].items must be empty"));
    }
    if items.len() > MAX_SECTION_ITEMS {
        return Err(format!("sections[{index}].items is too long"));
    }
    for (item_index, item) in items.iter().enumerate() {
        if !item.as_str().is_some_and(valid_item) {
            return Err(format!("sections[{index}].items[{item_index}] is invalid"));
        }
    }
    Ok(items.len())
}

fn valid_section_id(id: &str) -> bool {
    id == "chats" || id == "hidden" || is_custom_id(id)
}

/// `s` plus 7 lowercase letters or digits.
fn is_custom_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 8
        && bytes[0] == b's'
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn valid_item(item: &str) -> bool {
    if let Some(slug) = item.strip_prefix("bot:") {
        return crate::bots::valid_slug(slug);
    }
    if let Some(id) = item.strip_prefix("group:") {
        return valid_group_id(id);
    }
    false
}

/// Group ids are the hyphenated lowercase form `Uuid::to_string` writes.
fn valid_group_id(id: &str) -> bool {
    match uuid::Uuid::parse_str(id) {
        Ok(parsed) => parsed.to_string() == id,
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "settings_sections_tests.rs"]
mod tests;
