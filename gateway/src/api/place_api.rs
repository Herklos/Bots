//! Where this Bot is saved to run. A linked computer is stored only.

use super::*;
use crate::harness::ComputerCatalog;

pub(crate) async fn environment_get(
    State(s): S,
    Path(slug): Path<String>,
) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let catalog = computers_of(&s).await;
    Ok(Json(crate::bots::computer::place_view(&bot, &catalog)))
}

pub(crate) async fn environment_put(
    State(s): S,
    Path(slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&headers)?;
    let body = parse(&body)?;
    let Some(id) = body.get("id").and_then(Value::as_str) else {
        return Err(ApiError::bad_request("That computer id is not valid."));
    };
    let catalog = computers_of(&s).await;
    Ok(Json(s.bots.set_environment(&slug, id, &catalog).await?))
}

pub(crate) async fn computers_of(s: &AppState) -> ComputerCatalog {
    let Ok(provisioning) = s.harness.provisioning() else {
        return ComputerCatalog::offline();
    };
    provisioning
        .computers()
        .await
        .unwrap_or_else(|_| ComputerCatalog::offline())
}
