use super::*;

/// Limit for a file T3 wrote for `request_secret`.
pub(crate) const MAX_SECRET_BYTES: u64 = 16 * 1024;

/// How long a token file lives (`vault_secret_file`).
pub(crate) const SECRET_FILE_TTL: Duration = Duration::from_secs(60 * 60);

pub(crate) const CONTACT_FIELDS: &[&str] = &[
    "label", "name", "email", "phone", "street", "apt", "city", "state", "zip", "country", "kind",
    "hints",
];

pub(crate) const SECRET_MISSING: &str = "That secret file doesn't exist. It may have expired (T3 deletes them after 24 hours); ask the user again with request_secret, delivery \"file\".";

/// The value of a file T3's `request_secret` wrote, without its trailing newline, and the file's
/// real path. Only a small, non-empty file directly inside `root` is accepted.
#[allow(
    clippy::map_err_ignore,
    reason = "an IO error includes the path of the secret file the Bot asked to read"
)]
pub(crate) fn read_secret(root: &Path, file: &str) -> Result<(String, PathBuf), String> {
    let real = std::fs::canonicalize(file).map_err(|_| SECRET_MISSING.to_string())?;
    let root = std::fs::canonicalize(root).map_err(|_| SECRET_MISSING.to_string())?;
    if real.parent() != Some(root.as_path()) {
        return Err("secretFile must be the file request_secret returned.".into());
    }
    let meta = std::fs::metadata(&real).map_err(|_| SECRET_MISSING.to_string())?;
    if !meta.is_file() || meta.len() > MAX_SECRET_BYTES {
        return Err("That isn't a secret file from request_secret.".into());
    }
    let text = std::fs::read_to_string(&real)
        .map_err(|_| "That isn't a secret file from request_secret.".to_string())?;
    let value = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text.as_str())
        .to_string();
    if value.is_empty() {
        return Err("That secret file is empty.".into());
    }
    Ok((value, real))
}

pub(crate) async fn save_login(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    bot_for(d.bots, args)?;
    let website = args["website"].as_str().unwrap_or("").trim().to_string();
    let domain = vault::site_domain(&website);
    if domain.is_empty() {
        return Err("website must be a site address like https://example.com".into());
    }
    let (secret, path) = read_secret(d.secrets_dir, args["secretFile"].as_str().unwrap_or(""))?;
    let username = args["username"].as_str().unwrap_or("").trim().to_string();
    let api_token = args["apiToken"] == true;
    let optional = |k: &str| {
        args[k]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let existing = d
        .vault
        .list()
        .await
        .map_err(|e| e.message)?
        .into_iter()
        .find(|i| {
            i["type"] == "login"
                && i["domain"] == domain.as_str()
                && i["username"].as_str().unwrap_or("") == username
        });

    let mut input = Map::new();
    input.insert("password".into(), json!(secret));
    input.insert("botCanRead".into(), json!(api_token));
    if let Some(title) = optional("title") {
        input.insert("title".into(), json!(title));
    }
    if let Some(hints) = optional("hints") {
        input.insert("hints".into(), json!(hints));
    }
    let (saved, updated) = match existing {
        Some(old) => {
            let id = old["id"].as_str().unwrap_or_default().to_string();
            (d.vault.update(&id, &Value::Object(input)).await, true)
        }
        None => {
            input.insert("type".into(), json!("login"));
            input.insert("website".into(), json!(website));
            input.insert("username".into(), json!(username));
            (d.vault.create(&Value::Object(input)).await, false)
        }
    };
    let item = saved.map_err(|e| e.message)?;
    let _ = std::fs::remove_file(&path);
    tracing::info!(
        "vault: {} login {} for {domain}{}",
        if updated { "updated" } else { "saved" },
        item["id"],
        if api_token { " (api token)" } else { "" }
    );
    Ok(json!({
        "saved": true,
        "updated": updated,
        "loginId": item["id"],
        "site": item["domain"],
        "title": item["title"],
        "apiToken": item["botCanRead"] == true,
    }))
}

pub(crate) async fn secret_file(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    bot_for(d.bots, args)?;
    let login = login_by_id(d, args).await?;
    if login["botCanRead"] != true {
        return Err("That's a website password, which Bots never read. Fill it into the sign-in page with vault_fill_password. If it's really an API token, ask the user to turn on \"Bots can read this\" for it in Settings › Vault.".into());
    }
    let token = login["password"]
        .as_str()
        .filter(|p| !p.is_empty())
        .ok_or("That login has no saved token.")?;
    prepare_token_dir(d.token_dir)?;
    let file = d.token_dir.join(hex::encode(rand::random::<[u8; 16]>()));
    #[allow(
        clippy::map_err_ignore,
        reason = "an IO error includes the path of the token file, next to the token itself"
    )]
    write_private(&file, token).map_err(|_| "Couldn't write the token file.".to_string())?;
    let path = file.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SECRET_FILE_TTL).await;
        let _ = tokio::fs::remove_file(path).await;
    });
    tracing::info!("vault: wrote token file for login {}", login["id"]);
    Ok(json!({
        "file": file.display().to_string(),
        "site": vault::site_domain(login_sites(&login).first().map(String::as_str).unwrap_or("")),
        "expiresInMinutes": SECRET_FILE_TTL.as_secs() / 60,
    }))
}

pub(crate) fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(file)?.write_all(text.as_bytes())
}

/// Creates the private folder and removes token files older than the TTL (left by an earlier run).
pub(crate) fn prepare_token_dir(dir: &Path) -> Result<(), String> {
    #[allow(
        clippy::map_err_ignore,
        reason = "an IO error includes the token-folder path"
    )]
    std::fs::create_dir_all(dir).map_err(|_| "Couldn't prepare the token folder.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    sweep(dir, SystemTime::now() - SECRET_FILE_TTL);
    Ok(())
}

/// Removes the files in `dir` last modified before `older_than`.
pub(crate) fn sweep(dir: &Path, older_than: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() && meta.modified().is_ok_and(|t| t < older_than) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

pub(crate) async fn save_contact(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    bot_for(d.bots, args)?;
    let mut input = Map::new();
    for f in CONTACT_FIELDS {
        if let Some(s) = args[*f].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            input.insert((*f).into(), json!(s));
        }
    }
    for f in ["emails", "phones"] {
        if let Some(list) = args[f].as_array() {
            input.insert(f.into(), json!(list));
        }
    }
    let has_something = ["name", "email", "phone", "street", "emails", "phones"]
        .iter()
        .any(|k| match input.get(*k) {
            Some(Value::Array(a)) => !a.is_empty(),
            Some(_) => true,
            None => false,
        });
    if !has_something {
        return Err("Give at least a name, email, phone or street.".into());
    }
    input.insert("type".into(), json!("contact"));
    let item = d
        .vault
        .create(&Value::Object(input))
        .await
        .map_err(|e| e.message)?;
    tracing::info!("vault: saved contact {}", item["id"]);
    Ok(json!({ "saved": true, "contactId": item["id"], "title": item["title"] }))
}
