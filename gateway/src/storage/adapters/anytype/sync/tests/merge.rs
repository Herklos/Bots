use super::super::merge::{merge_text, Merged};

#[test]
fn clean_merge_keeps_both_edits() {
    let base = "alpha\nmiddle\nomega\n";
    let local = "ALPHA\nmiddle\nomega\n";
    let remote = "alpha\nmiddle\nOMEGA\n";
    assert_eq!(
        merge_text(base, local, remote),
        Merged::Clean("ALPHA\nmiddle\nOMEGA\n".into())
    );
}

#[test]
fn overlapping_edit_conflicts() {
    let base = "alpha\nmiddle\nomega\n";
    let local = "alpha\nLOCAL\nomega\n";
    let remote = "alpha\nREMOTE\nomega\n";
    assert_eq!(merge_text(base, local, remote), Merged::Conflict);
}

#[test]
fn identical_edits_merge() {
    let base = "alpha\nmiddle\nomega\n";
    let edited = "alpha\nBOTH\nomega\n";
    assert_eq!(
        merge_text(base, edited, edited),
        Merged::Clean(edited.into())
    );
}

#[test]
fn crlf_is_preserved() {
    let base = "alpha\r\nmiddle\r\nomega\r\n";
    let local = "ALPHA\r\nmiddle\r\nomega\r\n";
    let remote = "alpha\r\nmiddle\r\nOMEGA\r\n";
    let Merged::Clean(text) = merge_text(base, local, remote) else {
        panic!("expected a clean merge");
    };
    assert_eq!(text, "ALPHA\r\nmiddle\r\nOMEGA\r\n");
    assert!(!text.replace("\r\n", "").contains('\n'));
}
