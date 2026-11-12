use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn names_and_addresses() {
    assert!(cache_name("Example.COM").as_deref() == Some("example.com"));
    assert!(cache_name("..").is_none());
    assert!(cache_name("a/b").is_none());
    assert!(cache_name("localhost").as_deref() == Some("localhost"));
    assert!(!ip_is_public("127.0.0.1".parse().unwrap()));
    assert!(!ip_is_public("10.1.2.3".parse().unwrap()));
    assert!(!ip_is_public("192.168.1.1".parse().unwrap()));
    assert!(!ip_is_public("172.16.0.1".parse().unwrap()));
    assert!(!ip_is_public("169.254.1.1".parse().unwrap()));
    assert!(!ip_is_public("100.64.0.1".parse().unwrap()));
    assert!(!ip_is_public("0.0.0.0".parse().unwrap()));
    assert!(!ip_is_public("::1".parse().unwrap()));
    assert!(!ip_is_public("fe80::1".parse().unwrap()));
    assert!(!ip_is_public("fd00::1".parse().unwrap()));
    assert!(ip_is_public("1.1.1.1".parse().unwrap()));
    assert!(ip_is_public("8.8.8.8".parse().unwrap()));
}

#[test]
fn the_icon_link_is_the_href() {
    let html = r#"<link rel="stylesheet" href="/a.css"><link rel="icon" href="/favicon.png"><link rel='shortcut icon' href='/x.ico'>"#;
    assert_eq!(icon_href(html), Some("/favicon.png"));
    assert_eq!(
        icon_href("<link href=\"/nope.css\" rel=\"stylesheet\">"),
        None
    );
}

#[test]
fn bytes_identify_a_png_when_the_header_is_generic() {
    let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a];
    assert_eq!(
        image_type("application/octet-stream", &png),
        Some("image/png")
    );
    assert_eq!(image_type("text/html", &png), None);
    assert_eq!(image_type("image/png", b"not really"), Some("image/png"));
}

#[tokio::test]
async fn a_private_address_is_refused_before_any_fetch() {
    let err = assert_public("127.0.0.1").await.unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    let err = assert_public("printer.local").await.unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_page_icon_is_fetched_and_cached() {
    let png = [0x89, b'P', b'N', b'G', 1, 2, 3, 4];
    let origin = serve(vec![
        ("/favicon.ico", 404, "text/plain", b"no".to_vec()),
        (
            "/",
            200,
            "text/html",
            b"<html><link rel=\"icon\" href=\"/icon.png\"></html>".to_vec(),
        ),
        ("/icon.png", 200, "image/png", png.to_vec()),
    ])
    .await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let url = Url::parse(&origin).unwrap();
    let (bytes, ctype) = load_site(&client, &url).await.unwrap();
    assert_eq!(
        (bytes.as_slice(), ctype.as_str()),
        (png.as_slice(), "image/png")
    );

    let dir = tempfile::tempdir().unwrap();
    let icons = Icons {
        dir: dir.path().to_path_buf(),
        client,
    };
    icons.store("example.com", &bytes, &ctype).await.unwrap();
    let (again, again_type) = icons.cached("example.com").await.unwrap();
    assert_eq!(again, bytes);
    assert_eq!(again_type, "image/png");
}

#[tokio::test]
async fn an_off_host_icon_link_is_refused() {
    let origin = serve(vec![
        ("/favicon.ico", 404, "text/plain", b"no".to_vec()),
        (
            "/",
            200,
            "text/html",
            b"<link rel=\"icon\" href=\"https://other.example/a.png\">".to_vec(),
        ),
    ])
    .await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let err = load_site(&client, &Url::parse(&origin).unwrap())
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

async fn serve(routes: Vec<(&str, u16, &str, Vec<u8>)>) -> String {
    let map: HashMap<String, (u16, String, Vec<u8>)> = routes
        .into_iter()
        .map(|(p, s, t, b)| (p.to_string(), (s, t.to_string(), b)))
        .collect();
    let map = Arc::new(map);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let map = map.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/");
                let (status, ctype, body) =
                    map.get(path)
                        .cloned()
                        .unwrap_or((404, "text/plain".into(), b"no".to_vec()));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
            });
        }
    });
    format!("http://{addr}/")
}
