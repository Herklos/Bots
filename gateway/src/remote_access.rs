//! The remote password, stored as scrypt in `access.json`.
//!
//! The file stores base64 salt and hash, `n` / `r` / `p` / `keyLen`.
//! `exposure` is `off`, `landing` or `full`.
use crate::exposure::Exposure;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use scrypt::{scrypt, Params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use subtle::ConstantTimeEq;

#[derive(Clone, Serialize, Deserialize)]
pub struct PasswordHash {
    pub kdf: String,
    pub salt: String,
    pub n: u32,
    pub r: u32,
    pub p: u32,
    #[serde(rename = "keyLen")]
    pub key_len: usize,
    pub hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccessFile {
    pub format: String,
    pub version: u32,
    #[serde(default = "off")]
    pub exposure: String,
    #[serde(default)]
    pub password: Option<PasswordHash>,
}

fn off() -> String {
    "off".into()
}

impl Default for AccessFile {
    fn default() -> Self {
        Self {
            format: "hbots-access".into(),
            version: 1,
            exposure: "off".into(),
            password: None,
        }
    }
}

struct Saved {
    file: AccessFile,
    /// A file we could not parse is left on disk and refused, not overwritten.
    broken: bool,
}

pub struct RemoteAccess {
    path: PathBuf,
    inner: Mutex<Saved>,
}

impl RemoteAccess {
    pub fn load(path: PathBuf) -> Self {
        let saved = match std::fs::read(&path) {
            Ok(bytes) if bytes.is_empty() => Saved {
                file: AccessFile::default(),
                broken: false,
            },
            Ok(bytes) => match serde_json::from_slice::<AccessFile>(&bytes) {
                Ok(file) if file.format == "hbots-access" && file.version == 1 => Saved {
                    file,
                    broken: false,
                },
                _ => Saved {
                    file: AccessFile::default(),
                    broken: true,
                },
            },
            Err(_) => Saved {
                file: AccessFile::default(),
                broken: false,
            },
        };
        Self {
            path,
            inner: Mutex::new(saved),
        }
    }

    pub fn password_set(&self) -> bool {
        let saved = self.inner.lock().unwrap();
        !saved.broken && saved.file.password.is_some()
    }

    pub fn verify(&self, password: &str) -> bool {
        let saved = self.inner.lock().unwrap();
        if saved.broken {
            return false;
        }
        saved
            .file
            .password
            .as_ref()
            .is_some_and(|hash| password_matches(hash, password))
    }

    pub fn mode(&self) -> Exposure {
        let saved = self.inner.lock().unwrap();
        if saved.broken {
            Exposure::Off
        } else {
            Exposure::parse(&saved.file.exposure)
        }
    }

    pub fn set_password(&self, password: &str) -> Result<(), String> {
        if password.len() < 8 {
            return Err("Use at least 8 characters.".into());
        }
        let hash = hash_password(password)?;
        self.edit(|file| file.password = Some(hash))
    }

    pub fn set_mode(&self, mode: Exposure) -> Result<(), String> {
        let label = mode.as_str().to_string();
        self.edit(|file| file.exposure = label)
    }

    fn edit(&self, change: impl FnOnce(&mut AccessFile)) -> Result<(), String> {
        let mut saved = self.inner.lock().unwrap();
        if saved.broken {
            return Err("access.json could not be read, so it was left unchanged.".into());
        }
        change(&mut saved.file);
        write_private(&self.path, &serde_json::to_vec_pretty(&saved.file).unwrap())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn password_matches(hash: &PasswordHash, password: &str) -> bool {
    if hash.kdf != "scrypt" || hash.key_len == 0 || hash.key_len > 128 {
        return false;
    }
    if !hash.n.is_power_of_two() || hash.n < 2 {
        return false;
    }
    let Ok(salt) = STANDARD.decode(hash.salt.trim()) else {
        return false;
    };
    let Ok(expected) = STANDARD.decode(hash.hash.trim()) else {
        return false;
    };
    if expected.len() != hash.key_len {
        return false;
    }
    let Ok(params) = Params::new(hash.n.trailing_zeros() as u8, hash.r, hash.p, hash.key_len)
    else {
        return false;
    };
    let mut out = vec![0u8; hash.key_len];
    if scrypt(password.as_bytes(), &salt, &params, &mut out).is_err() {
        return false;
    }
    bool::from(out.as_slice().ct_eq(expected.as_slice()))
}

fn hash_password(password: &str) -> Result<PasswordHash, String> {
    let salt: [u8; 16] = rand::random();
    let key_len = 32;
    let params = Params::new(14, 8, 1, key_len).map_err(|e| e.to_string())?;
    let mut out = vec![0u8; key_len];
    scrypt(password.as_bytes(), &salt, &params, &mut out).map_err(|e| e.to_string())?;
    Ok(PasswordHash {
        kdf: "scrypt".into(),
        salt: STANDARD.encode(salt),
        n: 16384,
        r: 8,
        p: 1,
        key_len,
        hash: STANDARD.encode(out),
    })
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

/// What `GET /api/access` returns. The hash itself stays in the file.
pub fn public_view(access: &RemoteAccess, port: u16) -> Value {
    crate::exposure::status(access.mode(), port, access.password_set())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_node_written_access_json_verifies() {
        let raw = include_str!("../tests/fixtures/access.json");
        let file: AccessFile = serde_json::from_str(raw).unwrap();
        let hash = file.password.unwrap();
        assert!(password_matches(&hash, "correct horse"));
        assert!(!password_matches(&hash, "wrong password"));
        assert_eq!(file.exposure, "off");
    }

    #[test]
    fn node_can_read_a_hash_this_gateway_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let access = RemoteAccess::load(dir.path().join("access.json"));
        access.set_password("correct horse").unwrap();
        access.set_mode(Exposure::Landing).unwrap();
        assert!(access.verify("correct horse"));
        assert!(!access.verify("nope"));
        assert_eq!(access.mode(), Exposure::Landing);
        let script = r#"
          const fs = require('fs');
          const crypto = require('crypto');
          const file = JSON.parse(fs.readFileSync(process.argv[1], 'utf8'));
          const p = file.password;
          const out = crypto.scryptSync('correct horse', Buffer.from(p.salt, 'base64'), p.keyLen, { N: p.n, r: p.r, p: p.p });
          if (!out.equals(Buffer.from(p.hash, 'base64'))) process.exit(2);
          if (file.exposure !== 'landing') process.exit(3);
        "#;
        let status = std::process::Command::new("node")
            .arg("-e")
            .arg(script)
            .arg(dir.path().join("access.json"))
            .status()
            .expect("node");
        assert!(status.success(), "node rejected the scrypt hash");
    }

    #[test]
    fn a_broken_file_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("access.json");
        std::fs::write(&path, b"{not json").unwrap();
        let access = RemoteAccess::load(path.clone());
        assert!(!access.password_set());
        assert!(access.set_password("correct horse").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
    }

    #[test]
    fn short_passwords_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let access = RemoteAccess::load(dir.path().join("access.json"));
        assert!(access.set_password("short").is_err());
        assert!(!path_exists(&dir.path().join("access.json")));
        let _ = json!({});
    }

    fn path_exists(path: &Path) -> bool {
        path.exists()
    }
}
