# Theme and colour

The colour system behind the app, in light and dark. Values come from `tokens.json`; the contrast figures are from `cargo run -p hbots-design -- contrast`.

## Core palette

| Token | Value | Use |
|---|---|---|
| Tide | `#1cc3b0` | The brand. The Face, the icon, the default Bot colour, a brand fill. Not for text: it is 2.2:1 on white. |
| Night | `#16141f` | Ink on Tide, the dark canvas. |
| Paper | `#f9f8fd` | The light canvas for the mark's eyes and light surfaces. |
| Mist | `#8f8da0` | An outline that reads on both canvases (3.1:1 on Paper). |
| White | `#ffffff` | Ink on dark fills; the light page. |

## Semantic colours

Each has a light and a dark value, chosen so text reads on its own canvas.

| Token | Light | Dark | Contrast (light on white / dark on Night) |
|---|---|---|---|
| accent (the app tint, `AccentColor`) | `#0a7a6e` | `#16a090` | 5.22:1 / 5.60:1 |
| muted (secondary text) | `#5d5a6e` | `#a8a5b8` | 6.66:1 / 7.57:1 |
| danger | `#d0112b` | `#ff5c6c` | 5.53:1 / 6.07:1 |
| success | `#007a45` | `#2fdc8f` | 5.43:1 / 10.19:1 |
| warning | `#8a5a00` | `#ffbf5c` | 5.93:1 / 11.14:1 |
| info | `#0a66d6` | `#5aa9ff` | 5.41:1 / 7.41:1 |

Ink on the tint in dark is 3:1 or better for a filled control's white label (the check the crate enforces for UIKit), and black on the dark tint reads above 4.5:1 for the web's label.

## Why the tint is darker than Tide

Tide is 2.2:1 on white, so the app can't use it for text, links or a selected state. The tint is Tide's hue taken down (light) and up (dark) until it reads as text. The brand fill keeps Tide, with Night ink on it (8.2:1).

## The avatar palette

Sixteen named colours, the same in the client and the gateway. Each is a `brand.<name>` token in `app/app.tamber` (Tide is `brand.default`); the app stores the plain name and `app/app.ts` maps it to the token. The picker offers six (`teal`, `blue`, `violet`, `pink`, `orange`, `green`). Initials use `ink.light` (white) or `ink.dark` (Night), chosen per colour by `avatarInk()` in `app/app.contract`. The crate checks each choice (minimum 4.3:1). Black on the avatar palette is white in dark mode, so a black Bot still shows.

## Light and dark

- The app follows the system scheme by default. Settings offers System, Light and Dark.
- The eyes of the Face take the canvas colour (`light-dark(Paper, Night)`), so a Face reads on both canvases without a separate asset.
- The mark's favicon uses Night eyes on Tide, which reads on light and dark browser tabs.

## Known warnings

The build reports two warnings that are deliberate:

- `tamber-unreachable`: the tint is set for the whole app. The platform's own controls on iOS (disabled prominent buttons), macOS (switches, spinners, selects) and the web and Linux (disabled prominent buttons, selects) keep their own look. The app does not style those controls differently yet.
- `lower-colour-one-scheme` on the avatar background: a Bot's colour is user data, so it cannot be a role. The brand's default is a fixed colour in both schemes, by design.
