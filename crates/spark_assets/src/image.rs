//! Images, decoded. An FFmpeg subprocess turns whatever bytes a file or a
//! GLB holds — JPEG, PNG, WebP, anything it reads — into raw RGBA, the way
//! `spark_audio` turns any audio file into PCM: piped, never linked. Raw
//! video carries no dimensions, so those are read here from the file's
//! own header (PNG and JPEG), or asked of `ffprobe` for anything else.
//!
//! A decoded image also builds its own **mip chain**: a 4K texture on a
//! 1080-unit canvas shimmers without one. Each level halves the last with
//! a box filter run in *linear* light — averaging sRGB bytes directly
//! darkens every edge — with alpha averaged as the coverage it is.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{Error, invalid};

/// Straight (not premultiplied) 8-bit sRGB RGBA, rows top to bottom.
#[derive(Clone, PartialEq, Debug)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }
}

pub fn decode_file(path: &Path) -> Result<Rgba, Error> {
    decode(&std::fs::read(path)?)
}

/// Decode encoded image bytes through FFmpeg.
pub fn decode(bytes: &[u8]) -> Result<Rgba, Error> {
    let (width, height) = match dimensions(bytes) {
        Some(d) => d,
        None => probe(bytes)?,
    };
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i", "pipe:0", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Io(std::io::Error::other(format!("ffmpeg spawn failed: {e}"))))?;
    // Feed stdin from its own thread: FFmpeg blocks on a full stdout pipe
    // before it has read all of a large input, and so would we.
    let mut stdin = child.stdin.take().expect("piped");
    let input = bytes.to_vec();
    let feeder = std::thread::spawn(move || stdin.write_all(&input));
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    child
        .stdout
        .take()
        .expect("piped")
        .read_to_end(&mut pixels)?;
    let status = child.wait()?;
    let _ = feeder.join();
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(invalid(format!("ffmpeg could not decode the image: {}", err.trim())));
    }
    if pixels.len() != (width * height * 4) as usize {
        return Err(invalid(format!(
            "decoded {} bytes, expected {}×{}×4",
            pixels.len(),
            width,
            height
        )));
    }
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

/// Width and height out of a PNG's IHDR or a JPEG's SOF marker; `None`
/// for anything else.
pub fn dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.len() >= 24 && &b[12..16] == b"IHDR" {
        return Some((be32(b, 16), be32(b, 20)));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 4 <= b.len() {
            if b[i] != 0xFF {
                return None;
            }
            let m = b[i + 1];
            // Fill bytes, and markers with no payload.
            if m == 0xFF {
                i += 1;
                continue;
            }
            if (0xD0..=0xD9).contains(&m) || m == 0x01 {
                i += 2;
                continue;
            }
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            // SOF0..SOF15, minus the three markers that share the range.
            if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
                if i + 9 > b.len() {
                    return None;
                }
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// `ffprobe` for the formats whose headers aren't read here.
fn probe(bytes: &[u8]) -> Result<(u32, u32), Error> {
    let mut child = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            "pipe:0",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Io(std::io::Error::other(format!("ffprobe spawn failed: {e}"))))?;
    let mut stdin = child.stdin.take().expect("piped");
    let input = bytes.to_vec();
    let feeder = std::thread::spawn(move || stdin.write_all(&input));
    let out = child.wait_with_output()?;
    let _ = feeder.join();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut it = text.trim().split(',').filter_map(|t| t.trim().parse::<u32>().ok());
    match (it.next(), it.next()) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err(Error::Unsupported("an image whose size ffprobe can't read".into())),
    }
}

// ------------------------------------------------------------------- mips

fn srgb_to_linear(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(l: f32) -> u8 {
    let s = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (s.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// One level down: each pixel the linear-light average of a 2×2 block
/// (odd edges fold the last row or column in on itself).
pub fn half(src: &Rgba) -> Rgba {
    let lut: [f32; 256] = std::array::from_fn(|i| srgb_to_linear(i as u8));
    let w = (src.width / 2).max(1);
    let h = (src.height / 2).max(1);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(src.width - 1);
                let sy = (y * 2 + dy).min(src.height - 1);
                let p = src.pixel(sx, sy);
                acc[0] += lut[p[0] as usize];
                acc[1] += lut[p[1] as usize];
                acc[2] += lut[p[2] as usize];
                acc[3] += p[3] as f32 / 255.0;
            }
            pixels.push(linear_to_srgb(acc[0] * 0.25));
            pixels.push(linear_to_srgb(acc[1] * 0.25));
            pixels.push(linear_to_srgb(acc[2] * 0.25));
            pixels.push((acc[3] * 0.25 * 255.0).round() as u8);
        }
    }
    Rgba {
        width: w,
        height: h,
        pixels,
    }
}

/// The full chain, base first, down to 1×1.
pub fn mips(base: Rgba) -> Vec<Rgba> {
    let mut levels = vec![base];
    while levels.last().is_some_and(|l| l.width > 1 || l.height > 1) {
        let next = half(levels.last().expect("non-empty"));
        levels.push(next);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored-block zlib stream: no compression, just framing.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01, 0x01];
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(data.len() as u16)).to_le_bytes());
        out.extend_from_slice(data);
        let (mut a, mut b) = (1u32, 0u32);
        for &d in data {
            a = (a + d as u32) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in bytes {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
        out
    }

    /// A real PNG, built by hand: `w`×`h` RGBA, filter 0 on every row.
    fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut raw = Vec::new();
        for row in rgba.chunks_exact((w * 4) as usize) {
            raw.push(0);
            raw.extend_from_slice(row);
        }
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend(chunk(b"IHDR", &ihdr));
        out.extend(chunk(b"IDAT", &zlib_stored(&raw)));
        out.extend(chunk(b"IEND", &[]));
        out
    }

    #[test]
    fn png_and_jpeg_headers_give_their_size() {
        assert_eq!(dimensions(&png(7, 3, &[0; 7 * 3 * 4])), Some((7, 3)));
        // A JPEG skeleton: SOI, an APP0 to skip, then SOF0 with 480×640.
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        j.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8, 0x01, 0xE0, 0x02, 0x80]);
        assert_eq!(dimensions(&j), Some((640, 480)));
        assert_eq!(dimensions(b"RIFF....WEBP"), None);
    }

    #[test]
    fn a_hand_built_png_decodes_through_ffmpeg() {
        let px = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128];
        let img = match decode(&png(2, 2, &px)) {
            Ok(i) => i,
            Err(Error::Io(e)) => {
                eprintln!("no ffmpeg here ({e}) — skipping");
                return;
            }
            Err(e) => panic!("{e}"),
        };
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(img.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(img.pixel(1, 0), [0, 255, 0, 255]);
        assert_eq!(img.pixel(0, 1), [0, 0, 255, 255]);
        assert_eq!(img.pixel(1, 1), [255, 255, 255, 128]);
    }

    #[test]
    fn a_mip_averages_in_linear_light() {
        // Black and white average to linear 0.5, which is sRGB 188 — not
        // the 128 a byte average would give.
        let base = Rgba {
            width: 2,
            height: 2,
            pixels: vec![0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255],
        };
        let m = mips(base);
        assert_eq!(m.len(), 2);
        assert_eq!((m[1].width, m[1].height), (1, 1));
        assert_eq!(m[1].pixel(0, 0), [188, 188, 188, 255]);
    }

    #[test]
    fn the_chain_ends_at_one_by_one_and_folds_odd_edges() {
        let base = Rgba {
            width: 5,
            height: 3,
            pixels: vec![200; 5 * 3 * 4],
        };
        let m = mips(base);
        let sizes: Vec<_> = m.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, vec![(5, 3), (2, 1), (1, 1)]);
        assert_eq!(m[2].pixel(0, 0), [200, 200, 200, 200]);
    }
}
