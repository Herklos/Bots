use super::{EFFORTS, MODEL_DENIED, MODE_DENIED, RUNTIME_MODES};
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{json, Value};

pub(super) fn resolve_model(
    input: &Value,
    parent: &Value,
    policy: &Value,
) -> ApiResult<Option<Value>> {
    let allow_models = &policy["allowlist"]["models"];
    let allow_efforts = &policy["allowlist"]["efforts"];
    let default_effort = policy["defaults"]["effort"].as_str();
    if let Some(raw) = input.get("model").filter(|value| !value.is_null()) {
        require_selection(raw)?;
        let effort = if has_effort(raw) {
            None
        } else {
            default_effort
        };
        return Ok(Some(finish_model(
            with_effort(raw, effort),
            allow_models,
            allow_efforts,
            true,
        )?));
    }
    if policy["defaults"]["model"].is_object() {
        return Ok(Some(finish_model(
            with_effort(&policy["defaults"]["model"], default_effort),
            allow_models,
            allow_efforts,
            true,
        )?));
    }
    if parent["model"].is_object() {
        if model_allowed(&parent["model"], allow_models) {
            return Ok(Some(finish_model(
                with_effort(&parent["model"], default_effort),
                allow_models,
                allow_efforts,
                false,
            )?));
        }
        return Ok(Some(finish_model(
            fallback_model(allow_models, default_effort)?,
            allow_models,
            allow_efforts,
            false,
        )?));
    }
    if default_effort.is_some() || !model_allowed(&default_haiku(), allow_models) {
        let base = if model_allowed(&default_haiku(), allow_models) {
            with_effort(&default_haiku(), default_effort)
        } else {
            fallback_model(allow_models, default_effort)?
        };
        return Ok(Some(finish_model(
            base,
            allow_models,
            allow_efforts,
            false,
        )?));
    }
    Ok(None)
}

pub(super) fn resolve_mode(
    input: &Value,
    parent: &Value,
    policy: &Value,
) -> ApiResult<Option<String>> {
    let allow = &policy["allowlist"]["runtimeModes"];
    let (mode, explicit) = if let Some(mode) = input.get("runtimeMode").and_then(Value::as_str) {
        (mode, true)
    } else if let Some(mode) = policy["defaults"]["runtimeMode"].as_str() {
        (mode, false)
    } else if let Some(mode) = parent.get("runtimeMode").and_then(Value::as_str) {
        (mode, false)
    } else {
        return Ok(None);
    };
    if listed(allow, mode) {
        return Ok(Some(mode.to_string()));
    }
    if explicit {
        return Err(ApiError::bad_request(MODE_DENIED));
    }
    for candidate in RUNTIME_MODES {
        if listed(allow, candidate) {
            return Ok(Some(candidate.to_string()));
        }
    }
    Err(ApiError::bad_request(MODE_DENIED))
}

fn finish_model(
    model: Value,
    allow_models: &Value,
    allow_efforts: &Value,
    explicit: bool,
) -> ApiResult<Value> {
    if !model_allowed(&model, allow_models) {
        return Err(ApiError::bad_request(MODEL_DENIED));
    }
    let effort = crate::harness::providers::effort_of(&model);
    if effort.is_empty() || listed(allow_efforts, &effort) {
        return Ok(model);
    }
    if explicit {
        return Err(ApiError::bad_request(
            "That effort isn't allowed for this Bot's helpers.",
        ));
    }
    let Some(replacement) = ["medium", "low", "high"]
        .into_iter()
        .find(|effort| listed(allow_efforts, effort))
    else {
        return Err(ApiError::bad_request(
            "That effort isn't allowed for this Bot's helpers.",
        ));
    };
    Ok(with_effort(&model, Some(replacement)))
}

fn fallback_model(allow: &Value, effort: Option<&str>) -> ApiResult<Value> {
    let haiku = default_haiku();
    let Some(items) = allow.as_array() else {
        return Ok(with_effort(&haiku, effort));
    };
    if items
        .iter()
        .any(|row| row["instanceId"] == haiku["instanceId"] && row["model"] == haiku["model"])
    {
        return Ok(with_effort(&haiku, effort));
    }
    let Some(first) = items.first() else {
        return Err(ApiError::bad_request(MODEL_DENIED));
    };
    if first["instanceId"].as_str().is_none() || first["model"].as_str().is_none() {
        return Err(ApiError::bad_request(MODEL_DENIED));
    }
    Ok(with_effort(
        &json!({ "instanceId": first["instanceId"], "model": first["model"] }),
        effort.or(Some("medium")),
    ))
}

fn with_effort(model: &Value, effort: Option<&str>) -> Value {
    let Some(effort) = effort else {
        return model.clone();
    };
    let mut out = model.clone();
    out["options"] = json!([{ "id": "effort", "value": effort }]);
    out
}

fn default_haiku() -> Value {
    crate::bots::provisioner::default_model()
}

fn has_effort(model: &Value) -> bool {
    let effort = crate::harness::providers::effort_of(model);
    EFFORTS.contains(&effort.as_str())
}

pub(super) fn require_selection(model: &Value) -> ApiResult<()> {
    if model["instanceId"]
        .as_str()
        .is_some_and(|id| !id.is_empty())
        && model["model"].as_str().is_some_and(|name| !name.is_empty())
    {
        return Ok(());
    }
    Err(ApiError::bad_request(
        "model must include an instanceId and a model",
    ))
}

pub(super) fn model_allowed(model: &Value, allow: &Value) -> bool {
    let Some(items) = allow.as_array() else {
        return true;
    };
    let instance = model["instanceId"].as_str().unwrap_or("");
    let name = model["model"].as_str().unwrap_or("");
    items.iter().any(|row| {
        row["instanceId"].as_str() == Some(instance) && row["model"].as_str() == Some(name)
    })
}

/// Null or a missing list means every value is allowed.
pub(super) fn listed(allow: &Value, needle: &str) -> bool {
    match allow.as_array() {
        None => true,
        Some(items) => items.iter().any(|item| item.as_str() == Some(needle)),
    }
}
