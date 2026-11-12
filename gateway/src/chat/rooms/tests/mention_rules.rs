use super::*;

fn assert_mentions(text: &str, want: &[&str]) {
    let got = mention_words(text);
    let want: Vec<String> = want.iter().copied().map(str::to_string).collect();
    assert_eq!(got, want, "{text:?}");
}

#[test]
pub(crate) fn punctuation_before_at_starts_a_mention() {
    for text in [
        "@ada",
        " @ada",
        "\n@ada",
        "(@ada)",
        "[@ada]",
        "{@ada}",
        "<@ada>",
        "\"@ada\"",
        "'@ada'",
        "*@ada*",
        ".@ada",
        "~@ada",
        "`@ada",
        "\u{201C}@ada",
        "ping (@ada) please",
    ] {
        assert_mentions(text, &["ada"]);
    }
}

#[test]
pub(crate) fn a_letter_slash_underscore_or_comma_before_at_is_not_a_mention() {
    for text in [
        "ada@bea",
        "1@bea",
        "foo_@bea",
        "https://x/@bea",
        "@@bea",
        "bea,@ada",
        "say!@ada",
    ] {
        assert_mentions(text, &[]);
    }
}

#[test]
pub(crate) fn mention_tokens_keep_slug_hyphens_and_lower_case() {
    assert_mentions("@Ada-Lovelace", &["ada-lovelace"]);
}

#[test]
pub(crate) fn fenced_and_inline_code_are_not_mentions() {
    assert_mentions("```\n@ada\n```\n@bea", &["bea"]);
    assert_mentions("```rust\n@ada\n```\n@bea", &["bea"]);
    assert_mentions("```\n@ada\n  ```\n@bea", &["bea"]);
    assert_mentions("~~~\n@ada\n~~~\n@bea", &["bea"]);
    assert_mentions("see `@ada` and @bea", &["bea"]);
    assert_mentions("see ``@ada`` and @bea", &["bea"]);
    assert_mentions("@ada\n```\n@bea\n```", &["ada"]);
    assert_mentions("```\n@ada\n@bea", &[]);
    assert_mentions("``` @ada ```", &[]);
}

#[test]
pub(crate) fn code_does_not_wake_and_punctuation_does() {
    let members = trio();
    assert_eq!(
        plan_dispatches(
            &members,
            "```\n@ada\n```\n@bea",
            Some("cy"),
            AnswerPolicy::Last,
            None
        ),
        vec![1],
        "a name inside a fence is not a wake"
    );
    assert_eq!(
        plan_dispatches(
            &members,
            "```\n@everyone\n```\nthanks",
            Some("bea"),
            AnswerPolicy::Last,
            None
        ),
        vec![1],
        "@everyone inside a fence does not ask the group"
    );
    assert_eq!(
        plan_dispatches(&members, "(@ada)", Some("cy"), AnswerPolicy::Last, None),
        vec![0],
        "punctuation before @ still names that Bot"
    );
}
