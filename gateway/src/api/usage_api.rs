use super::*;

/// Tokens and activity for one Bot or all of them over a period: `period` is today, 7d, 30d or
/// all; `bot` is a slug or all; `offset` is the person's minutes east of UTC.
pub(crate) async fn usage_report(
    State(s): S,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let period = q.get("period").map(String::as_str).unwrap_or("7d");
    let offset = q
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .clamp(-840, 840);
    let which = q.get("bot").map(String::as_str).unwrap_or("all");
    let now_ms = i64::try_from(unix_now()).unwrap_or(i64::MAX) * 1000;
    let range = crate::bots::usage::period_days(period, now_ms, offset)
        .ok_or_else(|| ApiError::bad_request("Pick today, 7d, 30d or all."))?;
    let bots: Vec<Value> = s
        .bots
        .list()
        .await?
        .into_iter()
        .filter(|b| which == "all" || b["slug"] == which)
        .collect();
    if which != "all" && bots.is_empty() {
        return Err(ApiError::not_found("Bot not found"));
    }
    let doc = s.usage.doc().await?;
    let facts = s.roster.facts(&s).await;
    let (mut turns, mut by_bot, mut error) = (Vec::new(), Vec::new(), "");
    for bot in &bots {
        let slug = bot["slug"].as_str().unwrap_or("");
        let mine = match bot["mainThreadId"].as_str().filter(|t| !t.is_empty()) {
            Some(thread) => {
                let stamp = facts.get(thread).map(|f| f.stamp.as_str()).unwrap_or("");
                match s.usage.turns_for(&s, thread, stamp).await {
                    Ok(t) => t,
                    Err(_) => {
                        error = "Open T3 Code, then try again.";
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };
        let own = crate::bots::usage::report(
            &mine,
            &crate::bots::usage::activity_by_day(&doc, slug),
            range,
            offset,
        );
        by_bot.push(json!({
            "slug": slug, "name": bot["name"], "color": bot["color"], "shape": bot["shape"],
            "tokens": own["totals"]["tokens"], "turns": own["totals"]["turns"], "messages": own["totals"]["messages"],
        }));
        turns.extend(mine);
    }
    let activity =
        crate::bots::usage::activity_by_day(&doc, if which == "all" { "" } else { which });
    let mut out = crate::bots::usage::report(&turns, &activity, range, offset);
    out["period"] = json!(period);
    out["error"] = json!(error);
    out["byBot"] = json!(if which == "all" { by_bot } else { Vec::new() });
    Ok(Json(out))
}

pub(crate) async fn bot_usage(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let (messages, attachments) = s.usage.bot_activity(&slug).await?;
    let Some(thread) = bot["mainThreadId"].as_str().filter(|t| !t.is_empty()) else {
        return Ok(Json(crate::bots::usage::bot_payload(
            messages,
            attachments,
            &crate::bots::usage::TokenTotals::default(),
            false,
            "This Bot is still getting set up.",
        )));
    };
    let Ok(transcript) = s.harness.transcript() else {
        return Ok(Json(crate::bots::usage::bot_payload(
            messages,
            attachments,
            &crate::bots::usage::TokenTotals::default(),
            false,
            "Open T3 Code, then try again.",
        )));
    };
    let snapshot = match transcript.read(thread).await {
        Ok(snapshot) => snapshot,
        Err(_) => {
            return Ok(Json(crate::bots::usage::bot_payload(
                messages,
                attachments,
                &crate::bots::usage::TokenTotals::default(),
                false,
                "Open T3 Code, then try again.",
            )));
        }
    };
    let (totals, known) = snapshot.token_totals();
    Ok(Json(crate::bots::usage::bot_payload(
        messages,
        attachments,
        &totals,
        known,
        "",
    )))
}
