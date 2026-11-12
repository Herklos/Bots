//! The `bot` query on the gateway MCP URL names which Bot is asking.
//!
//! `.mcp.json` points that Bot at `POST /mcp/bot?bot=<slug>`. `tools/list` reads the
//! query and applies the stored `subagent.toolAllow`. This is not a T3 command, and
//! it does not filter T3's own tools.

use super::valid_slug;

pub(crate) fn bot_mcp_url(base: &str, slug: &str) -> String {
    if !valid_slug(slug) || query_names_bot(base) {
        return base.to_string();
    }
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}bot={slug}")
}

pub(crate) fn bot_slug_from_query(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "bot" {
            return valid_slug(value).then(|| value.to_string());
        }
    }
    None
}

fn query_names_bot(url: &str) -> bool {
    url.split_once('?')
        .is_some_and(|(_, query)| bot_slug_from_query(Some(query)).is_some())
}
