# User experience

How hbots feels and how a person moves through it. The screens are in `app/app.contract`; the wording below is what the app says today.

## Principles

1. **A Bot is a person you talk to, not a chat log.** Each Bot has a name, a colour, a face and one main conversation. The inbox is a list of Bots, not of threads.
2. **Calm by default.** One primary action per screen. Status is a word, not a spinner, unless work is actually running.
3. **Say where things live.** The Bot is on this machine; the Vault is encrypted on this machine; the harness is T3 Code. Errors name the thing to check.
4. **Nothing secret on screen by default.** A password is hidden until the person presses Reveal.
5. **Works on a phone first.** Every screen is usable at 340 pt wide, with a 44 pt touch target.

## Navigation

```
Inbox ──┬── Bot chat ──── Bot details
        ├── New Bot
        ├── Vault ──┬── Add login
        │           └── Edit login
        ├── Projects (beta) ──┬── Add project
        │                     └── Edit project
        └── Settings
```

- The inbox is the root. On a phone it is a list; on a wide window it is a sidebar beside the chat.
- Back is always the platform's back. Settings and Vault are reached from the inbox, not from inside a chat.
- A Bot in the Hidden section stays reachable from the inbox's Hidden row.

## User flows

### 1. First run

1. The person opens the web app or the installed app. If a password is set, the sign-in screen appears first: the Face, the wordmark, "Your Bots, at home.", then the password.
2. Sign in with the password from Settings → Remote access. Five wrong tries lock sign-in for 15 minutes.
3. The inbox shows **No Bots Yet** with "A Bot is a long-lived T3 Code thread with a face and a memory." and **Create Your First Bot**.

### 2. Create a Bot

1. **New Bot**: the Face of the colour you picked sits above the name. The colour defaults to Tide; the picker offers six.
2. **Create Bot** is disabled until the name has text.
3. The new Bot shows **Getting set up…** until its main conversation exists (the provisioner creates it), then **Ready**.

### 3. Talk to a Bot

1. Open the Bot. Its conversation is loaded. An empty one shows the Face and "One conversation. Say the first thing." The composer reads **Message**.
2. Send. The reply appears as **Working…** while the Bot runs, then as a message.
3. If the Bot asks for something the app can't answer yet, the request is shown as a card: "Answer this in T3 Code for now. It will show up here once the Bot can take your answer."
4. **Stop** interrupts the running reply.

### 4. Save and use a login

1. **Vault** → **Add a Login**. A login needs a website or a title; **Save** is disabled until one is filled.
2. The list shows the title and username. The password shows as ••••••••.
3. **Reveal** shows the password for that login only; it is not kept on screen after the person leaves.
4. A Bot can fill a login into a browser tab without reading the password.

### 5. Projects (beta)

1. Settings → Projects beta turns the board on. Off, the Projects screen says **Projects Are Off** and points to Settings.
2. **Add a Project** needs a title. Kind, status, area and notes are optional.
3. The board is shared with the Bots, which can read and write it.

### 6. Remote access

1. Settings → Remote access sets the password. Save is disabled until a password is typed.
2. Anyone who opens hbots from another device signs in with that password.
3. Tailscale and Cloudflare routes are set up from the same screen.

### 7. Settings

- Name, Appearance (System, Light, Dark), Projects beta, Remote access.
- Changing Appearance applies at once, with no Save.

## Empty and failure states

Each has a title, a message that says what to do, and a symbol.

| Screen | Title | Message |
|---|---|---|
| Inbox, no Bots | No Bots Yet | A Bot is a long-lived T3 Code thread with a face and a memory. |
| Vault, no logins | No Logins Yet | Save a login here. Its password stays in the Vault and shows only when you press Reveal. |
| Vault, gateway down | Can’t Open the Vault | Check that `hbots` is running, then try again. |
| Any screen, gateway down | Can’t Reach the Gateway | Start the `hbots` gateway on this machine or on your server, then try again. |
| Projects, beta off | Projects Are Off | Turn on the Projects beta in Settings, then try again. |
| Projects, empty | No Projects Yet | Track projects, tasks, machines, services, repos, notes and links on one board. |

Errors are words in red next to the field or action that failed (`-exact-system-red`), never a modal, unless the person must confirm something.

## Confirmations

Destructive actions name the thing and say whether it can be undone:

- **Delete Bot**: "Its folder moves to the trash. This can’t be undone from here."
- **Delete Login**: "It is removed from the Vault. This can’t be undone."
- **Delete** (project): "It is removed from the board. This can’t be undone."

## Voice

- Plain and short. Sentence case for messages; Title Case for screen titles and buttons, as the platform does.
- Say what happened and what to do next. Name the thing ("the Vault", "the Bot", "T3 Code") rather than "it".
- Don't blame the person. "Check that `hbots` is running" rather than "You didn't start hbots".
- Use "Bot" with a capital B, and "hbots" (lower case) for the app and the gateway.
- No exclamation marks in errors.

## Motion

- Fast (120 ms) for a press; base (200 ms) for a sheet or a row; slow (320 ms) for a screen change.
- The Face breathes at rest, rocks while the Bot works, and blinks. It holds still when reduced motion is on (`prefer prefers-reduced-motion reduce`).
