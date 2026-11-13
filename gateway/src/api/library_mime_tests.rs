use super::*;

#[test]
fn library_mime_ignores_extension_case() {
    assert_eq!(library_mime("page.HTML"), "text/html; charset=utf-8");
    assert_eq!(library_mime("page.HTM"), "text/html; charset=utf-8");
    assert_eq!(library_mime("shot.PNG"), "image/png");
    assert_eq!(library_mime("shot.JPEG"), "image/jpeg");
    assert_eq!(library_mime("shot.JPG"), "image/jpeg");
    assert_eq!(library_mime("shot.GIF"), "image/gif");
    assert_eq!(library_mime("shot.WEBP"), "image/webp");
    assert_eq!(library_mime("notes.TXT"), "application/octet-stream");
}
