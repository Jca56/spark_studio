//! The glTF containers. A `.glb` is one binary file — a 12-byte header, a
//! JSON chunk, a BIN chunk — and a `.gltf` is the JSON as text with its
//! buffers beside it as files or inlined as base64 data URIs. Either way
//! the loader gets the document and the buffers it indexes.

use std::path::Path;

use crate::json::Json;
use crate::{Error, invalid};

const MAGIC: &[u8; 4] = b"glTF";
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// A parsed container: the document and every buffer, in index order.
pub struct Container {
    pub json: Json,
    pub buffers: Vec<Vec<u8>>,
}

/// Open a `.glb` or `.gltf` by content — the extension is a hint at
/// best. External buffers and images resolve relative to the file.
pub fn open(path: &Path) -> Result<Container, Error> {
    let bytes = std::fs::read(path)?;
    let base = path.parent().unwrap_or(Path::new("."));
    from_bytes(&bytes, base)
}

/// Parse container bytes; `base` is where relative URIs resolve.
pub fn from_bytes(bytes: &[u8], base: &Path) -> Result<Container, Error> {
    let (json, bin) = if bytes.starts_with(MAGIC) {
        split_glb(bytes)?
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("glTF text is not UTF-8"))?;
        (Json::parse(text)?, None)
    };
    let buffers = buffers(&json, bin, base)?;
    Ok(Container { json, buffers })
}

/// The JSON document and the BIN chunk, if any, out of a `.glb`.
fn split_glb(b: &[u8]) -> Result<(Json, Option<Vec<u8>>), Error> {
    if b.len() < 12 {
        return Err(invalid("GLB shorter than its header"));
    }
    let version = u32_at(b, 4);
    if version != 2 {
        return Err(Error::Unsupported(format!("GLB version {version}")));
    }
    let length = (u32_at(b, 8) as usize).min(b.len());
    let mut at = 12;
    let mut json = None;
    let mut bin = None;
    while at + 8 <= length {
        let len = u32_at(b, at) as usize;
        let kind = u32_at(b, at + 4);
        let data = b
            .get(at + 8..at + 8 + len)
            .ok_or_else(|| invalid("GLB chunk runs past the file"))?;
        match kind {
            CHUNK_JSON if json.is_none() => {
                let text = std::str::from_utf8(data).map_err(|_| invalid("GLB JSON is not UTF-8"))?;
                json = Some(Json::parse(text)?);
            }
            CHUNK_BIN if bin.is_none() => bin = Some(data.to_vec()),
            // Unknown chunk kinds are to be skipped, says the spec.
            _ => {}
        }
        at += 8 + len;
    }
    Ok((json.ok_or_else(|| invalid("GLB has no JSON chunk"))?, bin))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Every `buffers[i]` as bytes. A buffer with no URI is the GLB's BIN
/// chunk (only the first may be); a `data:` URI is inlined base64; any
/// other URI is a file beside the document.
fn buffers(json: &Json, mut bin: Option<Vec<u8>>, base: &Path) -> Result<Vec<Vec<u8>>, Error> {
    let Some(list) = json.get("buffers").and_then(Json::as_array) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(list.len());
    for (i, b) in list.iter().enumerate() {
        let bytes = match b.get("uri").and_then(Json::as_str) {
            None => bin
                .take()
                .ok_or_else(|| invalid(format!("buffer {i} has no URI and there is no BIN chunk")))?,
            Some(uri) => load_uri(uri, base)?,
        };
        if let Some(len) = b.get("byteLength").and_then(Json::as_usize)
            && bytes.len() < len
        {
            return Err(invalid(format!(
                "buffer {i} is {} bytes, declared {len}",
                bytes.len()
            )));
        }
        out.push(bytes);
    }
    Ok(out)
}

/// The bytes a URI names: inlined base64, or a file beside the document.
pub fn load_uri(uri: &str, base: &Path) -> Result<Vec<u8>, Error> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (_, payload) = rest
            .split_once(";base64,")
            .ok_or_else(|| Error::Unsupported("data URI that is not base64".into()))?;
        return base64(payload);
    }
    Ok(std::fs::read(base.join(percent_decode(uri)))?)
}

/// `%20` and friends in a relative URI.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Standard base64 with optional `=` padding; whitespace ignored.
pub fn base64(s: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return Err(invalid("bad base64")),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// Build a `.glb` from a document and a BIN chunk — what the tests feed
/// the loader, and what a writer would produce.
pub fn assemble(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    let total = 12 + 8 + j.len() + if bin.is_empty() { 0 } else { 8 + b.len() };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&j);
    if !bin.is_empty() {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        assert_eq!(base64("").unwrap(), b"");
        assert_eq!(base64("Zg==").unwrap(), b"f");
        assert_eq!(base64("Zm8=").unwrap(), b"fo");
        assert_eq!(base64("Zm9v").unwrap(), b"foo");
        assert_eq!(base64("Zm9v\nYmFy").unwrap(), b"foobar");
        assert!(base64("Zm9v!").is_err());
    }

    #[test]
    fn a_glb_splits_into_its_chunks() {
        let glb = assemble(r#"{"buffers":[{"byteLength":5}]}"#, &[1, 2, 3, 4, 5]);
        let c = from_bytes(&glb, Path::new(".")).unwrap();
        assert_eq!(c.buffers, vec![vec![1, 2, 3, 4, 5, 0, 0, 0]]);
        assert!(c.json.get("buffers").is_some());
    }

    #[test]
    fn a_data_uri_buffer_inlines() {
        let text = r#"{"buffers":[{"byteLength":3,"uri":"data:application/octet-stream;base64,AQID"}]}"#;
        let c = from_bytes(text.as_bytes(), Path::new(".")).unwrap();
        assert_eq!(c.buffers, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn a_short_buffer_is_refused() {
        let glb = assemble(r#"{"buffers":[{"byteLength":50}]}"#, &[1, 2, 3]);
        assert!(matches!(from_bytes(&glb, Path::new(".")), Err(Error::Invalid(_))));
    }

    #[test]
    fn percent_escapes_decode() {
        assert_eq!(percent_decode("my%20logo.bin"), "my logo.bin");
        assert_eq!(percent_decode("plain.bin"), "plain.bin");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
