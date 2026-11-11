# User experience

How hbots feels and how a person moves through it. The screens are in `app/app.contract`. On-screen wording lives in `app/strings/en.json` and `app/strings/fr.json`: the host locale picks the table (`fr` uses French, anything else uses English). The names hbots and T3 Code stay as written. The sentences below describe the flows; the tables are what the app shows.

## Principles

1. **A Bot is a person you talk to, not a chat log.** Each Bot has a name, a colour, a face and one main conversation. The inbox is a list of Bots, not of threads.
2. **Calm by default.** One primary action per screen. Status is a word, not a spinner, unless work is actually running.
3. **Say where things live.** The Bot is on this machine; the Vault is encrypted on this machine; the harness is T3 Code. Errors name the thing to check.
4. **Nothing secret on screen by default.** A password is hidden until the person presses Reveal.
5. **Works on a phone first.** Every screen is usable at 340 pt wide, with a 44 pt touch target.

## Navigation

```
Inbox ──┬── Bot chat ──┬── Library
        │              └── Computer
        ├── New Bot
        └── Settings ──┬── This machine
                       └── Vault ──┬── Add login
                                   └── Edit login
```

- The inbox is the root. On a phone it is a list; on a wide window it is a sidebar beside the chat.
- The inbox bar carries a search field that filters the Bots as you type and shows an empty state when nothing matches. The open Bot's row is highlighted in the list; the main Bot carries a small star.
- The inbox footer is the person's own chip (initials or a person glyph, their name from Settings, and the Bot count) and opens Settings. Vault is reached only from Settings, whose bar also carries a search field. The old Projects board is deferred and has no entry point. A Bot's own projects, notes, tasks, and plans live in that Bot's Library.
- On a wide window (1200 points and up) the open Bot's editor sits in a 320-point panel beside the chat and opens on its own. On a narrower screen, tapping the Bot's name in the chat header opens the same editor over the conversation, and tapping it again closes it. A hidden Bot opens into that editor. The panel has Details, Library, and Computer tabs. Switching tabs keeps the conversation open; the Library and Computer URLs also remain available as standalone screens.
- Back is always the platform's back.
- A Bot in the Hidden section stays reachable from the inbox's Hidden row.

## User flows

### 1. First run

1. The person opens the web app or the installed app. If a password is set, the sign-in screen appears first: the Face, the wordmark, "Your own Bots, in one place.", then the password.
2. Sign in with the password from Settings → Remote access. Five wrong tries lock sign-in for 15 minutes.
3. When the roster has loaded and it is empty, the inbox is one form: a 96-point Face, **Meet your first Bot**, "Give them a name. The look can change later.", the name, ten colours and eight shapes, **Create Bot**, then "Or pick a starting point." and the suggestion rows. Search and the plus menu are hidden. Settings stays. On a wide window that form is the detail column; the rail shows the wordmark and the person chip. A suggestion fills the name, colour, and shape, and the person can still edit the name. **Create Bot** opens that chat. If the gateway is down, the screen stays **Can’t Reach the Gateway** and **Try again**, with no name field.

### 2. Create a Bot

1. **New Bot**: the Face of the colour you picked sits above the name. The colour defaults to Tide; the picker offers six, the chosen one ringed in the accent.
2. **Create Bot** is disabled until the name has text.
3. The new Bot shows **Getting set up…** until its main conversation exists (the provisioner creates it), then **Ready**. In the inbox its row carries a star while it is the main Bot.

### 3. Talk to a Bot

1. Open the Bot. The centered header shows its Face and name, without a readiness subtitle. An empty conversation shows the Face, the name and three starter pills; the composer reads **Write a message…**.
2. Messages sit in bubbles: yours are Tide-filled with Night ink and a small tail corner on the last of a run; the Bot's are cards. Attached files show as a chip under the message.
3. Send. The reply appears as a **Working…** bubble with the Face while the Bot runs, then as a message. **Stop** (a red circle in the composer) interrupts the running reply.
4. If the Bot asks for something the app can't answer yet, the request is a card with an info glyph: "Answer this in T3 Code for now. It will show up here once the Bot can take your answer."

### 3b. Bot details

1. The chat header's Bot name is the toggle (no detail button, no details sheet). On a narrow screen, tapping the name opens the editor over the conversation below the header, and tapping the name again closes it. On a wide window (1200 points and up) the same editor sits beside the chat and opens on its own; tapping the name closes it and the chat widens. A hidden Bot opens its chat with the editor open, from the Hidden row.
2. The editor shows the large Face, the name, and a status chip (a dot and a word: Ready to chat, Working, Hidden from your inbox). The Details tab has labelled sections (**Identity**, **Inbox**, **Model**, **Routines**, **Working on**) and sit in grouped cards with icon tiles. Name and colour swatches are in Identity. Main Bot and Hide from inbox are rows in Inbox. Library and Computer are sibling tabs, not navigation rows. Delete Bot is in its own card, disabled for the main Bot.
3. **Model** is a segmented choice. Haiku is the default for a new Bot. The list is Haiku, plus this Bot's current choice and the default from Settings when those differ.
4. **Routines** start off. The person names one and picks Hourly, Daily at 09:00, or Once. Run, the switch, and Delete sit on the row. A Bot that has no conversation yet says "This Bot is still getting set up."
5. **Working on** lists one-off child threads that still need attention (Working, Waiting, Failed, or Stopped). Settled work stays out. A remote thread is labelled "One-off child thread, other machine". Opening a local child shows a strip under the header to return to the Bot’s main chat.
6. Tapping the name again closes the editor. The conversation is still there. Tabs, scroll positions, and form drafts stay mounted when the panel closes. Static model and routine choices load on entry and refresh after edits; regular polling updates live activity without remounting the panel.

### 3c. Library

1. The **Library** sidebar tab shows that Bot's shelf under its persistent Face and name, with one sentence: how many things are here, or "Files and pages shared with {name} appear here."
2. Filters are pills: All, Pages, Media, Links, Files, Projects, Notes, Tasks, Plans, and More (repos, machines, services). Rows group into Today, This week, and Older.
3. **Keep something** adds a note, task, plan, project, link, repo, machine, or service. Delete asks first: "It is removed from this Bot’s library. This can’t be undone."
4. A file opens through the gateway. A link from the conversation opens in a new tab. The app does not fetch that page itself.
5. **Connectors** are http(s) MCP servers for this Bot. The vault connector stays On and cannot be removed. A token is saved and is not shown again. The note under the list says "Applies when this Bot starts its next session."

### 3d. Computer

1. The **Computer** sidebar tab watches one page this Bot has open. The Face sits beside the page title. An empty watch says "This Bot has not opened a page."
2. Click the screen picture to open it large. **Take Control** is a second step and opens the large view for typing and clicking. Watching does not take the page. **Release** gives it back, and leaving the Computer tab releases it too, so the Bot can use the page.
3. While holding the page, clicks, the wheel, typed text, paste, and copy/cut work in the large view.

### 4. Save and use a login

1. **Settings** → **Vault** → **Add a Login**. A login needs a website or a title; **Save** is disabled until one is filled.
2. The Vault bar has a search field that filters the logins. Each row shows a lock tile, the title and the username. The password shows as ••••••••.
3. **Reveal** shows the password for that login only; it is not kept on screen after the person leaves.
4. A Bot can fill a login into a browser tab without reading the password.

### 5. Projects (deferred)

The old Projects board has no navigation entry or beta toggle. Per-Bot projects, notes, tasks, and plans are in that Bot's Library.

### 6. Remote access

1. Settings → Access sets the password. Save is disabled until a password is typed.
2. Anyone who opens hbots from another device signs in with that password.
3. Network, on the same page, chooses this device, the local network, or Tailscale Serve. Local network and Tailscale require the password. The local network address is HTTP. Tailscale Serve is HTTPS. A listener change applies when hbots restarts. The Restart button is there only when hbots is the installed service. `HBOTS_NETWORK_ACCESS` locks the control. The Cloudflare tunnel is not on this screen.

### 7. Settings

- Name, Appearance (System, Light, Dark), This machine, Vault, the password, and where hbots listens. Connected apps is postponed.
- **This machine** is one sentence: the counts since the file started, or "Nothing counted yet." The footer says "Counted on this machine. Nothing is sent anywhere." A failed load says so. It does not show zeros.
- Changing Appearance applies at once, with no Save.

## Empty and failure states

Each has a title, a message that says what to do, and a symbol.

| Screen | Title | Message |
|---|---|---|
| Inbox, no Bots | Meet your first Bot | Give them a name. The look can change later. |
| Vault, no logins | No Logins Yet | Save a login here. Its password stays in the Vault and shows only when you press Reveal. |
| Vault, gateway down | Can’t Open the Vault | Check that `hbots` is running, then try again. |
| Any screen, gateway down | Can’t Reach the Gateway | Start the `hbots` gateway on this machine or on your server, then try again. |
| This machine, nothing counted | (no title) | Nothing counted yet. |
| This machine, gateway down | (no title) | Can’t load usage. Check that hbots is running, then try again. |
| Library, empty | (the Bot's name) | Files and pages shared with {name} appear here. |
| Library, gateway down | (the Face) | Can’t open this library. Check that hbots is running, then try again. |
| Computer, no page | (the Bot's name) | This Bot has not opened a page. Open browser asks the Bot to open a page without a chat bubble. |
| Computer, gateway down | (the Face) | Can’t open this Bot’s browser. Check that hbots is running, then try again. |
| Working on, no other threads | (no title) | No other threads yet. |

Errors are words in red next to the field or action that failed (`-exact-system-red`), never a modal, unless the person must confirm something.

## Confirmations

Destructive actions name the thing and say whether it can be undone:

- **Delete Bot**: "Its folder moves to the trash. This can’t be undone from here."
- **Delete Login**: "It is removed from the Vault. This can’t be undone."
- **Delete** (project on the old board): "It is removed from the board. This can’t be undone."
- **Delete** (library item): "It is removed from this Bot’s library. This can’t be undone."

## Voice

- Plain and short. Sentence case for messages; Title Case for screen titles and buttons, as the platform does.
- Say what happened and what to do next. Name the thing ("the Vault", "the Bot", "T3 Code") rather than "it".
- Don't blame the person. "Check that `hbots` is running" rather than "You didn't start hbots".
- Use "Bot" with a capital B, and "hbots" (lower case) for the app and the gateway.
- No exclamation marks in errors.

## Motion

- Fast (120 ms) for a press; base (200 ms) for a sheet or a row; slow (320 ms) for a screen change.
- The Face breathes at rest, rocks while the Bot works, and blinks. It holds still when reduced motion is on (`prefer prefers-reduced-motion reduce`).


## Redesign pass (branch `redesign`)

- Inbox: search and New Bot are circles; the main and pinned Bots are tiles; rows show the latest message, a time and an unread dot; a Vault row sits above the user chip.
- Chat: Bot replies are Markdown on `surface.bubble`; day marks, event lines (Messaged, Message from, Routine ran, Created routine), file and image cards; requests are answered in the chat; a sent message shows at once, and "Not sent" offers Retry.
- Bot panel: Details (model row, usage card, routines to read, what the Bot is working on), Library (pictures, files and links, read only) and Computer (the page as a card). The gear opens Edit Bot. The model page picks provider, model and effort; the usage page shows today, 7 days, 30 days or all time, as a summary or detail.
- New Bot: a live Face, ten colours, eight shapes, and suggestions.
- Settings: profile, theme cards, the default model, usage by Bot and period, Vault, remote access and about.
