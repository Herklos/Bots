use super::{parse, render, CodecError, Sidecar};
use serde_json::json;

#[test]
fn render_supported_blocks_and_normalize() {
    let input = vec![
        json!({"type":"heading_2", "text":"Memory"}),
        json!({"type":"paragraph", "text":"**bold** [link](anytype://object?objectId=abc)\nline two"}),
        json!({"type":"bulleted_list_item", "text":"first", "indent":0}),
        json!({"type":"checkbox", "text":"nested", "checked":true, "indent":1}),
        json!({"type":"numbered_list_item", "text":"numbered"}),
        json!({"type":"quote", "text":"line one\nline two"}),
        json!({"type":"code", "language":"rust", "text":"let x = `a`;\nprintln!(\"hi\");"}),
        json!({"type":"divider"}),
        json!({"type":"table", "columns":[{},{}], "rows":[{"is_header":true,"cells":["A", "B"]},{"cells":["x|y", "z"]}]}),
    ];
    let first = render(&input);
    assert!(first.sidecar.is_empty());
    assert!(first.markdown.contains("  - [x] nested"));
    assert!(first.markdown.contains("x\\|y"));
    let blocks = parse(&first.markdown, &first.sidecar).unwrap();
    assert_eq!(render(&blocks), first);
}

#[test]
fn opaque_blocks_round_trip_and_cannot_be_silently_removed() {
    let unknown = json!({"type":"callout","id":"full-id","text":"hello","icon":{"format":"emoji","emoji":"💡"},"fields":{"future":42}});
    let styled = json!({"type":"paragraph", "id":"styled", "text":"hello", "color":"red"});
    let first = render(&[unknown.clone(), styled.clone()]);
    assert_eq!(first.sidecar.len(), 2);
    assert_eq!(
        parse(&first.markdown, &first.sidecar).unwrap(),
        vec![unknown, styled]
    );
    assert_eq!(
        parse("", &first.sidecar),
        Err(CodecError::RemovedOpaqueBlock)
    );
    assert_eq!(
        parse(
            &format!("{}\n{}", first.markdown, first.markdown),
            &first.sidecar
        ),
        Err(CodecError::DuplicateOpaqueBlock)
    );
    assert_eq!(
        parse(&first.markdown, &Sidecar::new()),
        Err(CodecError::MissingOpaqueBlock)
    );
}

#[test]
fn front_matter_and_unclosed_fences_are_rejected() {
    assert_eq!(
        parse("---\nname: test\n---\nbody", &Sidecar::new()),
        Err(CodecError::FrontMatter)
    );
    assert_eq!(
        parse("```rust\nx", &Sidecar::new()),
        Err(CodecError::UnclosedFence)
    );
}

#[test]
fn generated_cases_are_idempotent() {
    let atoms = [
        "plain",
        "**bold**",
        "a [link](https://example.com)",
        "one\ntwo",
        "a `code` bit",
        "",
        "éclair",
    ];
    for text in atoms {
        for kind in ["paragraph", "quote", "code"] {
            let source = vec![json!({"type":kind,"text":text})];
            let one = render(&source);
            let two = render(&parse(&one.markdown, &one.sidecar).unwrap());
            let three = render(&parse(&two.markdown, &two.sidecar).unwrap());
            assert_eq!(two, three, "{kind} {text:?}");
        }
    }
}

#[test]
fn repository_skill_fixture_normalizes_idempotently() {
    let source = include_str!("../../../../../../.agents/skills/compatibility/SKILL.md");
    // Repository skills have YAML front matter, which must be handled by the
    // migration before page conversion rather than silently discarded.
    assert_eq!(parse(source, &Sidecar::new()), Err(CodecError::FrontMatter));
    let body = source.splitn(3, "---").nth(2).unwrap();
    let once = render(&parse(body, &Sidecar::new()).unwrap());
    assert_eq!(render(&parse(&once.markdown, &once.sidecar).unwrap()), once);
}

proptest::proptest! {
    #[test]
    fn normalization_is_idempotent_for_arbitrary_text(text in ".{0,200}") {
        for kind in ["paragraph", "quote", "code"] {
            let source = vec![json!({"type": kind, "text": text})];
            let first = render(&source);
            let parsed = parse(&first.markdown, &first.sidecar).unwrap();
            proptest::prop_assert_eq!(parsed[0]["text"].as_str(), Some(text.as_str()));
            let normalized = render(&parsed);
            let again = render(&parse(&normalized.markdown, &normalized.sidecar).unwrap());
            proptest::prop_assert_eq!(normalized, again);
        }
    }
}

#[test]
fn ambiguous_content_is_opaque_instead_of_reinterpreted() {
    let source = vec![
        json!({"type":"divider","id":"first"}),
        json!({"type":"paragraph","text":"# literal heading"}),
        json!({"type":"bulleted_list_item","text":"[x] literal checkbox"}),
        json!({"type":"table","columns":[{}],"rows":[{"is_header":true,"cells":[" padding "]}]}),
        json!({"type":"divider","id":"last"}),
    ];
    let output = render(&source);
    assert_eq!(output.sidecar.len(), 4);
    let parsed = parse(&output.markdown, &output.sidecar).unwrap();
    assert_eq!(&parsed[..4], &source[..4]);
    assert_eq!(render(&parsed), output);
}
