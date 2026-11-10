# hbots design

The brand, the design guidelines, the user experience and the theme of hbots, in one crate. The crate is the source of truth: `tokens.json` holds every value, and the crate writes the files that repeat them.

| Path | What |
|---|---|
| `tokens.json` | Every brand value: name, colours, avatar palette, spacing, radius, size, motion, the Face's geometry. Edit values here only. |
| `src/lib.rs` | Reads the tokens, computes contrast, generates the files, and tests the brand's rules. |
| `src/main.rs` | The `hbots-design` command: `sync`, `check`, `contrast`. |
| `assets/` | `mark.svg` (the Face), `favicon.svg`, `app-icon.svg`, `lockup.svg`. Generated. |
| `generated/tokens.css` | Every token as CSS custom properties. Generated. |
| `scripts/render.cjs` | Renders the SVG files to PNG with Chromium (the app icon, and previews). |
| `docs/guidelines.md` | Brand, voice, colour, type, space, motion, the Face and avatars. |
| `docs/theme.md` | The colour system, light and dark, with the contrast it was checked against. |
| `docs/ux.md` | The user experience: principles, user flows, empty and error states. |

## Commands

```sh
cargo run -p hbots-design -- sync       # write every generated file from tokens.json
cargo run -p hbots-design -- check      # fail if any generated file is out of step
cargo run -p hbots-design -- contrast   # print the contrast of every colour pair
cargo test -p hbots-design              # the brand's rules, and the drift check
```

## What is generated, and where it lands

| Output | Consumer |
|---|---|
| `app/app.tamber` | Exact's design: the tint (`AccentColor`), the avatar colours as `brand.default` and `brand.<name>`, the inks `ink.light` and `ink.dark`, the spacing and radius scale |
| `app/app.ts` (between `design:begin palette`) | Maps a stored colour name, its hex or its token to a `brand.*` token |
| `app/app.contract` (between `design:begin colours`) | `pickerColours()` and `avatarInk()` |
| `app/face.contract` (between `design:begin colours`) | The Face's fill and edge functions |
| `app/app.json` | `theme_color`, `background_color` |
| `gateway/src/bots.rs` | The default Bot colour (`hbots_design::tokens().avatar.default`) |
| `gateway/src/main.rs` | The favicon fallback when a web build has no icon |
| `app/assets/icon-1024.png` | The app icon, rendered by `scripts/render.cjs` |

A file outside a marked region is generated whole. A region is the text between `// design:begin <name>` and `// design:end <name>`; keep those markers.

## Rules the crate enforces

- Each text colour reads at least 4.5:1 on its canvas (light on white, dark on Night); each control label at least 3:1.
- The ink on any avatar colour is Night or white, whichever reads better, and reads at least 4.3:1.
- Every palette name in `picker` is defined in `colors`, and every colour is lower-case `#rrggbb`.
- The brand colour is the default avatar colour.
- Generated files match `tokens.json` (`generated_files_are_in_step_with_the_tokens`), so CI catches a stale file.

To change the brand, edit `tokens.json`, run `sync`, then run the tests and the app build. Do not edit a generated file by hand.
