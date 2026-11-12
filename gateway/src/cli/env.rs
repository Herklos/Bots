use super::*;

// ---------- arguments ----------

/// Options as the Node gateway's parser reads them: `--name value`, `--name=value`, `--name`, and `--no-name`.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Flags(HashMap<String, String>);

impl Flags {
    /// The flag is given and not turned off.
    pub(crate) fn has(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|v| v != "false")
    }
    /// The flag was given as `--no-<name>`.
    pub(crate) fn disabled(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|v| v == "false")
    }
    /// The value of a flag that takes one (`--port 3780`).
    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .filter(|v| *v != "true" && *v != "false")
    }
}

/// Split the arguments after the command into options and the rest.
pub(crate) fn parse_args(argv: &[String]) -> (Flags, Vec<String>) {
    let mut flags = HashMap::new();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        if let Some(name) = arg.strip_prefix("--no-") {
            flags.insert(name.to_string(), "false".to_string());
        } else if let Some(body) = arg.strip_prefix("--") {
            if let Some((name, value)) = body.split_once('=') {
                flags.insert(name.to_string(), value.to_string());
            } else if matches!(body, "port" | "t3-url") {
                i += 1;
                flags.insert(
                    body.to_string(),
                    argv.get(i).cloned().unwrap_or_else(|| "true".into()),
                );
            } else {
                flags.insert(body.to_string(), "true".to_string());
            }
        } else {
            rest.push(arg.clone());
        }
        i += 1;
    }
    (Flags(flags), rest)
}

// ---------- settings ----------

/// The `KEY=value` lines of a `.env` file. Quotes around a value are removed.
pub(crate) fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim_start().split_once('=')?;
            let key = key.trim_end();
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return None;
            }
            let value = value.trim();
            let quoted = value.len() >= 2
                && (value.starts_with('"') && value.ends_with('"')
                    || value.starts_with('\'') && value.ends_with('\''));
            let value = if quoted {
                &value[1..value.len() - 1]
            } else {
                value
            };
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

pub(crate) fn is_key_line(line: &str, key: &str) -> bool {
    line.trim_start()
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// `.env` text with `KEY=value` set: the first such line is replaced, or the line is added. Other lines stay.
pub(crate) fn set_env_text(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key}={value}");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    match lines.iter().position(|l| is_key_line(l, key)) {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    lines.join("\n") + "\n"
}

/// HBOTS_HOME/.env, with the real environment winning, for the settings the CLI reads.
pub(crate) fn settings(home: &Path) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = std::fs::read_to_string(home.join(".env"))
        .map(|text| parse_env(&text).into_iter().collect())
        .unwrap_or_default();
    for key in ["PORT", "T3_URL", "T3_TOKEN", "T3_TOKEN_FILE", "HOST"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                map.insert(key.to_string(), value);
            }
        }
    }
    map
}

pub(crate) fn port_of(settings: &HashMap<String, String>) -> u16 {
    settings
        .get("PORT")
        .and_then(|p| p.parse().ok())
        .unwrap_or(3780)
}

pub(crate) fn service_spec(home: &Path) -> service::ServiceSpec {
    let exe = std::env::current_exe().unwrap_or_default();
    // An installed copy runs through the `current` link, so an update only swaps the link.
    let exe = if release::installed_runtime(&exe, home) {
        release::current_binary(home)
    } else {
        exe
    };
    service::spec(exe, home, &std::env::var("PATH").unwrap_or_default())
}

pub(crate) fn print_line(line: &str) {
    println!("{line}");
}
