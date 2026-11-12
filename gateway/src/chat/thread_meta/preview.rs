use super::*;

pub(crate) static THREAD_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,300}$").unwrap());

pub(crate) static ITEM_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,600}$").unwrap());

// Emoji: 1 to 16 emoji code points, with at least one pictographic or regional
// indicator (so "1", "#" and "*" alone are refused).
pub(crate) static EMOJI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{Extended_Pictographic}\p{Regional_Indicator}\p{Emoji_Component}\x{200D}\x{FE0F}]{1,16}$").unwrap()
});

pub(crate) static EMOJI_LEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{Extended_Pictographic}\p{Regional_Indicator}]").unwrap());
