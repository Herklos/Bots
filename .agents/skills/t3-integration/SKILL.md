---
name: t3-integration
description: Implement or debug hbots gateway integration with the T3 Code harness, including RPC, thread projections, routines and approvals.
---

# T3 integration

Start with the relevant gateway implementation and tests: `gateway/src/t3.rs` for transport, `chat.rs` and `lineage.rs` for threads, `routines.rs` for scheduled tasks, `approvals.rs` for requests, and `provisioner.rs` for Bot setup. Check actual T3 contracts or existing fixtures before changing payloads; distinguish observed protocol behavior from assumptions.

Long-lived `orchestration.subscribeShell` and `scheduledTasks.subscribe` reads need a snapshot and must interrupt the subscription. In particular, preserve the update safeguard that waits when a busy-thread snapshot fails or returns `snapshot-required`. Keep remote child environment and thread identities distinct from local and parent threads.

Keep harness tokens and secret values out of clients, logs and test output. Add or run focused tests for the affected protocol behavior, and report limitations where real T3 behavior was not verified. Follow the default Bot model in `AGENTS.md` when creating Bot threads.
