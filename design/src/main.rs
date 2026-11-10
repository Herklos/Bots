//! `hbots-design sync` writes every file the tokens generate. `hbots-design check` changes
//! nothing and fails when one is out of step. `hbots-design contrast` prints the brand's pairs.
use hbots_design::{contrast, ink_on, repo_root, sync, tokens};

fn main() {
    let command = std::env::args().nth(1).unwrap_or_default();
    let code = match command.as_str() {
        "sync" => run(true),
        "check" => run(false),
        "contrast" => {
            print_contrast();
            0
        }
        _ => {
            eprintln!("Usage: hbots-design sync | check | contrast");
            2
        }
    };
    std::process::exit(code);
}

fn run(write: bool) -> i32 {
    match sync(&repo_root(), write) {
        Err(e) => {
            eprintln!("{e}");
            1
        }
        Ok(changed) if changed.is_empty() => {
            println!("Everything is in step with design/tokens.json.");
            0
        }
        Ok(changed) => {
            for path in &changed {
                println!("{} {path}", if write { "wrote" } else { "out of step:" });
            }
            if write {
                0
            } else {
                eprintln!("Run `cargo run -p hbots-design -- sync`.");
                1
            }
        }
    }
}

fn print_contrast() {
    let t = tokens();
    let c = &t.color;
    let row =
        |what: &str, a: &str, b: &str| println!("{:>5.2}:1  {what} ({a} on {b})", contrast(a, b));
    row("Night on Tide", &c.night, &c.tide);
    row("Night on Paper", &c.night, &c.paper);
    row("Paper on Night", &c.paper, &c.night);
    for (name, p) in [
        ("accent", &c.accent),
        ("muted", &c.muted),
        ("faint", &c.faint),
        ("danger", &c.danger),
        ("success", &c.success),
        ("warning", &c.warning),
        ("info", &c.info),
    ] {
        row(&format!("{name}, light, on white"), &p.light, &c.white);
        row(&format!("{name}, dark, on Night"), &p.dark, &c.night);
    }
    for (name, hex) in t.avatar.all() {
        row(&format!("initials on {name}"), ink_on(hex), hex);
    }
}
