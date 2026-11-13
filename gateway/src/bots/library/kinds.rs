use super::*;

pub(crate) const RECORD_KINDS: &[&str] = &[
    "project", "note", "task", "plan", "link", "repo", "machine", "service",
    "epic", "user_story",
];

pub(crate) fn statuses(kind: &str) -> &'static [&'static str] {
    match kind {
        "project" => &["active", "paused", "idea", "done"],
        "task" => &["todo", "doing", "waiting", "done"],
        "plan" => &["draft", "active", "done"],
        "machine" => &["online", "offline", "unknown"],
        "service" => &["running", "stopped", "failing", "unknown"],
        "repo" => &["active", "archived"],
        "epic" | "user_story" => &["backlog", "ready", "doing", "done", "cancelled"],
        _ => &[],
    }
}

pub(crate) fn default_status(kind: &str) -> &'static str {
    match kind {
        "project" | "repo" => "active",
        "task" => "todo",
        "plan" => "draft",
        "machine" | "service" => "unknown",
        "epic" | "user_story" => "backlog",
        _ => "",
    }
}

/// Today / This week / Older, by UTC day. A missing or future day has no group.
pub(crate) fn group_day(day: &str, today: &str, week_start: &str) -> &'static str {
    if day.len() != 10 || today.len() != 10 || day > today {
        return "";
    }
    if day == today {
        "Today"
    } else if week_start.len() == 10 && day >= week_start {
        "This week"
    } else {
        "Older"
    }
}

pub(crate) fn file_kind(name: &str) -> &'static str {
    let lower = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match lower.as_str() {
        "html" | "htm" => "page",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "mp4" | "webm" | "mov" => "media",
        _ => "file",
    }
}

pub(crate) fn media_kind(mime: &str) -> &'static str {
    if mime.starts_with("image/") || mime.starts_with("video/") {
        "media"
    } else {
        "file"
    }
}

/// http(s) links in message text. Trailing punctuation is not part of the link.
pub(crate) fn web_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        let start = rest
            .find("https://")
            .or_else(|| rest.find("http://"))
            .map(|n| i + n);
        let Some(start) = start else { break };
        let tail = &text[start..];
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | ')' | ']' | '*')
            })
            .unwrap_or(tail.len());
        let mut href = tail[..end]
            .trim_end_matches([')', ',', '.', ';', '!', '?', ']'])
            .to_string();
        if href.parse::<url::Url>().is_ok() && !out.contains(&href) {
            out.push(std::mem::take(&mut href));
        }
        i = start + end.max(1);
    }
    out
}

pub(crate) fn detail_host(href: &str) -> String {
    url::Url::parse(href)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

pub(crate) const SHELF_CAP: usize = 60;

/// File-name extension, compared without case. The default macOS volume treats `.JSON` and `.json` as one file.
pub(crate) fn ascii_ext(name: &str, ext: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(ext))
}

pub(crate) fn is_picture(title: &str, detail: &str) -> bool {
    detail.starts_with("image/")
        || matches!(
            title
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
}

/// A Bot's shelf for the panel: pictures, files and links, newest first, nothing else. Records
/// kept by the person or the Bot (notes, tasks, plans and the rest) and connectors stay out.
/// `url` is a gateway path the app prefixes with its address (or a link's own address).
pub(crate) fn shelf_from(slug: &str, view: &Value) -> Value {
    let (mut pictures, mut files, mut links) = (Vec::new(), Vec::new(), Vec::new());
    for e in view["entries"].as_array().into_iter().flatten() {
        let id = e["id"].as_str().unwrap_or("");
        let (title, detail) = (
            e["title"].as_str().unwrap_or(""),
            e["detail"].as_str().unwrap_or(""),
        );
        let url = if let Some(path) = id.strip_prefix(&format!("file:{slug}/")) {
            format!("/api/bots/{slug}/library/file?path={}", encode_query(path))
        } else if let Some(att) = id.strip_prefix("attachment:") {
            format!("/api/bots/{slug}/attachments/{att}")
        } else if id.starts_with("http://") || id.starts_with("https://") {
            id.to_string()
        } else {
            continue;
        };
        let item =
            json!({ "id": id, "title": title, "detail": detail, "url": url, "group": e["group"] });
        match e["kind"].as_str().unwrap_or("") {
            "media" if is_picture(title, detail) => pictures.push(item),
            "media" | "file" | "page" => files.push(item),
            "link" => links.push(item),
            _ => {}
        }
    }
    let counts = json!({ "pictures": pictures.len(), "files": files.len(), "links": links.len() });
    pictures.truncate(SHELF_CAP);
    files.truncate(SHELF_CAP);
    links.truncate(SHELF_CAP);
    json!({ "ok": true, "counts": counts, "pictures": pictures, "files": files, "links": links })
}

/// A path as a query value: the characters a path uses stay, the rest are percent-encoded.
pub(crate) fn encode_query(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' | b'~' => {
                char::from(b).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub(crate) fn file_entry(slug: &str, entry: DirEntry, today: &str, week: &str) -> Value {
    let day = if entry.modified == 0 {
        String::new()
    } else {
        day_key(entry.modified)
    };
    json!({
        "id": format!("file:{slug}/library/{}", entry.name),
        "kind": file_kind(&entry.name),
        "title": entry.name,
        "detail": if file_kind(&entry.name) == "page" { "Private" } else { "Workspace file" },
        "group": group_day(&day, today, week),
        "status": "",
        "body": "",
    })
}

pub(crate) fn new_id() -> String {
    const ABC: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let n = rand::random::<u64>();
    (0..8)
        .map(|i| char::from(ABC[usize::try_from(n >> (i * 5)).unwrap_or(0) % ABC.len()]))
        .collect()
}

pub(crate) fn encode_file(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
