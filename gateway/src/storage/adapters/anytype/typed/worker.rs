//! The typed-sync worker drains the outbox that store writes enqueue. Store
//! calls flush inline already; this loop covers retries after backoff and any
//! write the process crashed past.

use super::TypedCtx;
use std::sync::Arc;

pub(crate) fn start(ctx: Arc<TypedCtx>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(super::sleep_duration());
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            ctx.drain().await;
        }
    });
}
