//! The agent runtime the gateway talks to. `adapters` holds one file per runtime (T3 Code today).

pub(crate) mod adapters;
pub(crate) mod providers; // The providers and models T3 Code offers.
pub(crate) use adapters::t3;
