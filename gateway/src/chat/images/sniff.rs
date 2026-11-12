//! File signatures and the width and height stored in the header.
//! A file whose header does not match its extension is not an image.
//! Dimensions that cannot be read are zero; the signature still decides.
use sha2::Digest;

/// An image larger than this is refused. One extra byte is enough to know.
pub(crate) const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Png,
    Jpeg,
    Gif,
    Webp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Picture {
    pub(crate) mime: &'static str,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn kind_of(path: &str) -> Option<Kind> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(Kind::Png),
        "jpg" | "jpeg" => Some(Kind::Jpeg),
        "gif" => Some(Kind::Gif),
        "webp" => Some(Kind::Webp),
        _ => None,
    }
}

pub(crate) fn picture(kind: Kind, bytes: &[u8]) -> Option<Picture> {
    match kind {
        Kind::Png => png(bytes),
        Kind::Jpeg => jpeg(bytes),
        Kind::Gif => gif(bytes),
        Kind::Webp => webp(bytes),
    }
}

/// A strong etag: the first 16 hex digits of sha256, quoted.
pub(crate) fn etag_of(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    format!("\"{}\"", &hex::encode(digest)[..16])
}

pub(crate) fn image_id(path: &str) -> String {
    let digest = sha2::Sha256::digest(path.as_bytes());
    format!("lib:{}", &hex::encode(digest)[..8])
}

const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

fn png(bytes: &[u8]) -> Option<Picture> {
    if bytes.len() < PNG_SIG.len() || bytes[..PNG_SIG.len()] != PNG_SIG {
        return None;
    }
    let (width, height) = if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        (be_u32(bytes, 16), be_u32(bytes, 20))
    } else {
        (0, 0)
    };
    Some(Picture {
        mime: "image/png",
        width,
        height,
    })
}

fn jpeg(bytes: &[u8]) -> Option<Picture> {
    if bytes.len() < 3 || bytes[0] != 0xFF || bytes[1] != 0xD8 || bytes[2] != 0xFF {
        return None;
    }
    let mut i = 2usize;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            break;
        }
        i += 1;
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let marker = bytes[i];
        i += 1;
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0x00 || marker == 0xD8 || marker == 0xD9 {
            break;
        }
        if i + 1 >= bytes.len() {
            break;
        }
        let len = usize::from(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
        if len < 2 || i + len > bytes.len() {
            break;
        }
        if is_sof(marker) {
            if len < 7 {
                break;
            }
            return Some(Picture {
                mime: "image/jpeg",
                width: u32::from(u16::from_be_bytes([bytes[i + 5], bytes[i + 6]])),
                height: u32::from(u16::from_be_bytes([bytes[i + 3], bytes[i + 4]])),
            });
        }
        if marker == 0xDA {
            break;
        }
        i += len;
    }
    Some(Picture {
        mime: "image/jpeg",
        width: 0,
        height: 0,
    })
}

fn is_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
    )
}

fn gif(bytes: &[u8]) -> Option<Picture> {
    if bytes.len() < 6 || (&bytes[..6] != b"GIF87a" && &bytes[..6] != b"GIF89a") {
        return None;
    }
    let (width, height) = if bytes.len() >= 10 {
        (
            u32::from(u16::from_le_bytes([bytes[6], bytes[7]])),
            u32::from(u16::from_le_bytes([bytes[8], bytes[9]])),
        )
    } else {
        (0, 0)
    };
    Some(Picture {
        mime: "image/gif",
        width,
        height,
    })
}

fn webp(bytes: &[u8]) -> Option<Picture> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let mut i = 12usize;
    let mut width = 0u32;
    let mut height = 0u32;
    while i + 8 <= bytes.len() {
        let tag = &bytes[i..i + 4];
        let n = usize::try_from(u32::from_le_bytes([
            bytes[i + 4],
            bytes[i + 5],
            bytes[i + 6],
            bytes[i + 7],
        ]))
        .unwrap_or(usize::MAX);
        let start = i + 8;
        let Some(end) = start.checked_add(n) else {
            break;
        };
        if end > bytes.len() {
            break;
        }
        let payload = &bytes[start..end];
        if let Some((w, h)) = chunk_size(tag, payload) {
            width = w;
            height = h;
            break;
        }
        let Some(next) = end.checked_add(n % 2) else {
            break;
        };
        i = next;
    }
    Some(Picture {
        mime: "image/webp",
        width,
        height,
    })
}

fn chunk_size(tag: &[u8], payload: &[u8]) -> Option<(u32, u32)> {
    match tag {
        b"VP8X" => vp8x(payload),
        b"VP8 " => vp8(payload),
        b"VP8L" => vp8l(payload),
        _ => None,
    }
}

fn vp8x(payload: &[u8]) -> Option<(u32, u32)> {
    if payload.len() < 10 {
        return None;
    }
    let width =
        u32::from(payload[4]) | (u32::from(payload[5]) << 8) | (u32::from(payload[6]) << 16);
    let height =
        u32::from(payload[7]) | (u32::from(payload[8]) << 8) | (u32::from(payload[9]) << 16);
    Some((width + 1, height + 1))
}

fn vp8(payload: &[u8]) -> Option<(u32, u32)> {
    if payload.len() < 10 || payload[0] & 1 != 0 || payload[3..6] != [0x9D, 0x01, 0x2A] {
        return None;
    }
    let width = u32::from(u16::from_le_bytes([payload[6], payload[7]]) & 0x3FFF);
    let height = u32::from(u16::from_le_bytes([payload[8], payload[9]]) & 0x3FFF);
    Some((width, height))
}

fn vp8l(payload: &[u8]) -> Option<(u32, u32)> {
    if payload.len() < 5 || payload[0] != 0x2F {
        return None;
    }
    let bits = u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]);
    Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
}

fn be_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
pub(crate) fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = PNG_SIG.to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0]);
    bytes
}

#[cfg(test)]
pub(crate) fn jpeg_bytes(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08];
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    bytes
}

#[cfg(test)]
pub(crate) fn gif_bytes(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes
}

#[cfg(test)]
pub(crate) fn webp_bytes(width: u32, height: u32) -> Vec<u8> {
    let ww = width.saturating_sub(1);
    let hh = height.saturating_sub(1);
    let mut chunk = b"VP8X".to_vec();
    chunk.extend_from_slice(&10u32.to_le_bytes());
    chunk.extend_from_slice(&[0, 0, 0, 0]);
    chunk.extend_from_slice(&[byte_at(ww, 0), byte_at(ww, 8), byte_at(ww, 16)]);
    chunk.extend_from_slice(&[byte_at(hh, 0), byte_at(hh, 8), byte_at(hh, 16)]);
    riff(&chunk)
}

#[cfg(test)]
fn byte_at(value: u32, shift: u32) -> u8 {
    u8::try_from((value >> shift) & 0xFF).unwrap_or(0)
}

#[cfg(test)]
fn riff(chunk: &[u8]) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    let size = u32::try_from(4 + chunk.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(chunk);
    bytes
}
