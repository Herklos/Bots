# Changelog

## 0.3.8

This release adds side conversations and prompt links, and improves helper policy enforcement and computer preview stability. It includes the changes described under 0.3.7, which has not yet been published.

### New features

- Start a side chat alongside a Bot's main thread with `/side` or `/btw`, then merge it back into the main thread.
- Open a link containing a Bot and prompt as a draft. The message is sent only when you choose to send it.

### Improvements

- Apply the parent's helper policy when Bots create durable subagents through MCP, including enabled state, limits and defaults.
- Keep the large computer preview visible while the next frame loads.
- Close the mention menu with Escape or after a completed mention, and use shared mention rules across conversations.
- Run the standard CI test matrix on Ubuntu; macOS release packaging remains in the release workflow.

## 0.3.7

This release improves group mentions and adds routine review, computer preferences and provider usage information. It also includes the independent mode and memory API described under 0.3.6.

### New features

- View and edit a Bot’s memory in its details panel, with a conflict message if the file changed since it was opened.
- Let Bots propose memory facts for a person to accept; accepted suggestions become Bot-owned facts.
- Choose Independent as the default approval mode in Settings. Independent mode runs with full harness access.
- Hold newly discovered Bot-created routines for approval. A routine scheduled just a few seconds ahead can still run once before the watcher disables it.
- Save a preferred computer for a Bot; this records the choice but does not yet place the Bot on that computer.
- Expose provider usage limits from the cached model list through the API.

### Improvements

- Apply a parent Bot’s helper policy when creating durable children, including enabled state, limits and model allowlists.
- Validate saved sidebar sections and preserve valid sections when other settings change.

### Bug fixes

- Ignore group mentions inside inline or fenced code while recognizing mentions after punctuation.

## 0.3.6

This release adds an independent Bot runtime mode and a revision-safe API for editing memory files.

### New features

- Store `independent` as a Bot or default runtime mode. Independent Bots receive additional instructions on judgment and when to ask; the harness runs this mode with full access.
- Read and save a Bot's `MEMORY.md` or the shared memory file through the memory API. Saves require the revision that was read, and Bot memory saves refresh its managed instructions.

### Improvements

- Add an app screenshot to the README.

## 0.3.5

This release adds diagnostics and image tools, strengthens memory ownership, and improves browser control.

### New features

- Run `hbots doctor` or `hbots doctor --json` for a redacted report on the installation, gateway, T3 connection, providers, storage and recent logs.
- Let Bots use `image_show` to copy supported local pictures into their library for a reply. Library images display inline at a stable size.
- Keep person-owned facts outside Bot folders and include them before Bot memory. Bot tools reject changes to those facts and restore a tampered user memory file.

### Improvements

- Update the Computer preview while control is held, without waiting for release.
- Require a separate confirmation before changing a Bot to a different model provider.
- Add groundwork for Bot-authored routine review and a default-off code gate; the routine review watcher and user controls are not yet implemented.

## 0.3.4

This release adds memory editing tools, safer Bot replies and more control over Bot instructions and approval modes.

### New features

- Edit, forget and search Bot memory through tools, with revision history and journals for recovery. Subagents write to their own memory store.
- Set a Bot's instructions when creating or editing it, and cap the approval mode used for Bot runs.
- Prepare the hbots schema in an Anytype space. Bot storage continues to use the file system; schema preparation does not enable an Anytype storage backend.
- Show eligible library images in Bot replies while removing other image references.

### Improvements

- Route chat, browser and other T3 operations through the harness boundary, and read Bot memory through the shared workspace.
- Split client screens and tests into smaller modules and document planned work separately from shipped features.

### Bug fixes

- Sandbox HTML pages served from the Bot library to protect the user's session.
- Save Bot instructions from the field's blur event and remove a duplicate creation-wizard binding.

## 0.3.2

This release corrects a group setting label and continues organizing the client and T3 integration for future work.

### Bug fixes

- Show the correct default answer policy label when configuring a group.

### Improvements

- Split group and Settings screens into focused client modules and move T3-specific approval, routine and provisioning calls into the T3 adapter without changing their intended behavior.
- Document plans for Anytype, memory, webhook triggers, reply images and client architecture. These plans do not indicate that the features have shipped.

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
