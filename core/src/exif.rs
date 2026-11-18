//! Minimal TIFF / EXIF reader.
//!
//! Most RAW formats (CR2, NEF, ARW, DNG, ORF, RW2, PEF, SRW…) are TIFF containers. We read
//! only the handful of tags the culling needs, plus the location of embedded JPEG previews,
//! so that a 30 MB RAW file costs a few small reads instead of a full read.

use serde::{Deserialize, Serialize};

/// Random-access byte source: a file on disk or a buffer in memory.
pub trait ByteSource {
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>>;
    fn size(&self) -> u64;
}

impl ByteSource for &[u8] {
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(len)?;
        self.get(start..end).map(|s| s.to_vec())
    }
    fn size(&self) -> u64 {
        self.len() as u64
    }
}

pub struct FileSource {
    file: std::fs::File,
    size: u64,
}

impl FileSource {
    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self { file, size })
    }
}

impl ByteSource for FileSource {
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        if offset.checked_add(len as u64)? > self.size {
            return None;
        }
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; len];
        self.file.read_exact(&mut buf).ok()?;
        Some(buf)
    }
    fn size(&self) -> u64 {
        self.size
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Exif {
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub lens: Option<String>,
    pub orientation: Option<u16>,
    /// "YYYY:MM:DD HH:MM:SS" as written by the camera (local camera clock).
    pub datetime: Option<String>,
    pub subsec: Option<String>,
    pub iso: Option<u32>,
    pub exposure_s: Option<f32>,
    pub fnumber: Option<f32>,
    pub focal_mm: Option<f32>,
}

impl Exif {
    /// Fill missing fields from another record (e.g. RAW container + its preview's own EXIF).
    pub fn merge_missing(&mut self, other: &Exif) {
        macro_rules! fill {
            ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = other.$f.clone(); } )* };
        }
        fill!(make, model, serial, lens, orientation, datetime, subsec, iso, exposure_s, fnumber, focal_mm);
    }

    /// Camera identity used to group files from several bodies ("Canon EOS R6 #0123").
    pub fn camera_id(&self) -> String {
        let mut s = String::new();
        if let Some(m) = &self.make {
            s.push_str(m.trim());
        }
        if let Some(m) = &self.model {
            let m = m.trim();
            // Many models already repeat the make ("Canon Canon EOS R6").
            if !s.is_empty() && m.to_lowercase().starts_with(&s.to_lowercase()) {
                s.clear();
            }
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(m);
        }
        if let Some(sn) = &self.serial {
            let sn = sn.trim();
            if !sn.is_empty() {
                s.push_str(" #");
                s.push_str(&sn[sn.len().saturating_sub(4)..]);
            }
        }
        if s.is_empty() {
            "Không rõ máy".to_string()
        } else {
            s
        }
    }

    /// Seconds since 2000-01-01 on the camera clock, with sub-second precision when present.
    pub fn timestamp(&self) -> Option<f64> {
        let dt = self.datetime.as_deref()?;
        let b = dt.as_bytes();
        if b.len() < 19 {
            return None;
        }
        let num = |a: usize, n: usize| -> Option<i64> { std::str::from_utf8(&b[a..a + n]).ok()?.trim().parse().ok() };
        let (y, mo, d, h, mi, s) = (num(0, 4)?, num(5, 2)?, num(8, 2)?, num(11, 2)?, num(14, 2)?, num(17, 2)?);
        if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
            return None;
        }
        let days = days_from_civil(y, mo, d) - days_from_civil(2000, 1, 1);
        let mut t = (days * 86400 + h * 3600 + mi * 60 + s) as f64;
        if let Some(ss) = &self.subsec {
            let digits: String = ss.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                if let Ok(v) = digits.parse::<f64>() {
                    t += v / 10f64.powi(digits.len() as i32);
                }
            }
        }
        Some(t)
    }
}

/// Howard Hinnant's days-from-civil.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// A JPEG stream stored inside a TIFF container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Default)]
pub struct TiffInfo {
    pub exif: Exif,
    pub jpegs: Vec<JpegRange>,
}

struct Reader<'a> {
    src: &'a mut dyn ByteSource,
    base: u64,
    le: bool,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, off: u64, len: usize) -> Option<Vec<u8>> {
        self.src.read_at(self.base + off, len)
    }
    fn u16(&mut self, off: u64) -> Option<u16> {
        let b = self.bytes(off, 2)?;
        Some(if self.le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    }
    fn u32(&mut self, off: u64) -> Option<u32> {
        let b = self.bytes(off, 4)?;
        let a = [b[0], b[1], b[2], b[3]];
        Some(if self.le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) })
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    tag: u16,
    typ: u16,
    count: u32,
    /// Offset (relative to the TIFF header) of the value or of the 4-byte inline field.
    value_at: u64,
}

fn type_size(t: u16) -> u32 {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => 1,
    }
}

impl Entry {
    fn read_u32s(&self, r: &mut Reader, max: usize) -> Vec<u32> {
        let n = (self.count as usize).min(max);
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let x = match self.typ {
                3 => r.u16(self.value_at + 2 * i as u64).map(u32::from),
                4 | 13 => r.u32(self.value_at + 4 * i as u64),
                1 | 7 => r.bytes(self.value_at + i as u64, 1).map(|b| b[0] as u32),
                _ => None,
            };
            match x {
                Some(x) => v.push(x),
                None => break,
            }
        }
        v
    }
    fn first_u32(&self, r: &mut Reader) -> Option<u32> {
        self.read_u32s(r, 1).first().copied()
    }
    fn rational(&self, r: &mut Reader) -> Option<f32> {
        if self.typ != 5 && self.typ != 10 {
            return self.first_u32(r).map(|v| v as f32);
        }
        let a = r.u32(self.value_at)?;
        let b = r.u32(self.value_at + 4)?;
        if b == 0 {
            return None;
        }
        Some(if self.typ == 10 { (a as i32) as f32 / (b as i32) as f32 } else { a as f32 / b as f32 })
    }
    fn string(&self, r: &mut Reader) -> Option<String> {
        if self.typ != 2 {
            return None;
        }
        let b = r.bytes(self.value_at, (self.count as usize).min(128))?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        let s = String::from_utf8_lossy(&b[..end]).trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

fn read_ifd(r: &mut Reader, off: u64) -> Option<(Vec<Entry>, u32)> {
    let n = r.u16(off)? as u64;
    if n == 0 || n > 1000 {
        return None;
    }
    let raw = r.bytes(off + 2, (n * 12) as usize)?;
    let le = r.le;
    let g16 = |b: &[u8]| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let g32 = |b: &[u8]| if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) };
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n as usize {
        let e = &raw[i * 12..i * 12 + 12];
        let (tag, typ, count) = (g16(&e[0..2]), g16(&e[2..4]), g32(&e[4..8]));
        let size = type_size(typ) as u64 * count as u64;
        let field_at = off + 2 + (i as u64) * 12 + 8;
        let value_at = if size <= 4 { field_at } else { g32(&e[8..12]) as u64 };
        out.push(Entry { tag, typ, count, value_at });
    }
    let next = r.u32(off + 2 + n * 12).unwrap_or(0);
    Some((out, next))
}

/// Parse a TIFF structure that starts at `base` inside `src`.
pub fn parse_tiff(src: &mut dyn ByteSource, base: u64) -> Option<TiffInfo> {
    let hdr = src.read_at(base, 8)?;
    let le = match &hdr[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    // Magic is 42 for TIFF, but ORF ("RO"/"SR") and RW2 (0x55) use their own; accept them.
    let mut r = Reader { src, base, le };
    let first = r.u32(4)? as u64;
    let mut info = TiffInfo::default();
    let mut queue: Vec<(u64, bool)> = vec![(first, true)];
    let mut seen = std::collections::HashSet::new();
    let mut exif_ifd = None;
    while let Some((off, is_chain)) = queue.pop() {
        if off < 8 || !seen.insert(off) || seen.len() > 64 {
            continue;
        }
        let Some((entries, next)) = read_ifd(&mut r, off) else { continue };
        let mut jpeg_off = None;
        let mut jpeg_len = None;
        let mut compression = None;
        let mut photometric = None;
        let mut strips: Vec<u32> = vec![];
        let mut strip_counts: Vec<u32> = vec![];
        for e in &entries {
            match e.tag {
                0x010F => info.exif.make = info.exif.make.take().or_else(|| e.string(&mut r)),
                0x0110 => info.exif.model = info.exif.model.take().or_else(|| e.string(&mut r)),
                0x0112 if info.exif.orientation.is_none() && seen.len() == 1 => {
                    info.exif.orientation = e.first_u32(&mut r).map(|v| v as u16)
                }
                0x0103 => compression = e.first_u32(&mut r),
                0x0106 => photometric = e.first_u32(&mut r),
                0x0111 => strips = e.read_u32s(&mut r, 4),
                0x0117 => strip_counts = e.read_u32s(&mut r, 4),
                0x0201 => jpeg_off = e.first_u32(&mut r),
                0x0202 => jpeg_len = e.first_u32(&mut r),
                0x014A => {
                    for s in e.read_u32s(&mut r, 8) {
                        queue.push((s as u64, false));
                    }
                }
                0x8769 => exif_ifd = e.first_u32(&mut r).map(|v| v as u64),
                _ => {}
            }
        }
        if let (Some(o), Some(l)) = (jpeg_off, jpeg_len) {
            if l > 1024 {
                info.jpegs.push(JpegRange { offset: base + o as u64, len: l as u64 });
            }
        }
        // A single JPEG-compressed strip that is not CFA sensor data (DNG previews, some NEF/PEF).
        let is_sensor = matches!(photometric, Some(32803) | Some(34892));
        if matches!(compression, Some(6) | Some(7)) && !is_sensor && strips.len() == 1 && strip_counts.len() == 1 && strip_counts[0] > 1024 {
            info.jpegs.push(JpegRange { offset: base + strips[0] as u64, len: strip_counts[0] as u64 });
        }
        if is_chain && next != 0 {
            queue.push((next as u64, true));
        }
    }
    if let Some(off) = exif_ifd {
        if let Some((entries, _)) = read_ifd(&mut r, off) {
            for e in &entries {
                match e.tag {
                    0x829A => info.exif.exposure_s = e.rational(&mut r),
                    0x829D => info.exif.fnumber = e.rational(&mut r),
                    0x8827 => info.exif.iso = e.first_u32(&mut r),
                    0x9003 => info.exif.datetime = e.string(&mut r),
                    0x9291 => info.exif.subsec = e.string(&mut r),
                    0x920A => info.exif.focal_mm = e.rational(&mut r),
                    0xA431 => info.exif.serial = e.string(&mut r),
                    0xA434 => info.exif.lens = e.string(&mut r),
                    _ => {}
                }
            }
        }
    }
    Some(info)
}

/// EXIF from a JPEG's APP1 segment.
pub fn jpeg_exif(jpeg: &[u8]) -> Option<Exif> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut p = 2usize;
    while p + 4 < jpeg.len() {
        if jpeg[p] != 0xFF {
            return None;
        }
        let m = jpeg[p + 1];
        if m == 0xDA || m == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([jpeg[p + 2], jpeg[p + 3]]) as usize;
        if m == 0xE1 && jpeg.get(p + 4..p + 10) == Some(b"Exif\0\0") {
            let mut s: &[u8] = jpeg;
            return parse_tiff(&mut s, (p + 10) as u64).map(|i| i.exif);
        }
        p += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a little-endian TIFF with IFD0 (make, orientation, preview pointer) and an EXIF IFD.
    pub fn tiny_tiff() -> Vec<u8> {
        let mut b = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
        // IFD0 at 8 with 4 entries
        let entries: Vec<(u16, u16, u32, u32)> = vec![
            (0x010F, 2, 6, 200),  // Make -> "Canon\0" at 200
            (0x0112, 3, 1, 6),    // Orientation 6
            (0x0201, 4, 1, 300),  // preview offset
            (0x0202, 4, 1, 2000), // preview length
            (0x8769, 4, 1, 100),  // EXIF IFD at 100
        ];
        b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (t, ty, c, v) in &entries {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&ty.to_le_bytes());
            b.extend_from_slice(&c.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        b.resize(100, 0);
        // EXIF IFD: ISO 3200, DateTimeOriginal
        b.extend_from_slice(&2u16.to_le_bytes());
        for (t, ty, c, v) in [(0x8827u16, 3u16, 1u32, 3200u32), (0x9003, 2, 20, 240)] {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&ty.to_le_bytes());
            b.extend_from_slice(&c.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        b.resize(200, 0);
        b.extend_from_slice(b"Canon\0");
        b.resize(240, 0);
        b.extend_from_slice(b"2026:10:31 14:05:09\0");
        b.resize(2400, 0);
        b
    }

    #[test]
    fn reads_tags_and_preview_pointer() {
        let t = tiny_tiff();
        let mut s: &[u8] = &t;
        let info = parse_tiff(&mut s, 0).unwrap();
        assert_eq!(info.exif.make.as_deref(), Some("Canon"));
        assert_eq!(info.exif.orientation, Some(6));
        assert_eq!(info.exif.iso, Some(3200));
        assert_eq!(info.jpegs, vec![JpegRange { offset: 300, len: 2000 }]);
        assert!(info.exif.timestamp().is_some());
    }

    #[test]
    fn timestamps_order_and_subseconds() {
        let a = Exif { datetime: Some("2026:10:31 14:05:09".into()), subsec: Some("25".into()), ..Default::default() };
        let b = Exif { datetime: Some("2026:10:31 14:05:10".into()), ..Default::default() };
        let (ta, tb) = (a.timestamp().unwrap(), b.timestamp().unwrap());
        assert!((tb - ta - 0.75).abs() < 1e-6);
    }

    #[test]
    fn camera_id_avoids_duplicate_make() {
        let e = Exif { make: Some("Canon".into()), model: Some("Canon EOS R6".into()), serial: Some("012345678".into()), ..Default::default() };
        assert_eq!(e.camera_id(), "Canon EOS R6 #5678");
    }
}
