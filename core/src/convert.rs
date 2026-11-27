//! Change a photo's format (the "file extension"): RAW, HEIC from phones, PNG… to JPG, PNG,
//! TIFF or WebP. Originals are never changed; a photo already in the format asked is copied
//! as it is (no quality lost).

use anyhow::{bail, Context, Result};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// As the camera wrote it (RAW stays RAW).
    #[default]
    Keep,
    Jpeg,
    Png,
    Tiff,
    /// Lossless WebP.
    Webp,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Keep => "",
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Tiff => "tif",
            Format::Webp => "webp",
        }
    }

    /// Whether a file of this extension already is in this format.
    fn is(self, ext: &str) -> bool {
        match self {
            Format::Keep => true,
            Format::Jpeg => matches!(ext, "jpg" | "jpeg" | "jpe" | "jfif"),
            Format::Png => ext == "png",
            Format::Tiff => matches!(ext, "tif" | "tiff"),
            Format::Webp => ext == "webp",
        }
    }
}

/// JPEG quality used when none is given.
pub const DEFAULT_QUALITY: u8 = 92;

/// The whole photo, the right way up, at full size: for RAW the camera's embedded full-size
/// preview (what the camera shows), for HEIC and the others the picture itself.
pub fn decode_full(path: &Path) -> Result<RgbImage> {
    let p = crate::preview::load(path, 1 << 20).with_context(|| format!("không đọc được {}", path.display()))?;
    Ok(p.rgb)
}

/// Encode a picture.
pub fn encode(img: &RgbImage, fmt: Format, quality: u8) -> Result<Vec<u8>> {
    let mut out = vec![];
    match fmt {
        Format::Keep => bail!("không có định dạng để chuyển"),
        Format::Jpeg => return crate::metrics::encode_jpeg(img, quality.clamp(30, 100)),
        Format::Png => img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?,
        Format::Tiff => img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Tiff)?,
        Format::Webp => image::codecs::webp::WebPEncoder::new_lossless(&mut out).encode(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)?,
    }
    Ok(out)
}

/// Cut a crop (fractions of the upright picture: x, y, width, height) out of `img`.
pub fn apply_crop(img: &RgbImage, r: crate::crop::Rect) -> RgbImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let x = (r[0] * w).round().clamp(0.0, w - 1.0) as u32;
    let y = (r[1] * h).round().clamp(0.0, h - 1.0) as u32;
    let cw = ((r[2] * w).round() as u32).clamp(1, img.width() - x);
    let ch = ((r[3] * h).round() as u32).clamp(1, img.height() - y);
    image::imageops::crop_imm(img, x, y, cw, ch).to_image()
}

/// A name in `folder` not used yet (by a file there or by this run).
pub fn free_path(folder: &Path, stem: &str, ext: &str, used: &mut HashSet<PathBuf>) -> PathBuf {
    let mut p = folder.join(format!("{stem}.{ext}"));
    let mut k = 2;
    while used.contains(&p) || p.exists() {
        p = folder.join(format!("{stem}_{k}.{ext}"));
        k += 1;
    }
    used.insert(p.clone());
    p
}

/// Write `src` into `folder` in format `fmt` (with an optional crop). Returns the new file and
/// whether it was converted (false: already in that format, copied as it is).
pub fn convert_file(src: &Path, folder: &Path, fmt: Format, quality: u8, crop: Option<crate::crop::Rect>, used: &mut HashSet<PathBuf>) -> Result<(PathBuf, bool)> {
    std::fs::create_dir_all(folder)?;
    let stem = src.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = crate::preview::ext_of(src);
    if fmt.is(&ext) && crop.is_none() {
        let dst = free_path(folder, &stem, &ext, used);
        crate::export::copy_verified(src, &dst)?;
        return Ok((dst, false));
    }
    let mut img = decode_full(src)?;
    if let Some(r) = crop {
        img = apply_crop(&img, r);
    }
    let bytes = encode(&img, fmt, quality)?;
    let dst = free_path(folder, &stem, fmt.ext(), used);
    let tmp = dst.with_extension(format!("{}.tmp", fmt.ext()));
    std::fs::write(&tmp, &bytes).with_context(|| format!("ghi {}", dst.display()))?;
    std::fs::rename(&tmp, &dst)?;
    Ok((dst, true))
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ConvertReport {
    pub out_dir: PathBuf,
    /// Photos changed to the new format, and photos already in it (copied as they are).
    pub converted: usize,
    pub copied: usize,
    pub bytes: u64,
    /// Files that are not pictures the app reads (videos, documents).
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// «Chuyển định dạng thư mục»: every picture of `src` (and its sub-folders, kept as they are)
/// into `out` in format `fmt`. One file per photo: a RAW with its camera JPEG becomes one.
pub fn convert_folder(src: &Path, out: &Path, fmt: Format, quality: u8, progress: &(dyn Fn(usize, usize) + Sync), cancel: &AtomicBool) -> Result<ConvertReport> {
    if fmt == Format::Keep {
        bail!("Chọn định dạng muốn chuyển sang");
    }
    let mut files = vec![];
    let mut skipped = 0;
    let mut stack = vec![src.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).with_context(|| format!("đọc thư mục {}", d.display()))? {
            let p = e?.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                if p != out {
                    stack.push(p);
                }
            } else if crate::preview::is_supported(&p) {
                files.push(p);
            } else {
                skipped += 1;
            }
        }
    }
    files.sort();
    // A RAW and its camera JPEG are one photo: take the JPEG (what the camera made of it).
    let mut by_stem: std::collections::BTreeMap<(PathBuf, String), Vec<PathBuf>> = Default::default();
    for f in files {
        let stem = f.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
        by_stem.entry((f.parent().unwrap_or(src).to_path_buf(), stem)).or_default().push(f);
    }
    let photos: Vec<PathBuf> = by_stem
        .into_values()
        .map(|g| g.iter().find(|f| !crate::preview::is_raw(f)).cloned().unwrap_or_else(|| g[0].clone()))
        .collect();
    let mut rep = ConvertReport { out_dir: out.to_path_buf(), skipped, ..Default::default() };
    let mut used = HashSet::new();
    let total = photos.len();
    for (k, f) in photos.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            rep.errors.push("Đã dừng theo yêu cầu".into());
            break;
        }
        progress(k, total);
        let rel = f.parent().and_then(|p| p.strip_prefix(src).ok()).unwrap_or(Path::new(""));
        match convert_file(f, &out.join(rel), fmt, quality, None, &mut used) {
            Ok((dst, conv)) => {
                rep.bytes += std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
                if conv {
                    rep.converted += 1;
                } else {
                    rep.copied += 1;
                }
            }
            Err(e) => rep.errors.push(format!("{}: {e:#}", f.display())),
        }
    }
    progress(total, total);
    Ok(rep)
}

/// Folder next to `src` for its converted copy: «<name>_JPG».
pub fn default_out(src: &Path, fmt: Format) -> PathBuf {
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "anh".into());
    src.with_file_name(format!("{name}_{}", fmt.ext().to_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_changes_format_and_originals_stay() {
        let d = std::env::temp_dir().join(format!("rawpick-conv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let src = d.join("sinh vien");
        std::fs::create_dir_all(src.join("lop A")).unwrap();
        let img = RgbImage::from_fn(300, 200, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 77]));
        img.save(src.join("a.png")).unwrap();
        img.save(src.join("lop A").join("b.bmp")).unwrap();
        std::fs::write(src.join("c.jpg"), crate::metrics::encode_jpeg(&img, 90).unwrap()).unwrap();
        std::fs::write(src.join("ghi chu.txt"), b"x").unwrap();
        let before = std::fs::read(src.join("a.png")).unwrap();
        let out = default_out(&src, Format::Jpeg);
        assert!(out.ends_with("sinh vien_JPG"));
        let rep = convert_folder(&src, &out, Format::Jpeg, 90, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((rep.converted, rep.copied, rep.skipped), (2, 1, 1), "{rep:?}");
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        let a = image::open(out.join("a.jpg")).unwrap();
        assert_eq!((a.width(), a.height()), (300, 200));
        assert!(out.join("lop A").join("b.jpg").exists(), "sub-folders kept");
        assert_eq!(std::fs::read(out.join("c.jpg")).unwrap(), std::fs::read(src.join("c.jpg")).unwrap(), "already JPG: copied as it is");
        assert_eq!(std::fs::read(src.join("a.png")).unwrap(), before, "original untouched");
        // PNG and a crop.
        let mut used = HashSet::new();
        let (p, conv) = convert_file(&src.join("c.jpg"), &d.join("png"), Format::Png, 90, Some([0.5, 0.0, 0.5, 0.5]), &mut used).unwrap();
        assert!(conv && p.extension().unwrap() == "png");
        let c = image::open(&p).unwrap();
        assert_eq!((c.width(), c.height()), (150, 100));
        for f in [Format::Tiff, Format::Webp] {
            let (p, _) = convert_file(&src.join("a.png"), &d.join("x"), f, 90, None, &mut used).unwrap();
            assert_eq!(image::open(&p).unwrap().width(), 300, "{f:?}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn phone_heic_is_told_by_its_header() {
        let mut b = vec![0, 0, 0, 24];
        b.extend(b"ftypheic");
        b.extend([0; 12]);
        assert!(crate::preview::is_heif(&b));
        assert!(!crate::preview::is_heif(b"\\xFF\\xD8\\xFF\\xE0 not heif at all"));
        assert!(crate::preview::is_supported(Path::new("IMG_0001.HEIC")) && crate::preview::is_supported(Path::new("DSCF0001.HIF")));
    }
}
