//! «Chế độ sự kiện trực tiếp»: while an event is going on, photos keep arriving in a folder
//! (card copies, the camera's Wi-Fi transfer). The app sees new photos, picks the good ones,
//! and «Đăng nhanh» writes each new pick once as a light JPEG the size Facebook keeps
//! (2048 px long side) into a Google Drive folder, so Sơn posts them from his phone minutes
//! later. Originals are never changed.

use anyhow::{Context, Result};
use image::RgbImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

/// Long side Facebook shows without shrinking again.
pub const FACEBOOK_LONG: u32 = 2048;
/// JPEG quality for posting: no visible loss, about 1 MB a photo.
pub const POST_QUALITY: u8 = 90;
/// Folder (inside the posting folder) for one show.
pub const POST_SUFFIX: &str = " - dang ngay";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostOptions {
    /// Long side in pixels; 0 keeps the full size.
    pub long_side: u32,
    pub quality: u8,
}

impl Default for PostOptions {
    fn default() -> Self {
        Self { long_side: FACEBOOK_LONG, quality: POST_QUALITY }
    }
}

/// One photo to post.
#[derive(Debug, Clone)]
pub struct Shot {
    /// Files of the photo (RAW, its camera JPEG…): the camera JPEG is used when there is one.
    pub files: Vec<PathBuf>,
    /// Capture time (seconds since 2000-01-01, the camera's clock), to name files in order.
    pub time: Option<f64>,
    /// Sơn's crop: fractions of the upright photo.
    pub crop: Option<crate::crop::Rect>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PostReport {
    pub dir: PathBuf,
    /// Written now, in capture order.
    pub written: Vec<PathBuf>,
    /// Which of the shots given were written (their place in the list).
    pub done: Vec<usize>,
    pub bytes: u64,
    pub errors: Vec<String>,
}

/// The file of a photo to read: the camera's JPEG (its colours, as the camera made them), else
/// the RAW's full-size preview, else whatever picture there is.
pub fn source_of(files: &[PathBuf]) -> Option<&PathBuf> {
    files
        .iter()
        .find(|f| crate::preview::is_supported(f) && !crate::preview::is_raw(f))
        .or_else(|| files.iter().find(|f| crate::preview::is_raw(f)))
        .or_else(|| files.iter().find(|f| crate::preview::is_supported(f)))
}

/// The photo ready to post: upright, cropped, no larger than `long_side`.
pub fn prepare(src: &Path, crop: Option<crate::crop::Rect>, opts: &PostOptions) -> Result<RgbImage> {
    let mut img = crate::convert::decode_full(src)?;
    if let Some(r) = crop {
        img = crate::convert::apply_crop(&img, r);
    }
    let (w, h) = img.dimensions();
    let long = w.max(h);
    if opts.long_side > 0 && long > opts.long_side {
        let k = opts.long_side as f32 / long as f32;
        let (nw, nh) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
        img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::CatmullRom);
    }
    Ok(img)
}

/// «HHMMSS_<name>»: the phone lists the photos in the order they were taken, whichever
/// camera took them.
fn stem_for(shot: &Shot, src: &Path) -> String {
    let stem = src.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    match shot.time {
        Some(t) if t.is_finite() && t >= 0.0 => {
            let s = (t as i64).rem_euclid(86_400);
            format!("{:02}{:02}{:02}_{stem}", s / 3600, s / 60 % 60, s % 60)
        }
        _ => stem,
    }
}

/// Write each shot into `dir` as a JPEG ready to post.
pub fn post(shots: &[Shot], dir: &Path, opts: &PostOptions, progress: &(dyn Fn(usize, usize) + Sync), cancel: &AtomicBool) -> Result<PostReport> {
    std::fs::create_dir_all(dir).with_context(|| format!("tạo thư mục {}", dir.display()))?;
    let mut used = HashSet::new();
    // Names first (one thread), pictures in parallel.
    let mut jobs = vec![];
    let mut rep = PostReport { dir: dir.to_path_buf(), ..Default::default() };
    for (i, s) in shots.iter().enumerate() {
        let Some(src) = source_of(&s.files) else {
            rep.errors.push("ảnh không có file đọc được".into());
            continue;
        };
        let dst = crate::convert::free_path(dir, &stem_for(s, src), "jpg", &mut used);
        jobs.push((i, s, src.clone(), dst));
    }
    let total = jobs.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    progress(0, total);
    let results: Vec<(usize, Result<u64>)> = jobs
        .par_iter()
        .enumerate()
        .map(|(k, (_, s, src, dst))| {
            if cancel.load(Ordering::Relaxed) {
                return (k, Err(anyhow::anyhow!("Đã dừng")));
            }
            let r = (|| -> Result<u64> {
                let img = prepare(src, s.crop, opts)?;
                let bytes = crate::metrics::encode_jpeg(&img, opts.quality.clamp(60, 100))?;
                let tmp = dst.with_extension("jpg.tmp");
                std::fs::write(&tmp, &bytes).with_context(|| format!("ghi {}", dst.display()))?;
                std::fs::rename(&tmp, dst)?;
                Ok(bytes.len() as u64)
            })()
            .with_context(|| src.display().to_string());
            progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            (k, r)
        })
        .collect();
    for (k, r) in results {
        match r {
            Ok(b) => {
                rep.bytes += b;
                rep.written.push(jobs[k].3.clone());
                rep.done.push(jobs[k].0);
            }
            Err(e) => rep.errors.push(format!("{e:#}")),
        }
    }
    Ok(rep)
}

/// What a folder holds, to notice new photos: how many picture files, their total size and the
/// newest change. Equal signatures: nothing new.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub files: usize,
    pub bytes: u64,
    /// Newest modification, seconds since 1970.
    pub newest: u64,
}

impl Signature {
    /// Seconds since the newest file last changed (a file still being copied changes).
    pub fn quiet_for(&self) -> u64 {
        now_secs().saturating_sub(self.newest)
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The signature of the photos the app would analyse in `dir`.
pub fn signature(dir: &Path, recursive: bool) -> Option<Signature> {
    let groups = crate::analyze::collect(dir, recursive).ok()?;
    let mut s = Signature::default();
    for (_, files) in groups {
        for f in files {
            if let Ok(m) = std::fs::metadata(&f) {
                s.files += 1;
                s.bytes += m.len();
                let t = m.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                s.newest = s.newest.max(t);
            }
        }
    }
    Some(s)
}

/// Whether the folder changed and has settled: `now` differs from what was last analysed, and
/// equals the previous look (`seen`) with nothing written for `quiet` seconds (no file still
/// being copied).
pub fn ready_to_analyse(now: Signature, analysed: Option<Signature>, seen: Option<Signature>, quiet: u64) -> bool {
    Some(now) != analysed && Some(now) == seen && now.files > 0 && now.quiet_for() >= quiet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_become_light_photos_in_capture_order() {
        let d = std::env::temp_dir().join(format!("rawpick-post-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let img = RgbImage::from_fn(4000, 3000, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        let a = d.join("IMG_0002.png");
        img.save(&a).unwrap();
        let small = RgbImage::from_fn(1200, 1600, |x, _| image::Rgb([(x % 256) as u8, 10, 10]));
        let b = d.join("IMG_0001.jpg");
        std::fs::write(&b, crate::metrics::encode_jpeg(&small, 90).unwrap()).unwrap();
        let before = std::fs::read(&a).unwrap();
        let shots = vec![
            Shot { files: vec![a.clone()], time: Some(19.0 * 3600.0 + 42.0 * 60.0 + 7.0), crop: None },
            Shot { files: vec![b.clone()], time: Some(19.0 * 3600.0 + 5.0), crop: Some([0.0, 0.0, 0.5, 0.5]) },
        ];
        let out = d.join("Le tot nghiep - dang ngay");
        let rep = post(&shots, &out, &PostOptions::default(), &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(rep.done, vec![0, 1]);
        let big = image::open(out.join("194207_IMG_0002.jpg")).unwrap();
        assert_eq!((big.width(), big.height()), (2048, 1536), "shrunk to Facebook's size");
        let cut = image::open(out.join("190005_IMG_0001.jpg")).unwrap();
        assert_eq!((cut.width(), cut.height()), (600, 800), "crop kept, small photo not enlarged");
        assert_eq!(std::fs::read(&a).unwrap(), before, "original untouched");
        // Posting again never overwrites what was already sent.
        let rep = post(&shots[..1], &out, &PostOptions::default(), &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert!(rep.written[0].ends_with("194207_IMG_0002_2.jpg"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn new_photos_are_noticed_once_they_settle() {
        let d = std::env::temp_dir().join(format!("rawpick-sig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let empty = signature(&d, true).unwrap();
        assert_eq!(empty.files, 0);
        assert!(!ready_to_analyse(empty, None, Some(empty), 0), "nothing to look at");
        std::fs::write(d.join("a.jpg"), b"x").unwrap();
        let one = signature(&d, true).unwrap();
        assert!(!ready_to_analyse(one, Some(empty), Some(empty), 0), "just changed: wait one more look");
        assert!(ready_to_analyse(one, Some(empty), Some(one), 0), "same twice: go");
        assert!(!ready_to_analyse(one, Some(empty), Some(one), 3600), "still being written");
        assert!(!ready_to_analyse(one, Some(one), Some(one), 0), "already analysed");
        std::fs::write(d.join("notes.txt"), b"x").unwrap();
        assert_eq!(signature(&d, true).unwrap(), one, "only photos count");
        let _ = std::fs::remove_dir_all(&d);
    }
}
