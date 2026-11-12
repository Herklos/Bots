use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Backend {
    Filesystem,
    Anytype,
}

impl Backend {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "filesystem" => Ok(Self::Filesystem),
            "anytype" => Ok(Self::Anytype),
            _ => Err("Storage must be filesystem or anytype.".into()),
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Anytype => "anytype",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stored {
    pub(crate) backend: Backend,
    /// `None` is the default folder, `data_root/bots`.
    pub(crate) root: Option<String>,
    pub(crate) host: String,
    pub(crate) space: String,
    pub(crate) key_name: String,
}

impl Stored {
    pub(crate) fn defaults() -> Self {
        Self {
            backend: Backend::Filesystem,
            root: None,
            host: DEFAULT_HOST.into(),
            space: DEFAULT_SPACE.into(),
            key_name: KEY_NAME.into(),
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "backend": self.backend.as_str(),
            "root": self.root,
            "anytype": {
                "host": self.host,
                "space": self.space,
                "keyName": self.key_name,
            }
        })
    }
}

pub(crate) struct Load {
    pub(crate) stored: Stored,
    /// Why a saved file was not applied. Startup still continues.
    pub(crate) error: Option<String>,
}

pub(crate) fn default_root(data_root: &Path) -> PathBuf {
    data_root.join("bots")
}

pub(crate) fn storage_path(data_root: &Path) -> PathBuf {
    data_root.join(FILE_NAME)
}

pub(crate) fn key_path(data_root: &Path) -> PathBuf {
    data_root.join(KEY_FILE)
}

pub(crate) fn read_load(data_root: &Path) -> Load {
    let path = storage_path(data_root);
    if !path.exists() {
        return Load {
            stored: Stored::defaults(),
            error: None,
        };
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            return Load {
                stored: Stored::defaults(),
                error: Some(
                    "The saved storage settings could not be read. hbots is using the file system."
                        .into(),
                ),
            };
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            return Load {
                stored: Stored::defaults(),
                error: Some(
                    "The saved storage settings could not be read. hbots is using the file system."
                        .into(),
                ),
            };
        }
    };
    match stored_from_value(&value) {
        Ok(stored) => explain_stored(data_root, stored),
        Err(error) => Load {
            stored: Stored::defaults(),
            error: Some(error),
        },
    }
}

pub(crate) fn explain_stored(data_root: &Path, mut stored: Stored) -> Load {
    let mut error = None;
    if let Err(message) = Backend::parse(stored.backend.as_str()) {
        error = Some(message);
        stored.backend = Backend::Filesystem;
    }
    if let Some(root) = stored.root.clone() {
        if let Err(message) = validate_folder(data_root, &root) {
            error = Some(message);
        }
    }
    if let Err(message) = normalize_host(&stored.host) {
        error = Some(message);
        stored.host = DEFAULT_HOST.into();
    } else if let Ok(host) = normalize_host(&stored.host) {
        stored.host = host;
    }
    if let Err(message) = normalize_space(&stored.space) {
        error = Some(message);
        stored.space = DEFAULT_SPACE.into();
    }
    Load { stored, error }
}

pub(crate) fn stored_from_value(value: &Value) -> Result<Stored, String> {
    let Some(object) = value.as_object() else {
        return Err(
            "The saved storage settings could not be read. hbots is using the file system.".into(),
        );
    };
    let backend = match object.get("backend") {
        None => Backend::Filesystem,
        Some(Value::String(text)) => Backend::parse(text)?,
        Some(_) => {
            return Err(
                "The saved storage settings could not be read. hbots is using the file system."
                    .into(),
            );
        }
    };
    let root = match object.get("root") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err("The saved folder is not a path. hbots is using the file system.".into());
        }
    };
    let anytype = object.get("anytype").cloned().unwrap_or_else(|| json!({}));
    let host = anytype["host"].as_str().unwrap_or(DEFAULT_HOST).to_string();
    let space = anytype["space"]
        .as_str()
        .unwrap_or(DEFAULT_SPACE)
        .to_string();
    let key_name = anytype["keyName"].as_str().unwrap_or(KEY_NAME).to_string();
    Ok(Stored {
        backend,
        root,
        host,
        space,
        key_name,
    })
}
