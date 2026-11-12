//! One tool call's text, fetched when the chat is on the detail level.
use super::*;

pub(crate) async fn read_activity(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let child = q.get("child").map(String::as_str);
    let side = q.get("side").map(String::as_str);
    let (child, side) = crate::chat::side::lane_ids(child, side)?;
    let thread = resolve_chat_thread(&s, &slug, child, side).await?;
    let snapshot = s
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(&thread)
        .await
        .map_err(|err| err.api())?;
    let items = crate::harness::t3::transcript::visible_items(snapshot.body());
    let detail = crate::harness::t3::transcript::call_detail(&items, &id)
        .ok_or_else(|| ApiError::not_found("Activity not found"))?;
    Ok(Json(detail))
}
