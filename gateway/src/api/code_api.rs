//! `GET /api/bots/{slug}/code`. With `beta.code` off the body is `{ "enabled": false }`
//! and the harness is not asked. There is no write action on this route.

use super::*;
use std::time::Instant;

pub(crate) async fn code_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let master = s.settings().await["beta"]["code"] == true;
    let cwd = s.bots.dir(&slug)?;
    let gate = crate::chat::code::evaluate(
        s.harness.as_ref(),
        &s.code_signals,
        Instant::now(),
        &slug,
        &cwd.to_string_lossy(),
        master,
        crate::chat::code::mode_of(&bot),
    )
    .await;
    if !gate.enabled {
        return Ok(Json(json!({ "enabled": false })));
    }
    Ok(Json(json!({ "enabled": true, "signal": gate.show })))
}
