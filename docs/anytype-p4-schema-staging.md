# P4 schema v2

Prepare now creates the P4 catalog in `gateway/src/storage/adapters/anytype/schema/p4.rs` and records schema version 2. Existing v1 spaces require Prepare again before the Tier A engine starts. The catalog declares P4 types and their fields, including bundled query properties; no typed writer is enabled yet. Mock tests exercise the v1 upgrade and idempotency, but live scratch-space bootstrap remains unverified.

## Permanent-key conflicts

Existing v1 spaces have `m_owner_bot` and `m_status` as text. Anytype property formats are permanent, so P4 must not reinterpret or replace them. The staged typed schemas use `m_owner_bot_ref` (objects) for Bot links and `m_record_status` (select) for the union of the Library record status values. The original text properties remain available for Tier A files and devices. Each record kind still validates its own status subset. The schema design's references to typed `m_owner_bot` and select `m_status` should be read as these new keys when implementing P4 mappers.

`m_first_at` and `m_last_at` are text so segment timestamps retain their full precision. `m_tags` is text, allowing exact serialization of tags without relying on a globally closed option set. Record mappers must agree on the encoding before enabling writers. These choices are provisional until tested against actual typed objects.

## Release and migration

Version 2 writes `m_min_gateway` as `0.4.4`, the intended first release with this schema. The current source tree still has package version `0.4.3`: this specific development build recognizes schema v2 with exactly that minimum, but rejects higher or malformed minimums. **Before releasing this code, bump the package version to at least 0.4.4.** A later gateway preserves a higher compatible minimum rather than lowering it.

Preparing a v1 space is additive and requires all current gateways to be upgraded or stopped first. A new minimum-version field cannot retroactively teach previously shipped binaries to honor it. In particular, tagged 0.4.3 uses the earlier `hb_*` namespace, while unreleased `m_*` v1 builds lack the current metadata guard. Do not advertise old-gateway read-only protection until it has been demonstrated with actual binaries. Current gateways use cooperative locks for local overlay updates; older gateways do not honor them.

Mock tests cover schema creation, existing custom type properties, v1 metadata upgrade, unchanged Tier A objects, repeated Prepare and version guard behavior. Actual select option creation, type layout behavior and full bootstrap on a disposable Anytype space still need live validation. The connected key in this session did not grant a scratch space; no existing space was changed.
