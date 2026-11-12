# Changelog

## 0.3.1

This release improves group setup, Bot instruction updates and browser control, and adds an Anytype connection flow.

### New features

- Pair with Anytype and check a named space from Settings before saving it as a storage choice. Bot storage still uses the file system; the Anytype storage adapter is not yet wired into the running gateway.
- Choose a group chief during creation and see answer-policy hints while configuring a group.

### Improvements

- Refresh managed instructions in open Claude threads when a Bot's `AGENTS.md` changes.
- Keep a browser preview connection open while controlling a Bot's computer, with a fallback if the connection drops.
- Clarify the Bot and group creation flows, and show file-system folder test and copy results in Settings.
- Organize gateway and client code into smaller domain modules, with size checks in CI.

### Bug fixes

- Restore the end-to-end harness for the current client and gateway.

## 0.3.0

This release makes creating and managing Bots and groups easier, adds storage controls, and improves conversation responsiveness.

### New features

- Create a Bot through a guided flow for its name, appearance, optional template and first message.
- Create a group through a guided flow for its members, answer policy and optional kickoff; add or remove members from existing groups while keeping at least two.
- Choose whether groups follow the machine default or use their own last-speaker, everyone or chief answer policy. Insert `@everyone` from the group composer to address every Bot.
- Configure the file-system Bot folder in Settings → Access → Storage and restart when the folder changes.

### Improvements

- Keep an unchanged conversation in place using conditional, held chat reads, and reduce writes when saving composer drafts.
- Highlight only the open conversation in the sidebar and replace the conversation route when switching on wide screens.
- Share Bot and group transcript components and split the Exact client into smaller Contract and host modules.

### Bug fixes

- Keep a sent message on one transcript row and retain the working indicator while its turn is in flight.
- Restore inbox loading by removing an unsupported timer from the room data source.
- Make options excluded by a locked network setting visibly unavailable.

## 0.2.2

This release keeps Bot instructions and memory available across threads, improves message controls, and adds groundwork for configurable Bot storage.

### New features

- Include a bounded snapshot of Bot memory, logs, shared notes and inherited parent notes in managed instructions; refresh it when those files change. Ephemeral threads are excluded.
- Record Bot storage preferences, including a folder under the hbots home. The running gateway continues to use the file system; Anytype can be saved as a choice but is currently unavailable.

### Improvements

- Include each Bot's prompt in its managed instructions, including updates to durable children when the parent changes.
- Show a floating message menu in Bot and group chats with reactions, reply, pin and copy controls.
- Reduce refresh work by sharing room reads and caching chat metadata while keeping unread notifications current.

### Bug fixes

- Keep the message menu overlay within the chat screen.
- Restore Contract validation on a clean CI checkout.

## 0.2.1

This release makes conversations easier to follow, improves the visibility of provider failures, and adds options when creating Bots and groups.

### New features

- Show completed Markdown pipe tables as grids in Bot and group conversations.
- Accept a goal, approval mode and deferred routines when creating a Bot, and an optional kickoff message when creating a group. A failed routine setup leaves the Bot in place.
- Show which Bots are working in a group.

### Improvements

- Show terminal provider failures in chat, including rate limits and reset times when available. Retries still in progress stay off the transcript.
- Give group chat the same jump-to-end and reply-following behavior as Bot chat.
- Poll the inbox and open chat independently, with a longer interval while hidden and polling paused during sends, answers and stops.
- Guide Bots to prefer bullet lists over Markdown tables.

## 0.2.0

This release gives you more control over network access, notifications and group conversations, and makes browser interaction and updates easier.

### New features

- Choose whether hbots listens on this device, the local network or Tailscale Serve in Settings → Access → Network. Access beyond this device requires a password; changes take effect on the next start. `HBOTS_NETWORK_ACCESS` overrides the setting.
- Receive notifications for unread Bot and group messages, manage notification settings and mute individual conversations without clearing their unread indicators.
- Check for an available release and update and restart a background service from Settings → About.
- Use `@everyone` or `@all` in groups to ask every member to answer. A Bot's own broadcast mention does not wake the whole group, and an individual Bot named `everyone` or `all` takes precedence.
- Configure the gateway's default group answer policy and per-group overrides: last speaker, everyone, or a designated chief. Explicit mentions continue to select recipients; existing groups retain last-speaker behavior by default.

### Improvements

- Control the Bot browser with keyboard codes and Shift, Alt and Ctrl, and request sharper frames in the enlarged view.
- Open a new Bot's chat after creation and show the creation form when the inbox is empty. Chat scrolls to a pending message after sending, and working indicators animate.
- Access the development watch stack through local, LAN, Tailscale and hostname addresses using its LAN proxy. Port conflicts now include a command to stop the listener.
- Add Contract validation to the development workflow, commit hook and CI, and document project-specific development and release workflows.

### Bug fixes

- Mark groups unread when a Bot replies after the group has been opened, and close a chat's notification when it is opened.
- Report browser-control authorization failures and allow the T3 token to be reissued when it cannot operate the browser.
- Keep authentication, Vault and T3 secrets out of debug output.
- Correct Contract syntax and preserve navigation state through the Exact router.

## 0.1.6

This release makes conversations easier to navigate on small screens and adds faster ways to mention Bots.

### New features

- Complete bot mentions in chat and group composers, including group members and `@all`, using the keyboard or touch.
- Access the development stack over a trusted local network with `bun scripts/dev.mjs --lan`.

### Improvements

- Simplified bot details and sidebar controls on small screens.
- Show localized activity chips for bot tools and routines, including the other Bot’s name where available.
- Documented the development workflow and proposed memory maintenance design.

## 0.1.5

- Fixed build-time rendering of the host clock.

## 0.1.4

- Made Exact packages available to the release web build.

## 0.1.3

- Corrected the Chrome path passed to release setup.

## 0.1.2

- Pinned Bun and installed Chrome for release builds.

## 0.1.1

- Fixed release packaging when Exact is checked out inside the Rust workspace.

## 0.1.0

Initial version of hbots.

- Create personal Bots with their own folders, instructions, memory and conversations, powered by T3 Code.
- Chat with Bots, stop runs, answer requests, share attachments and use reactions and voice records.
- Organize conversations with groups, subagents, pins and a shared library of files and links.
- Store logins in an encrypted Vault and let Bots use credentials through tools; import and export CSV files.
- Manage Projects and scheduled routines, including access through Bot tools.
- Customize models, appearance and user settings.
- Access hbots remotely with password protection, Tailscale or Cloudflare Tunnel and Access.
- Install checksummed releases, update or roll back the gateway, and run it as a background service.
- Use the shared client on the web and supported desktop and mobile platforms.
