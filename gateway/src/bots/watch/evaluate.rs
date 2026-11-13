//! What a watch compares, and when it fires (`docs/webhook-triggers-plan.md`, Phase 4).
//!
//! A poll produces an [`Eval`]: a short value for the row and the delivery body, and a
//! boolean for the checks that have one (`json` with an operator, `contains`, `regex`).
//! Firing is then a pure function of the fire mode, the eval, and the stored state.
//! The cooldown lives in T3: the hidden routine coalesces on the watch id with a window
//! equal to the cooldown, so a stream of matches becomes one delivery.

use serde_json::{json, Value};

/// The regex crate's compile size limit: bounded, with no catastrophic pattern.
pub(crate) const REGEX_SIZE_LIMIT: usize = 1 << 20;
/// The row and the delivery body carry at most this much of a value.
pub(crate) const SNIPPET_MAX: usize = 1024;

pub(crate) struct Eval {
    /// The short value this poll produced: the hash, the status class, the pointed-out
    /// JSON value, the literal match, or the found/absent mark.
    pub value: String,
    /// The check's boolean answer, when it has one. `None` for pure change checks.
    pub truth: Option<bool>,
}

impl Eval {
    pub(crate) fn snapshot(&self) -> String {
        snippet(&self.value)
    }
}

/// Everything a poll decided, derived from the fire mode, the check, the fresh result,
/// and the stored state.
pub(crate) fn outcome(fire: &Value, check: &Value, eval: &Eval, state: &Value) -> Option<Value> {
    // The first poll establishes the baseline: no value to compare against yet,
    // so nothing has changed. `lastAt` is set on every completed poll.
    if state["lastAt"].as_str().map(str::is_empty).unwrap_or(true) {
        return None;
    }
    let fired = match fire["mode"].as_str().unwrap_or("on_change") {
        // on_true: only the false-to-true edge.
        "on_true" if has_truth(check) => eval.truth == Some(true) && state["lastTrue"] != true,
        // every: each poll the check matches, still capped by the cooldown in T3.
        "every" if has_truth(check) => eval.truth == Some(true),
        _ => {
            if has_truth(check) {
                state["lastTrue"] != json!(eval.truth)
            } else {
                state["last"].as_str() != Some(eval.value.as_str())
            }
        }
    };
    if fired {
        Some(json!({
            "reason": reason(fire, check, state),
            "previous": previous(check, state),
            "current": eval.snapshot(),
        }))
    } else {
        None
    }
}

fn has_truth(check: &Value) -> bool {
    matches!(check["type"].as_str(), Some("contains") | Some("regex"))
        || (matches!(check["type"].as_str(), Some("json"))
            && check["op"].as_str() != Some("changes"))
}

fn reason(fire: &Value, check: &Value, state: &Value) -> String {
    let name = check["type"].as_str().unwrap_or("value");
    let mode = fire["mode"].as_str().unwrap_or("on_change");
    match (name, mode) {
        ("status", _) => "the status class changed".to_string(),
        ("hash", _) => "the content changed".to_string(),
        ("json", "on_true") | ("json", "every") => {
            format!(
                "the value at {} matched",
                check["pointer"].as_str().unwrap_or("")
            )
        }
        ("json", _) => {
            format!(
                "the value at {} changed",
                check["pointer"].as_str().unwrap_or("")
            )
        }
        ("contains", _) => {
            if state["lastTrue"] == true {
                format!(
                    "{} disappeared",
                    check["substring"].as_str().unwrap_or("the substring")
                )
            } else {
                format!(
                    "{} appeared",
                    check["substring"].as_str().unwrap_or("the substring")
                )
            }
        }
        ("regex", _) => "the pattern match changed".to_string(),
        _ => "value changed".to_string(),
    }
}

fn previous(check: &Value, state: &Value) -> String {
    let state_value = state["last"].as_str().unwrap_or("");
    if has_truth(check) {
        snippet(&format!(
            "{}\n{}",
            state_value,
            bool_word(state["lastTrue"] == true)
        ))
    } else {
        snippet(state_value)
    }
}

fn bool_word(value: bool) -> &'static str {
    if value {
        "found"
    } else {
        "absent"
    }
}

pub(crate) fn snippet(text: &str) -> String {
    if text.len() <= SNIPPET_MAX {
        text.to_string()
    } else {
        let mut cut = SNIPPET_MAX;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}…", &text[..cut])
    }
}

/// Run the check over one poll's result. `status` is 0 for a file target.
pub(crate) fn evaluate(check: &Value, status: u16, body: &[u8]) -> Result<Eval, String> {
    match check["type"].as_str() {
        Some("hash") => Ok(Eval {
            value: hex::encode(sha2_digest(body)),
            truth: None,
        }),
        Some("status") => {
            let class = match status {
                200..=299 => "2xx",
                300..=399 => "3xx",
                400..=499 => "4xx",
                500..=599 => "5xx",
                _ => "none",
            };
            Ok(Eval {
                value: class.to_string(),
                truth: None,
            })
        }
        Some("json") => {
            let pointer = check["pointer"].as_str().unwrap_or("");
            let parsed: Value =
                serde_json::from_slice(body).map_err(|_bad| "The body is not JSON.".to_string())?;
            let found = pointer_value(&parsed, pointer);
            let value_text = match &found {
                None => "absent".to_string(),
                Some(value) => match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                },
            };
            match check["op"].as_str() {
                Some("equals") => Ok(Eval {
                    value: value_text,
                    truth: Some(found.as_ref() == Some(&check["value"])),
                }),
                Some("not_equals") => Ok(Eval {
                    value: value_text,
                    truth: Some(found.as_ref() != Some(&check["value"])),
                }),
                Some("gt") | Some("lt") => {
                    let number = found
                        .as_ref()
                        .and_then(Value::as_f64)
                        .ok_or("Compare gt and lt against a number.")?;
                    let bound = check["value"].as_f64().unwrap_or(f64::NAN);
                    if check["op"] == "gt" {
                        Ok(Eval {
                            value: value_text,
                            truth: Some(number > bound),
                        })
                    } else {
                        Ok(Eval {
                            value: value_text,
                            truth: Some(number < bound),
                        })
                    }
                }
                _ => Ok(Eval {
                    value: value_text,
                    truth: None,
                }),
            }
        }
        Some("contains") => {
            let needle = check["substring"].as_str().unwrap_or("").as_bytes();
            let present = !needle.is_empty()
                && body.len() >= needle.len()
                && body.windows(needle.len()).any(|w| w == needle);
            Ok(Eval {
                value: bool_word(present).to_string(),
                truth: Some(present),
            })
        }
        Some("regex") => {
            let text = String::from_utf8_lossy(body).to_string();
            let pattern = check["regex"].as_str().ok_or("The pattern went missing.")?;
            let regex = regex::RegexBuilder::new(pattern)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|_bad| "The pattern does not compile.")?;
            match regex.find(&text) {
                Some(found) => Ok(Eval {
                    value: found.as_str().to_string(),
                    truth: Some(true),
                }),
                None => Ok(Eval {
                    value: bool_word(false).to_string(),
                    truth: Some(false),
                }),
            }
        }
        _ => Err("A watch needs a check.".to_string()),
    }
}

fn sha2_digest(bytes: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.to_vec()
}

/// A small RFC 6901 pointer: only `/a/b` and `/-`-free object and array indexing.
pub(crate) fn pointer_value(doc: &Value, pointer: &str) -> Option<Value> {
    let mut current = doc;
    for token in pointer.trim_start_matches('/').split('/') {
        if token.is_empty() {
            return None;
        }
        current = match current {
            Value::Object(fields) => fields.get(token)?,
            Value::Array(items) => {
                let index: usize = token.parse().ok()?;
                items.get(index)?
            }
            _ => return None,
        };
    }
    Some(current.clone())
}
