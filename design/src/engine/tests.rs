use super::*;

fn at_least(what: &str, a: &str, b: &str, min: f64) {
    let got = contrast(a, b);
    assert!(
        got >= min,
        "{what}: {a} on {b} is {got:.2}:1, under {min}:1"
    );
}

#[test]
fn contrast_matches_known_values() {
    assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 0.001);
    assert!((contrast("#777777", "#ffffff") - 4.48).abs() < 0.01);
}

#[test]
fn text_colours_read_on_their_canvas() {
    let c = &tokens().color;
    // Light: on white (the app's page) and on Paper. Dark: on black and on Night.
    for (name, p) in [
        ("accent", &c.accent),
        ("muted", &c.muted),
        ("danger", &c.danger),
        ("success", &c.success),
        ("warning", &c.warning),
        ("info", &c.info),
    ] {
        at_least(name, &p.light, &c.white, 4.5);
        at_least(name, &p.light, &c.paper, 4.5);
        at_least(name, &p.dark, "#000000", 4.5);
        at_least(name, &p.dark, &c.night, 4.5);
    }
    at_least("ink on paper", &c.night, &c.paper, 7.0);
    at_least("mist as an outline", &c.mist, &c.paper, 3.0);
    at_least("mist as an outline", &c.mist, &c.night, 3.0);
}

#[test]
fn text_reads_on_authored_surfaces() {
    let t = tokens();
    for name in ["canvas", "card", "soft", "sidebar", "bubble", "raised"] {
        let surface = &t.surface[name];
        at_least(name, &t.color.night, &surface.light, 4.5);
        at_least(name, &t.color.white, &surface.dark, 4.5);
        at_least(name, &t.color.muted.light, &surface.light, 4.5);
        at_least(name, &t.color.muted.dark, &surface.dark, 4.5);
    }
    // Faint ink is for times and event lines: the page, a row, a selected row, a card.
    for name in ["canvas", "card", "sidebar", "raised"] {
        let surface = &t.surface[name];
        at_least(
            &format!("faint on {name}"),
            &t.color.faint.light,
            &surface.light,
            4.5,
        );
        at_least(
            &format!("faint on {name}"),
            &t.color.faint.dark,
            &surface.dark,
            4.5,
        );
    }
}

#[test]
fn a_bot_bubble_stands_apart_from_its_page() {
    let t = tokens();
    // The Bot's bubble was once a card on a near-white canvas and vanished in light mode.
    for (what, a, b) in [
        (
            "light bubble on canvas",
            &t.surface["bubble"].light,
            &t.surface["canvas"].light,
        ),
        (
            "dark bubble on canvas",
            &t.surface["bubble"].dark,
            &t.surface["canvas"].dark,
        ),
        (
            "light raised on sidebar",
            &t.surface["raised"].light,
            &t.surface["sidebar"].light,
        ),
        (
            "dark raised on sidebar",
            &t.surface["raised"].dark,
            &t.surface["sidebar"].dark,
        ),
    ] {
        at_least(what, a, b, 1.08);
    }
}

#[test]
fn the_brand_fill_takes_dark_ink() {
    let c = &tokens().color;
    at_least("ink on Tide", &c.night, &c.tide, 7.0);
    assert_eq!(ink_on(&c.tide), c.night);
    // White on Tide is what the brand must not do.
    assert!(contrast(&c.white, &c.tide) < 3.0);
}

#[test]
fn a_filled_accent_control_reads() {
    let c = &tokens().color;
    at_least("white on the light tint", &c.white, &c.accent.light, 4.5);
    // On dark, UIKit keeps a filled button's label white and the web picks black: both
    // must read. 3:1 is the ratio for a control's label.
    at_least("white on the dark tint", &c.white, &c.accent.dark, 3.0);
    at_least("black on the dark tint", "#000000", &c.accent.dark, 4.5);
}

#[test]
fn initials_read_on_every_avatar_colour() {
    for (name, hex) in tokens().avatar.all() {
        at_least(&format!("initials on {name}"), ink_on(hex), hex, 4.3);
    }
}

#[test]
fn the_palette_is_whole() {
    let a = &tokens().avatar;
    assert_eq!(a.all().len(), 16);
    assert!(a.hex(&a.default).is_some());
    for name in &a.picker {
        assert!(a.hex(name).is_some(), "{name} is offered but not defined");
    }
    for (name, hex) in a.all() {
        let ok = hex.len() == 7
            && hex.starts_with('#')
            && hex[1..]
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        assert!(ok, "{name}: {hex} is not lower-case #rrggbb");
    }
    // The brand colour is the default avatar.
    assert_eq!(a.default_hex(), tokens().color.tide);
}

#[test]
fn the_face_has_eight_shapes_with_whole_outlines() {
    let f = &tokens().face;
    assert_eq!(
        f.shape_names(),
        ["ball", "egg", "square", "pill", "triangle", "hex", "cloud", "drop"]
    );
    for name in f.shape_names() {
        let (path, dy, k) = f.shape(name).unwrap();
        assert!(
            path.starts_with('M') && path.ends_with('Z'),
            "{name}: an open outline"
        );
        assert!(
            path.chars().all(|c| c != '"' && c != '\n'),
            "{name}: path has a quote"
        );
        assert!(
            (0.0..=20.0).contains(&dy) && (0.8..=1.2).contains(&k),
            "{name}: eyes off the face"
        );
    }
}

#[test]
fn a_region_is_replaced_and_its_markers_kept() {
    let text = "a\n// design:begin x\nold\n// design:end x\nb\n";
    assert_eq!(
        replace_region(text, "x", "new\n").unwrap(),
        "a\n// design:begin x\nnew\n// design:end x\nb\n"
    );
    assert!(replace_region("nothing", "x", "new\n").is_err());
}

#[test]
fn app_json_keeps_its_other_keys() {
    let out =
        app_json("{ \"name\": \"n\", \"theme_color\": \"#000\", \"background_color\": \"#111\" }")
            .unwrap();
    assert_eq!(
        out,
        "{ \"name\": \"n\", \"theme_color\": \"#1cc3b0\", \"background_color\": \"#ffffff\" }"
    );
}

#[test]
fn the_embedded_favicon_is_the_generated_one() {
    assert_eq!(FAVICON_SVG, favicon_svg());
}

/// The check CI runs: nothing the tokens generate has drifted.
#[test]
fn generated_files_are_in_step_with_the_tokens() {
    let changed = sync(&repo_root(), false).unwrap();
    assert!(
        changed.is_empty(),
        "out of step with design/tokens.json: {changed:?}. Run `cargo run -p hbots-design -- sync`."
    );
}
