use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessEnv {
    pub storage: Option<String>,
    pub root: Option<String>,
    pub anytype_url: Option<String>,
    pub anytype_space: Option<String>,
    /// `HBOTS_ANYTYPE_KEY` or `HBOTS_ANYTYPE_KEY_FILE` is set. The key itself is not kept.
    pub key_locked: bool,
    /// The environment supplies a non-empty key. The text is not kept.
    pub key_set: bool,
}

impl ProcessEnv {
    pub fn capture() -> Self {
        let key_var = non_empty_var("HBOTS_ANYTYPE_KEY");
        let key_file = non_empty_var("HBOTS_ANYTYPE_KEY_FILE");
        let key_set = key_var
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
            || key_file
                .as_deref()
                .is_some_and(|path| file_nonempty(Path::new(path)));
        Self {
            storage: non_empty_var("HBOTS_STORAGE"),
            root: non_empty_var("HBOTS_ROOT"),
            anytype_url: non_empty_var("HBOTS_ANYTYPE_URL"),
            anytype_space: non_empty_var("HBOTS_ANYTYPE_SPACE"),
            key_locked: key_var.is_some() || key_file.is_some(),
            key_set,
        }
    }
}

pub(crate) fn non_empty_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
