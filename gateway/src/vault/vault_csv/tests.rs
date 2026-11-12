use super::*;

fn rows(text: &str) -> Vec<Vec<String>> {
    parse_csv(text)
}

fn s(v: &str) -> String {
    v.to_string()
}

#[test]
fn parses_quotes_crlf_and_bom() {
    assert_eq!(
        rows("\u{feff}a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,2,3\r\n"),
        vec![
            vec![s("a"), s("b,c"), s("say \"hi\"")],
            vec![s("1"), s("2"), s("3")],
        ]
    );
}

#[test]
fn keeps_newlines_inside_quotes_and_drops_blank_rows() {
    assert_eq!(
        rows("x,y\n\n\"line one\nline two\",z"),
        vec![vec![s("x"), s("y")], vec![s("line one\nline two"), s("z")]]
    );
    // A trailing row without a newline still counts; a row of spaces does not.
    assert_eq!(rows("a\n   \nb"), vec![vec![s("a")], vec![s("b")]]);
}

#[test]
fn keeps_empty_trailing_fields() {
    assert_eq!(rows("a,,\r\n"), vec![vec![s("a"), s(""), s("")]]);
}

#[test]
fn writes_quoted_cells_only_when_needed() {
    let out = to_csv(&[vec![
        s("plain"),
        s("has,comma"),
        s("has \"quote\""),
        s("line\r\nbreak"),
        s(" leading"),
        s("trailing "),
        s(""),
    ]]);
    assert_eq!(
        out,
        "plain,\"has,comma\",\"has \"\"quote\"\"\",\"line\r\nbreak\",\" leading\",\"trailing \",\r\n"
    );
    assert_eq!(to_csv(&[]), "\r\n");
}

#[test]
fn chrome_fixture() {
    let r = parse_vault_csv(
        "name,url,username,password,note\r\nGitHub,https://github.com/login,octo,hunter2,\"two, lines\"\r\n",
    );
    assert_eq!(r.format, "Chrome or Edge");
    assert_eq!(r.skipped, 0);
    assert_eq!(
        r.items,
        vec![
            json!({"type":"login","title":"GitHub","website":"https://github.com/login","username":"octo","password":"hunter2","hints":"two, lines"})
        ]
    );
}

#[test]
fn one_password_fixture() {
    let r = parse_vault_csv(
        "Title,Website,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\nExample,https://example.com,ann,pw1,,false,false,,remember me\n",
    );
    assert_eq!(r.format, "1Password");
    assert_eq!(r.items[0]["title"], "Example");
    assert_eq!(r.items[0]["website"], "https://example.com");
    assert_eq!(r.items[0]["hints"], "remember me");
}

#[test]
fn safari_fixture() {
    let r = parse_vault_csv(
        "Title,URL,Username,Password,Notes,OTPAuth\nBank,https://bank.example,bob,pw2,,\n",
    );
    assert_eq!(r.format, "Safari or Apple Passwords");
    assert_eq!(r.items[0]["username"], "bob");
    assert_eq!(r.items[0]["password"], "pw2");
}

#[test]
fn firefox_fixture_takes_the_title_from_the_host() {
    let r = parse_vault_csv(
        "url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,timePasswordChanged\nhttps://www.forum.example/x,cy,pw3,,https://forum.example,{1},0,0,0\n",
    );
    assert_eq!(r.format, "Firefox");
    assert_eq!(r.items[0]["title"], "forum.example");
    assert_eq!(r.items[0]["website"], "https://www.forum.example/x");
}

#[test]
fn bitwarden_fixture_skips_notes() {
    let r = parse_vault_csv(
        "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
         ,0,login,Mail,mailnote,,0,\"https://mail.example, https://webmail.example\",dee,pw4,\n\
         ,0,note,Secret note,body,,0,,,,\n",
    );
    assert_eq!(r.format, "Bitwarden");
    assert_eq!(r.skipped, 1);
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0]["website"], "https://mail.example");
    assert_eq!(r.items[0]["hints"], "mailnote");
}

#[test]
fn lastpass_fixture_skips_secure_notes() {
    let r = parse_vault_csv(
        "url,username,password,totp,extra,name,grouping,fav\n\
         http://sn,,,,secret stuff,Note,,0\n\
         https://shop.example,eve,pw5,,shop note,Shop,Store,0\n",
    );
    assert_eq!(r.format, "LastPass");
    assert_eq!(r.skipped, 1);
    assert_eq!(r.items[0]["title"], "Shop");
    assert_eq!(r.items[0]["hints"], "shop note");
}

#[test]
fn bot_contact_fixture_keeps_repeated_values() {
    let r = parse_vault_csv(concat!(
        "kind,label,name,email,phone,street,apt,city,state,zip,country,note,emails,phones\r\n",
        "home,Home,Ada,ada@example.com,555,1 Main St,,Springfield,IL,62701,US,door code 1234,",
        "\"[\"\"ada@example.com\"\",\"\"ada@home.example\"\"]\",\"[\"\"555\"\"]\"\r\n",
        "work,Broken,,,,,,,,,,,\"[1]\",\r\n",
    ));
    assert_eq!(r.format, "Bot contact info");
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.skipped, 1);
    let c = &r.items[0];
    assert_eq!(c["type"], "contact");
    assert_eq!(c["emails"], json!(["ada@example.com", "ada@home.example"]));
    assert_eq!(c["phones"], json!(["555"]));
    assert_eq!(c["hints"], "door code 1234");
    assert_eq!(c["kind"], "home");
}

#[test]
fn login_websites_keep_their_order_and_malformed_arrays_are_skipped() {
    let r = parse_vault_csv(concat!(
        "name,url,username,password,note,websites\n",
        "Sample,https://first.invalid,user,synthetic,,\"[\"\"https://first.invalid\"\",\"\"https://second.invalid\"\"]\"\n",
        "Broken,https://x.invalid,user,synthetic,,broken\n",
    ));
    assert_eq!(r.skipped, 1);
    assert_eq!(
        r.items[0]["websites"],
        json!(["https://first.invalid", "https://second.invalid"])
    );
}

#[test]
fn rows_without_a_login_or_a_site_are_skipped() {
    let r = parse_vault_csv("name,url,username,password\nNo login,,,\n,,me,\n");
    assert_eq!(r.items.len(), 0);
    assert_eq!(r.skipped, 2);
    assert_eq!(parse_vault_csv("name,url\n").items.len(), 0);
}

fn tmp_vault(dir: &tempfile::TempDir) -> Vault {
    Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"))
}

#[tokio::test]
async fn import_merges_by_domain_and_username() {
    let dir = tempfile::tempdir().unwrap();
    let v = tmp_vault(&dir);
    v.create(
        &json!({"type":"login","website":"https://github.com","username":"octo","password":"old"}),
    )
    .await
    .unwrap();
    let counts = import_items(
        &v,
        &json!([
            {"website":"https://www.github.com/login","username":"octo","password":"new"},
            {"website":"https://github.com","username":"octo","password":"new"},
            {"website":"https://gitlab.com","username":"octo","password":"x"},
            {"type":"contact","name":"Ada","email":"ada@example.com"},
            {"title":""},
        ]),
    )
    .await
    .unwrap();
    assert_eq!(
        counts,
        ImportCounts {
            added: 2,
            updated: 1,
            skipped: 2
        }
    );
    let github = v
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|i| i["domain"] == "github.com")
        .unwrap();
    let full = v.get(github["id"].as_str().unwrap(), true).await.unwrap();
    assert_eq!(full["password"], "new");
}

#[tokio::test]
async fn import_rejects_a_list_that_is_not_a_list() {
    let dir = tempfile::tempdir().unwrap();
    let err = import_items(&tmp_vault(&dir), &json!({"a": 1}))
        .await
        .unwrap_err();
    assert_eq!(err.status, 400);
}

#[tokio::test]
async fn import_skips_a_contact_already_saved() {
    let dir = tempfile::tempdir().unwrap();
    let v = tmp_vault(&dir);
    let contact = json!({"type":"contact","label":"Home","name":"Ada","emails":["ada@example.com","a@b.example"],"kind":"home"});
    assert_eq!(
        import_items(&v, &json!([contact.clone()]))
            .await
            .unwrap()
            .added,
        1
    );
    assert_eq!(
        import_items(&v, &json!([contact])).await.unwrap().skipped,
        1
    );
}

#[tokio::test]
async fn export_then_import_gives_the_same_items() {
    let dir_a = tempfile::tempdir().unwrap();
    let a = tmp_vault(&dir_a);
    a.create(&json!({"type":"login","title":"Mail","websites":["https://mail.example","https://webmail.example"],"username":"dee","password":" pass, \"quoted\"\r\n","hints":"two\nlines"}))
        .await
        .unwrap();
    a.create(
        &json!({"type":"login","website":"https://shop.example","username":"eve","password":"pw5"}),
    )
    .await
    .unwrap();
    a.create(&json!({"type":"contact","label":"Home","name":"Ada","email":"ada@example.com","phones":["555","556"],"kind":"work","street":"1 Main St, Apt 2"}))
        .await
        .unwrap();

    let logins = export_csv(&a, "login").await.unwrap();
    let contacts = export_csv(&a, "contact").await.unwrap();

    let dir_b = tempfile::tempdir().unwrap();
    let b = tmp_vault(&dir_b);
    let parsed_logins = parse_vault_csv(&logins);
    assert_eq!(parsed_logins.format, "Chrome or Edge");
    assert_eq!(parsed_logins.skipped, 0);
    let parsed_contacts = parse_vault_csv(&contacts);
    assert_eq!(parsed_contacts.format, "Bot contact info");
    assert_eq!(parsed_contacts.skipped, 0);
    import_items(&b, &Value::Array(parsed_logins.items))
        .await
        .unwrap();
    import_items(&b, &Value::Array(parsed_contacts.items))
        .await
        .unwrap();

    assert_eq!(export_csv(&b, "login").await.unwrap(), logins);
    assert_eq!(export_csv(&b, "contact").await.unwrap(), contacts);
    // The password survived the round trip, quotes, comma, CRLF and all.
    let reread = parse_vault_csv(&logins);
    assert_eq!(reread.items[0]["password"], " pass, \"quoted\"\r\n");
}

#[tokio::test]
async fn exports_passwords_only_in_the_login_file() {
    let dir = tempfile::tempdir().unwrap();
    let v = tmp_vault(&dir);
    v.create(&json!({"type":"login","website":"https://a.example","username":"u","password":"hidden-pass"}))
        .await
        .unwrap();
    assert!(export_csv(&v, "login")
        .await
        .unwrap()
        .contains("hidden-pass"));
    assert!(!export_csv(&v, "contact")
        .await
        .unwrap()
        .contains("hidden-pass"));
}
