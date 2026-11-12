//! Which `@` tokens count as mentions when a group message wakes someone.
//!
//! `@` opens a mention at the start of the text, or when the previous character
//! is whitespace or opening punctuation (parentheses, brackets, quotes, or a
//! markdown marker). An ASCII letter or digit, `_`, `/`, or another `@` does
//! not. Fenced blocks and inline code are skipped, so a name written inside a
//! snippet does not wake anyone. The token after `@` stays the slug form already
//! used for matching: letters, digits, `_`, and `-`, folded to lower case.

/// Punctuation that may sit immediately before `@`. `_` is an identifier
/// character, so it is not in this set.
const BOUNDARY_PUNCT: &[char] = &[
    '(', '[', '{', '<', '\'', '"', '\u{201C}', '`', '*', '~', '.',
];

/// Lowercase mention tokens in `text`, in order.
pub(crate) fn mention_words(text: &str) -> Vec<String> {
    let mut chars: Vec<char> = text.chars().collect();
    mask_code(&mut chars);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !is_mention_boundary(&chars, i) {
            i += 1;
            continue;
        }
        let mut word = String::new();
        let mut j = i + 1;
        while j < chars.len() && is_token_char(chars[j]) {
            word.extend(chars[j].to_lowercase());
            j += 1;
        }
        if !word.is_empty() {
            out.push(word);
        }
        i = j;
    }
    out
}

fn is_mention_boundary(chars: &[char], at: usize) -> bool {
    if at >= chars.len() || chars[at] != '@' {
        return false;
    }
    if at == 0 {
        return true;
    }
    let prev = chars[at - 1];
    if blocks_mention(prev) {
        return false;
    }
    prev.is_whitespace() || BOUNDARY_PUNCT.contains(&prev)
}

fn blocks_mention(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '@')
}

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

fn mask_code(chars: &mut [char]) {
    mask_fences(chars);
    mask_inline(chars);
}

/// A fence opens at the start of a line with three or more `` ` `` or `~`.
/// It closes on a later line with the same marker, or runs to the end of the
/// text when it never closes. The line break before the opening marker stays.
fn mask_fences(chars: &mut [char]) {
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = fence_at_line(chars, i) {
            blank(chars, i, end);
            i = end;
            continue;
        }
        i = next_line(chars, i);
    }
}

fn fence_at_line(chars: &[char], line: usize) -> Option<usize> {
    let mut j = line;
    while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
        j += 1;
    }
    let (marker, len) = opening_fence(chars, j)?;
    Some(closing_fence(chars, j + len, marker, len).unwrap_or(chars.len()))
}

fn opening_fence(chars: &[char], at: usize) -> Option<(char, usize)> {
    for marker in ['`', '~'] {
        let len = run_len(chars, at, marker);
        if len >= 3 {
            return Some((marker, len));
        }
    }
    None
}

fn closing_fence(chars: &[char], from: usize, marker: char, len: usize) -> Option<usize> {
    let mut k = from;
    while k < chars.len() {
        if chars[k] != '\n' {
            k += 1;
            continue;
        }
        let mut j = k + 1;
        while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
            j += 1;
        }
        if run_len(chars, j, marker) == len {
            let mut end = j + len;
            while end < chars.len() && (chars[end] == ' ' || chars[end] == '\t') {
                end += 1;
            }
            if end == chars.len() || chars[end] == '\n' {
                return Some(end);
            }
        }
        k += 1;
    }
    None
}

fn next_line(chars: &[char], i: usize) -> usize {
    let mut j = i;
    while j < chars.len() && chars[j] != '\n' {
        j += 1;
    }
    if j < chars.len() {
        j + 1
    } else {
        j
    }
}

/// Inline code is a run of backticks with the same run later. Fences are already
/// blank, so their markers are not read again here. An unclosed run stays text.
fn mask_inline(chars: &mut [char]) {
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`' {
            i += 1;
            continue;
        }
        let available = run_len(chars, i, '`');
        let closed = (1..=available)
            .rev()
            .find_map(|len| inline_close(chars, i + len, len));
        if let Some(end) = closed {
            blank(chars, i, end);
            i = end;
        } else {
            i += available;
        }
    }
}

fn inline_close(chars: &[char], from: usize, len: usize) -> Option<usize> {
    let mut k = from;
    while k < chars.len() {
        if chars[k] == '`' {
            let n = run_len(chars, k, '`');
            if n >= len {
                return Some(k + len);
            }
            k += n;
        } else {
            k += 1;
        }
    }
    None
}

fn run_len(chars: &[char], at: usize, marker: char) -> usize {
    let mut n = 0;
    while at + n < chars.len() && chars[at + n] == marker {
        n += 1;
    }
    n
}

fn blank(chars: &mut [char], from: usize, to: usize) {
    for slot in &mut chars[from..to] {
        *slot = ' ';
    }
}
