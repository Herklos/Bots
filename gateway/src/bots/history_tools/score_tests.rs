use super::score::{score, Query, Scored};

fn scored(text: &str, query: &str) -> Scored {
    let query = Query::prepare(query).unwrap_or_else(|| panic!("no query in {query:?}"));
    score(&query, text).unwrap_or_else(|| panic!("no hit for {query:?} in {text:?}"))
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not {expected}"
    );
}

fn slice_at(text: &str, offset: usize, len: usize) -> String {
    text.chars().skip(offset).take(len).collect()
}

#[test]
fn word_beats_prefix_beats_inside() {
    let whole = "The model and the mode switch";
    let hit = scored(whole, "mode");
    close(hit.quality, 1.0);
    assert_eq!(slice_at(whole, hit.offset, 4), "mode");

    let prefix = "model settings";
    let hit = scored(prefix, "mode");
    close(hit.quality, 0.8);
    assert_eq!(slice_at(prefix, hit.offset, 5), "model");

    let inside = "We trust the build";
    let hit = scored(inside, "rust");
    close(hit.quality, 0.3);
    assert_eq!(slice_at(inside, hit.offset, 4), "rust");

    // A short token has to start a word. "fts" inside "drafts" is not a hit.
    let drafts = Query::prepare("fts").expect("query");
    assert!(score(&drafts, "saved two drafts").is_none());
    close(scored("Enable FTS", "fts").quality, 1.0);

    // The whole word later beats the prefix earlier, accents included.
    let accented = "The cafeteria, then the Café";
    let hit = scored(accented, "cafe");
    close(hit.quality, 1.0);
    assert_eq!(slice_at(accented, hit.offset, 4), "Café");

    // A fragment past the start of a commit id is not its own hit.
    let sha = Query::prepare("cafe").expect("query");
    assert!(score(&sha, "see commit 7cafe52bb8").is_none());
    close(scored("7cafe52bb8, then the cafe", "cafe").quality, 1.0);

    close(scored("Fix the CLIAdapter state", "ada").quality, 0.8);
}

#[test]
fn quotes_and_words_written_together() {
    close(scored("Use the sidebar", "\"sidebar\"").quality, 1.0);
    close(scored("hello world", "\"hello\" \"world\"").quality, 1.25);
    close(scored("I don’t know why", "don't know").quality, 1.25);

    let together = "Run the tests, then vp run dev to check";
    let hit = scored(together, "\"vp run dev\"");
    close(hit.quality, 1.25);
    assert_eq!(slice_at(together, hit.offset, 10), "vp run dev");

    // The same words near each other, but not as typed, rank below the phrase.
    close(
        scored("dev notes: vp is slow, run it", "vp run dev").quality,
        1.1,
    );
    let far = format!(
        "run the tests. {}Then dev mode, then vp",
        "More notes. ".repeat(10)
    );
    close(scored(&far, "vp run dev").quality, 1.0);

    let missing = Query::prepare("vp run dev").expect("query");
    assert!(score(&missing, "vp run").is_none());
    assert!(Query::prepare("  \"\"  ").is_none());
}
