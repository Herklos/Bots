# Adopting ideas from threebot: index and sequencing

threebot (`github.com/RokaSwK/threebot`) is a sibling Bot app on T3 Code. These plans take specific commits and [PR #52](https://github.com/RokaSwK/threebot/pull/52) and map them onto Michel without copying their stack (Exact + Rust gateway stay).

| Plan | Doc | Size | Depends on |
|---|---|---|---|
| `@` mention menu improvements | [mention-menu-plan.md](mention-menu-plan.md) | S to M | none |
| Independent approval policy | [independent-policy-plan.md](independent-policy-plan.md) | S to M | optional: small-items `maxRuntimeMode` |
| Memory Markdown UX | [memory-markdown-ux-plan.md](memory-markdown-ux-plan.md) | M | none (coords with memory-store / ownership later) |
| Multi-machine product layer | [multi-machine-plan.md](multi-machine-plan.md) | L | [bot-execution-environment.md](bot-execution-environment.md) PR 2+ for list/status; PR 5 / R1 for remote read |

Left out of this batch (explored, not planned here): adopt-an-existing-T3-project ([6b66e238](https://github.com/RokaSwK/threebot/commit/6b66e238c3fb149d3abd2a1473c69b4a58440ef7)). Ask if that should become its own plan.

## Recommended order

1. **Mention menu improvements.** Small, local to Exact + a thin gateway twin; visible polish; no beta flag.
2. **Independent policy.** Opt-in mode + prompt + Always Ask wiring; no harness protocol change beyond mapping to `full-access`.
3. **Memory Markdown UX.** Unblocks the "person cannot edit memory" gap without waiting on the store plan.
4. **Multi-machine product layer.** After or with environments list (status fields); outage alerts next; remote transcript last behind forwarding.

## Shared Phase 0 spikes

- Exact caret / selection on composer input (mention plan).
- `put_settings` persistence for `rules.alwaysAsk` / `allowWithoutAsking` (Independent plan).
- Live `peerLinks.list` fixture with at least one link (multi-machine + environments).
- Memory poller latency after a file write (memory UX).

## Cross-cutting rules

- Rust ≤ 500 lines, Exact ≤ 300 lines per file; put new UI in new contracts (`mention-text`, `bot/memory`, machines page).
- Chrome strings only via `app/strings/en.json` and `fr.json`.
- No em-dashes in docs or UI strings.
- Defaults leave today's behaviour unchanged: mention Escape sticky is additive; Independent is opt-in; Memory tab is additive; Machines UI read-only until environments gates open.
- Do not send `environmentId` on RPCs the installed T3 build rejects (`Agents.md`).

## Source commits

| Source | Topic |
|---|---|
| [PR #52](https://github.com/RokaSwK/threebot/pull/52) | Mention menu |
| [1e6da7a](https://github.com/RokaSwK/threebot/commit/1e6da7abb553af08441df33a5aa3a6af6598bfd7) | Multi-machine |
| [4f7cd57](https://github.com/RokaSwK/threebot/commit/4f7cd57c87ece241547808135f9281f877b29776) | Independence prompt |
| [f359550](https://github.com/RokaSwK/threebot/commit/f35955039874e31f68d0dac690e925405b45225d) | Memory Markdown UX |
