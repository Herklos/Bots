use super::*;

/// The avatar palette as `app/app.ts` maps it: a stored name, its hex, or a token, to a token.
pub fn ts_region() -> String {
    let t = tokens();
    let mut s = String::from("const BRAND: Record<string, string> = {\n");
    for (name, hex) in t.avatar.all() {
        let token = brand_token(name);
        let _ = writeln!(
            s,
            "  {name}: '{token}', '{hex}': '{token}', '{token}': '{token}',"
        );
    }
    s.push_str("};\n");
    let picker: Vec<String> = t
        .avatar
        .picker
        .iter()
        .map(|n| format!("'{}'", brand_token(n)))
        .collect();
    let _ = writeln!(s, "export const SWATCHES = [{}];", picker.join(", "));
    let _ = writeln!(
        s,
        "const DEFAULT_COLOUR = '{}';",
        brand_token(&t.avatar.default)
    );
    s.push_str(
        "const fill = (c: unknown): string => (typeof c === 'string' ? (BRAND[c] ?? (/^#[0-9a-f]{6}$/i.test(c) ? c : DEFAULT_COLOUR)) : DEFAULT_COLOUR);\n",
    );
    s
}

/// The colour helpers of `app/app.contract`, over the design's tokens.
pub fn contract_region() -> String {
    let t = tokens();
    let a = &t.avatar;
    let mut s = String::new();
    let picker: Vec<String> = a
        .picker
        .iter()
        .map(|n| format!("\"{}\"", brand_token(n)))
        .collect();
    let _ = writeln!(s, "fn pickerColours(): string[] = [{}]", picker.join(", "));
    let inks: Vec<String> = a
        .all()
        .iter()
        .filter(|(_, hex)| ink_token(hex) == "ink.light")
        .map(|(name, _)| format!("c == \"{}\"", brand_token(name)))
        .collect();
    let _ = writeln!(
        s,
        "fn avatarInk(c: string): string = {} ? \"ink.light\" : \"ink.dark\"",
        inks.join(" || ")
    );
    s
}

/// The Face's colours in `app/face.contract`. That file builds without the app's design, so
/// it holds values, not token names.
pub fn face_region() -> String {
    let t = tokens();
    let c = &t.color;
    let arms: String = t
        .avatar
        .all()
        .iter()
        .filter(|(name, _)| *name != t.avatar.default)
        .map(|(name, hex)| {
            let value = if *name == "black" {
                "light-dark(#000000, #ffffff)".to_string()
            } else {
                hex.to_string()
            };
            format!("color == \"{name}\" ? \"{value}\" : ")
        })
        .collect();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "fn faceHex(color: string): string = color.startsWith(\"#\") ? color : {arms}\"{}\"",
        t.avatar.default_hex()
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "// A fill that would vanish into the page gets an outline. Only white"
    );
    let _ = writeln!(
        s,
        "// does: it sits close to the light canvas, so it takes a grey edge there. Black is white in dark mode."
    );
    let _ = writeln!(
        s,
        "fn faceEdge(color: string): string = color == \"white\" ? \"light-dark({}, {})\" : \"none\"",
        c.mist,
        a_white(t)
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "// The eyes are cut-outs: they take the canvas colour of the scheme."
    );
    let _ = writeln!(
        s,
        "fn faceEye(): string = \"light-dark({}, {})\"",
        c.paper, c.night
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "// A presence dot: online is the success colour, unread the info colour."
    );
    let _ = writeln!(
        s,
        "fn presenceColour(kind: string): string = kind == \"unread\" ? \"light-dark({}, {})\" : \"light-dark({}, {})\"",
        c.info.light, c.info.dark, c.success.light, c.success.dark
    );
    s
}

/// The Face's shapes in `app/face.contract`: each outline, and where its eyes sit.
pub fn shapes_region() -> String {
    let f = &tokens().face;
    let names = f.shape_names();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "fn faceShape(shape: string): string = {}",
        names
            .iter()
            .skip(1)
            .map(|n| format!("shape == \"{n}\" ? \"{n}\" : "))
            .collect::<String>()
            + &format!("\"{}\"", names[0])
    );
    let arms = |pick: &dyn Fn(&str, f64, f64) -> String, default: &str| -> String {
        let mut out = String::new();
        for n in names.iter().skip(1) {
            let (path, dy, k) = f.shape(n).expect("a listed shape has geometry");
            let _ = write!(out, "shape == \"{n}\" ? {} : ", pick(path, dy, k));
        }
        out + default
    };
    let (path0, dy0, k0) = f.shape(names[0]).expect("a first shape");
    let _ = writeln!(
        s,
        "fn shapePath(shape: string): string = {}",
        arms(&|p, _, _| format!("\"{p}\""), &format!("\"{path0}\""))
    );
    let _ = writeln!(
        s,
        "fn shapeDy(shape: string): number = {}",
        arms(&|_, d, _| num(d), &num(dy0))
    );
    let _ = writeln!(
        s,
        "fn shapeK(shape: string): number = {}",
        arms(&|_, _, k| num(k), &num(k0))
    );
    let list = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(s, "fn shapeNames(): string[] = [{list}]");
    s
}

pub(crate) fn a_white(t: &Tokens) -> &str {
    t.avatar.hex("white").expect("the palette has white")
}

/// Every token as CSS custom properties, for a page outside the app.
pub fn css() -> String {
    let t = tokens();
    let c = &t.color;
    let mut s = format!("/* {NOTE} Do not edit. */\n:root {{\n  color-scheme: light dark;\n");
    for (k, v) in [
        ("tide", &c.tide),
        ("night", &c.night),
        ("paper", &c.paper),
        ("mist", &c.mist),
    ] {
        let _ = writeln!(s, "  --hbots-{k}: {v};");
    }
    let _ = writeln!(s, "  --hbots-canvas: light-dark({}, {});", c.paper, c.night);
    let _ = writeln!(s, "  --hbots-ink: light-dark({}, {});", c.night, c.paper);
    for (k, p) in [
        ("accent", &c.accent),
        ("muted", &c.muted),
        ("faint", &c.faint),
        ("danger", &c.danger),
        ("success", &c.success),
        ("warning", &c.warning),
        ("info", &c.info),
    ] {
        let _ = writeln!(s, "  --hbots-{k}: light-dark({}, {});", p.light, p.dark);
    }
    for (name, pair) in &t.surface {
        let _ = writeln!(
            s,
            "  --hbots-surface-{name}: light-dark({}, {});",
            pair.light, pair.dark
        );
    }
    for (name, hex) in t.avatar.all() {
        let _ = writeln!(s, "  --hbots-avatar-{name}: {hex};");
    }
    for (k, v) in &t.space {
        let _ = writeln!(s, "  --hbots-space-{k}: {v}px;");
    }
    for (k, v) in &t.radius {
        let _ = writeln!(s, "  --hbots-radius-{k}: {v}px;");
    }
    let m = &t.motion;
    let _ = writeln!(s, "  --hbots-motion-fast: {}ms;", m.fast_ms);
    let _ = writeln!(s, "  --hbots-motion-base: {}ms;", m.base_ms);
    let _ = writeln!(s, "  --hbots-motion-slow: {}ms;", m.slow_ms);
    s.push_str("  --hbots-font: system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif;\n");
    s.push_str("  --hbots-font-mono: ui-monospace, \"SF Mono\", Menlo, Consolas, monospace;\n}\n");
    s
}

/// The Face at rest, as SVG shapes in a 100-unit box at `(dx, dy)`, scaled by `k`.
pub(crate) fn face_shapes(dx: f64, dy: f64, k: f64, eye: &str) -> String {
    let t = tokens();
    let f = &t.face;
    let mut s = format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>",
        num(dx + 50.0 * k),
        num(dy + 50.0 * k),
        num(f.radius * k),
        t.color.tide
    );
    for side in [-1.0, 1.0] {
        let _ = write!(
            s,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" {eye}/>",
            num(dx + (50.0 + side * f.eye.gap - f.eye.width / 2.0) * k),
            num(dy + (f.eye.y - f.eye.length / 2.0) * k),
            num(f.eye.width * k),
            num(f.eye.length * k),
            num(f.eye.corner * k),
        );
    }
    s
}

/// A style that gives `class` the canvas colour of the viewer's scheme.
pub(crate) fn scheme_style(class: &str, light: &str, dark: &str) -> String {
    format!("<style>.{class}{{fill:{light}}}@media (prefers-color-scheme: dark){{.{class}{{fill:{dark}}}}}</style>")
}
