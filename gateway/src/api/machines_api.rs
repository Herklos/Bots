//! Linked computers, read only.
//!
//! The handler lists this computer and the links T3 Code already has.
//! It does not pair, and it does not send `environmentId`.

use super::*;

pub(crate) async fn machines_get(State(s): S) -> Json<Value> {
    Json(crate::bots::computer::machines_view(
        &computers_of(&s).await,
    ))
}
