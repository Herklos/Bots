use super::{Rendered, Sidecar, MARKER_PREFIX, MARKER_SUFFIX};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Render a flat, full-ID AnyBlock array. Blocks with styling or fields we do
/// not model are kept opaque; otherwise a round trip would silently drop them.
pub(crate) fn render(blocks: &[Value]) -> Rendered {
    let mut sidecar = Sidecar::new();
    let mut chunks = Vec::new();
    let mut numbers = vec![0usize; 33];
    for (index, block) in blocks.iter().enumerate() {
        let kind = block["type"].as_str().unwrap_or("");
        if kind != "numbered_list_item" {
            numbers.fill(0);
        }
        // A leading divider followed by another divider is indistinguishable
        // from front matter, which the parser intentionally rejects.
        let ambiguous_divider = index == 0
            && kind == "divider"
            && blocks[1..]
                .iter()
                .any(|b| b["type"].as_str() == Some("divider") && supported(b));
        let chunk = if supported(block) && !ambiguous_divider {
            let text = block["text"].as_str().unwrap_or("");
            let depth = block["indent"].as_u64().unwrap_or(0) as usize;
            let indent = "  ".repeat(depth.min(32));
            match kind {
                "paragraph" => Some(text.to_owned()),
                "heading_1" | "heading_2" | "heading_3" | "heading_4" | "header_4" => {
                    let n = kind.chars().last().unwrap().to_digit(10).unwrap() as usize;
                    Some(format!("{} {text}", "#".repeat(n)))
                }
                "quote" => Some(
                    text.split('\n')
                        .map(|s| format!("> {s}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                "code" => {
                    let ticks = text
                        .split(|c| c != '`')
                        .map(str::len)
                        .max()
                        .unwrap_or(0)
                        .max(2)
                        + 1;
                    let fence = "`".repeat(ticks);
                    Some(format!(
                        "{fence}{}\n{text}\n{fence}",
                        block["language"].as_str().unwrap_or("")
                    ))
                }
                "divider" => Some("---".to_owned()),
                "checkbox" => Some(format!(
                    "{indent}- [{}] {text}",
                    if block["checked"].as_bool().unwrap_or(false) {
                        "x"
                    } else {
                        " "
                    }
                )),
                "bulleted_list_item" => Some(format!("{indent}- {text}")),
                "numbered_list_item" => {
                    numbers[depth + 1..].fill(0);
                    numbers[depth] += 1;
                    Some(format!("{indent}{}. {text}", numbers[depth]))
                }
                "table" => table(block),
                _ => None,
            }
        } else {
            None
        };
        chunks.push(chunk.unwrap_or_else(|| {
            let bytes = serde_json::to_vec(block).unwrap_or_default();
            let hash = hex::encode(Sha256::digest(&bytes));
            let key = format!("{index}-{hash}");
            sidecar.insert(key.clone(), block.clone());
            format!("{MARKER_PREFIX}{key}{MARKER_SUFFIX}")
        }));
    }
    Rendered {
        markdown: if chunks.is_empty() {
            String::new()
        } else {
            format!("{}\n", chunks.join("\n\n"))
        },
        sidecar,
    }
}

fn supported(block: &Value) -> bool {
    let Some(map) = block.as_object() else {
        return false;
    };
    let Some(kind) = block["type"].as_str() else {
        return false;
    };
    let allowed = match kind {
        "paragraph" | "heading_1" | "heading_2" | "heading_3" | "heading_4" | "header_4"
        | "quote" | "bulleted_list_item" | "numbered_list_item" => {
            &["type", "id", "indent", "text"][..]
        }
        "checkbox" => &["type", "id", "indent", "text", "checked"][..],
        "code" => &["type", "id", "indent", "text", "language"][..],
        "divider" => &["type", "id", "indent"][..],
        "table" => &["type", "id", "indent", "rows", "columns"][..],
        _ => return false,
    };
    if map.keys().any(|key| !allowed.contains(&key.as_str())) {
        return false;
    }
    if map.get("indent").is_some_and(|v| v.as_u64().is_none())
        || map.get("text").is_some_and(|v| v.as_str().is_none())
        || map.get("checked").is_some_and(|v| v.as_bool().is_none())
        || map.get("language").is_some_and(|v| v.as_str().is_none())
    {
        return false;
    }
    let depth = block["indent"].as_u64().unwrap_or(0);
    if depth > 32
        || (depth > 0
            && !matches!(
                kind,
                "checkbox" | "bulleted_list_item" | "numbered_list_item"
            ))
    {
        return false;
    }
    let text = block["text"].as_str().unwrap_or("");
    if text.contains('\r') || (kind == "paragraph" && text.ends_with('\n')) {
        return false;
    }
    if matches!(kind, "bulleted_list_item" | "numbered_list_item")
        && ["[ ] ", "[x] ", "[X] "]
            .iter()
            .any(|prefix| text.starts_with(prefix))
    {
        return false;
    }
    if kind == "code"
        && block["language"]
            .as_str()
            .unwrap_or("")
            .contains(['\n', '\r', '`'])
    {
        return false;
    }
    // Paragraph text is inline content: do not reinterpret it as headings,
    // lists, tables, or empty space when converting it back from Markdown.
    if kind == "paragraph"
        && (text.is_empty()
            || text
                .lines()
                .any(|line| line.trim().is_empty() || super::parse::special(line))
            || text.lines().any(|line| line.starts_with('|')))
    {
        return false;
    }
    if matches!(
        kind,
        "heading_1"
            | "heading_2"
            | "heading_3"
            | "heading_4"
            | "header_4"
            | "checkbox"
            | "bulleted_list_item"
            | "numbered_list_item"
    ) && text.contains('\n')
    {
        return false;
    }
    true
}

fn table(block: &Value) -> Option<String> {
    let rows = block["rows"].as_array()?;
    if rows.is_empty() || rows[0]["is_header"].as_bool() != Some(true) {
        return None;
    }
    let cols = block["columns"].as_array()?;
    if cols.is_empty()
        || cols
            .iter()
            .any(|c| c.as_object().is_none_or(|m| m.keys().any(|k| k != "id")))
    {
        return None;
    }
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let map = row.as_object()?;
        if map
            .keys()
            .any(|k| !["id", "cells", "is_header"].contains(&k.as_str()))
            || (i > 0 && row["is_header"].as_bool().unwrap_or(false))
        {
            return None;
        }
        let cells = row["cells"].as_array()?;
        if cells.len() != cols.len() {
            return None;
        }
        let mut line = String::from("|");
        for cell in cells {
            let s = cell.as_str()?;
            if s.trim() != s || s.contains(['\n', '\r', '\\']) {
                return None;
            }
            line.push(' ');
            line.push_str(&s.replace('|', "\\|"));
            line.push_str(" |");
        }
        out.push(line);
        if i == 0 {
            out.push(format!("|{}", " --- |".repeat(cols.len())));
        }
    }
    Some(out.join("\n"))
}
