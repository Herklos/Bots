//! The Bot's computer: T3's headless browser. Watching is read-only.
//! Take control is a separate action and is released when the person leaves it.

use serde_json::{json, Value};

mod machines;
mod messages;
mod place;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "the machines route reads the public view")]
pub(crate) use machines::machines_view;
#[allow(unused_imports, reason = "the place route reads the public view")]
pub(crate) use place::place_view;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use messages::*;

/// One held browser per Bot. Release drops it. A click does not open one.
/// `image` is the last JPEG from that interactive session, so a later watch
/// can show it without opening a second socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hold {
    pub tab: String,
    pub width: u32,
    pub height: u32,
    pub image: String,
}

#[derive(Default)]
pub struct Sessions {
    holds: std::collections::HashMap<String, Hold>,
}

impl Sessions {
    pub fn take(&mut self, slug: &str, tab: &str, width: u32, height: u32) {
        let previous = self
            .holds
            .get(slug)
            .filter(|hold| hold.tab == tab)
            .map(|hold| (hold.image.clone(), hold.width, hold.height));
        let (image, kept_w, kept_h) = previous.unwrap_or_default();
        self.holds.insert(
            slug.to_string(),
            Hold {
                tab: tab.to_string(),
                width: if width > 0 { width } else { kept_w },
                height: if height > 0 { height } else { kept_h },
                image,
            },
        );
    }

    /// The JPEG's pixel size, once a frame has arrived. A later take of the same
    /// page keeps this when the caller has no new size.
    pub fn remember_size(&mut self, slug: &str, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let Some(hold) = self.holds.get_mut(slug) {
            hold.width = width;
            hold.height = height;
        }
    }

    pub fn release(&mut self, slug: &str) {
        self.holds.remove(slug);
    }

    pub fn get(&self, slug: &str) -> Option<&Hold> {
        self.holds.get(slug)
    }

    /// The picture from the held session, when `tab` is the page being driven.
    /// `Some("")` means the page is held and no frame has arrived yet.
    pub fn picture(&self, slug: &str, tab: &str) -> Option<String> {
        let hold = self.holds.get(slug)?;
        if hold.tab != tab {
            return None;
        }
        Some(hold.image.clone())
    }

    pub fn note_image(&mut self, slug: &str, image: &str) {
        if image.is_empty() {
            return;
        }
        if let Some(hold) = self.holds.get_mut(slug) {
            hold.image = image.to_string();
        }
    }
}

pub fn sessions() -> &'static std::sync::Mutex<Sessions> {
    static SESSIONS: std::sync::OnceLock<std::sync::Mutex<Sessions>> = std::sync::OnceLock::new();
    SESSIONS.get_or_init(|| std::sync::Mutex::new(Sessions::default()))
}

#[cfg(test)]
mod etag_tests {
    use super::etag;

    #[test]
    fn the_same_frame_has_the_same_etag_and_another_frame_does_not() {
        assert_eq!(etag("AAAA"), etag("AAAA"));
        assert_ne!(etag("AAAA"), etag("AAAB"));
        assert!(!etag("").is_empty());
    }
}
