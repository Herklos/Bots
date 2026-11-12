use super::super::{gif_bytes, jpeg_bytes, kind_of, picture, png_bytes, webp_bytes, Kind};

#[test]
fn extension_picks_the_kind_and_a_lie_does_not() {
    assert_eq!(kind_of("library/shot.PNG"), Some(Kind::Png));
    assert_eq!(kind_of("library/shot.JPEG"), Some(Kind::Jpeg));
    assert_eq!(kind_of("library/shot.jpg"), Some(Kind::Jpeg));
    assert_eq!(kind_of("library/shot.GIF"), Some(Kind::Gif));
    assert_eq!(kind_of("library/shot.WEBP"), Some(Kind::Webp));
    assert!(kind_of("library/.png").is_none());
    assert!(kind_of("library/notes.txt").is_none());
    assert!(picture(Kind::Png, &jpeg_bytes(4, 3)).is_none());
    assert!(picture(Kind::Png, b"not really").is_none());
}

#[test]
fn signatures_carry_their_dimensions() {
    let png = picture(Kind::Png, &png_bytes(12, 7)).unwrap();
    assert_eq!((png.mime, png.width, png.height), ("image/png", 12, 7));
    let jpeg = picture(Kind::Jpeg, &jpeg_bytes(12, 7)).unwrap();
    assert_eq!((jpeg.mime, jpeg.width, jpeg.height), ("image/jpeg", 12, 7));
    let gif = picture(Kind::Gif, &gif_bytes(12, 7)).unwrap();
    assert_eq!((gif.mime, gif.width, gif.height), ("image/gif", 12, 7));
    let mut gif87 = gif_bytes(3, 2);
    gif87[..6].copy_from_slice(b"GIF87a");
    assert_eq!(picture(Kind::Gif, &gif87).unwrap().width, 3);
    let webp = picture(Kind::Webp, &webp_bytes(12, 7)).unwrap();
    assert_eq!((webp.mime, webp.width, webp.height), ("image/webp", 12, 7));
    assert_eq!(picture(Kind::Webp, &vp8(9, 4)).unwrap().width, 9);
    assert_eq!(picture(Kind::Webp, &vp8l(9, 4)).unwrap().height, 4);
}

#[test]
fn a_short_header_still_counts_when_the_signature_matches() {
    assert_eq!(
        picture(Kind::Png, &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
            .unwrap()
            .width,
        0
    );
    assert_eq!(picture(Kind::Jpeg, &[0xFF, 0xD8, 0xFF]).unwrap().height, 0);
    assert_eq!(picture(Kind::Gif, b"GIF89a").unwrap().width, 0);
    assert_eq!(
        picture(Kind::Webp, b"RIFF\x04\x00\x00\x00WEBP")
            .unwrap()
            .height,
        0
    );
    assert!(picture(Kind::Png, &[0x89, 0x50, 0x4E, 0x47]).is_none());
}

fn vp8(width: u16, height: u16) -> Vec<u8> {
    let mut payload = vec![0, 0, 0, 0x9D, 0x01, 0x2A];
    payload.extend_from_slice(&width.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    riff(b"VP8 ", &payload)
}

fn vp8l(width: u32, height: u32) -> Vec<u8> {
    let bits = (width - 1) | ((height - 1) << 14);
    let mut payload = vec![0x2F];
    payload.extend_from_slice(&bits.to_le_bytes());
    riff(b"VP8L", &payload)
}

fn riff(tag: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut chunk = tag.to_vec();
    chunk.extend_from_slice(
        &u32::try_from(payload.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    chunk.extend_from_slice(payload);
    if chunk.len() % 2 == 1 {
        chunk.push(0);
    }
    let mut bytes = b"RIFF".to_vec();
    let size = u32::try_from(4 + chunk.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&chunk);
    bytes
}
