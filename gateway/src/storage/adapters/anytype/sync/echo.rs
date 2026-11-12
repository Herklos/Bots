//! The sha this device just pushed, per path. A stream event with our device and that
//! sha is our own write coming back, and the feed ignores it.

use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(super) struct EchoBook {
    last: BTreeMap<String, String>,
}

impl EchoBook {
    pub(super) fn remember(&mut self, path: &str, sha: &str) {
        self.last.insert(path.to_string(), sha.to_string());
    }

    pub(super) fn matches(&self, path: &str, sha: &str) -> bool {
        self.last.get(path).is_some_and(|pushed| pushed == sha)
    }
}
