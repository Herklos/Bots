# Brand guidelines

hbots is a small, private place where your personal Bots live. The brand should feel calm, plain-spoken and at home: a friendly face on your own machine, not a corporate assistant.

The values below live in `design/tokens.json`. Do not copy them into a screen; reference the token, or let the platform supply the colour.

## Name and tagline

- **Name:** hbots. Lower case in running text and the wordmark. "Bot" is a noun with a capital B.
- **Tagline:** Your Bots, at home.
- **Promise:** Personal AI Bots in a messaging app, running on your own machine.
- **Traits:** calm, plain-spoken, private, at home.

## The mark

The mark is the Face at rest: a Tide ball with two eyes. The eyes are cut-outs that take the canvas colour, so the mark works on light and dark without a second drawing.

- Use `design/assets/mark.svg` for the mark; `lockup.svg` for the mark with the wordmark beside it.
- The app icon (`app/assets/icon-1024.png`) is the mark on Night, square, for the platform to mask.
- Minimum size: 16 px for the favicon; 24 px for the mark beside text.
- Clear space: one eye-width all round.
- Don't recolour the mark outside the palette, rotate it, or put it on a busy image.

## The Face

The Bot's face is the mark brought to life. It has four states, drawn in `app/face.contract`:

| State | Look | Motion |
|---|---|---|
| Idle | Level eyes | Breathes slowly (3 s) |
| Working | Eyes lean apart, longer | Rocks (2.4 s) |
| Storm | Eyes lean together, tall | Shakes briefly (0.3 s) |
| Asleep | Closed-looking slits | Breathes slower (4 s) |

Eyes blink every 4.2 s, except asleep and reduced motion. Under reduced motion the Face holds still.

## Colour

Tide is the brand. It is not a text colour; use the accent (the tint) for text and controls, and Tide for fills and the mark. See `theme.md` for the full palette, light and dark, with contrast figures.

- Text is never lighter than 4.5:1 on its canvas.
- A filled control's label is Night on Tide.
- Avatar colours are the sixteen named colours; a Bot picks one of six in the picker. Initials are Night or white, whichever reads.

## Type

The app uses the platform's type styles, so text grows with the person's setting. The brand adds no typeface. Headings are the platform's bold title; secondary text is the footnote style in the secondary label colour.

## Space and shape

- Space scale: 4, 8, 12, 16, 24 px (`xs` to `xl`).
- Corner radius: 8 px (small), 14 px (cards and fields), 20 px (bubbles and the composer), full for circles.
- Touch targets are at least 44 pt.
- An avatar is a circle: 28 px in a bar, 44 px in a row, 72 px on a Bot's page.

## Motion

Three durations: 120 ms (a press), 200 ms (a sheet, a row), 320 ms (a screen). Motion carries meaning (a state change, a Bot working); it is never decoration. Reduced motion removes the Face's movement and keeps the rest.

## Writing

See `ux.md`, Voice. In short: plain, short, sentence case for messages, Title Case for screen titles and buttons, and always say the next step.

## Applying the brand

- A new screen uses the tokens and the platform's colour roles, never a hex value.
- A new avatar colour is added to `tokens.json` (`avatar.colors` and, if offered, `avatar.picker`), then `sync`.
- A change to a value is made in `tokens.json` only, then `sync`, then the tests.
