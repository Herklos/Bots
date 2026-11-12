use super::*;

/// The app as the tailnet listener serves it.
pub(crate) fn tailnet_app(s: &Arc<AppState>) -> Router {
    crate::network::exposure::on_tailnet(app(s.clone()))
}

// ---------- app API ----------

pub(crate) async fn config_route(
    State(s): S,
    surface: Option<Extension<crate::network::exposure::Surface>>,
) -> Json<Value> {
    let on_tailnet =
        surface.is_some_and(|Extension(x)| x == crate::network::exposure::Surface::Tailnet);
    Json(json!({
        "botsRoot": s.cfg.bots_root,
        "accessLogout": s.access.is_some() && !on_tailnet,
        "appUrl": s.cfg.app_url,
        "version": env!("CARGO_PKG_VERSION"),
        "harness": { "name": "T3 Code", "connected": s.harness.connected().await },
    }))
}

// ---------- the Exact web build ----------

pub(crate) async fn static_files(State(s): S, uri: Uri, req: Request) -> Response {
    use tower::ServiceExt;
    let dir = s.cfg.static_dir.clone();
    let path = uri.path().trim_start_matches('/');
    let candidate = dir.join(path);
    // Unknown paths get the app shell, so links like /?item=… and deep routes load the app.
    let serve_index = path.is_empty() || path.contains("..") || !candidate.is_file();
    let target = if serve_index {
        dir.join("index.html")
    } else {
        candidate
    };
    // A web build with no icon of its own gets the brand's mark.
    if matches!(path, "favicon.ico" | "favicon.svg") && serve_index {
        return (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            hbots_design::FAVICON_SVG,
        )
            .into_response();
    }
    if !target.is_file() {
        return (StatusCode::NOT_FOUND, WEB_APP_MISSING).into_response();
    }
    // The web build is made with a placeholder gateway address (see app/host/config.ts); serve it with the
    // address this request came in on, so one build works wherever the gateway is reached.
    if target
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
    {
        if let Ok(js) = tokio::fs::read_to_string(&target).await {
            if js.contains(ORIGIN_PLACEHOLDER) {
                let body = js.replace(ORIGIN_PLACEHOLDER, &request_origin(req.headers()));
                return (
                    [
                        (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                    body,
                )
                    .into_response();
            }
        }
    }
    match tower_http::services::ServeFile::new(target)
        .oneshot(req)
        .await
    {
        Ok(res) => res.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// What the built web app uses for the gateway's address until it is served (see `app/host/config.ts`).
pub(crate) const ORIGIN_PLACEHOLDER: &str = "http://hbots-origin.invalid";

/// `index.html` is missing from `STATIC_DIR`. `cargo run` does not build the Exact web app.
pub(crate) const WEB_APP_MISSING: &str = "The Bots web app isn't built. Exact is a separate checkout (https://github.com/expo/exact), not a script in this repo. From that directory: EXACT_APP_DIR=<hbots>/app bun host/web/build.mjs bots-web (writes app/target/web-dist). An installed release looks for web/ next to the binary. Or set STATIC_DIR.";
