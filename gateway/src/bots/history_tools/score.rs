//! Grades one message against a query.
//!
//! A whole word is 1, a word prefix is 0.8, and a hit inside a word is 0.3.
//! Words written together as typed add 0.25. Otherwise words within 60
//! characters add 0.1. A token shorter than four characters has to start a
//! word, so "fts" is not a hit inside "drafts". Case and accents fold.
//! Wrapping quotes are not part of the words.

use unicode_normalization::UnicodeNormalization;

const PREFIX: f64 = 0.8;
const INSIDE: f64 = 0.3;
const TOGETHER: f64 = 0.25;
const NEAR: f64 = 0.1;
const NEAR_SPAN: usize = 60;
/// Shorter than this, an inside-word hit is noise.
const WORD_START_LEN: usize = 4;
const MIN_TOKEN: usize = 3;

/// A whole word outranks a prefix, which outranks a hit inside a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HitKind {
    Inside,
    Prefix,
    Word,
}

impl HitKind {
    fn quality(self) -> f64 {
        match self {
            Self::Word => 1.0,
            Self::Prefix => PREFIX,
            Self::Inside => INSIDE,
        }
    }
}

#[derive(Clone, Debug)]
struct Token {
    folded: Vec<char>,
    /// Long enough to change the grade. A shorter token only has to be present.
    grades: bool,
    must_start: bool,
}

/// One prepared query, reused across messages.
#[derive(Clone, Debug)]
pub(crate) struct Query {
    tokens: Vec<Token>,
    long_count: usize,
}

/// How one message meets the query. `offset` is a char index in the original text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scored {
    pub quality: f64,
    pub offset: usize,
}

impl Query {
    /// `None` when the query has no word to look for.
    pub(crate) fn prepare(raw: &str) -> Option<Self> {
        let tokens: Vec<Token> = normalized_query(raw)
            .split_whitespace()
            .filter_map(token_of)
            .collect();
        if tokens.is_empty() {
            return None;
        }
        let long_count = tokens.iter().filter(|token| token.grades).count();
        Some(Self { tokens, long_count })
    }
}

/// `None` when any word is missing, or a short word never starts a word.
pub(crate) fn score(query: &Query, text: &str) -> Option<Scored> {
    let text = Text::new(text);
    let mut worst = HitKind::Word;
    let mut places = Vec::with_capacity(query.tokens.len());
    for token in &query.tokens {
        let found = find_token(&text, &token.folded)?;
        if found.kind == HitKind::Inside && token.must_start {
            return None;
        }
        if token.grades {
            worst = worst.min(found.kind);
        }
        places.push(found.at);
    }
    let quality = worst.quality();
    if let Some(offset) = phrase_offset(&text, &query.tokens) {
        return Some(Scored {
            quality: quality + TOGETHER,
            offset,
        });
    }
    let (offset, span) = shortest_span(&places);
    let quality = if query.long_count > 1 && span <= NEAR_SPAN {
        quality + NEAR
    } else {
        quality
    };
    Some(Scored { quality, offset })
}

struct Found {
    kind: HitKind,
    at: Vec<usize>,
}

struct Text {
    original: Vec<char>,
    folded: Vec<char>,
}

impl Text {
    fn new(raw: &str) -> Self {
        let original: Vec<char> = raw.chars().collect();
        let folded = original.iter().copied().map(fold_char).collect();
        Self { original, folded }
    }
}

fn normalized_query(raw: &str) -> String {
    let stripped = strip_wrapping_quotes(raw.trim());
    let straight: String = stripped.chars().map(straighten_apostrophe).collect();
    straight.nfc().collect::<String>().to_lowercase()
}

fn strip_wrapping_quotes(query: &str) -> &str {
    query
        .trim_start_matches(is_wrapping_quote)
        .trim_end_matches(is_wrapping_quote)
        .trim()
}

fn is_wrapping_quote(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '`' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}'
    )
}

fn straighten_apostrophe(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        other => other,
    }
}

/// Quotes left around one word after the wrapping pair is gone: `"hello" "world"`.
fn trim_token_quotes(token: &str) -> &str {
    token.trim_matches(|c: char| matches!(c, '"' | '`' | '\u{201C}' | '\u{201D}'))
}

fn token_of(raw: &str) -> Option<Token> {
    let raw = trim_token_quotes(raw);
    let chars = raw.chars().count();
    if chars == 0 {
        return None;
    }
    let grades = chars >= MIN_TOKEN || (chars == 2 && has_cjk(raw));
    Some(Token {
        folded: raw.chars().map(fold_char).collect(),
        grades,
        must_start: chars < WORD_START_LEN,
    })
}

fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => return '\'',
        'ς' => return 'σ',
        _ => {}
    }
    let stripped: String = c.nfkd().filter(|ch| !is_combining(*ch)).collect();
    let lower = stripped.to_lowercase();
    let mut chars = lower.chars();
    match (chars.next(), chars.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

fn is_combining(c: char) -> bool {
    matches!(
        c,
        '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05BD}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06ED}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{3099}'..='\u{309A}'
    )
}

fn has_cjk(token: &str) -> bool {
    token.chars().any(is_cjk_or_hangul)
}

fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
    )
}

fn is_cjk_or_hangul(c: char) -> bool {
    is_cjk(c) || ('\u{AC00}'..='\u{D7AF}').contains(&c)
}

fn find_token(text: &Text, token: &[char]) -> Option<Found> {
    if token.is_empty() || token.len() > text.folded.len() {
        return None;
    }
    let mut best: Option<HitKind> = None;
    let mut at = Vec::new();
    let last = text.folded.len() - token.len();
    for index in 0..=last {
        if text.folded[index..index + token.len()] != token[..] {
            continue;
        }
        if inside_hex(text, index, token.len()) {
            continue;
        }
        let kind = kind_at(text, index, token.len());
        let replace = match best {
            None => true,
            Some(current) => kind > current,
        };
        if replace {
            best = Some(kind);
            at.clear();
            at.push(index);
        } else if best == Some(kind) {
            at.push(index);
        }
    }
    best.map(|kind| Found { kind, at })
}

fn kind_at(text: &Text, index: usize, len: usize) -> HitKind {
    if !word_boundary_at(&text.original, index) {
        return HitKind::Inside;
    }
    if ends_word(&text.original, index + len) {
        HitKind::Word
    } else {
        HitKind::Prefix
    }
}

/// A fragment past the start of a commit id or UUID is not a separate hit.
fn inside_hex(text: &Text, index: usize, len: usize) -> bool {
    let mut start = index;
    while start > 0 && text.original[start - 1].is_alphanumeric() {
        start -= 1;
    }
    if start == index {
        return false;
    }
    let mut end = index + len;
    while end < text.original.len() && text.original[end].is_alphanumeric() {
        end += 1;
    }
    is_hex_id(&text.original[start..end])
}

fn is_hex_id(chars: &[char]) -> bool {
    chars.len() >= 7
        && chars.iter().any(|c| c.is_ascii_digit())
        && chars.iter().all(|c| c.is_ascii_hexdigit())
}

fn ends_word(original: &[char], end: usize) -> bool {
    end >= original.len() || word_boundary_at(original, end)
}

fn word_boundary_at(original: &[char], index: usize) -> bool {
    let Some(ch) = original.get(index).copied() else {
        return true;
    };
    if !ch.is_alphanumeric() {
        return true;
    }
    let prev = index.checked_sub(1).and_then(|i| original.get(i)).copied();
    let before = index.checked_sub(2).and_then(|i| original.get(i)).copied();
    let next = original.get(index + 1).copied();
    is_word_start(before, prev, ch, next)
}

fn is_word_start(before: Option<char>, prev: Option<char>, ch: char, next: Option<char>) -> bool {
    // "don't": the letter after the apostrophe stays in the same word.
    let after_apostrophe = prev.is_some_and(|p| p == '\'' || p == '\u{2019}')
        && before.is_some_and(char::is_alphanumeric);
    if after_apostrophe {
        return false;
    }
    match prev {
        None => true,
        Some(prev) => {
            !prev.is_alphanumeric()
                || (prev.is_lowercase() && ch.is_uppercase())
                || (prev.is_uppercase()
                    && ch.is_uppercase()
                    && next.is_some_and(char::is_lowercase))
                || prev.is_numeric() != ch.is_numeric()
                || is_cjk(ch)
        }
    }
}

fn phrase_offset(text: &Text, tokens: &[Token]) -> Option<usize> {
    if tokens.len() < 2 {
        return None;
    }
    let first = &tokens[0].folded;
    if first.is_empty() || first.len() > text.folded.len() {
        return None;
    }
    let last = text.folded.len() - first.len();
    for index in 0..=last {
        if is_whole_word(text, index, first)
            && tail_is_phrase(text, index + first.len(), &tokens[1..])
        {
            return Some(index);
        }
    }
    None
}

fn tail_is_phrase(text: &Text, mut at: usize, tokens: &[Token]) -> bool {
    for token in tokens {
        if at >= text.folded.len() || !text.folded[at].is_whitespace() {
            return false;
        }
        while at < text.folded.len() && text.folded[at].is_whitespace() {
            at += 1;
        }
        if !is_whole_word(text, at, &token.folded) {
            return false;
        }
        at += token.folded.len();
    }
    true
}

fn is_whole_word(text: &Text, index: usize, word: &[char]) -> bool {
    let end = index + word.len();
    end <= text.folded.len()
        && text.folded[index..end] == word[..]
        && word_boundary_at(&text.original, index)
        && ends_word(&text.original, end)
}

/// The shortest stretch that holds one place of each word.
fn shortest_span(places: &[Vec<usize>]) -> (usize, usize) {
    let mut hits = Vec::new();
    for (word, at) in places.iter().enumerate() {
        for &index in at {
            hits.push((index, word));
        }
    }
    hits.sort_by_key(|hit| hit.0);
    let mut counts = vec![0usize; places.len()];
    let mut covered = 0usize;
    let mut best_start = hits.first().map(|hit| hit.0).unwrap_or(0);
    let mut best_len = usize::MAX;
    let mut left = 0usize;
    for (right_index, word) in &hits {
        if counts[*word] == 0 {
            covered += 1;
        }
        counts[*word] += 1;
        while covered == places.len() && left < hits.len() {
            let (start, start_word) = hits[left];
            let len = right_index.saturating_sub(start);
            if len < best_len {
                best_len = len;
                best_start = start;
            }
            counts[start_word] -= 1;
            if counts[start_word] == 0 {
                covered -= 1;
            }
            left += 1;
        }
    }
    (best_start, best_len)
}
