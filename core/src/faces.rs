//! People in the frame: face detection (YuNet, ~0.3 MB ONNX, runs on CPU through tract) and the
//! checks culling cares about: closed eyes, blurred / dark / turned / cut-off faces.

use crate::metrics::Gray;
use image::RgbImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawFace {
    /// Box in the coordinates of the image given to `detect`.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    /// YuNet order: right eye, left eye, nose tip, right mouth corner, left mouth corner.
    pub lm: [(f32, f32); 5],
}

fn iou(a: &RawFace, b: &RawFace) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    inter / (a.w * a.h + b.w * b.h - inter).max(1e-6)
}

pub fn nms(mut v: Vec<RawFace>, thr: f32) -> Vec<RawFace> {
    v.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let mut keep: Vec<RawFace> = vec![];
    for f in v {
        if keep.iter().all(|k| iou(k, &f) < thr) {
            keep.push(f);
        }
    }
    keep
}

/// One YuNet output tensor: its element count per anchor (1, 4 or 10) and data.
pub struct Head<'a> {
    pub per_anchor: usize,
    pub data: &'a [f32],
}

/// Decode YuNet (2023) heads: for each stride 8/16/32 there is cls (1), obj (1), bbox (4) and kps (10).
/// `size` is the square input side. Order between cls and obj does not matter (score = sqrt(cls*obj)).
pub fn decode_yunet(heads: &[Head], size: usize, thr: f32) -> Vec<RawFace> {
    let mut out = vec![];
    for stride in [8usize, 16, 32] {
        let cols = size / stride;
        let n = cols * cols;
        let pick = |k: usize| heads.iter().filter(move |h| h.per_anchor == k && h.data.len() == n * k);
        let ones: Vec<&Head> = pick(1).collect();
        let (Some(bbox), Some(kps)) = (pick(4).next(), pick(10).next()) else { continue };
        if ones.len() < 2 {
            continue;
        }
        let (cls, obj) = (ones[0].data, ones[1].data);
        for idx in 0..n {
            let s = (cls[idx].clamp(0.0, 1.0) * obj[idx].clamp(0.0, 1.0)).sqrt();
            if s < thr {
                continue;
            }
            let (r, c) = ((idx / cols) as f32, (idx % cols) as f32);
            let st = stride as f32;
            let b = &bbox.data[idx * 4..idx * 4 + 4];
            let cx = (c + b[0]) * st;
            let cy = (r + b[1]) * st;
            let w = b[2].exp() * st;
            let h = b[3].exp() * st;
            let k = &kps.data[idx * 10..idx * 10 + 10];
            let mut lm = [(0f32, 0f32); 5];
            for j in 0..5 {
                lm[j] = ((k[2 * j] + c) * st, (k[2 * j + 1] + r) * st);
            }
            out.push(RawFace { x: cx - w / 2.0, y: cy - h / 2.0, w, h, score: s, lm });
        }
    }
    nms(out, 0.3)
}

#[cfg(feature = "faces")]
pub struct FaceDetector {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
    size: usize,
}

#[cfg(feature = "faces")]
impl FaceDetector {
    pub const INPUT: usize = 640;

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use tract_onnx::prelude::*;
        let s = Self::INPUT;
        let model = tract_onnx::onnx()
            .model_for_path(path)?
            .with_input_fact(0, f32::fact([1, 3, s, s]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { model, size: s })
    }

    /// Detect faces; coordinates are returned in `img` pixels.
    pub fn detect(&self, img: &RgbImage, thr: f32) -> anyhow::Result<Vec<RawFace>> {
        use tract_onnx::prelude::*;
        let s = self.size;
        let (w, h) = img.dimensions();
        let f = s as f32 / w.max(h) as f32;
        let small = crate::metrics::resize(img, ((w as f32 * f) as u32).max(1), ((h as f32 * f) as u32).max(1));
        let (sw, sh) = (small.width() as usize, small.height() as usize);
        // BGR, 0–255, letterboxed at the top-left.
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, 3, s, s), |(_, c, y, x)| {
            if x < sw && y < sh {
                small.get_pixel(x as u32, y as u32)[2 - c] as f32
            } else {
                0.0
            }
        })
        .into();
        let outs = self.model.run(tvec!(input.into()))?;
        let views: Vec<(usize, Vec<f32>)> = outs
            .iter()
            .filter_map(|t| {
                let v = t.to_plain_array_view::<f32>().ok()?;
                let last = *v.shape().last()?;
                Some((last, v.iter().copied().collect()))
            })
            .collect();
        let heads: Vec<Head> = views.iter().map(|(k, d)| Head { per_anchor: *k, data: d }).collect();
        let faces = decode_yunet(&heads, s, thr)
            .into_iter()
            .map(|r| RawFace { x: r.x / f, y: r.y / f, w: r.w / f, h: r.h / f, score: r.score, lm: r.lm.map(|(a, b)| (a / f, b / f)) })
            .filter(|r| r.x + r.w > 0.0 && r.y + r.h > 0.0 && r.x < w as f32 && r.y < h as f32)
            .collect();
        Ok(faces)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceInfo {
    /// x, y, w, h as fractions of the image.
    pub bbox: [f32; 4],
    pub eyes: [(f32, f32); 2],
    pub eye_open: f32,
    pub closed: bool,
    pub sharp: f32,
    pub lum: f32,
    pub yaw: f32,
    pub edge: bool,
    pub small: bool,
    pub main: bool,
    pub score: f32,
    pub issues: Vec<String>,
    /// Height of the dark gap between the lips relative to mouth width: 0 closed,
    /// above ~0.25 the mouth is clearly open (speaking, shouting, laughing).
    #[serde(default)]
    pub mouth_open: f32,
    /// Eye-state network: probability the eyes are closed (both eyes averaged).
    #[serde(default)]
    pub eye_p: Option<f32>,
    /// Pixel openness of the more closed eye.
    #[serde(default)]
    pub eye_min: f32,
    /// Whether the eyes could be judged at all.
    #[serde(default)]
    pub eye_seen: bool,
    /// Combined probability of closed eyes (after Sơn's corrections).
    #[serde(default)]
    pub p_closed: f32,
    /// Sơn said himself whether these eyes are open.
    #[serde(default)]
    pub by_hand: bool,
    /// Face score before the eye check.
    #[serde(default)]
    pub base: f32,
}

impl FaceInfo {
    pub fn eye_features(&self) -> [f32; crate::eyes::N] {
        crate::eyes::features(self.eye_p, self.eye_open, self.eye_min)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct People {
    /// None when there are no faces: the criterion then does not count.
    pub score: Option<f32>,
    pub faces: Vec<FaceInfo>,
    pub closed_eyes: bool,
    pub blurred_face: bool,
    pub notes: Vec<String>,
    /// Centre of the main faces' eyes (0–1), used as the composition subject.
    pub eye_center: Option<(f32, f32)>,
}

/// Fraction of a short vertical band through the eye that is iris/pupil-dark. Landmark models
/// happily place an eye on a closed lid; the pixels do not lie: an open eye shows a tall dark iris,
/// a closed one only a thin lash line.
fn eye_open_ratio(g: &Gray, ex: f32, ey: f32, ew: f32) -> Option<f32> {
    if ew < 8.0 || ex - ew < 0.0 || ey - ew < 0.0 || ex + ew >= g.w as f32 || ey + ew >= g.h as f32 {
        return None;
    }
    let (mut ck, mut cn) = (0f32, 0f32);
    let mut y = (ey + ew * 0.55) as usize;
    while (y as f32) < ey + ew * 0.85 && y < g.h {
        let mut x = (ex - ew * 0.3) as usize;
        while (x as f32) < ex + ew * 0.3 && x < g.w {
            ck += g.at(x, y);
            cn += 1.0;
            x += 1;
        }
        y += 1;
    }
    let thr = if cn > 0.0 { ck / cn } else { 128.0 } * 0.6;
    let hh = ew * 0.35;
    let mut cols = vec![];
    let mut x = (ex - ew * 0.2).round() as i64;
    while x as f32 <= ex + ew * 0.2 {
        let mut d = 0;
        let mut tot = 0;
        let mut y = (ey - hh).round() as i64;
        while y as f32 <= ey + hh {
            if x >= 0 && y >= 0 && (x as usize) < g.w && (y as usize) < g.h {
                tot += 1;
                if g.at(x as usize, y as usize) < thr {
                    d += 1;
                }
            }
            y += 1;
        }
        if tot > 0 {
            cols.push(d as f32 / tot as f32);
        }
        x += 1;
    }
    if cols.is_empty() {
        return None;
    }
    cols.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(cols[cols.len() / 2])
}

/// How open the mouth is, from pixels: the tallest run of dark pixels (mouth cavity) in the
/// columns around the mouth centre, relative to mouth width. Median over columns.
fn mouth_open_ratio(g: &Gray, mc: (f32, f32), mw: f32, skin: f32) -> Option<f32> {
    if mw < 10.0 {
        return None;
    }
    let (x0, x1) = (mc.0 - mw * 0.22, mc.0 + mw * 0.22);
    let (y0, y1) = (mc.1 - mw * 0.35, mc.1 + mw * 0.55);
    if x0 < 0.0 || y0 < 0.0 || x1 >= g.w as f32 || y1 >= g.h as f32 {
        return None;
    }
    let dark = (skin * 0.5).max(18.0);
    let mut runs = vec![];
    let mut x = x0 as usize;
    while (x as f32) <= x1 {
        let (mut best, mut cur) = (0usize, 0usize);
        let mut y = y0 as usize;
        while (y as f32) <= y1 {
            if g.at(x, y) < dark {
                cur += 1;
                best = best.max(cur);
            } else {
                cur = 0;
            }
            y += 1;
        }
        runs.push(best as f32 / mw);
        x += 1;
    }
    runs.sort_by(f32::total_cmp);
    runs.get(runs.len() / 2).copied()
}

fn face_sharpness(g: &Gray, x0: usize, y0: usize, x1: usize, y1: usize, face_w: f32) -> (f32, f32) {
    let (mut ls, mut lq, mut lm, mut n) = (0f64, 0f64, 0f64, 0f64);
    for y in y0.max(1)..y1.min(g.h - 1) {
        for x in x0.max(1)..x1.min(g.w - 1) {
            let i = y * g.w + x;
            let p = &g.px;
            let l = (p[i - g.w] + p[i + g.w] + p[i - 1] + p[i + 1] - 4.0 * p[i]) as f64;
            ls += l;
            lq += l * l;
            lm += p[i] as f64;
            n += 1.0;
        }
    }
    if n < 1.0 {
        return (0.0, 0.0);
    }
    let sd = (lq / n - (ls / n).powi(2)).max(0.0).sqrt() as f32;
    let fix = (face_w / 160.0).max(1.0).sqrt();
    let s = (10.0 / (1.0 + (-((sd * fix).max(0.01).ln() - 5f32.ln()) * 2.4).exp())).clamp(0.0, 10.0);
    (s, (lm / n) as f32)
}

/// Optional eye-state network, only in builds with the face models.
#[cfg(feature = "faces")]
pub type EyeNet = crate::eyes::EyeModel;
#[cfg(not(feature = "faces"))]
pub struct EyeNet;

#[cfg(feature = "faces")]
fn eye_net_p(net: Option<&EyeNet>, img: &RgbImage, e: (f32, f32), side: f32) -> Option<f32> {
    net?.p_closed(img, e.0, e.1, side)
}
#[cfg(not(feature = "faces"))]
fn eye_net_p(_: Option<&EyeNet>, _: &RgbImage, _: (f32, f32), _: f32) -> Option<f32> {
    None
}

/// Measure every detected face on the high-resolution image `img`/`g`, then judge it with the
/// starting eye rule.
pub fn assess(img: &RgbImage, g: &Gray, raw: &[RawFace]) -> People {
    assess_with(img, g, raw, None)
}

pub fn assess_with(img: &RgbImage, g: &Gray, raw: &[RawFace], net: Option<&EyeNet>) -> People {
    let (w, h) = (img.width() as f32, img.height() as f32);
    if raw.is_empty() {
        return People { notes: vec!["Không thấy khuôn mặt nào, tiêu chí này không tính vào điểm".into()], ..Default::default() };
    }
    let max_area = raw.iter().map(|f| f.w * f.h).fold(0f32, f32::max);
    let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut faces: Vec<FaceInfo> = raw
        .iter()
        .map(|f| {
            let (re, le, nose) = (f.lm[0], f.lm[1], f.lm[2]);
            let iod = dist(re, le).max(1.0);
            let ew = iod * 0.47;
            let small = f.w < 36.0;
            let ratios: Vec<f32> = [re, le].iter().filter_map(|e| eye_open_ratio(g, e.0, e.1, ew)).collect();
            let avg = if ratios.is_empty() { 1.0 } else { ratios.iter().sum::<f32>() / ratios.len() as f32 };
            let min = ratios.iter().cloned().fold(1.0f32, f32::min);
            let side = f.w.max(f.h) * crate::eyes::PATCH;
            let nets: Vec<f32> = if small { vec![] } else { [re, le].iter().filter_map(|&e| eye_net_p(net, img, e, side)).collect() };
            let eye_p = (!nets.is_empty()).then(|| nets.iter().sum::<f32>() / nets.len() as f32);
            let (x0, y0) = (f.x.max(0.0) as usize, f.y.max(0.0) as usize);
            let (x1, y1) = ((f.x + f.w).min(w - 1.0) as usize, (f.y + f.h).min(h - 1.0) as usize);
            let (sharp, lum) = face_sharpness(g, x0, y0, x1, y1, f.w);
            let mid = ((re.0 + le.0) / 2.0, (re.1 + le.1) / 2.0);
            let yaw = (nose.0 - mid.0) / iod;
            let edge = f.x < w * 0.01 || f.y < h * 0.01 || f.x + f.w > w * 0.99 || f.y + f.h > h * 0.99;
            let mut issues = vec![];
            let mut s = 10.0f32;
            if sharp < 4.0 {
                s -= 4.0;
                issues.push("mặt mờ".into());
            } else if sharp < 6.5 {
                s -= 1.5;
                issues.push("mặt hơi mềm".into());
            }
            if lum < 65.0 {
                s -= 2.0;
                issues.push("mặt thiếu sáng".into());
            } else if lum > 215.0 {
                s -= 2.0;
                issues.push("mặt bị cháy sáng".into());
            }
            let mouth_open = if small {
                0.0
            } else {
                let (rm, lm) = (f.lm[3], f.lm[4]);
                mouth_open_ratio(g, ((rm.0 + lm.0) / 2.0, (rm.1 + lm.1) / 2.0), dist(rm, lm), lum).unwrap_or(0.0)
            };
            if mouth_open > 0.3 {
                s -= 0.8;
                issues.push("đang há miệng".into());
            }
            if yaw.abs() > 0.38 {
                // A turned face can be the best frame: no penalty, Sơn's own choices decide.
                issues.push("mặt nghiêng".into());
            }
            if edge {
                s -= 2.0;
                issues.push("mặt bị cắt mép khung".into());
            }
            FaceInfo {
                bbox: [f.x / w, f.y / h, f.w / w, f.h / h],
                eyes: [(re.0 / w, re.1 / h), (le.0 / w, le.1 / h)],
                eye_open: avg,
                closed: false,
                sharp,
                lum,
                yaw,
                edge,
                small,
                main: f.w * f.h >= max_area * 0.25,
                score: s.clamp(0.0, 10.0),
                issues,
                mouth_open,
                eye_p,
                eye_min: min,
                eye_seen: !small && (eye_p.is_some() || !ratios.is_empty()),
                p_closed: 0.0,
                by_hand: false,
                base: s,
            }
        })
        .collect();
    faces.sort_by(|a, b| a.bbox[0].partial_cmp(&b.bbox[0]).unwrap());
    judge(faces, &crate::eyes::EyeBook::default(), &|_| None)
}

/// Decide the eyes of every face (with Sơn's corrections in `book`, and his word on single faces
/// from `forced(face number)`), then score the people in the photo. Cheap: no pixels involved,
/// so it runs again whenever he corrects an eye.
pub fn judge(mut faces: Vec<FaceInfo>, book: &crate::eyes::EyeBook, forced: &dyn Fn(usize) -> Option<bool>) -> People {
    if faces.is_empty() {
        return People { notes: vec!["Không thấy khuôn mặt nào, tiêu chí này không tính vào điểm".into()], ..Default::default() };
    }
    for (i, f) in faces.iter_mut().enumerate() {
        f.issues.retain(|x| x != "nhắm mắt" && x != "mắt lim dim");
        let mut s = f.base;
        let hand = forced(i);
        f.by_hand = hand.is_some();
        f.p_closed = match hand {
            Some(c) => c as u8 as f32,
            None if f.eye_seen => book.p_closed(&f.eye_features()),
            None => 0.0,
        };
        f.closed = f.p_closed >= 0.5;
        if f.closed {
            s -= 6.0;
            f.issues.insert(0, "nhắm mắt".to_string());
        } else if hand.is_none() && f.p_closed >= 0.3 {
            s -= 2.0;
            f.issues.insert(0, "mắt lim dim".to_string());
        }
        f.score = s.clamp(0.0, 10.0);
    }
    let main: Vec<&FaceInfo> = faces.iter().filter(|f| f.main).collect();
    let mn = main.iter().map(|f| f.score).fold(10.0f32, f32::min);
    let avg = main.iter().map(|f| f.score).sum::<f32>() / main.len() as f32;
    let mut notes = vec![format!(
        "{} khuôn mặt{}",
        faces.len(),
        if faces.len() > main.len() { format!(" ({} mặt chính, còn lại ở xa)", main.len()) } else { String::new() }
    )];
    for (i, f) in main.iter().enumerate() {
        let lbl = if main.len() > 1 { format!("Người {}: ", i + 1) } else { String::new() };
        let body = if f.issues.is_empty() { "mắt mở, mặt nét, đủ sáng".to_string() } else { f.issues.join(", ") };
        let tail = if f.small {
            " (mặt nhỏ, chưa kiểm tra mắt)"
        } else if f.by_hand {
            " (mắt: theo bạn sửa)"
        } else {
            ""
        };
        notes.push(format!("{lbl}{body}{tail}"));
    }
    let ec = {
        let (mut x, mut y) = (0f32, 0f32);
        for f in &main {
            x += (f.eyes[0].0 + f.eyes[1].0) / 2.0;
            y += (f.eyes[0].1 + f.eyes[1].1) / 2.0;
        }
        (x / main.len() as f32, y / main.len() as f32)
    };
    // Audience and crowd shots: someone always blinks. Only the two biggest faces (the ones a
    // viewer looks at) decide, and the score leans on the average rather than the worst face.
    let crowd = main.len() >= 5;
    let lead: Vec<&FaceInfo> = if crowd {
        let mut by_size = main.clone();
        by_size.sort_by(|a, b| (b.bbox[2] * b.bbox[3]).total_cmp(&(a.bbox[2] * a.bbox[3])));
        by_size.truncate(2);
        by_size
    } else {
        main.clone()
    };
    if crowd {
        let blinks = main.iter().filter(|f| f.closed).count();
        if blinks > 0 && !lead.iter().any(|f| f.closed) {
            notes.push(format!("Ảnh đông người: {blinks} người ở xa nhắm mắt, không tính"));
        }
    }
    let score = if crowd { 0.3 * mn + 0.7 * avg } else { 0.6 * mn + 0.4 * avg };
    People {
        score: Some(score.clamp(0.0, 10.0)),
        closed_eyes: lead.iter().any(|f| f.closed),
        blurred_face: lead.iter().any(|f| f.sharp < 4.0),
        notes,
        eye_center: Some(ec),
        faces,
    }
}

/// Faces a viewer looks at in this photo (the ones whose closed eyes count), by face number.
pub fn lead_faces(p: &People) -> Vec<usize> {
    let main: Vec<usize> = (0..p.faces.len()).filter(|&i| p.faces[i].main).collect();
    if main.len() >= 5 {
        let mut m = main;
        m.sort_by(|&a, &b| {
            let (fa, fb) = (&p.faces[a], &p.faces[b]);
            (fb.bbox[2] * fb.bbox[3]).total_cmp(&(fa.bbox[2] * fa.bbox[3]))
        });
        m.truncate(2);
        m
    } else {
        main
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_one_anchor_and_suppresses_overlaps() {
        let size = 64;
        // stride 8 -> 8x8 = 64 anchors; strides 16/32 -> 16 and 4 anchors.
        let mk = |n: usize, k: usize| vec![0f32; n * k];
        let (mut cls8, mut obj8, mut box8, kps8) = (mk(64, 1), mk(64, 1), mk(64, 4), mk(64, 10));
        // anchor at row 2, col 3
        let idx = 2 * 8 + 3;
        cls8[idx] = 0.9;
        obj8[idx] = 0.9;
        box8[idx * 4..idx * 4 + 4].copy_from_slice(&[0.5, 0.5, (2.0f32).ln(), (2.0f32).ln()]);
        // a duplicate next to it with a lower score
        cls8[idx + 1] = 0.7;
        obj8[idx + 1] = 0.7;
        box8[(idx + 1) * 4..(idx + 1) * 4 + 4].copy_from_slice(&[-0.4, 0.5, (2.0f32).ln(), (2.0f32).ln()]);
        let (c16, o16, b16, k16) = (mk(16, 1), mk(16, 1), mk(16, 4), mk(16, 10));
        let (c32, o32, b32, k32) = (mk(4, 1), mk(4, 1), mk(4, 4), mk(4, 10));
        let heads = vec![
            Head { per_anchor: 1, data: &cls8 },
            Head { per_anchor: 1, data: &c16 },
            Head { per_anchor: 1, data: &c32 },
            Head { per_anchor: 1, data: &obj8 },
            Head { per_anchor: 1, data: &o16 },
            Head { per_anchor: 1, data: &o32 },
            Head { per_anchor: 4, data: &box8 },
            Head { per_anchor: 4, data: &b16 },
            Head { per_anchor: 4, data: &b32 },
            Head { per_anchor: 10, data: &kps8 },
            Head { per_anchor: 10, data: &k16 },
            Head { per_anchor: 10, data: &k32 },
        ];
        let f = decode_yunet(&heads, size, 0.5);
        assert_eq!(f.len(), 1);
        let f = f[0];
        assert!((f.x - 20.0).abs() < 1e-3 && (f.y - 12.0).abs() < 1e-3 && (f.w - 16.0).abs() < 1e-3, "{f:?}");
        assert!((f.score - 0.9).abs() < 1e-3);
    }

    /// Paint a face-like patch: skin, two eyes either open (dark iris disc) or closed (thin lash line).
    fn face_img(open: bool) -> RgbImage {
        let mut img = RgbImage::from_pixel(400, 400, image::Rgb([200, 160, 140]));
        for (ex, ey) in [(170.0f32, 180.0f32), (230.0, 180.0)] {
            for y in 160..200 {
                for x in (ex as i32 - 20)..(ex as i32 + 20) {
                    let (dx, dy) = (x as f32 - ex, y as f32 - ey);
                    let dark = if open { dx * dx + dy * dy < 7.0 * 7.0 } else { dy.abs() < 1.5 && dx.abs() < 13.0 };
                    if dark {
                        img.put_pixel(x as u32, y as u32, image::Rgb([40, 30, 30]));
                    }
                }
            }
        }
        img
    }

    #[test]
    fn closed_eyes_are_caught_from_pixels() {
        let lm = [(170.0, 180.0), (230.0, 180.0), (200.0, 215.0), (180.0, 240.0), (220.0, 240.0)];
        let face = RawFace { x: 130.0, y: 120.0, w: 140.0, h: 170.0, score: 0.9, lm };
        for open in [true, false] {
            let img = face_img(open);
            let p = assess(&img, &Gray::from_rgb(&img), &[face]);
            assert_eq!(p.closed_eyes, !open, "open={open} ratio={}", p.faces[0].eye_open);
        }
    }
}
