//! JSON-RPC structure errors are distinct from operation validation failures.
use super::*;

pub(super) fn error(id: Option<&Value>, code: i32, message: &str) -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})),
    )
        .into_response()
}

pub(super) fn validate(msg: &Value) -> Result<(), (i32, &'static str)> {
    if !msg.is_object()
        || msg["jsonrpc"] != "2.0"
        || !msg["method"].is_string()
        || msg
            .get("id")
            .is_some_and(|id| !id.is_string() && !id.is_number())
    {
        return Err((-32600, "Invalid Request"));
    }
    if msg.get("params").is_some_and(|p| !p.is_object()) {
        return Err((-32602, "params must be an object"));
    }
    // Notifications never execute management operations and have no reply.
    if msg.get("id").is_none() {
        return Ok(());
    }
    let params = &msg["params"];
    match msg["method"].as_str().unwrap() {
        "initialize"
            if !params["protocolVersion"].is_string()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string() =>
        {
            Err((-32602, "Invalid initialize parameters"))
        }
        "tools/call"
            if !params["name"].is_string()
                || params.get("arguments").is_some_and(|a| !a.is_object()) =>
        {
            Err((-32602, "Invalid tools/call parameters"))
        }
        "tools/list" if params.get("cursor").is_some() => {
            Err((-32602, "No continuation cursor is available"))
        }
        _ => Ok(()),
    }
}
