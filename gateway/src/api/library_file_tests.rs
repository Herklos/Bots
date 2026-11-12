use super::tests::settings_state;
use super::*;
use axum::body::to_bytes;

fn bot(dir: &std::path::Path) {
    let root = dir.join("bots/ada");
    std::fs::create_dir_all(root.join(".bot")).unwrap();
    std::fs::create_dir_all(root.join("library")).unwrap();
    std::fs::write(root.join(".bot/bot.json"), r#"{"name":"Ada"}"#).unwrap();
}

async fn fetch(state: Arc<AppState>, path: &str, download: bool, etag: Option<&str>) -> Response {
    let mut query = HashMap::from([("path".into(), path.into())]);
    if download {
        query.insert("download".into(), "1".into());
    }
    let mut headers = HeaderMap::new();
    if let Some(etag) = etag {
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(etag).unwrap());
    }
    library_file(State(state), Path("ada".into()), Query(query), headers)
        .await
        .unwrap_or_else(|err| err.into_response())
}

fn header(res: &Response, name: header::HeaderName) -> Option<&str> {
    res.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
}

#[tokio::test]
async fn images_are_nosniff_typed_and_a_repeat_is_not_modified() {
    let dir = tempfile::tempdir().unwrap();
    bot(dir.path());
    let library = dir.path().join("bots/ada/library");
    std::fs::write(library.join("page.html"), b"<script>alert(1)</script>").unwrap();
    std::fs::write(
        library.join("shot.png"),
        crate::chat::images::png_bytes(2, 1),
    )
    .unwrap();
    std::fs::write(
        library.join("shot.gif"),
        crate::chat::images::gif_bytes(2, 1),
    )
    .unwrap();
    std::fs::write(
        library.join("shot.webp"),
        crate::chat::images::webp_bytes(2, 1),
    )
    .unwrap();
    let state = settings_state(dir.path());

    let html = fetch(state.clone(), "library/page.html", false, None).await;
    assert_eq!(html.status(), StatusCode::OK);
    assert_eq!(
        header(&html, header::CONTENT_SECURITY_POLICY),
        Some("sandbox")
    );
    assert_eq!(
        header(&html, header::X_CONTENT_TYPE_OPTIONS),
        Some("nosniff")
    );
    assert!(header(&html, header::ETAG).is_some());

    let png = fetch(state.clone(), "library/shot.png", false, None).await;
    assert_eq!(png.status(), StatusCode::OK);
    assert_eq!(header(&png, header::CONTENT_TYPE), Some("image/png"));
    assert!(png.headers().get(header::CONTENT_SECURITY_POLICY).is_none());
    assert_eq!(
        header(&png, header::X_CONTENT_TYPE_OPTIONS),
        Some("nosniff")
    );
    assert_eq!(
        header(&png, header::CONTENT_DISPOSITION),
        Some("inline; filename=\"shot.png\"")
    );
    let tag = header(&png, header::ETAG).unwrap().to_string();
    let again = fetch(state.clone(), "library/shot.png", false, Some(&tag)).await;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    let body = to_bytes(again.into_body(), 64).await.unwrap();
    assert!(body.is_empty());

    let gif = fetch(state.clone(), "library/shot.gif", false, None).await;
    assert_eq!(header(&gif, header::CONTENT_TYPE), Some("image/gif"));
    let webp = fetch(state.clone(), "library/shot.webp", false, None).await;
    assert_eq!(header(&webp, header::CONTENT_TYPE), Some("image/webp"));

    let saved = fetch(state, "library/shot.png", true, None).await;
    assert_eq!(
        header(&saved, header::CONTENT_DISPOSITION),
        Some("attachment; filename=\"shot.png\"")
    );
}

#[tokio::test]
async fn a_symlink_an_oversize_file_and_a_renamed_text_file_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    bot(dir.path());
    let library = dir.path().join("bots/ada/library");
    std::fs::write(dir.path().join("bots/ada/secret.bin"), b"secret-bytes").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("../secret.bin", library.join("link.png")).unwrap();
    let mut huge = crate::chat::images::png_bytes(1, 1);
    huge.resize(crate::chat::images::MAX_IMAGE_BYTES + 1, 0);
    std::fs::write(library.join("huge.png"), &huge).unwrap();
    let mut exact = crate::chat::images::png_bytes(1, 1);
    exact.resize(crate::chat::images::MAX_IMAGE_BYTES, 0);
    std::fs::write(library.join("exact.png"), &exact).unwrap();
    std::fs::write(library.join("note.png"), b"hello").unwrap();
    let state = settings_state(dir.path());

    #[cfg(unix)]
    {
        let link = fetch(state.clone(), "library/link.png", false, None).await;
        assert_eq!(link.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(link.into_body(), 1024).await.unwrap();
        assert!(!body.windows(6).any(|window| window == b"secret"));
    }

    let over = fetch(state.clone(), "library/huge.png", false, None).await;
    assert_eq!(over.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let fit = fetch(state.clone(), "library/exact.png", false, None).await;
    assert_eq!(fit.status(), StatusCode::OK);
    let text = fetch(state, "library/note.png", false, None).await;
    assert_eq!(text.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}
