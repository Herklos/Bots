use super::*;

#[test]
fn envelope_round_trips() {
    let key = [7u8; 32];
    let env = Vault::encrypt(&key, &json!({ "items": [{ "id": "a" }] }));
    assert_eq!(Vault::decrypt(&key, &env).unwrap()["items"][0]["id"], "a");
    assert!(Vault::decrypt(&[8u8; 32], &env).is_err());
}

#[test]
fn decrypts_a_vault_written_by_node() {
    // Fixture ciphertext. Key is 32 bytes of 0x07. See tests/fixtures/node-vault.json.
    let envelope: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/node-vault.json")).unwrap();
    let payload = Vault::decrypt(&[7u8; 32], &envelope).unwrap();
    assert_eq!(payload["items"][0]["password"], "from-node");
}

#[test]
fn site_matching_refuses_lookalikes() {
    assert!(matches_site("https://accounts.google.com/x", "google.com"));
    assert!(!matches_site("https://google.com.evil.io/", "google.com"));
    assert!(!matches_site("https://accounts-google.com/", "google.com"));
    assert!(!matches_site("http://google.com/", "google.com"));
    assert_eq!(site_domain("https://www.GitHub.com/login"), "github.com");
}

#[tokio::test]
async fn create_redacts_and_reveal_returns_password() {
    let dir = tempfile::tempdir().unwrap();
    let v = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let item = v.create(&json!({ "type": "login", "website": "github.com", "username": "me", "password": "s3cret" })).await.unwrap();
    assert!(item.get("password").is_none());
    assert_eq!(item["hasPassword"], true);
    assert_eq!(item["website"], "https://github.com");
    let id = item["id"].as_str().unwrap();
    assert_eq!(v.get(id, true).await.unwrap()["password"], "s3cret");
    v.update(id, &json!({ "username": "you", "password": "" }))
        .await
        .unwrap();
    // A fresh instance reads the file back with the same key.
    let again = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let got = again.get(id, true).await.unwrap();
    assert_eq!(
        (got["username"].as_str(), got["password"].as_str()),
        (Some("you"), Some("s3cret"))
    );
    assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
}
