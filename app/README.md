# App

The Exact client. On-screen wording is in `strings/en.json` and `strings/fr.json`. The product name in that copy is hbots.

`app.contract` is the root. It holds `routes nav`, the screen `use` lines, and `component Bots`: a `members` list, one `provide` list, and the view that picks a screen from the route stack. It stays at or under 300 lines. `scripts/check-size.mjs` enforces that for every contract and TypeScript file under `app/`, and `scripts/size-baseline.json` is empty.

## Where a value lives

`Bots` owns resources, mutations, tasks, and the state that must survive leaving a route. Those declarations sit in `members` files (`members Bots`), not in the root file:

| File | What |
|---|---|
| `shell/held.contract` | Root state, resources, mutations, and the actions that send them |
| `shell/flags.contract` | The `pending()` and `failed()` derives |
| `shell/bag-chat.contract`, `bag-settings.contract`, `bag-rest.contract` | One record per screen |
| `<feature>/model.contract` | The rest of that feature's root members |

A child cannot call `pending()` or `failed()` on an injected value, and it cannot assign the root's state. Actions that send or assign stay on `Bots` and are injected by name.

Each screen injects one record (`chatBag`, `settingsBag`, `inboxBag`, …) and reads fields on it (`chatBag.draft`). Its `provide` list re-exports a field when a descendant still injects that single name (`draft = chatBag.draft`). A value that only one call site needs is a prop, as `LoginSheet` is.

Moving a state onto a screen would reset it when the route unmounts. Search text, the settings domain, and the other root state stay on `Bots` for that reason.
