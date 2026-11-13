//! How a photo *looks*, as a short vector: MobileNetV2 (ImageNet, ~14 MB ONNX, CPU through
//! tract) pooled features, projected to [`DIM`] numbers and stored as signed bytes.
//!
//! Two photos that look alike (same kind of moment, framing, light, gesture) get vectors that
//! point the same way.
//!
//! Since v4 a vector has three parts: what is in the picture (whole frame, [`DIM`] numbers),
//! what is where (the four quarters of the frame, [`REG`] numbers) and where the eye goes (a 7×7
//! map of where the picture is busiest: subject position and size, [`MAP`] numbers). The last
//! two are how the AI remembers Sơn's framing and camera angle. The learner compares a new photo with the photos Sơn kept and dropped
//! before, so the AI learns his taste from the pictures themselves, not only from measurements.

use image::RgbImage;
use std::sync::OnceLock;

/// File name of the model inside the app's `models/` folder.
pub const MODEL: &str = "mobilenetv2-12.onnx";
/// Length of the whole-frame part (and of vectors stored before v4).
pub const DIM: usize = 128;
/// Length of the quarters part.
pub const REG: usize = 96;
/// Cells of the "where the eye goes" map.
pub const MAP: usize = 49;
/// Length of a full vector.
pub const FULL: usize = DIM + REG + MAP;
/// Side of the network's last feature grid (224 / 32).
const GRID: usize = 7;
/// Width of the network's pooled features.
const FEAT: usize = 1280;
const INPUT: usize = 224;

/// A fixed random projection 1280 → 128 (same on every computer, so vectors from the Windows
/// and the Mac machine can be compared).
fn projection() -> &'static Vec<f32> {
    static P: OnceLock<Vec<f32>> = OnceLock::new();
    P.get_or_init(|| gaussian(0x5eed_50f0_u64, FEAT * DIM))
}

/// Fixed random projection for the quarters part (4 × 1280 → [`REG`]).
fn projection_regions() -> &'static Vec<f32> {
    static P: OnceLock<Vec<f32>> = OnceLock::new();
    P.get_or_init(|| gaussian(0x0b5e_55ed_u64, 4 * FEAT * REG))
}

fn gaussian(seed: u64, n: usize) -> Vec<f32> {
    let mut s = seed;
    let mut next = || {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        (s.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..n)
        .map(|_| {
            let (u, v) = (next().max(1e-12), next());
            ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
        })
        .collect()
}

/// Softmax over the 1000 ImageNet classes, the likeliest (at least 2 %), at most 8.
pub fn top_classes(logits: &[f32]) -> Vec<(u16, u8)> {
    if logits.len() != 1000 {
        return vec![];
    }
    let m = logits.iter().cloned().fold(f32::MIN, f32::max);
    let e: Vec<f32> = logits.iter().map(|x| (x - m).exp()).collect();
    let sum: f32 = e.iter().sum();
    let mut v: Vec<(u16, u8)> = e.iter().enumerate().map(|(i, x)| (i as u16, (x / sum * 100.0).round() as u8)).filter(|&(_, p)| p >= 2).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(8);
    v
}

/// Normalise to unit length and quantise.
fn quantise(v: &[f32]) -> Vec<i8> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    v.iter().map(|x| (x / norm * 127.0).round().clamp(-127.0, 127.0) as i8).collect()
}

/// Square root, centre, project.
fn project(f: &[f32], p: &[f32], out_dim: usize) -> Vec<f32> {
    let f: Vec<f32> = f.iter().map(|v| v.max(0.0).sqrt()).collect();
    let mean = f.iter().sum::<f32>() / f.len().max(1) as f32;
    let mut out = vec![0f32; out_dim];
    for (i, v) in f.iter().enumerate() {
        let c = v - mean;
        if c == 0.0 {
            continue;
        }
        for (o, r) in out.iter_mut().zip(&p[i * out_dim..(i + 1) * out_dim]) {
            *o += c * r;
        }
    }
    out
}

/// The full vector from the network's last feature grid (`FEAT` channels × 7 × 7, channel-major).
pub fn compress_grid(grid: &[f32]) -> Vec<i8> {
    let cells = GRID * GRID;
    debug_assert_eq!(grid.len(), FEAT * cells);
    let at = |c: usize, y: usize, x: usize| grid[c * cells + y * GRID + x];
    // Whole frame: the same average the classifier uses (comparable with pre-v4 vectors).
    let global: Vec<f32> = (0..FEAT).map(|c| (0..cells).map(|k| grid[c * cells + k]).sum::<f32>() / cells as f32).collect();
    let mut out = compress(&global);
    // Four overlapping quarters (rows/cols 0–3 and 3–6).
    let mut quarters = Vec::with_capacity(4 * FEAT);
    for (y0, x0) in [(0, 0), (0, 3), (3, 0), (3, 3)] {
        for c in 0..FEAT {
            let mut v = 0f32;
            for y in y0..y0 + 4 {
                for x in x0..x0 + 4 {
                    v += at(c, y, x);
                }
            }
            quarters.push(v / 16.0);
        }
    }
    out.extend(quantise(&project(&quarters, projection_regions(), REG)));
    // Where the picture is busiest: total (square-rooted) activation per cell, centred.
    let mut map: Vec<f32> = (0..cells).map(|k| (0..FEAT).map(|c| grid[c * cells + k].max(0.0).sqrt()).sum::<f32>()).collect();
    let m = map.iter().sum::<f32>() / cells as f32;
    for v in map.iter_mut() {
        *v -= m;
    }
    out.extend(quantise(&map));
    out
}

/// A vector this version can compare (whole-frame only, or full).
pub fn usable(e: &[i8]) -> bool {
    e.len() == DIM || e.len() == FULL
}

/// Project, normalise and quantise pooled features (the whole-frame part).
pub fn compress(feat: &[f32]) -> Vec<i8> {
    let p = projection();
    // Square root first: pooled activations are heavy-tailed, this keeps one strong channel
    // from deciding everything.
    let f: Vec<f32> = feat.iter().map(|v| v.max(0.0).sqrt()).collect();
    let mean = f.iter().sum::<f32>() / f.len().max(1) as f32;
    let mut out = vec![0f32; DIM];
    for (i, v) in f.iter().enumerate().take(FEAT) {
        let c = v - mean;
        if c == 0.0 {
            continue;
        }
        let row = &p[i * DIM..(i + 1) * DIM];
        for (o, r) in out.iter_mut().zip(row) {
            *o += c * r;
        }
    }
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    out.iter().map(|v| (v / norm * 127.0).round().clamp(-127.0, 127.0) as i8).collect()
}

fn cos(a: &[i8], b: &[i8]) -> f32 {
    let dot: i32 = a.iter().zip(b).map(|(x, y)| *x as i32 * *y as i32).sum();
    dot as f32 / (127.0 * 127.0)
}

/// How alike two photos look, −1…1: content, framing and where the subject sits. Falls back to
/// content alone when either vector is from before v4.
pub fn similarity(a: &[i8], b: &[i8]) -> f32 {
    if !usable(a) || !usable(b) {
        return 0.0;
    }
    let g = cos(&a[..DIM], &b[..DIM]);
    if a.len() != FULL || b.len() != FULL {
        return g;
    }
    let r = cos(&a[DIM..DIM + REG], &b[DIM..DIM + REG]);
    let m = cos(&a[DIM + REG..], &b[DIM + REG..]);
    0.45 * g + 0.35 * r + 0.2 * m
}

/// Content alone (whole frame): for telling copies of one picture apart from other pictures.
pub fn similarity_content(a: &[i8], b: &[i8]) -> f32 {
    if !usable(a) || !usable(b) {
        return 0.0;
    }
    cos(&a[..DIM], &b[..DIM])
}

/// Serde helper: vectors are saved as hex text (compact, and JSON stays readable).
pub mod hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[i8], s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(v.len() * 2);
        for b in v {
            out.push_str(&format!("{:02x}", *b as u8));
        }
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i8>, D::Error> {
        let s = String::deserialize(d)?;
        let b = s.as_bytes();
        if b.len() % 2 != 0 {
            return Ok(vec![]);
        }
        let val = |c: u8| (c as char).to_digit(16);
        Ok(b.chunks(2).map_while(|p| Some(((val(p[0])? << 4) | val(p[1])?) as u8 as i8)).collect())
    }
}

#[cfg(feature = "faces")]
pub struct Embedder {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
}

#[cfg(feature = "faces")]
impl Embedder {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use tract_onnx::prelude::*;
        let mut m = tract_onnx::onnx().model_for_path(path)?;
        // The 7×7 feature grid that feeds the classifier's global average (kept whole, so
        // where things are in the frame is not averaged away).
        let grid = m.node_by_name("GlobalAveragePool_97")?.inputs[0];
        // And the ImageNet classes (microphone, stage, academic gown…) for sorting photos by kind.
        let classes = m.output_outlets()?[0];
        m.select_output_outlets(&[grid, classes])?;
        let model = m
            .with_input_fact(0, f32::fact([1, 3, INPUT, INPUT]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { model })
    }

    /// The whole frame squeezed to 224×224 (composition matters, so no crop).
    pub fn embed(&self, img: &RgbImage) -> anyhow::Result<Vec<i8>> {
        Ok(self.look(img)?.0)
    }

    /// The look vector and the likeliest ImageNet classes as (class, percent), at least 2 %.
    pub fn look(&self, img: &RgbImage) -> anyhow::Result<(Vec<i8>, Vec<(u16, u8)>)> {
        use tract_onnx::prelude::*;
        let small = crate::metrics::resize(img, INPUT as u32, INPUT as u32);
        const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
        const STD: [f32; 3] = [0.229, 0.224, 0.225];
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, 3, INPUT, INPUT), |(_, c, y, x)| {
            (small.get_pixel(x as u32, y as u32)[c] as f32 / 255.0 - MEAN[c]) / STD[c]
        })
        .into();
        let out = self.model.run(tvec!(input.into()))?;
        let v = out[0].to_plain_array_view::<f32>()?;
        let grid: Vec<f32> = v.iter().copied().collect();
        anyhow::ensure!(grid.len() == FEAT * GRID * GRID, "mô hình hình ảnh trả về {} số thay vì {}", grid.len(), FEAT * GRID * GRID);
        let logits: Vec<f32> = out.get(1).and_then(|t| t.to_plain_array_view::<f32>().ok()).map(|v| v.iter().copied().collect()).unwrap_or_default();
        Ok((compress_grid(&grid), top_classes(&logits)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_features_stay_similar() {
        let a: Vec<f32> = (0..FEAT).map(|i| ((i * 37) % 101) as f32 / 10.0).collect();
        let mut b = a.clone();
        for v in b.iter_mut().step_by(7) {
            *v *= 1.1;
        }
        let c: Vec<f32> = (0..FEAT).map(|i| ((i * 53 + 11) % 97) as f32 / 10.0).collect();
        let (ea, eb, ec) = (compress(&a), compress(&b), compress(&c));
        assert_eq!(ea.len(), DIM);
        assert!(similarity(&ea, &ea) > 0.99);
        assert!(similarity(&ea, &eb) > 0.9, "{}", similarity(&ea, &eb));
        assert!(similarity(&ea, &ec) < 0.5, "{}", similarity(&ea, &ec));
    }

    #[test]
    fn layout_tells_where_the_subject_is() {
        // Same content, subject on the left vs on the right.
        let mk = |left: bool| -> Vec<f32> {
            let mut g = vec![0.1f32; FEAT * GRID * GRID];
            for c in 0..FEAT {
                for y in 2..5 {
                    for x in 0..GRID {
                        let on = if left { x < 3 } else { x > 3 };
                        if on {
                            g[c * 49 + y * GRID + x] = ((c * 37) % 101) as f32 / 10.0;
                        }
                    }
                }
            }
            g
        };
        let (l, l2, r) = (compress_grid(&mk(true)), compress_grid(&mk(true)), compress_grid(&mk(false)));
        assert_eq!(l.len(), FULL);
        assert!(similarity(&l, &l2) > 0.99);
        assert!(similarity_content(&l, &r) > 0.95, "{}", similarity_content(&l, &r));
        assert!(similarity(&l, &r) < similarity_content(&l, &r) - 0.2, "{} vs {}", similarity(&l, &r), similarity_content(&l, &r));
        // Old whole-frame vectors still compare on content.
        assert!(similarity(&l[..DIM], &r) > 0.95);
    }

    #[test]
    fn hex_round_trip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct W(#[serde(with = "hex")] Vec<i8>);
        let v = vec![-127i8, -1, 0, 5, 127];
        let s = serde_json::to_string(&W(v.clone())).unwrap();
        assert_eq!(s, "\"81ff00057f\"");
        assert_eq!(serde_json::from_str::<W>(&s).unwrap().0, v);
    }

    #[cfg(feature = "faces")]
    #[test]
    fn real_model_tells_photos_apart() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../models").join(MODEL);
        if !p.exists() {
            eprintln!("bỏ qua: chưa có {MODEL}");
            return;
        }
        let e = Embedder::load(&p).unwrap();
        let grad = RgbImage::from_fn(640, 480, |x, y| image::Rgb([(x / 3) as u8, (y / 2) as u8, 128]));
        let grad2 = RgbImage::from_fn(640, 480, |x, y| image::Rgb([(x / 3) as u8 + 4, (y / 2) as u8, 132]));
        let checks = RgbImage::from_fn(640, 480, |x, y| if (x / 40 + y / 40) % 2 == 0 { image::Rgb([250, 250, 250]) } else { image::Rgb([10, 10, 10]) });
        let (a, b, c) = (e.embed(&grad).unwrap(), e.embed(&grad2).unwrap(), e.embed(&checks).unwrap());
        assert!(similarity(&a, &b) > similarity(&a, &c), "{} vs {}", similarity(&a, &b), similarity(&a, &c));
    }
}
