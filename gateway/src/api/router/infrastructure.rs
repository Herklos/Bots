//! Explicit non-tool routes. Registration and exclusion metadata share declarations.
use super::*;

macro_rules! record {
    ($entries:ident, $reason:literal, $path:literal, layer, $arg:expr) => {};
    ($entries:ident, $reason:literal, $path:literal, $method:ident, $arg:expr) => {
        $entries.push(json!({"path":$path,"method":stringify!($method).to_uppercase(),"classification":"infrastructure","reason":$reason}));
    };
}
macro_rules! route {
    ($router:ident, $entries:ident, $reason:literal, $path:literal, $first:ident($handler:expr) $(.$method:ident($arg:expr))*) => {{
        $router=$router.route($path,$first($handler) $(.$method($arg))*);
        record!($entries,$reason,$path,$first,$handler);
        $(record!($entries,$reason,$path,$method,$arg);)*
    }};
}

pub(super) fn control_routes() -> (Router<Arc<AppState>>, Vec<Value>) {
    let mut router = Router::new();
    let mut entries = Vec::new();
    route!(
        router,
        entries,
        "First-party credential control plane",
        "/api/management-mcp/credentials",
        get(crate::api::management::credentials_list)
            .post(crate::api::management::credentials_create)
            .layer(middleware::from_fn(crate::api::management::no_store))
    );
    route!(
        router,
        entries,
        "First-party credential control plane",
        "/api/management-mcp/credentials/{id}",
        delete(crate::api::management::credentials_delete)
            .layer(middleware::from_fn(crate::api::management::no_store))
    );
    route!(
        router,
        entries,
        "Harness proxy is not an unrestricted management tool",
        "/t3/{*rest}",
        any(t3_route)
    );
    (router, entries)
}

pub(super) fn public_routes() -> (Router<Arc<AppState>>, Vec<Value>) {
    let mut router = Router::new();
    let mut entries = Vec::new();
    route!(
        router,
        entries,
        "Health probe",
        "/api/health",
        get(|| async {
            Json(json!({"ok":true,"version":env!("CARGO_PKG_VERSION"),"gateway":"rust"}))
        })
    );
    route!(
        router,
        entries,
        "Application authentication",
        "/api/session",
        get(session)
    );
    route!(
        router,
        entries,
        "Application authentication",
        "/api/login",
        post(login)
    );
    route!(
        router,
        entries,
        "Application authentication",
        "/api/logout",
        post(logout).get(logout)
    );
    route!(
        router,
        entries,
        "Internal Bot tool transport",
        "/mcp/bot",
        any(mcp_route)
    );
    // Outside require_login. The URL token is the secret; no T3 bearer is added.
    route!(
        router,
        entries,
        "Webhook ingress. The URL token is the only secret.",
        "/hooks",
        any(hook_route).layer(axum::extract::DefaultBodyLimit::max(
            crate::api::hooks_api::BODY_LIMIT
        ))
    );
    route!(
        router,
        entries,
        "Webhook ingress. The URL token is the only secret.",
        "/hooks/{*rest}",
        any(hook_route).layer(axum::extract::DefaultBodyLimit::max(
            crate::api::hooks_api::BODY_LIMIT
        ))
    );
    route!(
        router,
        entries,
        "Management MCP transport",
        "/mcp",
        any(crate::api::management::route)
            .layer(axum::extract::DefaultBodyLimit::max(24 << 20))
            .layer(middleware::from_fn(crate::api::management::no_store))
    );
    route!(
        router,
        entries,
        "Unknown API route fallback",
        "/api/{*rest}",
        any(|| async { ApiError::not_found("not found") })
    );
    (router, entries)
}

#[cfg(test)]
pub(super) fn manifest() -> Vec<Value> {
    let mut entries = control_routes().1;
    entries.extend(public_routes().1);
    entries.push(json!({"path":"*","method":"ANY","classification":"infrastructure","reason":"Static application and landing-page fallback"}));
    entries
}
