use crate::platform::error::{ApiError, ApiResult};
use serde_json::{Map, Value};

pub(super) fn validate_policy(value: &Value, durable: bool) -> ApiResult<()> {
    let label = if durable { "subagents" } else { "childThreads" };
    let obj = object(value, label)?;
    let keys: &[&str] = if durable {
        &["enabled", "maxCount", "when", "defaults", "allowlist"]
    } else {
        &["enabled", "maxConcurrent", "when", "defaults", "allowlist"]
    };
    reject_unknown(obj, label, keys)?;
    bool_field(obj, label, "enabled")?;
    if durable {
        int_field(obj, label, "maxCount", 32)?;
    } else {
        int_field(obj, label, "maxConcurrent", 16)?;
    }
    when_field(obj, label)?;
    if let Some(defaults) = obj.get("defaults") {
        validate_defaults(defaults, label, durable)?;
    }
    if let Some(allow) = obj.get("allowlist") {
        validate_allowlist(allow, label)?;
    }
    check_ceiling(obj, label)
}

fn validate_defaults(value: &Value, label: &str, durable: bool) -> ApiResult<()> {
    let path = format!("{label}.defaults");
    let obj = object(value, &path)?;
    let keys: &[&str] = if durable {
        &[
            "model",
            "runtimeMode",
            "effort",
            "inheritParentPrompt",
            "toolAllow",
        ]
    } else {
        &["model", "runtimeMode", "effort"]
    };
    reject_unknown(obj, &path, keys)?;
    if let Some(model) = obj.get("model") {
        validate_model(model, &format!("{path}.model"))?;
    }
    if let Some(mode) = obj.get("runtimeMode") {
        enum_or_null(mode, &format!("{path}.runtimeMode"), &super::RUNTIME_MODES)?;
    }
    if let Some(effort) = obj.get("effort") {
        enum_or_null(effort, &format!("{path}.effort"), &super::EFFORTS)?;
    }
    if durable {
        bool_field(obj, &path, "inheritParentPrompt")?;
        if let Some(tools) = obj.get("toolAllow") {
            validate_tool_allow(Some(tools), &format!("{path}.toolAllow"))?;
        }
    }
    Ok(())
}

fn validate_allowlist(value: &Value, label: &str) -> ApiResult<()> {
    let path = format!("{label}.allowlist");
    let obj = object(value, &path)?;
    reject_unknown(obj, &path, &["models", "runtimeModes", "efforts"])?;
    if let Some(models) = obj.get("models") {
        validate_model_list(models, &format!("{path}.models"))?;
    }
    if let Some(modes) = obj.get("runtimeModes") {
        enum_list(
            modes,
            &format!("{path}.runtimeModes"),
            &super::RUNTIME_MODES,
            "mode",
        )?;
    }
    if let Some(efforts) = obj.get("efforts") {
        enum_list(
            efforts,
            &format!("{path}.efforts"),
            &super::EFFORTS,
            "effort",
        )?;
    }
    Ok(())
}

fn check_ceiling(obj: &Map<String, Value>, label: &str) -> ApiResult<()> {
    let defaults = obj.get("defaults").and_then(Value::as_object);
    let allow = obj.get("allowlist").and_then(Value::as_object);
    if let Some(mode) = defaults
        .and_then(|fields| fields.get("runtimeMode"))
        .and_then(Value::as_str)
    {
        if let Some(list) = allow.and_then(|fields| fields.get("runtimeModes")) {
            if !super::model::listed(list, mode) {
                return Err(ApiError::bad_request(format!(
                    "{label}.defaults.runtimeMode isn't in the allowlist"
                )));
            }
        }
    }
    if let Some(effort) = defaults
        .and_then(|fields| fields.get("effort"))
        .and_then(Value::as_str)
    {
        if let Some(list) = allow.and_then(|fields| fields.get("efforts")) {
            if !super::model::listed(list, effort) {
                return Err(ApiError::bad_request(format!(
                    "{label}.defaults.effort isn't in the allowlist"
                )));
            }
        }
    }
    if let Some(model) = defaults
        .and_then(|fields| fields.get("model"))
        .filter(|model| model.is_object())
    {
        if let Some(list) = allow.and_then(|fields| fields.get("models")) {
            if !super::model::model_allowed(model, list) {
                return Err(ApiError::bad_request(super::MODEL_DENIED));
            }
        }
        let effort = crate::harness::providers::effort_of(model);
        if !effort.is_empty() {
            if let Some(list) = allow.and_then(|fields| fields.get("efforts")) {
                if !super::model::listed(list, &effort) {
                    return Err(ApiError::bad_request(format!(
                        "{label}.defaults.model effort isn't in the allowlist"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_model(value: &Value, label: &str) -> ApiResult<()> {
    match value {
        Value::Null => Ok(()),
        Value::Object(_) => {
            super::model::require_selection(value)?;
            if let Some(options) = value.get("options") {
                let Some(items) = options.as_array() else {
                    return Err(ApiError::bad_request(format!(
                        "{label} options must be a list"
                    )));
                };
                for item in items {
                    let Some(option) = item.as_object() else {
                        return Err(ApiError::bad_request(format!(
                            "{label} options must be objects"
                        )));
                    };
                    if option.get("id").and_then(Value::as_str) == Some("effort") {
                        let effort = option.get("value").and_then(Value::as_str).unwrap_or("");
                        if !super::EFFORTS.contains(&effort) {
                            return Err(ApiError::bad_request(format!(
                                "{label} has an unknown effort"
                            )));
                        }
                    }
                }
            }
            Ok(())
        }
        _ => Err(ApiError::bad_request(format!(
            "{label} must be a model selection or null"
        ))),
    }
}

fn validate_model_list(value: &Value, label: &str) -> ApiResult<()> {
    match value {
        Value::Null => Ok(()),
        Value::Array(items) => {
            for item in items {
                if item["instanceId"].as_str().is_none() || item["model"].as_str().is_none() {
                    return Err(ApiError::bad_request(format!(
                        "{label} entries need an instanceId and a model"
                    )));
                }
                if item["instanceId"].as_str().unwrap_or("").is_empty()
                    || item["model"].as_str().unwrap_or("").is_empty()
                {
                    return Err(ApiError::bad_request(format!(
                        "{label} entries need an instanceId and a model"
                    )));
                }
            }
            Ok(())
        }
        _ => Err(ApiError::bad_request(format!(
            "{label} must be a list or null"
        ))),
    }
}

pub(super) fn validate_tool_allow(value: Option<&Value>, label: &str) -> ApiResult<()> {
    match value {
        None | Some(Value::Null) => Ok(()),
        Some(Value::Array(items)) if items.iter().all(Value::is_string) => Ok(()),
        Some(_) => Err(ApiError::bad_request(format!(
            "{label} must be a list of strings or null"
        ))),
    }
}

fn object<'a>(value: &'a Value, label: &str) -> ApiResult<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| ApiError::bad_request(format!("{label} must be an object")))
}

fn reject_unknown(obj: &Map<String, Value>, label: &str, keys: &[&str]) -> ApiResult<()> {
    if let Some(key) = obj.keys().find(|key| !keys.contains(&key.as_str())) {
        return Err(ApiError::bad_request(format!(
            "{label} has an unknown field {key}"
        )));
    }
    Ok(())
}

fn bool_field(obj: &Map<String, Value>, label: &str, key: &str) -> ApiResult<()> {
    match obj.get(key) {
        None | Some(Value::Bool(_)) => Ok(()),
        Some(_) => Err(ApiError::bad_request(format!(
            "{label}.{key} must be a boolean"
        ))),
    }
}

fn int_field(obj: &Map<String, Value>, label: &str, key: &str, max: i64) -> ApiResult<()> {
    let Some(value) = obj.get(key) else {
        return Ok(());
    };
    let Some(number) = value.as_i64() else {
        return Err(ApiError::bad_request(format!(
            "{label}.{key} must be a whole number from 0 to {max}"
        )));
    };
    if (0..=max).contains(&number) {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "{label}.{key} must be a whole number from 0 to {max}"
        )))
    }
}

fn when_field(obj: &Map<String, Value>, label: &str) -> ApiResult<()> {
    match obj.get("when") {
        None => Ok(()),
        Some(Value::String(text)) if text.chars().count() <= super::WHEN_LIMIT => Ok(()),
        Some(Value::String(_)) => Err(ApiError::bad_request(format!(
            "{label}.when must be at most {} characters",
            super::WHEN_LIMIT
        ))),
        Some(_) => Err(ApiError::bad_request(format!(
            "{label}.when must be a string"
        ))),
    }
}

fn enum_or_null(value: &Value, label: &str, allowed: &[&str]) -> ApiResult<()> {
    match value {
        Value::Null => Ok(()),
        Value::String(text) if allowed.contains(&text.as_str()) => Ok(()),
        _ => Err(ApiError::bad_request(format!(
            "{label} must be one of: {}",
            allowed.join(", ")
        ))),
    }
}

fn enum_list(value: &Value, label: &str, allowed: &[&str], noun: &str) -> ApiResult<()> {
    let Some(items) = value.as_array() else {
        return Err(ApiError::bad_request(format!("{label} must be a list")));
    };
    for item in items {
        let Some(text) = item.as_str() else {
            return Err(ApiError::bad_request(format!(
                "{label} has an unknown {noun}"
            )));
        };
        if !allowed.contains(&text) {
            return Err(ApiError::bad_request(format!(
                "{label} has an unknown {noun}"
            )));
        }
    }
    Ok(())
}
