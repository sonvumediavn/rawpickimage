//! Stars given on the camera (or later in Lightroom / Bridge), and the camera's "protect" lock.
//!
//! Cameras store the star rating in different places, so this looks everywhere it can be:
//! - the XMP packet inside the file (Canon CR3/JPEG, Nikon NEF/JPEG, Fujifilm, Lightroom
//!   exports): `xmp:Rating="3"` or `<xmp:Rating>3</xmp:Rating>`;
//! - the EXIF/TIFF Rating tag 0x4746 in the first IFD (Sony ARW/JPEG, Windows);
//! - an `.xmp` sidecar next to the file (Lightroom, Capture One, Photo Mechanic);
//! - "protect" on the camera marks the file read-only on the card, which the copy keeps.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Where stars hide: within the first part of the file.
const SCAN: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rating {
    /// 0 when unrated (−1 "rejected" in XMP also counts as 0).
    pub stars: u8,
    /// Locked on the camera (read-only file).
    pub protected: bool,
}

/// Rating of a photo made of `files` (RAW, its JPEG twin, sidecars): the highest stars found.
pub fn read(files: &[PathBuf]) -> Rating {
    let mut r = Rating::default();
    for f in files {
        let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let stars = if ext == "xmp" { std::fs::read(f).ok().and_then(|b| xmp_rating(&b)) } else { file_stars(f) };
        r.stars = r.stars.max(stars.unwrap_or(0));
        if ext != "xmp" && std::fs::metadata(f).is_ok_and(|m| m.permissions().readonly()) {
            r.protected = true;
        }
    }
    r
}

fn file_stars(path: &Path) -> Option<u8> {
    let mut buf = Vec::with_capacity(SCAN);
    std::fs::File::open(path).ok()?.take(SCAN as u64).read_to_end(&mut buf).ok()?;
    let x = xmp_rating(&buf);
    let t = tiff_rating(&buf);
    match (x, t) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0).max(b.unwrap_or(0))),
    }
}

/// `xmp:Rating` in an XMP packet (attribute or element form).
pub fn xmp_rating(b: &[u8]) -> Option<u8> {
    let mut best: Option<u8> = None;
    let needle = b"xmp:Rating";
    let mut i = 0;
    while let Some(p) = find(&b[i..], needle) {
        let at = i + p + needle.len();
        // Skip `="`, `='` or `>` and spaces, then read a number (may be -1).
        let rest = &b[at..b.len().min(at + 16)];
        let mut k = 0;
        while k < rest.len() && matches!(rest[k], b'=' | b'"' | b'\'' | b'>' | b' ') {
            k += 1;
        }
        let neg = rest.get(k) == Some(&b'-');
        if let Some(d) = rest.get(k + neg as usize).filter(|c| c.is_ascii_digit()) {
            let v = if neg { 0 } else { (d - b'0').min(5) };
            best = Some(best.unwrap_or(0).max(v));
        }
        i = at;
    }
    best
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// Rating tag 0x4746 in the first IFD of a TIFF-based file, or of a JPEG's EXIF block.
fn tiff_rating(b: &[u8]) -> Option<u8> {
    let base = if b.starts_with(&[0xFF, 0xD8]) {
        // JPEG: find the APP1 "Exif\0\0" header.
        find(&b[..b.len().min(128 * 1024)], b"Exif\0\0")? + 6
    } else {
        0
    };
    let t = b.get(base..)?;
    let le = match t.get(..4)? {
        [b'I', b'I', _, _] => true,
        [b'M', b'M', _, _] => false,
        _ => return None,
    };
    let u16_at = |o: usize| -> Option<u16> {
        let s = t.get(o..o + 2)?;
        Some(if le { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) })
    };
    let u32_at = |o: usize| -> Option<u32> {
        let s = t.get(o..o + 4)?;
        Some(if le { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) } else { u32::from_be_bytes([s[0], s[1], s[2], s[3]]) })
    };
    let ifd = u32_at(4)? as usize;
    let n = u16_at(ifd)? as usize;
    for k in 0..n.min(512) {
        let e = ifd + 2 + k * 12;
        if u16_at(e)? == 0x4746 {
            // SHORT, value stored inline.
            return Some((u16_at(e + 8)? as u8).min(5));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_xmp_stars_in_both_forms() {
        assert_eq!(xmp_rating(br#"<rdf:Description xmp:Rating="3" xmp:Label="Red"/>"#), Some(3));
        assert_eq!(xmp_rating(b"<xmp:Rating>5</xmp:Rating>"), Some(5));
        assert_eq!(xmp_rating(br#"xmp:Rating="-1""#), Some(0));
        assert_eq!(xmp_rating(b"no rating here"), None);
    }

    #[test]
    fn finds_the_tiff_rating_tag() {
        // Little-endian TIFF, IFD0 at 8 with two entries: Orientation and Rating = 4.
        let mut b = b"II*\0\x08\0\0\0".to_vec();
        b.extend([2, 0]);
        b.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
        b.extend([0x46, 0x47, 3, 0, 1, 0, 0, 0, 4, 0, 0, 0]);
        b.extend([0, 0, 0, 0]);
        assert_eq!(tiff_rating(&b), Some(4));
    }

    #[test]
    fn reads_files_sidecars_and_the_protect_lock() {
        let d = tempfile::tempdir().unwrap();
        let raw = d.path().join("IMG_1.CR3");
        let side = d.path().join("IMG_1.xmp");
        let other = d.path().join("IMG_2.JPG");
        std::fs::write(&raw, b"....ftypcrx ... <x:xmpmeta> xmp:Rating=\"2\" </x:xmpmeta>").unwrap();
        std::fs::write(&side, br#"<x:xmpmeta><rdf:Description xmp:Rating="4"/></x:xmpmeta>"#).unwrap();
        std::fs::write(&other, b"\xFF\xD8 nothing").unwrap();
        assert_eq!(read(&[raw.clone(), side]).stars, 4);
        assert_eq!(read(&[raw]).stars, 2);
        assert_eq!(read(&[other.clone()]), Rating::default());
        let mut perm = std::fs::metadata(&other).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&other, perm).unwrap();
        assert!(read(&[other.clone()]).protected);
        let mut perm = std::fs::metadata(&other).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&other, perm).unwrap();
    }
}
