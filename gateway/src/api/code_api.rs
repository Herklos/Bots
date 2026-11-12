//! `GET /api/bots/{slug}/code`. With `beta.code` off the body is `{ "enabled": false }`
//! and the harness is not asked. `open=1` is the Code tab: status and open pull requests.
//! There is no write action on this route.

use super::*;
use std::time::Instant;

pub(crate) async fn code_get(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let master = s.settings().await["beta"]["code"] == true;
    let cwd = s.bots.dir(&slug)?;
    let project = bot
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let open = q.get("open").is_some_and(|value| value == "1");
    let body = crate::chat::code::present(
        s.harness.as_ref(),
        &s.code_signals,
        &s.code_reads,
        Instant::now(),
        &slug,
        &cwd.to_string_lossy(),
        project.as_deref(),
        master,
        crate::chat::code::mode_of(&bot),
        open,
    )
    .await;
    Ok(Json(body))
}
