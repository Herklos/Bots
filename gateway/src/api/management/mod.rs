//! Management transport. Dispatches only registered application routes, never a proxy.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use tower::ServiceExt;
static CALLS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
mod content;
mod contracts;
mod credentials;
mod errors;
use errors::ToolError;
mod permissions;
mod protocol;
pub(crate) use credentials::*;
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct Operation {
    pub path: &'static str,
    pub method: &'static str,
    pub name: String,
}
impl Operation {
    pub(crate) fn new(path: &'static str, method: &'static str, handler: &'static str) -> Self {
        Self {
            path,
            method,
            name: format!("hbots_{}", handler.split("::").last().unwrap()),
        }
    }
    fn definition(&self) -> Value {
        json!({"name":self.name,"description":format!("{} {}. Required permission: {}. Supply API parameters in path/query and the JSON request in body.",self.method.to_uppercase(),self.path,self.permission()),"inputSchema":self.input_schema(),"outputSchema":self.output_schema(),"annotations":{"readOnlyHint":false,"destructiveHint":self.method != "get","openWorldHint":true}})
    }
}

fn local(h: &HeaderMap, peer: SocketAddr) -> bool {
    peer.ip().is_loopback()
        && ![
            "forwarded",
            "x-forwarded-for",
            "x-forwarded-host",
            "cf-ray",
            "cf-connecting-ip",
            "cf-access-jwt-assertion",
        ]
        .iter()
        .any(|k| h.contains_key(*k))
        && h.get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|host| loopback_origin(&format!("http://{host}")))
        && h.get(header::ORIGIN)
            .is_none_or(|o| o.to_str().ok().is_some_and(loopback_origin))
}

pub(crate) async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(crate) async fn route(
    State(s): S,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<crate::network::exposure::Surface>>,
    method: Method,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    if surface
        .is_some_and(|Extension(surface)| surface == crate::network::exposure::Surface::Tailnet)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    serve(State(s), ConnectInfo(peer), method, h, body).await
}

pub(crate) async fn serve(
    State(s): S,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    if !local(&h, peer) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(_permit) = CALLS.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    if s.settings().await["managementMcp"]["enabled"] != true {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(actor) = credentials::authenticate(&s, &h).await else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    };
    if method != Method::POST {
        return (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response();
    }
    if h.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| {
            !v.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
        })
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    if h.get("mcp-protocol-version")
        .is_some_and(|v| v != "2025-06-18" && v != "2025-11-25")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let msg: Value =
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => return Json(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
            )
            .into_response(),
        };
    if let Err((code, message)) = protocol::validate(&msg) {
        return protocol::error(
            if code == -32600 { None } else { msg.get("id") },
            code,
            message,
        );
    }
    let id = msg.get("id").cloned();
    if id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let result = match msg["method"].as_str().unwrap_or("") {
        "initialize" => {
            json!({"protocolVersion":if msg["params"]["protocolVersion"] == "2025-11-25" { "2025-11-25" } else { "2025-06-18" },"capabilities":{"tools":{}},"serverInfo":{"name":"hbots-management","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools":super::router::application_routes().1.iter().filter(|op| actor.permits(op)).map(Operation::definition).collect::<Vec<_>>()})
        }
        "tools/call"
            if !super::router::application_routes()
                .1
                .iter()
                .any(|op| Some(op.name.as_str()) == msg["params"]["name"].as_str()) =>
        {
            return protocol::error(id.as_ref(), -32602, "Unknown tool")
        }
        "tools/call" => match execute_as(&s, peer, &msg["params"], &actor).await {
            Ok(v) => content::success(v),
            Err(e) => e.result(),
        },
        _ => return Json(
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
        )
        .into_response(),
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"jsonrpc":"2.0","id":id,"result":result})),
    )
        .into_response()
}

#[cfg(test)]
async fn execute(s: &Arc<AppState>, peer: SocketAddr, params: &Value) -> Result<Value, ToolError> {
    dispatch(
        s,
        peer,
        params,
        &permissions::Actor {
            id: "fixture".into(),
            scopes: vec!["admin".into()],
        },
    )
    .await
}

async fn execute_as(
    s: &Arc<AppState>,
    peer: SocketAddr,
    params: &Value,
    actor: &permissions::Actor,
) -> Result<Value, ToolError> {
    let current = credentials::current_actor(s, &actor.id)
        .await
        .ok_or_else(|| ToolError::new("permission_denied", "Credential was revoked"))?;
    dispatch(s, peer, params, &current).await
}

async fn dispatch(
    s: &Arc<AppState>,
    peer: SocketAddr,
    params: &Value,
    actor: &permissions::Actor,
) -> Result<Value, ToolError> {
    let (router, operations) = super::router::application_routes();
    let op = operations
        .iter()
        .find(|op| Some(op.name.as_str()) == params["name"].as_str())
        .ok_or("Unknown tool")?;
    if !actor.permits(op) {
        return Err(ToolError::new("permission_denied", "Permission denied"));
    }
    let empty = json!({});
    let args = params.get("arguments").unwrap_or(&empty);
    let schema = op.input_schema();
    let validator = jsonschema::validator_for(&schema)
        .map_err(|_error| ToolError::new("internal_error", "Invalid server request schema"))?;
    if let Err(error) = validator.validate(args) {
        return Err(format!("Invalid arguments at {}", error.instance_path()).into());
    }
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    if args
        .as_object()
        .unwrap()
        .keys()
        .any(|k| !["path", "query", "body", "bodyBase64", "contentType"].contains(&k.as_str()))
    {
        return Err("Unknown argument".into());
    }
    if op.path == "/api/settings" {
        if let Some(control) = args["body"].get("managementMcp") {
            if control != &json!({"enabled":false}) {
                return Err("Manage MCP activation through Settings".into());
            }
        }
    }
    let mut path = op.path.to_string();
    for segment in op.path.split('/').filter(|s| s.starts_with('{')) {
        let key = segment.trim_matches(['{', '}']);
        let value = args["path"][key]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("Missing path parameter {key}"))?;
        // Disallow traversal and path separators, including encoded forms.
        if value.contains(['/', '\\', '%', '?', '#']) || matches!(value, "." | "..") {
            return Err("Invalid path parameter".into());
        }
        let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
        path = path.replace(segment, &encoded);
    }
    if let Some(query) = args.get("query") {
        let map = query.as_object().ok_or("query must be an object")?;
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in map {
            serializer.append_pair(k, v.as_str().ok_or("query values must be strings")?);
        }
        let query = serializer.finish();
        if !query.is_empty() {
            path.push('?');
            path.push_str(&query);
        }
    }
    if args.get("body").is_some() && args.get("bodyBase64").is_some() {
        return Err("Supply body or bodyBase64".into());
    }
    let content_type = args["contentType"]
        .as_str()
        .unwrap_or(if op.name == "hbots_import_csv" {
            "text/csv"
        } else {
            "application/json"
        });
    let bytes = if let Some(raw) = args.get("bodyBase64") {
        STANDARD
            .decode(raw.as_str().ok_or("bodyBase64 must be a string")?)
            .map_err(|_error| "Invalid base64")?
    } else if op.name == "hbots_import_csv" {
        args["body"].as_str().unwrap_or("").as_bytes().to_vec()
    } else {
        args.get("body")
            .map(|b| serde_json::to_vec(b).unwrap())
            .unwrap_or_default()
    };
    if args.get("bodyBase64").is_some()
        && content_type
            .to_ascii_lowercase()
            .starts_with("application/json")
    {
        let body: Value = serde_json::from_slice(&bytes).map_err(|_error| "Invalid JSON body")?;
        let validator = jsonschema::validator_for(&schema["properties"]["body"])
            .map_err(|_error| ToolError::new("internal_error", "Invalid server body schema"))?;
        if let Err(error) = validator.validate(&body) {
            return Err(format!("Invalid body at {}", error.instance_path()).into());
        }
    }
    if op.path == "/api/settings" && args.get("bodyBase64").is_some() {
        return Err("Settings requires a JSON body".into());
    }
    if s.settings().await["managementMcp"]["enabled"] != true {
        return Err("Management MCP is disabled".into());
    }
    let target = path.split('?').next().unwrap_or(&path).to_owned();
    let correlation_id = uuid::Uuid::new_v4();
    let mut req = axum::http::Request::builder()
        .method(op.method.to_uppercase().as_str())
        .uri(path)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(bytes))
        .map_err(|_error| "Invalid request")?;
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        router.with_state(s.clone()).oneshot(req),
    )
    .await
    .map_err(|_error| {
        ToolError::new(
            "timeout",
            "Operation timed out; it may already have taken effect",
        )
    })?
    .map_err(|_error| ToolError::new("internal_error", "Dispatch failed"))?;
    let status = response.status();
    tracing::info!(credential = %actor.id, tool = %op.name, target, correlation_id = %correlation_id, status = status.as_u16(), "management MCP operation");
    let mime = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 16 << 20)
        .await
        .map_err(|_error| {
            ToolError::new(
                "payload_too_large",
                "Response exceeds 16 MiB; the operation may already have taken effect",
            )
        })?;
    if !status.is_success() {
        return Err(ToolError::api(status, &bytes));
    }
    let binary = matches!(
        op.name.as_str(),
        "hbots_download" | "hbots_export" | "hbots_library_file" | "hbots_get_icon"
    );
    let body = if !binary
        && mime
            .split(';')
            .next()
            .is_some_and(|v| v == "application/json" || v.ends_with("+json"))
    {
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|_error| ToolError::new("backend_unavailable", "Invalid API JSON response"))?
    } else {
        json!({"contentType":mime,"bodyBase64":STANDARD.encode(&bytes),"contentDisposition":disposition})
    };
    Ok(json!({"status":status.as_u16(),"body":body}))
}
