use super::super::decide::{decide, Action, Local, Remote};
use super::super::policy::MergePolicy;

fn present(sha: &str) -> Local {
    Local::Present(sha.into())
}

fn remote(sha: &str) -> Remote {
    Remote::Present(sha.into())
}

fn assert_action(
    base: Option<&str>,
    local: &Local,
    remote: &Remote,
    policy: MergePolicy,
    expected: Action,
) {
    assert_eq!(decide(base, local, remote, policy), expected);
}

#[test]
fn row_nothing() {
    assert_action(
        None,
        &Local::Absent,
        &Remote::Absent,
        MergePolicy::Text3Way,
        Action::Noop,
    );
}

#[test]
fn row_new_local_file() {
    assert_action(
        None,
        &present("a"),
        &Remote::Absent,
        MergePolicy::Text3Way,
        Action::PushCreate,
    );
}

#[test]
fn row_new_remote_file() {
    assert_action(
        None,
        &Local::Absent,
        &remote("a"),
        MergePolicy::Text3Way,
        Action::PullCreate,
    );
}

#[test]
fn row_both_created_the_same_bytes() {
    assert_action(
        None,
        &present("a"),
        &remote("a"),
        MergePolicy::Text3Way,
        Action::Adopt,
    );
}

#[test]
fn row_both_created_different_bytes() {
    assert_action(
        None,
        &present("a"),
        &remote("b"),
        MergePolicy::Text3Way,
        Action::Conflict,
    );
}

#[test]
fn row_unchanged() {
    assert_action(
        Some("b"),
        &present("b"),
        &remote("b"),
        MergePolicy::Whole,
        Action::Noop,
    );
}

#[test]
fn row_local_edited() {
    assert_action(
        Some("b"),
        &present("local"),
        &remote("b"),
        MergePolicy::Whole,
        Action::PushUpdate,
    );
}

#[test]
fn row_remote_edited() {
    assert_action(
        Some("b"),
        &present("b"),
        &remote("remote"),
        MergePolicy::Text3Way,
        Action::PullUpdate,
    );
}

#[test]
fn row_both_made_the_same_edit() {
    assert_action(
        Some("b"),
        &present("same"),
        &remote("same"),
        MergePolicy::Text3Way,
        Action::Adopt,
    );
}

#[test]
fn row_both_edited_text_merges() {
    assert_action(
        Some("b"),
        &present("local"),
        &remote("remote"),
        MergePolicy::Text3Way,
        Action::Merge,
    );
}

#[test]
fn row_both_edited_whole_conflicts() {
    assert_action(
        Some("b"),
        &present("local"),
        &remote("remote"),
        MergePolicy::Whole,
        Action::Conflict,
    );
}

#[test]
fn row_local_deleted() {
    assert_action(
        Some("b"),
        &Local::Absent,
        &remote("b"),
        MergePolicy::Text3Way,
        Action::PushTombstone,
    );
}

#[test]
fn row_remote_deleted() {
    assert_action(
        Some("b"),
        &present("b"),
        &Remote::Tombstone,
        MergePolicy::Text3Way,
        Action::DeleteLocal,
    );
}

#[test]
fn row_both_deleted() {
    assert_action(
        Some("b"),
        &Local::Absent,
        &Remote::Tombstone,
        MergePolicy::Text3Way,
        Action::Drop,
    );
}

#[test]
fn row_local_edited_remote_deleted() {
    assert_action(
        Some("b"),
        &present("local"),
        &Remote::Tombstone,
        MergePolicy::Text3Way,
        Action::EditWinsPush,
    );
}

#[test]
fn row_local_deleted_remote_edited() {
    assert_action(
        Some("b"),
        &Local::Absent,
        &remote("remote"),
        MergePolicy::Whole,
        Action::EditWinsPull,
    );
}

#[test]
fn rows_the_table_does_not_list() {
    let policy = MergePolicy::Text3Way;
    assert_action(
        Some("b"),
        &present("b"),
        &Remote::Absent,
        policy,
        Action::PushCreate,
    );
    assert_action(
        Some("b"),
        &Local::Absent,
        &Remote::Absent,
        policy,
        Action::Drop,
    );
    assert_action(
        None,
        &present("a"),
        &Remote::Tombstone,
        policy,
        Action::Conflict,
    );
    assert_action(
        None,
        &Local::Absent,
        &Remote::Tombstone,
        policy,
        Action::Noop,
    );
}

#[test]
fn base_none_is_never_destructive() {
    let locals = [Local::Absent, present("a"), present("b")];
    let remotes = [Remote::Absent, remote("a"), remote("b"), Remote::Tombstone];
    let policies = [MergePolicy::Text3Way, MergePolicy::Whole];
    for local in &locals {
        for remote in &remotes {
            for policy in policies {
                let action = decide(None, local, remote, policy);
                assert!(
                    !action.destroys_content(),
                    "base None {local:?} {remote:?} {policy:?} -> {action:?}"
                );
                match action {
                    Action::Noop
                    | Action::PushCreate
                    | Action::PullCreate
                    | Action::Adopt
                    | Action::Conflict => {}
                    Action::PushUpdate
                    | Action::PullUpdate
                    | Action::Merge
                    | Action::PushTombstone
                    | Action::DeleteLocal
                    | Action::Drop
                    | Action::EditWinsPush
                    | Action::EditWinsPull => {
                        panic!("base None yielded {action:?} for {local:?} {remote:?} {policy:?}")
                    }
                }
            }
        }
    }
}
