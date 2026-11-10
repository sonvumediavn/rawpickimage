//! Per-photo analysis: load the preview once, run the cheap measures, and only run face analysis
//! on photos that survive the first pass.

use crate::exif::Exif;
use crate::faces::{self, People};
use crate::metrics::{self, Color, Composition, Exposure, Gray, Noise, Sharpness, Signature};
use crate::preview::{self, SourceKind};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Criteria in display order.
pub const CRITERIA: [&str; 6] = ["exposure", "sharp", "comp", "color", "noise", "people"];

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Scores {
    pub exposure: f32,
    pub sharp: f32,
    pub comp: f32,
    pub color: f32,
    pub noise: f32,
    pub people: Option<f32>,
}

impl Scores {
    pub fn get(&self, k: &str) -> Option<f32> {
        match k {
            "exposure" => Some(self.exposure),
            "sharp" => Some(self.sharp),
            "comp" => Some(self.comp),
            "color" => Some(self.color),
            "noise" => Some(self.noise),
            "people" => self.people,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Analysis {
    /// File used for scoring (a JPEG twin is preferred over RAW: same picture, faster).
    pub path: PathBuf,
    /// All files that belong to this photo (e.g. IMG_0001.CR3 + IMG_0001.JPG + .xmp).
    pub files: Vec<PathBuf>,
    pub name: String,
    pub kind: SourceKind,
    pub exif: Exif,
    pub camera: String,
    pub time: Option<f64>,
    pub width: u32,
    pub height: u32,
    pub scores: Scores,
    pub exposure: Exposure,
    pub sharp: Sharpness,
    pub noise: Noise,
    pub color: Color,
    pub comp: Composition,
    pub people: People,
    /// Whether face analysis ran (skipped for photos already ruined in pass 1).
    pub faces_checked: bool,
    pub signature: Signature,
    pub thumb: Option<PathBuf>,
    /// How the photo looks, for learning Sơn's taste from pictures ([`crate::visual`]).
    #[serde(default, with = "crate::visual::hex")]
    pub embed: Vec<i8>,
    /// Stars given on the camera or in Lightroom (0 = none), see [`crate::rating`].
    #[serde(default)]
    pub stars: u8,
    /// Locked ("protect") on the camera.
    #[serde(default)]
    pub protected: bool,
}

pub struct Analyzer {
    #[cfg(feature = "faces")]
    pub faces: Option<faces::FaceDetector>,
    #[cfg(feature = "faces")]
    pub visual: Option<crate::visual::Embedder>,
    #[cfg(feature = "faces")]
    pub eyes: Option<crate::eyes::EyeModel>,
    pub thumbs_dir: Option<PathBuf>,
}

impl Analyzer {
    /// `face_model` is the YuNet file; the image model ([`crate::visual::MODEL`]) is looked
    /// for in the same folder.
    pub fn new(face_model: Option<&Path>, thumbs_dir: Option<PathBuf>) -> (Self, Option<String>) {
        #[cfg(feature = "faces")]
        {
            let visual = face_model
                .and_then(|p| p.parent())
                .map(|d| d.join(crate::visual::MODEL))
                .filter(|p| p.exists())
                .and_then(|p| crate::visual::Embedder::load(&p).ok());
            let (det, warn) = match face_model {
                Some(p) if p.exists() => match faces::FaceDetector::load(p) {
                    Ok(d) => (Some(d), None),
                    Err(e) => (None, Some(format!("Không nạp được mô hình khuôn mặt: {e}"))),
                },
                _ => (None, Some("Chưa có mô hình nhận diện khuôn mặt, tiêu chí «Người» tạm bỏ qua".to_string())),
            };
            let eyes = face_model
                .and_then(|p| p.parent())
                .map(|d| d.join(crate::eyes::MODEL))
                .filter(|p| p.exists())
                .and_then(|p| crate::eyes::EyeModel::load(&p).ok());
            (Self { faces: det, visual, eyes, thumbs_dir }, warn)
        }
        #[cfg(not(feature = "faces"))]
        {
            let _ = face_model;
            (Self { thumbs_dir }, Some("Bản dựng không có nhận diện khuôn mặt".to_string()))
        }
    }

    pub fn analyze(&self, path: &Path, files: Vec<PathBuf>, _idx: usize) -> Result<Analysis> {
        // One decode at >=1280 px long side (DCT-scaled), everything else derives from it.
        let pv = preview::load(path, 1280)?;
        let hi = pv.rgb;
        let a640 = metrics::resize_long(&hi, 640);
        let g640 = Gray::from_rgb(&a640);
        let ghi = Gray::from_rgb(&hi);

        let mut exposure = metrics::exposure(&g640);
        let sig_hi = metrics::noise_sigma(&ghi);
        // Downscaling averages noise away roughly in proportion to the scale factor.
        let sig_native = sig_hi / pv.scale.max(0.05);
        let noise = metrics::noise(sig_native, pv.exif.iso);
        let sharp = metrics::sharpness(&ghi, sig_hi);
        let color = metrics::color(&a640, &g640);
        let mut comp = metrics::composition(&a640, &g640);
        let signature = metrics::signature(&a640, &g640);

        // Pass 2 (faces) only for photos that are not already ruined. Exposure doesn't count:
        // a blown projector screen behind a well-lit speaker is a normal event photo.
        let ruined = sharp.score < 2.5;
        let mut people = People::default();
        let mut faces_checked = false;
        #[cfg(feature = "faces")]
        if let (Some(det), false) = (&self.faces, ruined) {
            if let Ok(raw) = det.detect(&hi, 0.6) {
                people = faces::assess_with(&hi, &ghi, &raw, self.eyes.as_ref());
                faces_checked = true;
            }
        }
        let _ = ruined;
        subject_exposure(&mut exposure, &people);
        if let Some((ex, ey)) = people.eye_center {
            // With people in frame, the subject is their eyes.
            let (p3, pc, pos) = metrics::placement(ex, ey);
            let pc_portrait = (1.0 - (ex - 0.5).abs() / 0.12).clamp(0.0, 1.0) * (1.0 - (ey - 1.0 / 3.0).abs() / 0.15).clamp(0.0, 1.0) * 0.9;
            let pos = pos.max(pc_portrait);
            comp.score = (comp.score + 7.0 * (pos - comp.pos)).clamp(0.0, 10.0);
            comp.pos = pos;
            comp.cx = ex;
            comp.cy = ey;
            comp.notes.retain(|n| !n.starts_with("Chủ thể") && !n.starts_with("Không có chủ thể"));
            comp.notes.insert(
                0,
                if p3 >= pc.max(pc_portrait) && p3 > 0.55 {
                    "Mắt người nằm gần điểm mạnh 1/3"
                } else if pc_portrait > 0.5 || pc > 0.5 {
                    "Chân dung cân giữa, mắt ở khoảng 1/3 trên"
                } else {
                    "Vị trí mắt chưa theo 1/3, thử crop lại"
                }
                .into(),
            );
        }
        // Sharpest tile far from the subject: focus probably landed on the background.
        let t = metrics::SHARP_GRID as f32;
        let (bx, by) = (((sharp.best % metrics::SHARP_GRID) as f32 + 0.5) / t, ((sharp.best / metrics::SHARP_GRID) as f32 + 0.5) / t);
        let mut sharp = sharp;
        if sharp.score >= 5.0 && ((bx - comp.cx).powi(2) + (by - comp.cy).powi(2)).sqrt() > 0.38 {
            sharp.notes.push("Vùng nét nhất nằm xa chủ thể, có thể lấy nét nhầm hậu cảnh".into());
        }

        #[cfg(feature = "faces")]
        let embed = self.visual.as_ref().and_then(|v| v.embed(&hi).ok()).unwrap_or_default();
        #[cfg(not(feature = "faces"))]
        let embed = vec![];
        let thumb = self.thumbs_dir.as_ref().and_then(|d| {
            let t = metrics::resize_long(&hi, 480);
            // Named after the file, not its position, so cached thumbnails stay right
            // when the folder's contents change.
            let p = d.join(format!("{}.jpg", file_key(path)));
            std::fs::write(&p, metrics::encode_jpeg(&t, 82).ok()?).ok()?;
            Some(p)
        });

        let r1 = |v: f32| (v * 10.0).round() / 10.0;
        let scores = Scores {
            exposure: r1(exposure.score),
            sharp: r1(sharp.score),
            comp: r1(comp.score),
            color: r1(color.score),
            noise: r1(noise.score),
            people: people.score.map(r1),
        };
        Ok(Analysis {
            name: path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            path: path.to_path_buf(),
            files,
            kind: pv.kind,
            camera: pv.exif.camera_id(),
            time: pv.exif.timestamp(),
            exif: pv.exif,
            width: pv.native_w,
            height: pv.native_h,
            scores,
            exposure,
            sharp,
            noise,
            color,
            comp,
            people,
            faces_checked,
            signature,
            thumb,
            embed,
            stars: 0,
            protected: false,
        })
    }
}

/// Judge exposure on the people when there are any: blown screens, stage lights and windows
/// behind a well-exposed face are part of the event, not a mistake.
fn subject_exposure(e: &mut metrics::Exposure, p: &People) {
    let main: Vec<&faces::FaceInfo> = p.faces.iter().filter(|f| f.main && !f.small).collect();
    if main.is_empty() {
        return;
    }
    let lum = main.iter().map(|f| f.lum).sum::<f32>() / main.len() as f32;
    if !(70.0..=205.0).contains(&lum) {
        return;
    }
    // 10 at a mid-tone face, ~6.5 at the edges of the good range.
    let face_score = 10.0 - ((lum - 135.0).abs() / 70.0).powi(2) * 3.5;
    let blended = 0.25 * e.score + 0.75 * face_score;
    if blended > e.score + 0.2 {
        e.score = blended.clamp(0.0, 10.0);
        e.notes.retain(|n| !n.starts_with("Phơi sáng cân đối"));
        e.notes.insert(0, "Mặt người đủ sáng; vùng cháy sáng nằm ở nền (màn chiếu, đèn, cửa sổ) nên chỉ trừ nhẹ".into());
    }
}

/// Short stable name for cache files derived from a photo's path.
pub fn file_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("{:08x}{:08x}", crc32fast::hash(s.as_bytes()), crc32fast::hash(s.chars().rev().collect::<String>().as_bytes()))
}

/// Group files of one shot (same folder + stem): RAW + JPEG twins, XMP sidecars.
/// Returns (file used for scoring, all files), sorted by name.
pub fn collect(dir: &Path, recursive: bool) -> std::io::Result<Vec<(PathBuf, Vec<PathBuf>)>> {
    collect_with(dir, recursive, true)
}

/// Like [`collect`]; `skip_ours = false` also walks folders this app wrote (to learn from them).
pub fn collect_with(dir: &Path, recursive: bool, skip_ours: bool) -> std::io::Result<Vec<(PathBuf, Vec<PathBuf>)>> {
    let mut all = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let e = e?;
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.starts_with("._") {
                continue;
            }
            let ft = e.file_type()?;
            if ft.is_dir() {
                // Never re-scan our own output.
                let ours = name.ends_with("_da-loc") || name.ends_with(" - Anh chon") || name.ends_with(crate::export::OUT_SUFFIX) || p.join(crate::pipeline::COPY_MARKER).exists();
                if recursive && !(skip_ours && ours) {
                    stack.push(p);
                }
            } else {
                all.push(p);
            }
        }
    }
    let mut groups: std::collections::BTreeMap<(PathBuf, String), Vec<PathBuf>> = Default::default();
    for p in all {
        let ext = preview::ext_of(&p);
        if !(preview::is_supported(&p) || ext == "xmp") {
            continue;
        }
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        groups.entry((p.parent().unwrap_or(dir).to_path_buf(), stem)).or_default().push(p);
    }
    let mut out = vec![];
    for (_, mut files) in groups {
        files.sort();
        // Prefer a JPEG twin for scoring (full-size, no container to parse), else the RAW.
        let jpeg = files.iter().find(|p| matches!(preview::ext_of(p).as_str(), "jpg" | "jpeg"));
        let raw = files.iter().find(|p| preview::is_raw(p));
        let other = files.iter().find(|p| preview::is_supported(p));
        if let Some(main) = jpeg.or(raw).or(other) {
            out.push((main.clone(), files.clone()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Re-decide the eyes in an analysed photo with Sơn's corrections (no pixels needed).
pub fn rejudge(a: &mut Analysis, book: &crate::eyes::EyeBook, forced: &dyn Fn(usize) -> Option<bool>) {
    if a.people.faces.is_empty() {
        return;
    }
    let faces = std::mem::take(&mut a.people.faces);
    a.people = faces::judge(faces, book, forced);
    a.scores.people = a.people.score.map(|v| (v * 10.0).round() / 10.0);
}
