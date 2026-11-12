//! Markdown image marks in a Bot reply.
//!
//! Fenced and inline code are left as written, so a Bot explaining the syntax
//! does not show a picture. Every other image mark loses its URL. A `library/`
//! path can still be shown after the file checks.
use std::ops::Range;

/// One image mark outside code. `path` is set only for an accepted `library/` target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageMark {
    pub(crate) range: Range<usize>,
    pub(crate) alt: String,
    pub(crate) path: Option<String>,
}

const SCHEMES: &[&str] = &["http:", "https:", "data:", "file:", "javascript:"];

pub(crate) fn marks(text: &str, workspace_root: &str) -> Vec<ImageMark> {
    let fences = fence_ranges(text);
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = covering(&fences, i) {
            i = end;
            continue;
        }
        if bytes[i] == b'`' {
            if let Some(end) = inline_close(text, i) {
                i = end;
                continue;
            }
        }
        if bytes[i] == b'!' && bytes.get(i + 1) == Some(&b'[') && !escaped(bytes, i) {
            if let Some(mark) = parse_image(text, i, workspace_root) {
                i = mark.range.end;
                out.push(mark);
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Resolved library images are removed. Every other mark becomes `[alt]`, or
/// nothing when the alt is empty, so the URL never reaches the client.
pub(crate) fn strip(
    text: &str,
    found: &[ImageMark],
    shown: &std::collections::HashSet<String>,
) -> String {
    if found.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for mark in found {
        if mark.range.start < cursor || mark.range.end > text.len() {
            continue;
        }
        out.push_str(&text[cursor..mark.range.start]);
        let keep = mark.path.as_ref().is_some_and(|path| shown.contains(path));
        if !keep && !mark.alt.is_empty() {
            out.push('[');
            out.push_str(&mark.alt);
            out.push(']');
        }
        cursor = mark.range.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// A workspace-relative `library/...` path from a query string. Already decoded.
pub(crate) fn library_rel(path: &str) -> Option<String> {
    let rel = path.trim().trim_start_matches('/');
    segments_ok(rel).then(|| rel.to_string())
}

fn parse_image(text: &str, start: usize, workspace_root: &str) -> Option<ImageMark> {
    let close = text[start + 2..].find(']')? + start + 2;
    let alt = text[start + 2..close].trim().to_string();
    let after = text.get(close + 1..)?;
    if let Some(rest) = after.strip_prefix('[') {
        let label_end = rest.find(']')?;
        let end = close + 1 + 1 + label_end + 1;
        return Some(ImageMark {
            range: start..end,
            alt,
            path: None,
        });
    }
    let body = after.strip_prefix('(')?;
    let (dest, used) = destination(body)?;
    Some(ImageMark {
        range: start..close + 2 + used,
        alt,
        path: normalize(dest, workspace_root),
    })
}

fn destination(body: &str) -> Option<(&str, usize)> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let dest_at = i;
    let dest = if bytes[i] == b'<' {
        let end = body[i + 1..].find('>')? + i + 1;
        i = end + 1;
        &body[dest_at + 1..end]
    } else if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        let end = body[i + 1..].find(char::from(quote))? + i + 1;
        i = end + 1;
        &body[dest_at + 1..end]
    } else {
        while i < bytes.len() && bytes[i] != b')' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        &body[dest_at..i]
    };
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
        let quote = bytes[i];
        i += 1;
        let end = body[i..].find(char::from(quote))? + i;
        i = end + 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    if bytes.get(i) != Some(&b')') {
        return None;
    }
    Some((dest, i + 1))
}

fn normalize(raw: &str, workspace_root: &str) -> Option<String> {
    let trimmed = unwrap_once(raw.trim()).trim();
    if trimmed.is_empty() || forbidden(trimmed) {
        return None;
    }
    let decoded = percent_decode(trimmed)?;
    if forbidden(&decoded) {
        return None;
    }
    let rel = if let Some(rest) = decoded.strip_prefix("./") {
        rest.to_string()
    } else if decoded.starts_with('/') {
        under_root(&decoded, workspace_root)?
    } else {
        decoded
    };
    if !segments_ok(&rel) || super::kind_of(&rel).is_none() {
        return None;
    }
    Some(rel)
}

fn unwrap_once(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let (open, close) = (bytes[0], bytes[bytes.len() - 1]);
        let wrapped = (open == b'<' && close == b'>')
            || (open == b'"' && close == b'"')
            || (open == b'\'' && close == b'\'');
        if wrapped {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn under_root(path: &str, root: &str) -> Option<String> {
    let root = root.trim().trim_end_matches('/');
    if root.is_empty() || !path.starts_with(root) {
        return None;
    }
    let rest = path[root.len()..].strip_prefix('/')?;
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

fn forbidden(value: &str) -> bool {
    value.starts_with("//")
        || value.contains('\\')
        || value.chars().any(|c| c == '\0' || c.is_control())
        || has_scheme(value)
}

fn has_scheme(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    SCHEMES.iter().any(|scheme| scheme_at(&lower, scheme))
}

fn scheme_at(value: &str, scheme: &str) -> bool {
    let mut rest = value;
    while let Some(at) = rest.find(scheme) {
        let boundary = at == 0 || !rest.as_bytes()[at - 1].is_ascii_alphanumeric();
        if boundary {
            return true;
        }
        rest = &rest[at + scheme.len()..];
    }
    false
}

fn segments_ok(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') || forbidden(rel) {
        return false;
    }
    let mut parts = rel.split('/');
    if parts.next() != Some("library") {
        return false;
    }
    let mut file = false;
    for part in parts {
        file = true;
        if part.is_empty() || part == "." || part == ".." || part.starts_with('.') {
            return false;
        }
        if part.contains(['?', '#']) {
            return false;
        }
    }
    file
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex_val(*bytes.get(i + 1)?)?;
            let lo = hex_val(*bytes.get(i + 2)?)?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn escaped(bytes: &[u8], at: usize) -> bool {
    let mut slashes = 0;
    let mut i = at;
    while i > 0 && bytes[i - 1] == b'\\' {
        slashes += 1;
        i -= 1;
    }
    slashes % 2 == 1
}

fn covering(ranges: &[(usize, usize)], at: usize) -> Option<usize> {
    ranges
        .iter()
        .find(|(start, end)| at >= *start && at < *end)
        .map(|(_, end)| *end)
}

fn inline_close(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut n = 0;
    while open + n < bytes.len() && bytes[open + n] == b'`' {
        n += 1;
    }
    let mut i = open + n;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let mut m = 0;
        while i + m < bytes.len() && bytes[i + m] == b'`' {
            m += 1;
        }
        if m == n {
            return Some(i + m);
        }
        i += m;
    }
    None
}

fn fence_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        if let Some(end) = fence_from(text, offset) {
            out.push((offset, end));
            if end == offset {
                break;
            }
            offset = end;
            continue;
        }
        match text[offset..].find('\n') {
            Some(n) => offset += n + 1,
            None => break,
        }
    }
    out
}

fn fence_from(text: &str, line_start: usize) -> Option<usize> {
    let (marker, len, content_at) = open_fence(text, line_start)?;
    Some(close_fence(text, content_at, marker, len).unwrap_or(text.len()))
}

fn open_fence(text: &str, line_start: usize) -> Option<(u8, usize, usize)> {
    let line = &text[line_start..];
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < 3 && bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    let marker = *bytes.get(i)?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut len = 0;
    while bytes.get(i + len) == Some(&marker) {
        len += 1;
    }
    if len < 3 {
        return None;
    }
    let line_end = line.find('\n').unwrap_or(line.len());
    if marker == b'`' && line[i + len..line_end].contains('`') {
        return None;
    }
    let after = line_start + line_end + usize::from(line_end < line.len());
    Some((marker, len, after))
}

fn close_fence(text: &str, from: usize, marker: u8, len: usize) -> Option<usize> {
    let mut offset = from;
    while offset < text.len() {
        let line = &text[offset..];
        if fence_closes(line, marker, len) {
            let line_end = line.find('\n').unwrap_or(line.len());
            return Some(offset + line_end + usize::from(line_end < line.len()));
        }
        let n = line.find('\n')?;
        offset += n + 1;
    }
    None
}

fn fence_closes(line: &str, marker: u8, len: usize) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < 3 && bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    let mut run = 0;
    while bytes.get(i + run) == Some(&marker) {
        run += 1;
    }
    if run < len {
        return false;
    }
    let line_end = line.find('\n').unwrap_or(line.len());
    line[i + run..line_end].trim().is_empty()
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction]Z` as unix milliseconds. Anything else is unknown.
pub(crate) fn parse_time_ms(stamp: &str) -> Option<u64> {
    let bytes = stamp.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[10].eq_ignore_ascii_case(&b'T')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let year = i32::try_from(parse_n(&bytes[0..4])?).ok()?;
    let month = parse_n(&bytes[5..7])?;
    let day = parse_n(&bytes[8..10])?;
    let hour = parse_n(&bytes[11..13])?;
    let minute = parse_n(&bytes[14..16])?;
    let second = parse_n(&bytes[17..19])?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let rest = &stamp[19..];
    let (millis, zone) = if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let mut padded = frac[..digits].to_string();
        while padded.len() < 3 {
            padded.push('0');
        }
        let millis = padded[..3].parse::<u32>().ok()?;
        (millis, &frac[digits..])
    } else {
        (0, rest)
    };
    if zone != "Z" && zone != "z" && zone != "+00:00" {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let secs = days * 86_400 + u64::from(hour) * 3_600 + u64::from(minute) * 60 + u64::from(second);
    Some(secs * 1_000 + u64::from(millis))
}

fn parse_n(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut n = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u32::from(*byte - b'0'))?;
    }
    Some(n)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<u64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = year - i32::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = u32::try_from(year - era * 400).ok()?;
    let month_index = month + u32::from(month <= 2) * 12 - 3;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = yoe * 365 + yoe / 4 - yoe / 100 + day_of_year;
    let days = i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468;
    u64::try_from(days).ok()
}
