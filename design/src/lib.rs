//! The hbots brand in one place.
//!
//! `tokens.json` is the source. This crate reads it, checks the brand's own rules (contrast,
//! the palette) and writes every file that repeats a brand value: the app's design
//! (`app/app.tamber`), the marked regions of `app/app.ts`, `app/app.contract` and
//! `app/face.contract`, `app/app.json`, a stylesheet and the logo files. `hbots-design sync`
//! writes them; `hbots-design check` and this crate's tests fail when one is out of step.
//!
//! The guidelines are in `design/docs/`.
mod engine;

pub use engine::*;
