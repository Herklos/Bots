//! Starting points on New Bot: a name, a look and a first message. They live here, not in the
//! app, so the words can change without an app build.
use serde_json::{json, Value};

/// `(name, what it does, colour, shape, first message)`.
const STARTS: [(&str, &str, &str, &str, &str); 5] = [
    ("Night Shift", "Works overnight and preps your morning digest", "orange", "hex",
     "You are my night shift. While I sleep, go through what came in and have a short digest ready for the morning."),
    ("Inbox Triage", "Sorts your email and drafts replies in your voice", "pink", "cloud",
     "Help me keep my inbox at zero. Sort what came in, and draft replies for the ones that need one."),
    ("Research Desk", "Reads up on a topic and reports back with sources", "blue", "egg",
     "Be my research desk. Ask me what I want to understand, then read up and report back with sources."),
    ("Money Check", "Watches payments and what is overdue", "green", "square",
     "Keep an eye on my payments. Tell me what came in, what is overdue and what needs a follow up."),
    ("Trip Planner", "Plans a trip around your dates and budget", "violet", "drop",
     "Plan a trip with me. Ask for my dates and budget, then suggest a plan I can adjust."),
];

pub fn all() -> Vec<Value> {
    STARTS
        .iter()
        .map(|(name, about, color, shape, first)| {
            json!({ "name": name, "about": about, "color": color, "shape": shape, "firstMessage": first })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_start_has_a_real_colour_a_real_shape_and_a_first_message() {
        let t = hbots_design::tokens();
        for s in all() {
            assert!(t.avatar.hex(s["color"].as_str().unwrap()).is_some(), "{s}");
            assert!(crate::bots::shape_name(s["shape"].as_str().unwrap()).is_some(), "{s}");
            assert!(s["name"].as_str().unwrap().len() <= 24);
            assert!(s["firstMessage"].as_str().unwrap().len() <= 2000);
        }
        let names: Vec<String> = all().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "names repeat");
    }
}
