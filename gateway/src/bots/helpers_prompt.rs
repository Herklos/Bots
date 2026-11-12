//! The helper policy, written into a Bot's managed prompt.
//! A subagent has no policy of its own, so its prompt keeps the standing child-thread note.

use crate::bots::agent_context::{child_threads_section, plain_prompt, CHILD_THREADS};
use crate::bots::helpers_policy::{
    at_limit_message, child_threads_of, default_child_threads, default_subagents, subagents_of,
    HELPERS_OFF,
};
use crate::bots::is_subagent;
use serde_json::Value;

pub(crate) fn subagents_section(bot: &Value, children: &[&Value]) -> String {
    if is_subagent(bot) {
        return String::new();
    }
    let policy = subagents_of(bot);
    let max = policy["maxCount"].as_i64().unwrap_or(8);
    let count = i64::try_from(children.len()).unwrap_or(i64::MAX);
    let mut lines = String::from("\n## Subagents\n\n");
    if children.is_empty() {
        lines.push_str("You have no lasting helpers yet.\n");
    } else {
        for child in children {
            let name = child["name"].as_str().unwrap_or("Bot");
            let slug = child["slug"].as_str().unwrap_or("");
            lines.push_str(&format!("- {name} (`{slug}`)\n"));
        }
        lines.push('\n');
    }
    if policy["enabled"] != true {
        lines.push_str(HELPERS_OFF);
        lines.push_str(
            " Do not create one.\nTo see helpers that already exist, use `subagent_list`.\n",
        );
        return lines;
    }
    lines.push_str(&when_line(&policy["when"], &default_subagents()["when"]));
    lines.push_str("\n\n");
    lines.push_str(&defaults_line(&policy, true));
    lines.push_str(&ceiling_line(&policy, &default_subagents()));
    if count >= max {
        lines.push_str(&at_limit_message(count));
        lines.push_str(" Do not create another.\nTo list them, use `subagent_list`.\n");
        return lines;
    }
    lines.push_str(&format!("You may have up to {max} lasting helpers.\n"));
    lines.push_str(
        "To list lasting helpers, use `subagent_list`. Create one with `subagent_create` (its own folder, project, and main thread). A new helper follows this Bot's helper defaults. Work that ends with this turn stays a T3 child thread, and is not a lasting Subagent.\n",
    );
    lines
}

pub(crate) fn child_threads_section_for(bot: &Value) -> String {
    if is_subagent(bot) {
        return child_threads_section();
    }
    let policy = child_threads_of(bot);
    if policy["enabled"] != true {
        return "\n## Child threads\n\nOne-off child threads are turned off for this Bot. Do not start one.\n"
            .to_string();
    }
    let max = policy["maxConcurrent"].as_i64().unwrap_or(4);
    let when = when_line(&policy["when"], &default_child_threads()["when"]);
    let defaults = defaults_line(&policy, false);
    let ceiling = ceiling_line(&policy, &default_child_threads());
    format!("\n## Child threads\n\n{when}\n\nStart at most {max} of these at once.\n{defaults}{ceiling}\n{CHILD_THREADS}\n")
}

fn when_line(value: &Value, fallback: &Value) -> String {
    let clean = flat(value.as_str().unwrap_or(""));
    if clean.is_empty() {
        flat(fallback.as_str().unwrap_or(""))
    } else {
        clean
    }
}

fn defaults_line(policy: &Value, durable: bool) -> String {
    let defaults = &policy["defaults"];
    let mut parts = Vec::new();
    if let Some(name) = defaults["model"]["model"].as_str() {
        parts.push(format!("model {name}"));
    }
    if let Some(mode) = defaults["runtimeMode"].as_str() {
        parts.push(format!("approval {mode}"));
    }
    if let Some(effort) = defaults["effort"].as_str() {
        parts.push(format!("effort {effort}"));
    }
    if durable && defaults["inheritParentPrompt"] == true {
        parts.push("your prompt copied in".to_string());
    }
    if durable {
        if let Some(tools) = defaults["toolAllow"].as_array() {
            let names = strings(tools);
            if !names.is_empty() {
                parts.push(format!("tools {}", names.join(", ")));
            }
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    let subject = if durable {
        "A new helper"
    } else {
        "A child thread"
    };
    format!("{subject} starts with {}.\n", flat(&parts.join(", ")))
}

/// A missing list means every value is allowed, so the prompt stays quiet.
/// A list that differs from the product default is the ceiling the owner set.
fn ceiling_line(policy: &Value, full: &Value) -> String {
    let allow = &policy["allowlist"];
    let standard = &full["allowlist"];
    let mut parts = Vec::new();
    if let Some(models) = allow["models"].as_array() {
        let names = models
            .iter()
            .filter_map(|row| row["model"].as_str())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if names.is_empty() {
            parts.push("no models".to_string());
        } else {
            parts.push(format!("models {}", names.join(", ")));
        }
    }
    if allow["runtimeModes"] != standard["runtimeModes"] {
        parts.push(format!("approval {}", join_strings(&allow["runtimeModes"])));
    }
    if allow["efforts"] != standard["efforts"] {
        parts.push(format!("effort {}", join_strings(&allow["efforts"])));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("Stay within {}.\n", flat(&parts.join("; ")))
}

fn join_strings(value: &Value) -> String {
    let names = strings(value.as_array().map(Vec::as_slice).unwrap_or(&[]));
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn strings(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn flat(text: &str) -> String {
    plain_prompt(&text.replace(['\r', '\n'], " "))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
