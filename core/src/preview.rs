//! Get a decoded, upright image for scoring without demosaicing RAW data.
//!
//! Cameras embed a JPEG preview (often full size) in every RAW file. Decoding it with DCT
//! scaling (1/2, 1/4, 1/8 straight from the JPEG coefficients) is tens of times faster than
//! a RAW decode, which is what makes culling fast on old CPUs.

use crate::exif::{self, ByteSource, Exif, FileSource, JpegRange};
use anyhow::{anyhow, bail, Context, Result};
use image::{imageops, RgbImage};
use std::path::Path;

pub const RAW_EXTS: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "raf", "orf", "rw2", "pef", "srw", "3fr", "iiq", "erf", "kdc", "mrw", "x3f", "rwl",
];
pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff", "webp"];

pub fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}
pub fn is_raw(p: &Path) -> bool {
    RAW_EXTS.contains(&ext_of(p).as_str())
}
pub fn is_supported(p: &Path) -> bool {
    let e = ext_of(p);
    RAW_EXTS.contains(&e.as_str()) || IMAGE_EXTS.contains(&e.as_str())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceKind {
    RawPreview,
    Jpeg,
    #[default]
    Other,
}

pub struct Preview {
    /// Upright RGB image, long side >= the requested size when the source allows it.
    pub rgb: RgbImage,
    /// decoded pixels / native preview pixels (<= 1).
    pub scale: f32,
    pub native_w: u32,
    pub native_h: u32,
    pub exif: Exif,
    pub kind: SourceKind,
}

/// Header facts about one JPEG stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegHeader {
    pub w: u32,
    pub h: u32,
    pub end: usize,
}

/// Walk a JPEG's segments from SOI to EOI. Rejects lossless JPEG (the compressed sensor data
/// inside CR2/DNG uses SOF3), which ordinary decoders cannot open.
pub fn parse_jpeg(buf: &[u8], start: usize) -> Option<JpegHeader> {
    let n = buf.len();
    if start + 3 >= n || buf[start] != 0xFF || buf[start + 1] != 0xD8 {
        return None;
    }
    let (mut p, mut w, mut h, mut sof) = (start + 2, 0u32, 0u32, None::<u8>);
    let mut segs = 0;
    while p + 1 < n && segs < 500 {
        segs += 1;
        if buf[p] != 0xFF {
            return None;
        }
        let mut m = buf[p + 1];
        while m == 0xFF && p + 2 < n {
            p += 1;
            m = buf[p + 1];
        }
        p += 2;
        if m == 0xD9 {
            let ok = w >= 64 && h >= 64 && !matches!(sof, Some(3) | Some(7) | Some(11) | Some(15));
            return ok.then_some(JpegHeader { w, h, end: p });
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            continue;
        }
        if m < 0xC0 || p + 2 > n {
            return None;
        }
        let len = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
        if len < 2 {
            return None;
        }
        if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC && p + 7 < n {
            sof = Some(m - 0xC0);
            h = u16::from_be_bytes([buf[p + 3], buf[p + 4]]) as u32;
            w = u16::from_be_bytes([buf[p + 5], buf[p + 6]]) as u32;
        }
        p += len;
        if m == 0xDA {
            // Entropy-coded data: skip to the next real marker.
            while p + 1 < n {
                if buf[p] == 0xFF {
                    let nx = buf[p + 1];
                    if nx == 0xFF {
                        p += 1;
                        continue;
                    }
                    if nx == 0x00 || (0xD0..=0xD7).contains(&nx) {
                        p += 2;
                        continue;
                    }
                    break;
                }
                p += 1;
            }
        }
    }
    None
}

/// Find every decodable JPEG inside an arbitrary container (CR3, RAF, unknown layouts).
pub fn scan_jpegs(buf: &[u8]) -> Vec<(usize, JpegHeader)> {
    let mut out = vec![];
    let mut i = 0usize;
    while i + 3 < buf.len() {
        if buf[i] == 0xFF && buf[i + 1] == 0xD8 && buf[i + 2] == 0xFF {
            if let Some(h) = parse_jpeg(buf, i) {
                out.push((i, h));
                i = h.end;
                if out.len() > 30 {
                    break;
                }
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Bytes of the largest embedded JPEG plus the container's EXIF.
fn raw_preview_bytes(path: &Path) -> Result<(Vec<u8>, Exif)> {
    let mut src = FileSource::open(path).with_context(|| format!("không mở được {}", path.display()))?;
    let head = src.read_at(0, 16.min(src.size() as usize)).unwrap_or_default();
    let mut exif = Exif::default();
    let mut ranges: Vec<JpegRange> = vec![];

    if head.starts_with(b"FUJIFILMCCD-RAW") {
        // RAF: big-endian JPEG offset/length at 84/88.
        if let Some(b) = src.read_at(84, 8) {
            let off = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64;
            let len = u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as u64;
            ranges.push(JpegRange { offset: off, len });
        }
    } else if let Some(info) = exif::parse_tiff(&mut src, 0) {
        exif = info.exif;
        ranges = info.jpegs;
    }

    let mut best: Option<(u64, Vec<u8>)> = None;
    for r in ranges {
        if r.len > 64 * 1024 * 1024 {
            continue;
        }
        let Some(bytes) = src.read_at(r.offset, r.len as usize) else { continue };
        if let Some(h) = parse_jpeg(&bytes, 0) {
            let area = h.w as u64 * h.h as u64;
            if best.as_ref().map_or(true, |(a, _)| area > *a) {
                let mut b = bytes;
                b.truncate(h.end);
                best = Some((area, b));
            }
        }
    }
    if best.is_none() {
        // Unknown layout: read it all and scan.
        let all = std::fs::read(path)?;
        let found = scan_jpegs(&all);
        if let Some((s, h)) = found.into_iter().max_by_key(|(_, h)| h.w as u64 * h.h as u64) {
            best = Some((0, all[s..h.end].to_vec()));
        }
    }
    let (_, bytes) = best.ok_or_else(|| anyhow!("không tìm thấy ảnh xem trước nhúng trong file RAW"))?;
    Ok((bytes, exif))
}

/// Decode a JPEG so that its long side is at least `min_long` (DCT scaling when larger).
fn decode_jpeg_scaled(bytes: &[u8], min_long: u32) -> Result<(RgbImage, u32, u32)> {
    let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    dec.read_info().context("ảnh xem trước bị lỗi")?;
    let info = dec.info().ok_or_else(|| anyhow!("ảnh xem trước bị lỗi"))?;
    let (nw, nh) = (info.width as u32, info.height as u32);
    let long = nw.max(nh);
    if long > min_long {
        let f = min_long as f32 / long as f32;
        let rw = ((nw as f32 * f).ceil() as u16).max(1);
        let rh = ((nh as f32 * f).ceil() as u16).max(1);
        dec.scale(rw, rh).context("không thu nhỏ được ảnh")?;
    }
    let px = dec.decode().context("không giải mã được ảnh xem trước")?;
    let info = dec.info().unwrap();
    let (w, h) = (info.width as u32, info.height as u32);
    let rgb = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => RgbImage::from_raw(w, h, px),
        jpeg_decoder::PixelFormat::L8 => RgbImage::from_raw(w, h, px.iter().flat_map(|&v| [v, v, v]).collect()),
        jpeg_decoder::PixelFormat::L16 => {
            RgbImage::from_raw(w, h, px.chunks(2).flat_map(|c| [c[0], c[0], c[0]]).collect())
        }
        jpeg_decoder::PixelFormat::CMYK32 => RgbImage::from_raw(
            w,
            h,
            px.chunks(4)
                .flat_map(|c| {
                    let k = 255 - c[3] as u32;
                    [(255 - c[0] as u32) * k / 255, (255 - c[1] as u32) * k / 255, (255 - c[2] as u32) * k / 255].map(|v| v as u8)
                })
                .collect(),
        ),
    }
    .ok_or_else(|| anyhow!("kích thước ảnh không hợp lệ"))?;
    Ok((rgb, nw, nh))
}

pub fn orient(img: RgbImage, o: u16) -> RgbImage {
    match o {
        2 => imageops::flip_horizontal(&img),
        3 => imageops::rotate180(&img),
        4 => imageops::flip_vertical(&img),
        5 => imageops::flip_horizontal(&imageops::rotate90(&img)),
        6 => imageops::rotate90(&img),
        7 => imageops::flip_horizontal(&imageops::rotate270(&img)),
        8 => imageops::rotate270(&img),
        _ => img,
    }
}

/// Load an upright preview whose long side is at least `min_long` pixels when possible.
pub fn load(path: &Path, min_long: u32) -> Result<Preview> {
    let raw = is_raw(path);
    let (bytes, mut exif, kind) = if raw {
        let (b, e) = raw_preview_bytes(path)?;
        (b, e, SourceKind::RawPreview)
    } else {
        let b = std::fs::read(path).with_context(|| format!("không đọc được {}", path.display()))?;
        let k = if b.starts_with(&[0xFF, 0xD8]) { SourceKind::Jpeg } else { SourceKind::Other };
        (b, Exif::default(), k)
    };

    if kind == SourceKind::Other {
        let img = image::load_from_memory(&bytes).context("định dạng ảnh không hỗ trợ")?.to_rgb8();
        let (nw, nh) = img.dimensions();
        let long = nw.max(nh);
        let img = if long > min_long.saturating_mul(2) {
            let f = min_long as f32 / long as f32;
            crate::metrics::resize(&img, ((nw as f32 * f) as u32).max(1), ((nh as f32 * f) as u32).max(1))
        } else {
            img
        };
        let scale = img.width() as f32 / nw as f32;
        return Ok(Preview { rgb: img, scale, native_w: nw, native_h: nh, exif, kind });
    }

    let own = exif::jpeg_exif(&bytes).unwrap_or_default();
    // The preview's own orientation wins; otherwise the RAW container's (NEF/ARW previews carry none).
    let o = own.orientation.filter(|&o| o > 1).or(exif.orientation).unwrap_or(1);
    exif.merge_missing(&own);
    exif.orientation = Some(o);
    let (img, nw, nh) = decode_jpeg_scaled(&bytes, min_long)?;
    if img.width() == 0 || nw == 0 {
        bail!("ảnh rỗng");
    }
    let scale = img.width() as f32 / nw as f32;
    let img = orient(img, o);
    let (nw, nh) = if (5..=8).contains(&o) { (nh, nw) } else { (nw, nh) };
    Ok(Preview { rgb: img, scale, native_w: nw, native_h: nh, exif, kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut out = vec![];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img).unwrap();
        out
    }

    #[test]
    fn finds_largest_jpeg_in_container() {
        let mut blob = vec![0u8; 500];
        blob.extend(tiny_jpeg(160, 120));
        blob.extend(vec![7u8; 300]);
        let big = tiny_jpeg(640, 480);
        let at = blob.len();
        blob.extend(&big);
        blob.extend(vec![1u8; 100]);
        let found = scan_jpegs(&blob);
        assert_eq!(found.len(), 2);
        let (s, h) = found.iter().max_by_key(|(_, h)| h.w * h.h).unwrap();
        assert_eq!((*s, h.w, h.h, h.end), (at, 640, 480, at + big.len()));
    }

    #[test]
    fn dct_scaling_keeps_requested_size() {
        let j = tiny_jpeg(1600, 1200);
        let (img, nw, _) = decode_jpeg_scaled(&j, 400).unwrap();
        assert_eq!(nw, 1600);
        assert!(img.width() >= 400 && img.width() <= 800, "got {}", img.width());
    }

    #[test]
    fn orientation_6_rotates() {
        let img = RgbImage::new(30, 20);
        assert_eq!(orient(img, 6).dimensions(), (20, 30));
    }
}
