use super::*;

pub const TOKENS_JSON: &str = include_str!("../../tokens.json");

#[derive(Debug, Deserialize)]
pub struct Tokens {
    pub brand: Brand,
    pub color: Colors,
    pub avatar: Avatar,
    pub surface: BTreeMap<String, Pair>,
    pub space: BTreeMap<String, u32>,
    pub radius: BTreeMap<String, u32>,
    pub size: BTreeMap<String, u32>,
    pub motion: Motion,
    pub face: Face,
}

#[derive(Debug, Deserialize)]
pub struct Brand {
    pub name: String,
    pub tagline: String,
    pub promise: String,
    pub traits: Vec<String>,
}

/// A colour with a value for each scheme.
#[derive(Debug, Deserialize, Clone)]
pub struct Pair {
    pub light: String,
    pub dark: String,
}

#[derive(Debug, Deserialize)]
pub struct Colors {
    /// The brand colour: the Face, the icon, a brand fill.
    pub tide: String,
    /// The dark neutral: ink on Tide, the dark canvas.
    pub night: String,
    /// The light neutral: the light canvas.
    pub paper: String,
    /// An outline that reads on both canvases.
    pub mist: String,
    pub white: String,
    /// Tide made readable as text and as a control: the app's tint.
    pub accent: Pair,
    pub muted: Pair,
    /// Times and event lines: quieter than `muted`, still 4.5:1 on every surface it sits on.
    pub faint: Pair,
    pub danger: Pair,
    pub success: Pair,
    pub warning: Pair,
    pub info: Pair,
}

#[derive(Debug, Deserialize)]
pub struct Avatar {
    /// The colour name a new Bot gets.
    pub default: String,
    /// The names the app offers, in order.
    pub picker: Vec<String>,
    /// Every name a `bot.json` may carry, in the order they are listed.
    pub colors: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Motion {
    pub fast_ms: u32,
    pub base_ms: u32,
    pub slow_ms: u32,
    pub face: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct Face {
    pub radius: f64,
    pub eye: Eye,
    /// The shapes a Bot can take, in the order the picker lists them. Each has an outline in
    /// the 100-unit box, and where its eyes sit and how large they are.
    pub shapes: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Eye {
    pub gap: f64,
    pub width: f64,
    pub length: f64,
    pub y: f64,
    pub corner: f64,
}

pub(crate) static TOKENS: LazyLock<Tokens> =
    LazyLock::new(|| serde_json::from_str(TOKENS_JSON).expect("design/tokens.json is valid"));

pub fn tokens() -> &'static Tokens {
    &TOKENS
}

impl Face {
    /// The shape names in the listed order. The gateway accepts exactly these.
    pub fn shape_names(&self) -> Vec<&str> {
        self.shapes.keys().map(String::as_str).collect()
    }

    /// `(path, eye_dy, eye_k)` for a shape.
    pub fn shape(&self, name: &str) -> Option<(&str, f64, f64)> {
        let v = self.shapes.get(name)?;
        Some((
            v["path"].as_str()?,
            v["eye_dy"].as_f64()?,
            v["eye_k"].as_f64()?,
        ))
    }
}

impl Avatar {
    /// `(name, hex)` for every avatar colour, in the listed order.
    pub fn all(&self) -> Vec<(&str, &str)> {
        self.colors
            .iter()
            .filter_map(|(k, v)| Some((k.as_str(), v.as_str()?)))
            .collect()
    }

    pub fn hex(&self, name: &str) -> Option<&str> {
        self.colors.get(name).and_then(|v| v.as_str())
    }

    /// The hex of the colour a new Bot gets.
    pub fn default_hex(&self) -> &str {
        self.hex(&self.default)
            .expect("avatar.default names a colour")
    }
}
