//! Line three-way merge for text files. Whole files never reach this.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Merged {
    Clean(String),
    Conflict,
}

pub(super) fn merge_text(base: &str, local: &str, remote: &str) -> Merged {
    if local == remote {
        return Merged::Clean(local.to_string());
    }
    let crlf = uses_crlf(base) && uses_crlf(local) && uses_crlf(remote);
    let (base, local, remote) = if crlf {
        (to_lf(base), to_lf(local), to_lf(remote))
    } else {
        (base.to_string(), local.to_string(), remote.to_string())
    };
    let merged = match diffy::merge(&base, &local, &remote) {
        Ok(text) => text,
        Err(_) => return Merged::Conflict,
    };
    if crlf {
        Merged::Clean(to_crlf(&merged))
    } else {
        Merged::Clean(merged)
    }
}

fn uses_crlf(text: &str) -> bool {
    text.contains("\r\n") && !text.replace("\r\n", "").contains('\n')
}

fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn to_crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}
