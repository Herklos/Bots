# Plan: provider and quota visibility

Show how much of each provider's subscription window is used and when it resets, warn before a Bot runs into a limit, and let the person switch model from that warning. T3 already measures this; Michel reads the provider list but drops it.

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**.

## 1. Facts

### 1.1 T3 **[verified in upcomputer source]**

- `server.getConfig` returns `providers: ServerProvider[]`. Besides models, each provider has `instanceId`, `driver`, `displayName`, `enabled`, `installed`, `version`, `status`, `auth`, `message`, `statusPending`, `availability`, `unavailableReason`, `setup { canAuthenticate, canInstall, documentationUrl }`, `continuation`, and **`usageLimits`**.
- `usageLimits` is `{ checkedAt, windows: [{ id, kind: "session"|"weekly"|"monthly"|"other", label, usedPercent (0 to 100), resetsAt?, windowDurationMins? }], credentialFingerprint?, resetCredits?, externalUsage? { label, url }, unavailable? { reason: "unsupported"|"probeFailed", message? } }`. T3's own comment: clear bars for unavailable accounts, keep the last good bars after a failed probe.
- The config stream (`subscribeServerConfig`) also carries `usageLimitSources` (cliproxy account snapshots) and can add a `/usage-limits` slash command, only when the client sets `usageLimitsCommand` (otherwise it would arrive as a plain prompt).
- Other RPCs: `server.refreshProviders`, `server.getUsageSummary` (`{ sinceDay, untilDay, timeZone, resolution?, awaitRefresh?, modelFilter? }` returning day or hour buckets by provider and model with token totals and `costUsd`, `costSource`, plus `sources` status and `pricing` status), `server.refreshUsageRates`, `provider.consumeResetCredit`, and the provider management set `provider.auth.start|subscribe|respond|complete|cancel|logout`, `provider.install.start|subscribe|cancel|remove`, `server.updateProvider`.
- Provider kinds in usage: claude, codex, grok, cursor, opencode, antigravity, pi, muse.

### 1.2 Michel **[verified]**

- `harness/adapters/t3/provision.rs:149` calls `server.getConfig` and `harness/providers.rs` (`Providers::list`, 30-second cache) reduces it to what the app shows. Its header says only what the app shows leaves the module, "never an account's email or workspace". `usageLimits` is not used anywhere in `gateway/src` or `app/`.
- The transcript already shows terminal provider failures as events (`provider_failure`: `usage_limit` becomes `rate_limit`, with `resetAt`; chip text "Limited"). That is reactive. This plan is proactive.
- Usage view: `bots/usage/*` (per-Bot token counts from turns), `api/usage_api.rs`, `app/usage/model.contract`, `UsageView` in `app/settings/settings.contract`. Costs are not shown.
- The Bot panel's model page lists models by provider (`ModelList`, `pickModel`), and `thread.model-selection.set` changes a thread's model.
- Settings screens: `app/settings/models.contract` is 40 lines, so there is room.

### 1.3 Unknowns **[to verify]**

1. A real `server.getConfig` from the T3 build in use: which providers report `usageLimits`, which are `unsupported`, how often `checkedAt` changes.
2. Whether `usageLimits` fields contain anything personal (`credentialFingerprint` should never leave the gateway).
3. Token scope for `server.refreshProviders` and `server.getUsageSummary`.
4. Whether `usedPercent` semantics match across providers (used versus remaining).
5. What `statusPending` placeholders look like at startup.
6. The `provider.auth.*` flow shapes (device code, browser, API key) per driver.

## 2. Goals and non-goals

Goals
- Per-provider limit bars (session, weekly) with reset times in Settings, the Bot model page and the model picker.
- A proactive notice in a Bot's chat when its provider crosses 80 percent and 95 percent, with a one-tap "switch model" that picks another enabled provider with headroom.
- Provider health at a glance (signed out, not installed, update advised).
- Optional spend card from `server.getUsageSummary`.

Non-goals
- Resetting credits or buying more.
- Managing T3 itself. Sign-in and install flows are a later, optional phase.
- Exposing emails, workspace ids or fingerprints.

## 3. Design

### 3.1 Normalised shape

`harness/providers.rs` is small today, so the new code goes in `gateway/src/harness/providers/limits.rs` (convert `providers.rs` to a `providers/` module only if it nears 500 lines). Pure function:

```text
fn limits_of(provider: &Value) -> Option<Value>
```

Output per provider instance:

```json
{ "instanceId": "claude", "label": "Claude", "state": "ok|unavailable|signed-out|not-installed",
  "windows": [ { "id": "5h", "kind": "session", "label": "5 hour", "usedPercent": 62, "resetsAt": "2026-11-12T17:00:00Z" } ],
  "checkedAt": "...", "note": "" }
```

Rules: clamp `usedPercent` to 0 to 100; drop windows without a finite percent; sort `session`, `weekly`, `monthly`, `other`; `unavailable.reason == "unsupported"` yields no bars and no note (not an error); `probeFailed` keeps the last good windows with `note: "Could not refresh"`. Never copy `credentialFingerprint`, `resetCredits` ids or `runtimePaths`. `externalUsage.url` is allowed only when it parses as `https`.

### 3.2 API

`GET /api/providers/limits` returns `{ providers: [...], at }` from the same 30-second cache as the model list (no extra T3 call). `POST /api/providers/refresh` calls `server.refreshProviders` once, rate limited to one per minute, and returns the new list. Both require the normal session.

### 3.3 App

- Settings, Models: a card per provider with one bar per window (colour steps at 80 and 95 percent, text always present for accessibility), the reset time via `app/host/time.ts`, a "Refresh" button.
- Bot panel model page: the chosen provider's worst window next to its name.
- Model picker rows: "Claude 91% used, resets 17:00" for the provider with the worst window; a quiet label for `unsupported`.
- New component `app/settings/limits.contract` (under 300 lines) so `models.contract` stays small.

### 3.4 Proactive notice

A gateway task evaluates each Bot's current provider (from its model selection) on every cache refresh. When `max(usedPercent)` crosses 80 or 95 upward, it emits one event for that Bot and threshold until usage drops below the threshold or the window resets. Event kind `provider-near-limit`, rendered by the existing chip row (`eventVerb` entry plus strings): "Claude is at 95% until 17:00" with an action chip **Switch model**. The action opens the Bot's model page with an alternative preselected: the first enabled, installed, signed-in provider whose worst window is under 70 percent, otherwise none (the notice then only informs). Dedupe state is in memory plus a small `.botapp/limit-notices.json` so restarts do not repeat.

The existing terminal `Limited` event stays; it covers the case where the limit was hit anyway.

### 3.5 Spend card (optional)

`GET /api/usage/spend?period=` calls `server.getUsageSummary` with the person's time zone and `resolution: "day"` and returns totals by provider and model (`costUsd`, tokens, `costSource`, pricing status). It is machine-wide T3 usage, not per Bot, and the card says so. Unpriced records are shown as "no price" and not summed into totals. Cached 5 minutes. `awaitRefresh` is never set from a poll.

### 3.6 Sign-in and install (optional, last)

First version: show state and "Open T3 Code to sign in" with `documentationUrl`. Second: drive `provider.auth.*` and `provider.install.*` with their subscribe streams for drivers whose flow is a link or code; API-key flows go through `request_secret`-style input and are never logged.

## 4. Phases

### Phase 0: spikes (S)

- Q0.1 Capture a real `server.getConfig`; list providers, `usageLimits` presence and `unavailable` reasons. Save a redacted fixture.
- Q0.2 Confirm scopes for `server.refreshProviders` and `server.getUsageSummary`.
- Q0.3 Compare percent semantics for two providers against their own UI.
- Q0.4 Check how often T3 updates the windows (watch `checkedAt` for 10 minutes).
- Q0.5 Skim `provider.auth.*` for one driver (for Phase 6 sizing).
Exit: normaliser rules confirmed; thresholds D1 answered.

### Phase 1: normaliser and API [PR 1], S

- `limits.rs`, route, cache reuse, fixtures. Tests: missing `usageLimits`, `unsupported`, `probeFailed` with last good bars, out-of-range percent, secret fields stripped, non-https `externalUsage`.

### Phase 2: Settings and model page [PR 2], M

- `limits.contract`, strings, refresh button with cooldown state, per-provider cards, model-page hint. Contract tests: empty, one bar, near limit colour plus text, unavailable.

### Phase 3: picker hints [PR 3], S

- Hints in the picker rows; keyboard and screen-reader text.

### Phase 4: proactive notice [PR 4], M

- Evaluator task, threshold crossing with hysteresis (clear below 75 and 90), event kind and chip action, alternative selection, notice store. Tests: crossing once, reset clears, two Bots on the same provider each get one notice, no alternative available, restart does not repeat.

### Phase 5: spend card [PR 5], M (optional)

- Route, cache, card, caveat text. Tests: unpriced records, partial sources, time zone day boundaries.

### Phase 6: sign-in and install [PR 6], L (optional)

- Per Q0.5 findings. Out of the main path.

### Phase 7: docs [PR 7], S

- README features line, CHANGELOG, a privacy note that limits come from the provider's own endpoints through T3.

## 5. Test strategy

- Fixture-driven pure tests; no network.
- Event tests with a fake clock.
- A privacy test: serialising the API response never contains the fingerprint, runtime paths or email.
- App contract tests for each bar state.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Provider probes get rate limited if Michel forces refresh | Read cached config; manual refresh capped at one per minute |
| Percent semantics differ | Q0.3; show provider label text, not our own wording |
| Notice noise | Hysteresis, one per threshold per window, Bot's current provider only |
| `usageLimits` absent on stable T3 | No bars shown; no error |
| Personal data in limits | Whitelist fields in `limits_of` |

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | Warning thresholds | 80 and 95 percent |
| D2 | Ship spend card | Yes, after the bars, labelled machine-wide |
| D3 | Provider sign-in inside Michel | Not in the first release |
