use super::{CodecError, Sidecar, MARKER_PREFIX, MARKER_SUFFIX};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Parse Markdown into flat, read-shaped blocks. Opaque blocks are resolved
/// from the sidecar and retained exactly. A missing, duplicated or deleted
/// marker is an error, so a page update cannot accidentally delete it.
pub(crate) fn parse(markdown: &str, sidecar: &Sidecar) -> Result<Vec<Value>, CodecError> {
    let input = markdown.replace("\r\n", "\n");
    let lines: Vec<&str> = input.lines().collect();
    if lines.first().is_some_and(|s| *s == "---") && lines.iter().skip(1).any(|s| *s == "---") {
        return Err(CodecError::FrontMatter);
    }
    let mut blocks = Vec::new();
    let mut used = BTreeSet::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if line.starts_with(MARKER_PREFIX) {
            let key = line
                .strip_prefix(MARKER_PREFIX)
                .and_then(|s| s.strip_suffix(MARKER_SUFFIX))
                .ok_or(CodecError::InvalidMarker)?;
            let value = sidecar.get(key).ok_or(CodecError::MissingOpaqueBlock)?;
            if !used.insert(key.to_owned()) {
                return Err(CodecError::DuplicateOpaqueBlock);
            }
            blocks.push(value.clone());
            i += 1;
            continue;
        }
        if let Some((fence, language)) = fence_start(line) {
            let start = i + 1;
            i = start;
            while i < lines.len() && lines[i].trim() != fence {
                i += 1;
            }
            if i == lines.len() {
                return Err(CodecError::UnclosedFence);
            }
            blocks.push(
                json!({"type":"code", "text": lines[start..i].join("\n"), "language": language}),
            );
            i += 1;
            continue;
        }
        if line == "---" {
            blocks.push(json!({"type":"divider"}));
            i += 1;
            continue;
        }
        if let Some((n, text)) = heading(line) {
            blocks.push(json!({"type": format!("heading_{n}"), "text": text}));
            i += 1;
            continue;
        }
        if line.starts_with("> ") || line == ">" {
            let mut quote = Vec::new();
            while i < lines.len() && (lines[i].starts_with("> ") || lines[i] == ">") {
                quote.push(lines[i].strip_prefix("> ").unwrap_or(""));
                i += 1;
            }
            blocks.push(json!({"type":"quote", "text":quote.join("\n")}));
            continue;
        }
        if let Some((depth, kind, text, checked)) = list(line)? {
            let mut block = json!({"type":kind, "indent":depth, "text":text});
            if let Some(checked) = checked {
                block["checked"] = json!(checked);
            }
            blocks.push(block);
            i += 1;
            continue;
        }
        if i + 1 < lines.len() && line.starts_with('|') && is_separator(lines[i + 1]) {
            let header = cells(line)?;
            let separators = cells(lines[i + 1])?;
            if header.len() != separators.len() || header.is_empty() {
                return Err(CodecError::UnsupportedMarkdown("table width"));
            }
            let mut rows = vec![json!({"is_header":true,"cells":header})];
            i += 2;
            while i < lines.len() && lines[i].starts_with('|') {
                let row = cells(lines[i])?;
                if row.len() != separators.len() {
                    return Err(CodecError::UnsupportedMarkdown("table width"));
                }
                rows.push(json!({"cells":row}));
                i += 1;
            }
            blocks.push(
                json!({"type":"table", "columns":vec![json!({});separators.len()], "rows":rows}),
            );
            continue;
        }
        let mut paragraph = vec![line];
        i += 1;
        while i < lines.len() && !lines[i].trim().is_empty() {
            if special(lines[i]) {
                break;
            }
            paragraph.push(lines[i]);
            i += 1;
        }
        blocks.push(json!({"type":"paragraph", "text":paragraph.join("\n")}));
    }
    if used.len() != sidecar.len() {
        return Err(CodecError::RemovedOpaqueBlock);
    }
    Ok(blocks)
}

pub(super) fn special(line: &str) -> bool {
    line.starts_with(MARKER_PREFIX)
        || fence_start(line).is_some()
        || line == "---"
        || heading(line).is_some()
        || line.starts_with("> ")
        || line == ">"
        || matches!(list(line), Ok(Some(_)))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let count = line.bytes().take_while(|c| *c == b'#').count();
    if (1..=4).contains(&count) {
        Some((count, line.get(count..)?.strip_prefix(' ')?))
    } else {
        None
    }
}

fn fence_start(line: &str) -> Option<(&str, &str)> {
    let count = line.bytes().take_while(|c| *c == b'`').count();
    if count < 3 {
        return None;
    }
    let language = line.get(count..)?;
    if language.contains('`') {
        return None;
    }
    Some((&line[..count], language))
}

type ListItem<'a> = (usize, &'static str, &'a str, Option<bool>);
fn list(line: &str) -> Result<Option<ListItem<'_>>, CodecError> {
    let spaces = line.bytes().take_while(|c| *c == b' ').count();
    let rest = &line[spaces..];
    let (kind, text) = if let Some(text) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        ("bulleted_list_item", text)
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return Ok(None);
        }
        let Some(text) = rest[digits..].strip_prefix(". ") else {
            return Ok(None);
        };
        ("numbered_list_item", text)
    };
    if spaces % 2 != 0 || spaces > 64 {
        return Err(CodecError::UnsupportedMarkdown("list indentation"));
    }
    if let Some(text) = text.strip_prefix("[ ] ") {
        return Ok(Some((spaces / 2, "checkbox", text, Some(false))));
    }
    if let Some(text) = text
        .strip_prefix("[x] ")
        .or_else(|| text.strip_prefix("[X] "))
    {
        return Ok(Some((spaces / 2, "checkbox", text, Some(true))));
    }
    Ok(Some((spaces / 2, kind, text, None)))
}

fn cells(line: &str) -> Result<Vec<String>, CodecError> {
    let raw = line
        .strip_prefix('|')
        .and_then(|s| s.strip_suffix('|'))
        .ok_or(CodecError::UnsupportedMarkdown("table delimiters"))?;
    let mut result = Vec::new();
    let mut cell = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '|' => {
                result.push(cell.trim().to_owned());
                cell.clear();
            }
            '\\' => {
                if chars.next() != Some('|') {
                    return Err(CodecError::UnsupportedMarkdown("table escape"));
                }
                cell.push('|');
            }
            _ => cell.push(c),
        }
    }
    result.push(cell.trim().to_owned());
    Ok(result)
}
fn is_separator(line: &str) -> bool {
    cells(line).is_ok_and(|v| {
        !v.is_empty()
            && v.iter()
                .all(|s| s.len() >= 3 && s.chars().all(|c| c == '-'))
    })
}
