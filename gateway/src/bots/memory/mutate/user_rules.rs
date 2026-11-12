//! Stop a Bot from replacing, forgetting, or copying a fact the person wrote.
use super::super::user::{refuse_user_change, UserWrite};
use super::store::refusal;
use super::{MutateFail, Mutation, Op};
use std::io;

pub(super) async fn bot_user_refusal(
    input: &Mutation<'_>,
) -> Result<Option<MutateFail>, io::Error> {
    if input.trusted_user {
        return Ok(None);
    }
    let Some(root) = input.user_root else {
        return Ok(None);
    };
    let kind = match input.op {
        Op::Add => UserWrite::Add,
        Op::Replace | Op::Supersede => UserWrite::Replace,
        Op::Forget => UserWrite::Forget,
        Op::Note => UserWrite::Other,
    };
    let target = match input.op {
        Op::Forget => input.fact.or(input.old).unwrap_or(""),
        Op::Replace | Op::Supersede => input.old.unwrap_or(""),
        Op::Add | Op::Note => "",
    };
    let incoming = match input.op {
        Op::Add => input.fact.unwrap_or(""),
        Op::Replace | Op::Supersede => input.new_fact.unwrap_or(""),
        Op::Forget | Op::Note => "",
    };
    let name = if input.bot_name.trim().is_empty() {
        input.slug
    } else {
        input.bot_name
    };
    let block =
        refuse_user_change(root, input.slug, kind, target, incoming, name, input.now).await?;
    Ok(block.map(|block| refusal(block.code(), block.message())))
}
