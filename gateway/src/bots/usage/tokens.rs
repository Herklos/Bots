/// Token totals T3 reported on a thread projection. Cache figures are separate
/// from input and output, because T3 counts them that way.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct TokenTotals {
    pub turns: u64,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cache_write: u64,
}

// The projection readers live with the T3 transcript adapter.

/// One finished provider turn on a Bot's thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub at_ms: i64,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cache_write: u64,
}

/// `YYYY-MM-DDTHH:MM:SS(.sss)Z` as Unix milliseconds. T3 writes this form everywhere.
pub(crate) fn parse_iso_ms(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let n = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (hh, mm, ss) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let millis = text
        .get(19..)
        .and_then(|rest| rest.strip_prefix('.'))
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .take(3)
                .collect::<String>()
        })
        .filter(|digits| !digits.is_empty())
        .map(|digits| format!("{digits:0<3}").parse::<i64>().unwrap_or(0))
        .unwrap_or(0);
    // days_from_civil (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1000 + millis)
}
