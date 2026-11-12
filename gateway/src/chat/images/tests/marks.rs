use super::super::{marks, strip};
use std::collections::HashSet;

fn path_of(text: &str) -> Option<String> {
    marks(text, "")
        .into_iter()
        .next()
        .and_then(|mark| mark.path)
}

fn shown(paths: &[&str]) -> HashSet<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
}

#[test]
fn a_library_path_is_kept_and_every_other_target_is_not() {
    assert_eq!(
        path_of("![chart](library/chart.png)"),
        Some("library/chart.png".into())
    );
    assert_eq!(
        path_of("![chart](./library/chart.JPEG)"),
        Some("library/chart.JPEG".into())
    );
    assert_eq!(
        path_of("![chart](<library/chart.png>)"),
        Some("library/chart.png".into())
    );
    assert_eq!(
        path_of("![chart](\"library/chart.png\")"),
        Some("library/chart.png".into())
    );
    assert_eq!(
        path_of("![chart](library/chart.png \"the chart\")"),
        Some("library/chart.png".into())
    );
    assert_eq!(
        path_of("![chart](library/a%20b.png)"),
        Some("library/a b.png".into())
    );
    assert_eq!(
        path_of("![chart](library/%252e%252e/a.png)"),
        Some("library/%2e%2e/a.png".into())
    );
    assert_eq!(path_of("![chart](/bots/ada/library/chart.png)"), None);
    assert_eq!(
        marks("![chart](/bots/ada/library/chart.png)", "/bots/ada")[0]
            .path
            .as_deref(),
        Some("library/chart.png")
    );
    assert_eq!(
        marks("![chart](/bots/ada-evil/library/chart.png)", "/bots/ada")[0].path,
        None
    );
    for bad in [
        "![x](https://example.test/a.png)",
        "![x](http://example.test/a.png)",
        "![x](data:image/png;base64,aaaa)",
        "![x](file:///tmp/a.png)",
        "![x](javascript:alert(1))",
        "![x](//example.test/a.png)",
        "![x](library/../secret.png)",
        "![x](library/%2e%2e/secret.png)",
        "![x](library/.hidden.png)",
        "![x](library/chart.svg)",
        "![x][ref]",
        "![]()",
    ] {
        let found = marks(bad, "/bots/ada");
        assert!(found.iter().all(|mark| mark.path.is_none()), "{bad}");
    }
    assert!(marks("library/metadata:x.png is prose", "").is_empty());
    assert_eq!(
        path_of("![x](library/metadata:x.png)"),
        Some("library/metadata:x.png".into())
    );
}

#[test]
fn code_is_left_alone_and_a_remote_mark_becomes_its_alt() {
    let fenced = "```\n![x](https://example.test/a.png)\n```";
    assert!(marks(fenced, "").is_empty());
    let inline = "See `![x](https://example.test/a.png)` please";
    assert!(marks(inline, "").is_empty());
    let text = "Before ![remote](https://example.test/a.png) after";
    let found = marks(text, "");
    assert_eq!(strip(text, &found, &shown(&[])), "Before [remote] after");
    let empty = "Before ![](https://example.test/a.png) after";
    let found = marks(empty, "");
    assert_eq!(strip(empty, &found, &shown(&[])), "Before  after");
    let kept = "![chart](library/chart.png)";
    let found = marks(kept, "");
    assert_eq!(strip(kept, &found, &shown(&["library/chart.png"])), "");
    assert_eq!(marks(r"not \![x](library/a.png) a mark", "").len(), 0);
}

#[test]
fn nine_refs_and_a_duplicate_share_one_path() {
    let mut text = String::new();
    for n in 1..=9 {
        text.push_str(&format!("![n](library/{n}.png) "));
    }
    text.push_str("![again](library/1.png)");
    let found = marks(&text, "");
    let paths: Vec<_> = found.iter().filter_map(|mark| mark.path.clone()).collect();
    assert_eq!(paths.len(), 10);
    assert_eq!(paths[0], "library/1.png");
    assert_eq!(paths[9], "library/1.png");
}

#[test]
fn epoch_is_zero_and_a_bad_stamp_is_unknown() {
    assert_eq!(
        super::super::refs::parse_time_ms("1970-01-01T00:00:00Z"),
        Some(0)
    );
    assert_eq!(
        super::super::refs::parse_time_ms("1970-01-01T00:00:00.500Z"),
        Some(500)
    );
    assert!(super::super::refs::parse_time_ms("not a time").is_none());
}
