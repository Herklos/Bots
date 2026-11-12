# Plan: `@` mention menu improvements

Bring the composer and bubble polish from threebot [PR #52](https://github.com/RokaSwK/threebot/pull/52) into hbots, without changing how mentions wake group members.

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**.

## 1. Facts

### 1.1 hbots today **[verified]**

- Composer menu: `app/mentions/model.contract` (`syncMention`, `completeMention`, `clearMention`), `app/chat/mention.contract` (`MentionMenu`), wired from `app/chat/screen.contract` and `app/group/screen.contract`.
- Query detection: split on `@`, take the last segment; clear when the query contains space, newline or tab, or when the character before `@` is a `wordChar` (letters and digits only, `app/ui/root-types.contract`).
- Insert: `mentionReplace` replaces the trailing query with `slug` plus a space. Group chips call `mention(m)` and append `@slug `.
- Pool: `botMentions` (visible Bots) and `roomMentions` (`everyone`, `all`, then members). Filter: slug or name prefix, or name with spaces removed; cap 8.
- Keys: ArrowUp/Down wrap, Enter/Tab complete, Escape clears (`app/chat/model.contract`, `app/group/model.contract`).
- Gateway wake: `gateway/src/chat/rooms/dispatch.rs` (`mention_words`, `mention_matches`, `mentioned`, `wants_everyone`). Tokens after `@` are alphanumeric plus `-` `_`. Match is slug or compact name (alphanumeric only). A member named `everyone` / `all` wins over the broadcast.
- User bubbles: `app/chat/message.contract` draws `row.mine` as plain `text row.text` (no markdown, no mention chips). Assistant bubbles use `MessageParts` with markdown.
- Tests: `app/tests/mentions.test.contract` (open menu, filter, pick, everyone chip, stay on group).
- CHANGELOG already lists composer completions and `@everyone` / `@all`.

### 1.2 threebot PR #52 **[verified from PR body and source]**

- Shared pure module `server/mention-rules.mts`: boundary before `@`, longest name first, no match inside fenced or inline code, query closes on newline or after 32 characters.
- Sticky Escape: menu is derived from text, so Escape sets a dismissed marker; the same text does not reopen until the draft changes.
- Open Bot excluded from the list (self-mention is a dead link).
- Complete `@Name ` ends the query; exception when another visible name can still grow from what was typed (`Max` next to `Max Power`).
- User bubble draws `@Name` with the same avatar+name chip assistants use; on a Bot-coloured bubble the chip inherits the bubble foreground.
- Plain text on the wire; no server or send-format change.
- Dense unit and composer tests.

### 1.3 Gaps this plan closes

1. Escape is not sticky: clearing then recomputing on the same draft reopens the menu.
2. The open Bot (direct chat) is still offered, so a self-mention is a no-op link.
3. Boundary rules miss punctuation (`(`, `[`, quotes, markdown markers) and treat `/` and `_` as word chars inconsistently with the gateway.
4. No code-fence or inline-code exclusion in the composer.
5. After a complete `@slug `, typing more can leave an empty or stale menu state in edge cases (especially multi-word display names).
6. User bubbles show bare `@ada` text, not a chip.
7. Composer query logic and gateway `mention_words` can disagree on what counts as a mention.

## 2. Goals and non-goals

Goals

- Keep today's wake behaviour: insert **slug** (and `@everyone` / `@all`), gateway still decides who wakes.
- Sticky Escape, self-exclude in direct chat, richer boundaries, code exclusion, complete-query close with prefix-growth exception.
- User (and room) bubbles render `@slug` / `@everyone` as chips when the slug resolves.
- One shared mention-rules module used by the Exact helpers and covered by gateway unit tests for the same boundary cases (Rust port or golden fixtures).

Non-goals

- Changing send format to structured mention objects.
- Mentions of people (only Bots and broadcast tokens).
- Renaming insert from slug to display name (would break gateway match unless both are accepted; keep slug).
- Porting threebot's React `MentionMenu` component as-is; Exact already has one.

## 3. Design

### 3.1 Shared rules module (decision D1)

Add pure helpers in a small place both sides can mirror:

| Helper | Behaviour |
|---|---|
| `isMentionBoundary(text, at)` | `@` at 0, or previous char is whitespace or one of ``([{<'"“`*_~.``, never after letter, digit, `_`, `/`, `@`. |
| `mentionQueryAt(text, cursor)` | Walk back at most 33 chars; stop on newline; require boundary; query length ≤ 32; reject if cursor sits inside a fenced block or inline code span that started before the `@`. |
| `activeMention(text, cursor, names)` | Like query, but returns null when text from `@` already equals a complete `@slug ` / `@name ` and no longer visible name can grow from it. |
| `withMention(draft, cursor, slug)` | Replace `[start, cursor)` with `@slug `, collapse a following space. |
| `parseMentionSpans(text, known)` | Longest-slug / longest-compact-name first; skip code; emit `{ start, end, slug, kind: "bot" \| "everyone" \| "all" }` for rendering. |

**Recommended layout:** Exact-side pure fns in a new `app/mentions/rules.contract` (or `app/ui/mention-rules.contract`) under the 300-line cap. Rust twin in `gateway/src/chat/rooms/mention_rules.rs` used by `mention_words` / a new `mention_spans` helper. Golden cases live in both test files with the same inputs.

Decision D1 alternatives: Exact-only (gateway stays as today). Rejected because user chips and wake must agree on boundaries.

### 3.2 Sticky Escape

State: `mentionDismissed = false` and `mentionDismissedText = ""` (or a single optional string of the draft that was dismissed).

- Escape while the menu is open: `clearMention()`, set dismissed to the current draft text.
- `syncMention`: if dismissed text equals current text, leave `mentionOn = false` even when a query would match.
- Any draft change that differs from dismissed text clears the dismissed marker, then runs normal sync.

Apply in both chat and group composers.

### 3.3 Self-exclude and pool

- Direct chat (`roomId == ""`): `mentionPool = botMentions(bots).filter(h => h.slug != slug)` where `slug` is the open Bot.
- Group: keep members + everyone/all; do not remove the person typing (there is no self Bot). Optional later: exclude the last speaker if product wants it; **out of scope**.
- Hidden / `hideFromSidebar` Bots stay out of `botMentions` as today.

### 3.4 Complete-query close

After computing `mentionQueryAt`, if the text from `@` to cursor (case-insensitive) equals `@` + some hit's slug + space, and no other hit's slug or compact name is a longer prefix of the typed token, clear the menu. Use slug for equality because that is what we insert. Also accept `@everyone ` / `@all `.

### 3.5 User and room bubble chips

New component `MentionText` (or extend message drawing):

- Input: plain text, list of known hits (bots + everyone/all in rooms).
- Output: row of text runs and chips. A chip shows Avatar (when shape set) + name; `testId` like `mention-chip-{slug}`.
- On `row.mine` bubbles, inherit bubble foreground for contrast (threebot behaviour). Exact: pass a `onMine: bool` and set chip text/background from `surface.bubble` / inverted ink when mine.
- Assistant markdown path: either leave markdown as today (links already work for some cases) **[to verify]** or run `parseMentionSpans` before markup. Prefer: mine and room person lines use `MentionText`; Bot markdown stays markdown unless a spike shows `@slug` is invisible there.

Tap on a chip in a room: optional navigate to that Bot. **Phase 2**; Phase 1 is display only.

### 3.6 Cursor position **[to verify]**

Exact textarea `input` may not expose caret index. Spike E0.1:

1. If Exact gives selection start on input/key events, use it for `mentionQueryAt`.
2. If not, keep today's "last `@` segment" heuristic for opening the menu, and only use full rules for sticky Escape, complete-query, and rendering.

Document the chosen path in Phase 0 results. Do not invent a hidden caret API.

## 4. Phase 0 spikes

| # | Task | Exit |
|---|---|---|
| E0.1 | Confirm whether Exact composer events expose caret / selection | Written choice for 3.6 |
| E0.2 | Fixture texts: punctuation before `@`, code fence, inline code, `@slug ` then prose, `Max`/`Max Power` style slug pairs, self-mention | Shared golden JSON under `app/tests/` or `gateway/.../fixtures/` |
| E0.3 | Screenshot or tree dump of a mine bubble with a chip on web | Confirms contrast tokens |
| E0.4 | Confirm assistant markdown already styles `@slug` or not | Decide 3.5 Bot path |

## 5. Phases

### P1. Rules + sticky Escape + self-exclude (S)

- Add Exact rules helpers and wire `syncMention` / `completeMention`.
- Sticky Escape state.
- Self-exclude in direct chat.
- Complete-query close.
- Contract tests for Escape sticky, self-exclude, punctuation boundary, complete close.
- No visual chip work yet.

### P2. Gateway twin + wake parity (S)

- Port boundary helpers; make `mention_words` use them (or assert goldens match).
- Tests for fence skip and punctuation.
- Docs note in CHANGELOG under Unreleased.

### P3. User / room chips (M)

- `MentionText` component, mine + room person lines.
- Strings if any (`chat.mentionChip` aria).
- Tests: tree has `mention-chip-ada` after a sent `@ada` in a fixture transcript.
- French + English chrome only if new labels appear.

### P4. Optional chip navigate (S)

- Tap chip opens that Bot (or focuses member in room). Skip if Exact press-on-inline is awkward.

## 6. Files (expected)

| Area | Files |
|---|---|
| Exact | `app/mentions/rules.contract` (new), `app/mentions/model.contract`, `app/chat/model.contract`, `app/group/model.contract`, `app/chat/message.contract`, `app/chat/mention-text.contract` (new), `app/tests/mentions.test.contract`, maybe `app/tests/mention-rules.test.contract` |
| Gateway | `gateway/src/chat/rooms/mention_rules.rs` (new), `dispatch.rs` (call sites only), `tests` sibling |
| Strings | only if new aria/labels |

Stay under Exact 300 and Rust 500 line caps; split tests out.

## 7. Risks and decisions

| Id | Decision | Recommendation |
|---|---|---|
| D1 | Shared Exact+Rust rules vs Exact-only | Shared goldens |
| D2 | Chip navigate in P3 or later | Later (P4) |
| D3 | Insert display name instead of slug | No; keep slug |

Risks

- Caret missing: fall back to last-`@` heuristic; still ship sticky Escape and chips.
- Over-matching inside code in old transcripts: rendering skip is display-only; wake path should match.
- `mentionReplace` today assumes the query is a suffix of the whole draft (`text.length - q.length`). Multi-`@` drafts need replace at the active span start once caret exists; until then keep suffix replace but add a test that documents the limitation.

## 8. Out of scope (left for other plans)

- Structured mention entities on the wire.
- Multi-machine / Independent / Memory UX (separate docs).
