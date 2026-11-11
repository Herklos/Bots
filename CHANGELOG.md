# Changelog

## Unreleased

### New features

- Choose where hbots listens in Settings → Access → Network: this device, the local network, or Tailscale Serve. A password is required beyond this device, and a listener change applies on the next start. `HBOTS_NETWORK_ACCESS` overrides the setting and locks it.
- `@everyone` and `@all` in a group ask every member to answer for themselves. A member whose name is `everyone` or `all` is that member, and a Bot's own `@everyone` does not wake the group.
- Choose who answers in a group when nobody is named: the last speaker (`last`, the default), every member (`everyone`), or a chief (`chief`). Settings stores the default as `groupAnswerPolicy`. A group can override it with `answerPolicy`, and can name its `chief`. `@mention`, `@all`, and a single plain name still choose the recipients. Chief mode allows one fewer Bot-to-Bot handoff than there are members. Rooms saved before these fields keep today's last-speaker behavior.

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
