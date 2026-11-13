# Michel redesign plan

Status: built on branch `redesign` (steps 0 to 10). Not done: reactions, scroll-to-end and New messages pill, drag and drop and paste, Vault favicons, removing the Projects HTTP routes (B16), and the sign-in polish. Grouping of message runs stayed in `app.ts`. Values stay in `design/tokens.json`; every new value goes there first, then `cargo run -p hbots-design -- sync`.

## 0. Decisions already made

- Match or beat every reference screenshot. The Ship screenshot is the anatomy; the other six supply components.
- The sent bubble stays Tide with Night ink.
- The whole right panel, the left sidebar, the chat messages and Settings are redesigned from scratch.
- The Bot panel is read-only for Routines and Library. Model, provider and Usage become real, configurable screens.
- Backend changes are in scope.

## 1. Brief, as read

- **Fixed by the crate** (`design/docs/guidelines.md`): Tide brand, Night ink, the Face and its four states, platform type only, radius 8/14/20/full, space 4 to 24, 44 pt targets, motion 120/200/320 ms, reduced motion holds still, sentence-case voice.
- **Free to change:** layout, components, surfaces (new tokens), shapes and colours of the Face, copy, every screen.

## 2. What is wrong today

I read `app/app.contract`, `app/app.ts` and the gateway modules named in section 6.

| Area | Today | Effect |
|---|---|---|
| Light bubbles | Bot bubble is `surface.card` `#ffffff` on `#fcfcfd`, no border | Bot messages nearly vanish in light mode |
| Message text | Plain `pre-wrap` text | Bots write Markdown (lists, code, links); it shows as raw symbols |
| Messages | Bubbles only; no time, no day marks, no event lines, no cards, no reactions | Reads like a log, not a messenger |
| Sending | Bubble appears after the next 1.5 s poll; one shared `draft` for every Bot | Feels laggy; typing in Ship then opening Inbox shows the same draft |
| Requests | "Answer this in T3 Code for now" | The gateway already has `/api/bots/{slug}/requests/{id}/respond`; the app doesn't use it |
| Face | `shape` prop is ignored; always a ball | Bots differ only by colour |
| Left sidebar | Title, search pill, rows with a repeated status line, a star for the main Bot | No preview, time, unread or pinned Bots |
| Right panel | Six heavy cards (Identity, Inbox, Model, Routines, Working on, Delete); Library with filters, add, delete and connectors; Computer as a button | Dense; most of it is editing nobody asked for |
| Model | Haiku plus whatever is set; provider hard-coded to `claudeAgent`; effort fixed at medium | No way to choose a provider or model |
| Usage | Tokens are lifetime totals; counts are on Settings only | No periods, no per-model view |
| Settings | One inset-grouped list: name, three pills, one usage sentence, Vault link, password | Flat; no profile, no models, no usage, no access status |
| Dead code | `ProjectSheet`, the `board` resource, `setProjects`, standalone Library and Computer routes | Unreachable or duplicated |

## 3. Design system

### Concept

**The Face is the product.** Every Bot has a colour and a shape (eight shapes, ten colours). The open Bot's Face is the largest, only moving thing on screen. All else is neutral. Boldness is spent in one place: the Bot's body, with the Tide sent bubble as the crate's "one filled thing".

### Colour (all in `tokens.json`)

| Name | Value | Role |
|---|---|---|
| Tide | `#1cc3b0` | Sent bubble, send button, mark, chart bars |
| Night | `#16141f` | Ink on Tide, dark canvas base |
| Paper | `#f9f8fd` | Eye cut-outs, light surfaces |
| Mist | `#8f8da0` | Outlines |
| `surface.bubble` (new) | light `#eceef1`, dark `#26262c` | Bot bubble. Fixes the invisible light bubble |
| `surface.raised` (new) | light `#e8eaee`, dark `#2a2a31` | Selected tile and row, composer plus circle, hover |
| `ink.faint` (new) | light `#6f6c80`, dark `#8f8da0` | Times, event lines. Final hex decided by `contrast` at 4.5:1 |

Dark keeps the crate's `#17171b` canvas (not the references' pure black) so existing contrast checks hold. Presence reuses `success` (online) and `info` (unread). No new hue.

### Type

Platform type only. Scale: hero name large-title 700 (-0.8 px); screen title headline 600 (-0.2 px); row name and message body 600/400, line-height 1.5; preview, time and event line footnote in `ink.muted` / `ink.faint` with tabular figures. No all-caps labels, no eyebrows. Code uses the platform's monospace.

### Motion

120 ms press, 200 ms row, panel page or sheet, 320 ms screen. One orchestrated moment: a newly arrived message rises 4 px and fades in (120 ms), only for rows that arrive after the list loaded. The Face's states carry the rest. Reduced motion removes the rise and the Face's movement.

### Layout

```
>= 1200                                    760 to 1199                 < 760 (phone)
+---------+---------------+---------+     +---------+--------------+  +------------------+
| (o) (+) | (Face) Ship   |  Face   |     | (o) (+) | Ship   [mon] |  | (me)       (o) (+)|
| tiles   |               |  Ship   |     | tiles   |              |  | tiles (pinned)   |
| Inbox   |  messages     |  tabs   |     | Inbox   |  messages    |  | Inbox     8:41   |
| Ledger  |               | Details |     | Ledger  |              |  | Ledger    8:22   |
| Vault   |  (+) Ask Ship |  ...    |     | Vault   | (+) Ask Ship |  | Scout    Tues    |
| You     |               |         |     | You     |              |  +------------------+
+---------+---------------+---------+     +---------+--------------+
  288          fluid          320           panel overlays chat
```

Identity (Face and name) is centred in the chat header and in the panel hero. Rows and panel content are left aligned. Messages 46 rem max, forms 44 rem.

### Principles

1. The open Bot's Face is the largest, brightest thing.
2. Neutral surfaces; Tide only where the person acted or the brand speaks.
3. Status is a word or a dot, never a repeated subtitle.
4. Read first: the panel shows what the Bot is doing; editing lives behind one gear.
5. One component per job (one avatar, one row, one card, one model picker, one usage view), reused in the panel and in Settings.

### Plan review

Cut from the first draft: a Bot-colour wash behind the transcript (decoration, fights the Tide bubble), the "New Channel" menu (no such concept), the mic (no voice), dollar cost in Usage (no price data; tokens only).

## 4. Screens

### 4.1 Left sidebar

| Part | Design |
|---|---|
| Header | Space for the macOS window controls; circular search and plus buttons (36 visual, 44 hit). Search expands in place to a field with Cancel |
| Tiles | The main Bot and up to two more pinned Bots as 72 px tiles in a row; the open one sits on `surface.raised`. Uses the `pinned` flag already in `bot.json` |
| Rows | Avatar 44 with presence dot (online, unread), name 600 (bold when unread), time right, one-line preview, unread dot. Sorted by latest activity. A working Bot shows "Working…" and its Face rocks |
| Hidden | One collapsed row, "Hidden 2", that expands in place |
| Actions | Right-click or long-press a row: Pin, Hide, Mark as read, Edit |
| Footer | A Vault row, then the user chip (initials, name, Bot count) which opens Settings |
| Empty | Face, "Meet your first Bot", New Bot. No-match search keeps its EmptyState |
| Phone | Top bar: user avatar (Settings), search, plus. Tiles become a horizontal strip. Rows below |

### 4.2 Chat messages

| Part | Design |
|---|---|
| Bubbles | Bot: `surface.bubble`. Mine: Tide with Night ink. Radius 20, the last bubble of a run has an 8 px tail corner, 2 px between bubbles in a run, 12 px between runs, max width 78% and 66 characters |
| Markdown | Bot messages render Markdown: paragraphs, lists, bold, inline code, links, quotes. Code blocks sit on a darker card with a Copy button. Uses Exact's `markdown` module (**verify** it is usable from a contract) |
| Mentions and links | `@Bot` becomes a small chip with the Bot's Face. A URL becomes a chip with the site's favicon and host, below the bubble |
| Day marks | Centred "Today 9:41 AM" or "Tuesday" at a day change or a gap over an hour |
| Time | Shown on hover or long-press, under the bubble, in `ink.faint` |
| Event lines | Centred, `ink.faint`: "Messaged [Face] Inbox", "Message from [Face] Scout", "Started [Face] Scout", "Created routine [clock] Morning briefing" |
| Attachments | `ImageCard`: radius 20, max 320 px, opens a preview. `FileCard`: type tile, name, "12 pages, 1.2 MB", download glyph |
| Requests | `RequestCard` with the Bot's Face and the question. Approvals get Allow and Deny buttons; questions get the options or a text field; a secret request points to the Vault. These use the existing respond route |
| Working | One quiet bubble with three dots and the Face working, replacing "Working…" text; text streams into the bubble as it arrives |
| Sending | The message appears at once (60% ink while sending). On failure: "Not sent" with a Retry button in the bubble, not an error line above the composer |
| Reactions | A chosen emoji is a chip overlapping the bubble corner; the add control appears beside the bubble on hover or long-press. Uses `/api/threads/{thread}/reactions` |
| Scrolling | Stays at the end when you are at the end. Otherwise a "New messages" pill and a scroll-to-end circle |
| Empty | Face 96, the Bot's name, "A little help, a new idea, or just a hello.", three starter chips above the composer |

### 4.3 Composer

A pill that grows to 20 px radius at several lines. Left: a `surface.raised` plus circle. Right: Tide send circle (red stop while working). Enter sends, Shift+Enter adds a line. Each Bot keeps its own draft. Attachments: picker (multiple), drag and drop, and paste for images; chips become thumbnails above the pill. The transcript fades under the composer.

### 4.4 Right panel (redesigned)

The panel is a small navigator: a root page with three tabs, and pushed pages with a back chevron. The header carries back (only on pushed pages), a gear (Edit Bot) and close.

```
Root                         Model page                   Usage page
+-----------------------+    +-----------------------+    +--------------------------+
| (<)            (*) (x)|    | <  Model              |    | <  Usage                 |
|        (Face 72)      |    | Provider              |    | [Today][7d][30d][All]    |
|          Ship         |    | [Claude*][Codex][Grok]|    | [ Summary | Detail ]     |
|         Working       |    | Model                 |    | 1.2M tokens   42 turns   |
| [Details][Library][PC]|    | (o) Haiku  Recommended|    | ▂▃▅▂▇▃▅  (one bar a day) |
|-----------------------|    | ( ) Sonnet            |    | Detail: by day, by model |
| Model                 |    | ( ) Opus              |    +--------------------------+
| [C] Haiku   Medium  > |    | Effort [Low|Med|High] |
| Usage  7 days       > |    | Applies to the next   |
| 1.2M tokens ▂▃▅▂▇▃▅   |    | message.              |
| Routines              |    | [x] Use for new Bots  |
| (o) Morning briefing  |    +-----------------------+
|     Every day 8:00 AM |
| (||) Weekly  Paused   |
| Working on            |
| Ship > Scout Working  |
+-----------------------+
```

| Page | Contents |
|---|---|
| Hero | Face 72, name, one status word (Working, Ready, Hidden, Getting set up) |
| **Details** | **Model row** (provider glyph, model, effort; opens the Model page). **Usage card** (period label, token figure, 7-bar sparkline; opens the Usage page). **Routines**: read-only list, clock glyph, title, schedule, "Paused" with a pause glyph, "Next: Tue 8:00 AM"; the footer says "Ask Ship in chat to add or change a routine." (**verify** a Bot can create one). **Working on**: the lineage as a small tree with a status word per row |
| **Model page** | Provider chips (glyph, display name; a disabled provider is greyed with "Not set up in T3 Code"). Models for that provider as radio rows (name, a "Recommended" tag on the project default Haiku). Effort segmented Low, Medium, High when the model offers it. A note "Applies to Ship's next message". A switch "Use for new Bots" writes the default. A toast confirms: "Ship now uses Haiku" |
| **Usage page** | Period chips: Today, 7 days, 30 days, All time. A Summary or Detail switch. Summary: three figures (messages, tokens, turns), a bar chart of tokens per day in one Tide series, one sentence ("84% of input came from cache"). Detail: the same chart, then By day (expandable: input, output, cache read, cache write), By model (provider, model, turns, tokens), and totals. A data table backs the chart for screen readers |
| **Library** | Read-only and flat. Three blocks: Pictures (3-column grid), Files (type tile, name, size, date), Links (favicon, host, title). Newest first, no filters, no groups, no add, no delete, no connectors, no records. One sentence on top ("12 things with Ship."). Tapping opens a picture preview, downloads a file or opens a link in a new tab. Empty: "Files, pictures and links shared with Ship show up here." |
| **Computer** | `ScreenCard`: the page as a thumbnail, caption "Ship's screen", the host name, Take control as a button on the card; Release when held. Empty: "This Bot has not opened a page." plus **Open browser** (silent `[Computer]` steer) |
| **Edit Bot page** (gear) | Name, colour dots (10), shape row (8), Pin to top, Main Bot, Hide from inbox, Delete Bot (confirm dialog, disabled for the main Bot). All the editing that used to fill the Details tab lives here |

Removed from the panel: Identity, Inbox and Delete cards, the segmented model picker, Routine add, run, switch and delete, Library filters, add, delete and Connectors, the standalone `/library` and `/computer` routes.

### 4.5 New Bot

A live Face (120) that updates as you choose; 10 colour dots; 8 shapes; Name; Get started (disabled until named); a row of Suggestions ("Night Shift", "Inbox Triage") that fill name, colour, shape and the first message. On wide windows it replaces the chat pane and the sidebar stays; on phone it is the existing modal.

### 4.6 Settings (redesigned)

On wide windows Settings replaces the chat and panel, with the sidebar still visible and the user chip selected. It has a 220 px section list on the left and a 44 rem column on the right. On phone it is a grouped list that pushes one page per section.

| Section | Contents |
|---|---|
| You | Profile card: initials avatar, name field, Bot count |
| Appearance | Three theme cards (System, Light, Dark), each a small preview of the app; applies at once |
| Models | Default provider, model and effort for new Bots (the same `ModelPicker` as the panel), then a list of every Bot with its model; tap a Bot to open its Model page |
| Usage | The same `UsageView` as the panel with a Bot filter (All Bots or one), period chips, Summary or Detail, and a per-Bot breakdown. Footer: "Counted on this machine. Nothing is sent anywhere." |
| Vault | The logins list (search, favicon rows, add) as a section page, replacing the lock tile; edit and add stay as sheets |
| Access | The password, then where Michel listens: this device, the local network, or Tailscale Serve. A password is required off this device. A listener change applies on restart. The Cloudflare tunnel stays in the CLI. |
| About | Michel version, the gateway address, the harness (T3 Code) and whether it is connected, Sign out |

The Models section lists this computer's providers, which is what this redesign ships. Choosing which computer a Bot runs on, and listing that computer's models, is a later design. See [the execution environment design](bot-execution-environment.md). Settings sets the default computer. The Bot details page and a durable child's sheet show the current computer and can change the setting. `server.getConfig` on the current T3 build cannot name a link, so a Bot set to a link does not reuse this computer's list.

### 4.7 Other

- Sign-in: Face 96 and the existing copy.
- Vault edit and add: same sheets, favicon in the header.
- Toast: new, bottom centre, 3 s, for confirmations ("Ship now uses Haiku", "Copied").

## 5. Components

### New

`Avatar` (wraps `Face`: size, shape, colour, state, presence), eight Face shapes, `PinnedTiles`, `BotRow`, `SearchButton`, `Markdown`, `CodeBlock`, `DayMark`, `EventLine`, `ImageCard`, `FileCard`, `LinkChip`, `MentionChip`, `ReactionBar`, `RequestCard`, `PendingBubble`, `NewMessagesPill`, `ScrollToEnd`, `Composer`, `AttachTray`, `PanelHeader`, `PanelHero`, `ModelRow`, `UsageCard`, `Sparkline`, `RoutineRow`, `LineageTree`, `ProviderChips`, `ModelRadioRow`, `EffortSegment`, `ModelPicker`, `UsageView` (`PeriodChips`, `ViewSwitch`, `BarChart`, `DetailTable`), `Shelf` (`MediaGrid`, `FileRow`, `LinkRow`), `ScreenCard`, `EditBotPage`, `LiveFace`, `ColourDots`, `ShapeRow`, `Suggestions`, `SettingsNav`, `ProfileCard`, `ThemeCards`, `AccessCard`, `AboutCard`, `Toast`. New glyphs: monitor, clock, pause, download, file, image, arrow-down, more, pin, copy, smile.

### Deleted

`BotEditor`, `LibraryScreen`, `ComputerScreen`, `LibList`, `LibraryContent`'s add, delete and connectors UI, `ProjectSheet`, the `board` resource, the routine write mutations (`routineSaved`, `routineChanged`), the library write mutations (`libSaved`, `libRemoved`, `connectorSaved`, `connectorRemoved`), `setProjects`, `Prefs.projects`, the `library` and `computer` routes, the segmented model picker, the sidebar welcome placeholder, the inset-grouped Settings list, the old "This machine" sentence (replaced by the Usage section).

## 6. Backend

Read: `bots.rs`, `chat.rs`, `thread_meta.rs`, `usage.rs`, `library.rs`, `routines.rs`, `computer.rs`, `provisioner.rs`, `approvals.rs`, the route table in `main.rs`. **Verify** marks something that depends on T3 data I haven't seen live.

### Already there

| Need | What exists |
|---|---|
| Bot shape, pinned | `bot.json` stores `shape` (default `"sphere"`) and `pinned`; create and save accept them. `app.ts` drops both |
| Row previews | `POST /api/thread-previews`, with "Messaged {Bot}: …" text; unused by the app |
| Reactions | `POST /api/threads/{thread}/reactions`, `GET /api/threads/{thread}/meta` |
| Message time | `chat::transcript` returns `at` |
| Attachments | Raw attachment objects in the transcript; `GET /api/bots/{slug}/attachments/{id}` |
| Answering requests | `POST /api/bots/{slug}/requests/{id}/respond` (`approvals.rs`) |
| Provider list on this machine | T3 `settings.json` `providerInstances` (driver, displayName, enabled) and `model-manifest.json` (`currentModels` per provider) |
| Access status | `GET /api/access` |
| Icons | `GET /api/icons/{host}` |

### Changes

| # | Change | Files | Notes |
|---|---|---|---|
| B1 | **Shape and pin validation.** Accept the eight shape names, map legacy `"sphere"` to `"ball"`, require `pinned` to be a boolean, cap pinned Bots at the tile count | `bots.rs` | Names come from `hbots_design::tokens().face.shapes` |
| B2 | **List carries row data.** `GET /api/bots` adds `preview {kind, role, text, at}`, `working`, `unread` per Bot | `main.rs`, `thread_meta.rs`, `t3.rs` | One shared, short-cached T3 shell snapshot (the A6 read) supplies the stamp for the previews cache and the working flag |
| B3 | **Unread.** `POST /api/bots/{slug}/seen` stores `lastSeenAt`; `unread` is a Bot message newer than it | `main.rs`, `bots.rs` | Hidden Bots never show unread |
| B4 | **Structured transcript.** Per message: `at`, `day` (first of a day or after an hour), `head`, `tail`, `files [{id, name, mime, bytes, kind, url}]`; plus `events [{id, at, kind, botSlug, text}]` merged by time | `chat.rs`, `main.rs` | Events: `t3_thread_send` ("Messaged {Bot}"); a `user_message` with `senderThreadId` ("Message from {Bot}", the "From X:" prefix stays as a fallback); a `user_message` with `scheduledTaskId` ("Routine ran: {title}"); a `subagent` item ("Started {name}"); a `dynamic_tool` whose last name segment is `schedule_task` (full name `mcp__t3-code__schedule_task`), completed, with no `scheduledTaskId` in its input ("Created routine {title}", id from `output.scheduledTaskId`), with one in its input ("Updated routine"); `delete_scheduled_task` ("Deleted routine"). `error` items with a `retry` are ignored. Grouping moves out of `app.ts` |
| B5 | **Requests carry their choices.** Transcript `requests` carry `requestId` (not the item id), `requestKind`, `prompt`, `reason`, `options [{decision, label, warning}]` for approvals, `questions [...]` for user input, and `label`, `reason`, `placeholder` for secrets, so `RequestCard` can render Allow and Deny, option buttons, a text field or a secret field | `chat.rs`, `approvals.rs` | Fixes a mismatch: the respond route needs `requestId` but the transcript sends the item id. Answer bodies: approval `{kind, decision}`; question `{kind, answers: {id: value}}`; secret `{kind, secret}` or `{kind, decision: "decline"}`. Item shapes are now known from T3's schema (section 6b) |
| B6 | **Providers.** New `GET /api/providers` returns `[{instanceId, driver, displayName, enabled, models: [{id, label, efforts, recommended}]}]` | new `providers.rs`, `main.rs` | Source: RPC `server.getConfig` `.providers` (confirmed in step 0). Drop `providerModelPreferences.hiddenModels` and pass only `auth.status`, `type`, `label`, never the email. `recommended` is the project default (Haiku) |
| B7 | **Model write is validated.** `GET /api/bots/{slug}/model` returns provider display name, `effort`, and the choices; `PUT` takes `{instanceId, model, effort}` and checks all three against B6 before writing to the Bot, the T3 project and the main thread | `main.rs`, `provisioner.rs` | Replaces the hard-coded medium effort and the character check alone. A disabled provider is refused with a message that names it |
| B8 | **Default model.** `PUT /api/settings` accepts `defaultModel` `{instanceId, model, effort}`, validated like B7; `GET` returns it resolved | `main.rs` | Used by the provisioner for new Bots (already reads `settings.defaultModel`) |
| B9 | **Usage with periods.** `GET /api/usage` and `GET /api/bots/{slug}/usage` take `period=today\|7d\|30d\|all` and return `totals {messages, attachments, turns, input, output, cached, cacheWrite}`, `series [{day, messages, input, output}]`, `byModel [{instanceId, model, turns, input, output}]`, `byBot [...]` for the machine call | `usage.rs`, `main.rs` | Tokens come from `providerTurns[]` (`completedAt`, `model`, `turnTokenUsage`), bucketed on demand and cached by thread stamp. No ledger (confirmed in step 0). Messages and attachments already have daily counts (400 days kept). Tokens only for now; `server.getUsageSummary` could add machine-wide cost later |
| B10 | **Routines are read-only labels.** `routines::task_row` adds `paused`, `next` (from `nextRunAt`), `lastStatus` and `lastError` (from `lastRunStatus`), `blocked`, and the full schedule phrase: "Every day at 8:00 AM", "Weekdays at 8:00 AM" (weekdays 1 to 5), "Weekends" (0 and 6), "Mon, Wed at 8:00 AM", "Once, Tue 8:00 AM", "Every 2 hours", "When a webhook arrives" | `routines.rs` | Schema confirmed (section 6b). The write routes stay for now (Bots and the CLI may use them) but the app no longer calls them. A failed last run shows a small "Last run failed" chip |
| B11 | **Library shelf.** New `GET /api/bots/{slug}/shelf` returns `{ok, counts, pictures [], files [], links []}`, newest first, capped at 60 each, from the existing file, attachment and link scans only (no records, no connectors) | `library.rs`, `main.rs` | The write routes and connectors stay in the gateway: Bot MCP config still uses them. The app has no UI for them for now |
| B12 | **Cheap Computer frame.** Add an `etag` to `computer_get` and honour `?since=`; add `host` for the caption | `computer.rs`, `main.rs` | The app polls every 1.5 s while the tab is open |
| B13 | **Suggestions.** `GET /api/suggestions` returns templates (name, colour, shape, first message) from a fixed list in `suggestions.rs` | new `suggestions.rs`, `main.rs` | Keeps copy out of the app build |
| B14 | **Create with a look.** `createBot` sends `shape` and an optional first message; the provisioner sends it once the main thread exists | `provisioner.rs`, `main.rs` | |
| B15 | **Settings data.** `GET /api/config` adds `version` and `harness {name, connected}`. The app reads `GET /api/access` for the status card | `main.rs` | |
| B17 | **Tell Bots what they can do.** Extend the managed block in `render_agents_md` with a Routines paragraph ("To run something on a schedule, use `schedule_task`; weekdays are 0 for Sunday to 6 for Saturday") and an Other Bots paragraph ("To message another Bot, use `t3_thread_send`"); `Bots::save` already rewrites the block | `bots.rs` | Makes the panel's empty-state hint true. Run the controlled test below first |
| B16 | **Projects routes.** Remove `GET/POST /api/projects*`, `projects_events` and `beta.projects` once the app stops using them | `main.rs` | Keep `projects.rs`: the Bots' MCP tools may use it. Decide separately |

### App data layer (`app/app.ts`, shapes in the contract)

- `Bot`: `shape`, `pinned`, `preview`, `previewAt`, `working`, `unread`.
- `Row`: `at`, `day`, `files: FileRef[]`, `kind: "event"` with `botSlug`; `reactions`.
- New loaders: `loadProviders`, `loadUsage(period, bot)`, `loadShelf`, `loadSuggestions`, `loadAccess`, `loadAbout`. New actions: `markSeen`, `reshapeBot`, `pinBot`, `setBotModel`, `setDefaultModel`, `respondRequest`, `react`.
- `loadComputer` passes the last `etag` and keeps the previous image on `unchanged`.
- Deleted with the UI: routine and library write actions, project actions.
- Drafts become one per Bot.

### Tests

- `bots.rs`: shape and pin validation, `sphere` migration, `lastSeenAt` rule.
- `chat.rs`: events, grouping, day flags, file kinds, request options, from fixtures.
- `providers.rs`: manifest and settings parsing, hidden models, disabled provider.
- `usage.rs`: period boundaries (today, 7d, 30d, all), by-model split, ledger delta.
- `library.rs`: shelf caps and order, no records leak in.
- `computer.rs`: etag and `since`.
- `tests/mock-t3.ts`: shell snapshot, a `t3_thread_send` item, a provider list, turns with times.
- `cargo fmt --all`, `cargo clippy --all-targets` (no warnings), `cargo test`, then `tests/e2e.sh`.

## 6b. Step 0 findings (measured on this machine, T3 0.1.9 nightly)

Sanitised copies are in `tests/fixtures/t3/` (no text, paths, emails or workspace ids).

| Question | Answer |
|---|---|
| Provider and model list (B6) | RPC `server.getConfig` returns `providers[]`: `instanceId`, `driver`, `displayName`, `enabled`, `installed`, `status` (`ready` or `disabled`), `auth {status, type, label}`, and `models[] {slug, name, aliases, isDefault, capabilities.optionDescriptors}`. 13 instances here; 7 are ready. The Claude instance lists 14 models. No settings-file fallback is needed |
| Effort (B6, B7) | Each model carries an `effort` descriptor: a select with options (Low, Medium, High, Extra High, Max, Ultrathink) and a per-model `isDefault`, plus optional booleans (`fastMode`, `ultracode`). The stored selection is `{instanceId, model, options: [{id: "effort", value}]}`. The effort chip shows only what the model offers |
| Privacy (B6) | `auth` also carries an email and workspace id. The gateway must pass only `status`, `type` and `label` on |
| Per-turn usage (B9) | `providerTurns[]` carry `startedAt`, `completedAt`, `model` and `turnTokenUsage` (input, output, cached, cache creation, reasoning). Bucketing by day and model works directly from the projection. **No token ledger is needed** |
| Machine-wide usage and cost | `server.getUsageSummary({sinceDay, untilDay, timeZone})` returns per-day, provider and model `buckets` with `costUsd`, scanned from every provider's session logs on the machine, not per Bot. It could power a Settings "All activity on this machine" view with cost. Per-Bot usage stays on the thread projection. Cost per Bot is possible later by pricing the per-turn tokens with T3's rates |
| Row data (B2) | `orchestration.subscribeShell` snapshot threads carry `updatedAt` (the stamp), `status`, `activeRunId` (working), `pendingRuntimeRequest`, `latestVisibleMessage`, `latestUserMessageAt` and `itemCount`. The working flag and unread need no extra reads |
| Events (B4) | Turn items are `user_message`, `assistant_message`, `reasoning`, `command_execution`, `dynamic_tool` (with `toolName`), `checkpoint`, plus `subagent` and requests. `t3_thread_send` is a `dynamic_tool`. Checkpoints, reasoning and commands are not shown as messages. The routine-creating tool name is still unseen (this Bot has not made one) |
| Attachments | `{type, id, name, mimeType, sizeBytes}` on user messages |
| Weekday schedules (B10) | Confirmed in T3's source: `schedule` is `interval {everyMs}`, `fixed_time {timeOfDay "HH:MM" local, weekdays? [0..6], 0 = Sunday, absent = every day}`, `once {runAt}` or `webhook`. The task row also has `nextRunAt`, `lastRunAt`, `lastRunStatus` (`never`, `running`, `succeeded`, `failed`), `lastRunError`, `runCount`, `createdBy`, `blockedByPolicy`. No task exists on this machine yet, so there is no live sample |
| Routine tool (B4) | A Bot creates a routine with T3's `schedule_task` tool: a `dynamic_tool` item whose last name segment is `schedule_task`; it is a create when its input has no `scheduledTaskId`, an update when it does. `list_scheduled_tasks`, `update_scheduled_task` and `delete_scheduled_task` exist too. A routine's run arrives as a `user_message` carrying `scheduledTaskId`. A message from another Bot is a `user_message` carrying `senderThreadId` (more reliable than parsing "From X:") |
| Request items (B5) | `approval_request {requestId, requestKind (command, file-read, file-change, mcp-elicitation, permission), prompt, reason, appName, options [{decision (accept, acceptForSession, acceptAlways, decline, cancel), label, warning}]}`. `user_input_request {requestId, questions [{id, header, question, options [{label, description, value, preview}], multiSelect, allowCustomAnswer, initialAnswer, required}]}`, answered with `answers {questionId: value}`. `secret_request {label, reason, url, placeholder, delivery "file", secretStatus (pending, saved, declined, cancelled)}`, answered with the item id. The respond route needs `requestId`, not the item id; today's transcript sends only the item id |
| Do Bots have these tools? | **Yes, proven live** (`tests/fixtures/t3/routine-created.json`). Asked once, `ada` found the tool through ToolSearch (it is deferred), then called `mcp__t3-code__schedule_task` with `{title, prompt, schedule: {type: fixed_time, timeOfDay: "08:00", weekdays: [1,2,3,4,5]}, enabled: false}`. The `dynamic_tool` item's `output` is `{scheduledTaskId}`; the task row reads `createdBy: agent`, `creationSource: mcp`, `lastRunStatus: never`, `nextRunAt: null` while disabled. The test routine was deleted afterwards (0 tasks left). A transient `error` item titled "Provider recovered" appeared mid-run (a retry); event lines must ignore `error` items that have a `retry` |

Also found: the one Bot here (`ada`) runs `claude-opus-5-5`, but `AGENTS.md` says Bots use Haiku. The Model page makes this visible; the provisioner default is Haiku and this Bot was changed from T3 directly.

## 7. Crate changes (`design/`)

1. `tokens.json`: add `surface.bubble`, `surface.raised`, `ink.faint`; `avatar.picker` to ten (`brown`, `red`, `orange`, `amber`, `green`, `teal`, `blue`, `violet`, `pink`, `gray`); add `face.shapes`; add `size.avatar_tile = 72`, `size.avatar_new = 120`.
2. `src/lib.rs`: emit shape paths into `face.contract`; contrast tests for the three new tokens.
3. Docs: `guidelines.md` (shapes, ten colours), `theme.md` (surfaces), `ux.md` (rewrite flows 3b, 3c, 3d, 4, 6, 7: panel pages, read-only Library and Routines, Models, Usage, Settings sections).
4. `sync`, `check`, `contrast`, `cargo test -p hbots-design`.

## 8. Build order

Each step ends with a web build and a light and dark screenshot of the changed screen at 340, 800 and 1280 px, plus the checks named.

| # | Step | Verify |
|---|---|---|
| 0 | **Done.** Real T3 thread projection, shell snapshot, provider config and usage summary captured as fixtures; findings in section 6b | `tests/fixtures/t3/`, section 6b |
| 1 | **Done.** Tokens (`surface.bubble`, `surface.raised`, `ink.faint`, ten-colour picker, tile and new-Bot sizes), `sync`, bubble fix | `cargo test -p hbots-design` (12 pass), web build, `scripts/test-sidebar.mjs` (4 pass) |
| 2 | Face shapes and `Avatar` | Gallery: 8 shapes x 4 states; reduced motion holds still |
| 3 | Gateway B1, B2, B3, B14, B15 and the matching `app.ts` fields | `cargo test`, `clippy`; `curl /api/bots` shows `preview`, `working`, `unread` |
| 4 | Left sidebar (4.1) | `bun scripts/test-sidebar.mjs` updated |
| 5 | Gateway B4, B5; messages (4.2); composer (4.3) | Unit tests from fixtures; Enter sends; per-Bot drafts; focus ring visible |
| 6 | Gateway B6, B7, B8; `ModelPicker`; Model page | Switch provider and model; the thread's next message uses it |
| 7 | Gateway B9; `UsageView`; Usage card and page | Period boundaries test; chart matches the table |
| 8 | Gateway B10, B11, B12; panel root, Routines, Shelf, ScreenCard, Edit Bot page (4.4) | Tab arrow keys work; drafts survive polling; Take and Release work |
| 9 | Gateway B13; New Bot (4.5) | Create a Bot from a Suggestion; its first message arrives |
| 10 | Settings (4.6), Vault section, sign-in, Toast | `tests/e2e.sh` |
| 11 | Delete dead code (section 5) and B16 routes and setting | `cargo test`; `tests/e2e.sh`; docs updated |

## 9. Risks and open decisions

- **Exact support** is not confirmed for SVG paths, `backdrop-filter`, `clip-path`, the `markdown` module, drag and drop, paste and context menus in contracts. Steps 2, 5 and 8 each start with a one-line probe and have a plain fallback.
- **Routine hint:** unproven risk closed. The panel's "Ask Ship in chat to add or change a routine" hint can ship; B17 still helps because the Bot currently spends a ToolSearch step finding `schedule_task`.
- **Connectors lose their UI** (B11): Bots keep working with them, but nobody can add or remove one from the app until a later pass. Decide if that is acceptable.
- **Routine and library write routes stay** in the gateway though the app stops using them. Remove them later, once nothing else calls them.
- **Shell snapshot cost (B2):** cache it for about a second and share it with `busy_threads`.
- **Blink:** the Face's blink still dims the eye (per-axis scale is not built); shapes inherit that.
- **`scripts/test-sidebar.mjs`** depends on current test ids; steps 4 and 8 rename some.
