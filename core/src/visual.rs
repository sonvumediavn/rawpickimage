//! How a photo *looks*, as a short vector: MobileNetV2 (ImageNet, ~14 MB ONNX, CPU through
//! tract) pooled features, projected to [`DIM`] numbers and stored as signed bytes.
//!
//! Two photos that look alike (same kind of moment, framing, light, gesture) get vectors that
//! point the same way. The learner compares a new photo with the photos Sơn kept and dropped
//! before, so the AI learns his taste from the pictures themselves, not only from measurements.

use image::RgbImage;
use std::sync::OnceLock;

/// File name of the model inside the app's `models/` folder.
pub const MODEL: &str = "mobilenetv2-12.onnx";
/// Length of a stored vector.
pub const DIM: usize = 128;
/// Width of the network's pooled features.
const FEAT: usize = 1280;
const INPUT: usize = 224;

/// A fixed random projection 1280 → 128 (same on every computer, so vectors from the Windows
/// and the Mac machine can be compared).
fn projection() -> &'static Vec<f32> {
    static P: OnceLock<Vec<f32>> = OnceLock::new();
    P.get_or_init(|| {
        let mut s: u64 = 0x5eed_50f0_u64;
        let mut next = || {
            // xorshift64*
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
        };
        (0..FEAT * DIM)
            .map(|_| {
                // Box–Muller
                let (u, v) = (next().max(1e-12), next());
                ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
            })
            .collect()
    })
}

/// Project, normalise and quantise pooled features.
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

/// Cosine similarity of two stored vectors (both already normalised), −1…1.
pub fn similarity(a: &[i8], b: &[i8]) -> f32 {
    if a.len() != DIM || b.len() != DIM {
        return 0.0;
    }
    let dot: i32 = a.iter().zip(b).map(|(x, y)| *x as i32 * *y as i32).sum();
    dot as f32 / (127.0 * 127.0)
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
        // The pooled features just before the ImageNet classifier.
        let pool = m.node_by_name("GlobalAveragePool_97")?.id;
        m.select_output_outlets(&[OutletId::new(pool, 0)])?;
        let model = m
            .with_input_fact(0, f32::fact([1, 3, INPUT, INPUT]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { model })
    }

    /// The whole frame squeezed to 224×224 (composition matters, so no crop).
    pub fn embed(&self, img: &RgbImage) -> anyhow::Result<Vec<i8>> {
        use tract_onnx::prelude::*;
        let small = image::imageops::resize(img, INPUT as u32, INPUT as u32, image::imageops::FilterType::Triangle);
        const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
        const STD: [f32; 3] = [0.229, 0.224, 0.225];
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, 3, INPUT, INPUT), |(_, c, y, x)| {
            (small.get_pixel(x as u32, y as u32)[c] as f32 / 255.0 - MEAN[c]) / STD[c]
        })
        .into();
        let out = self.model.run(tvec!(input.into()))?;
        let v = out[0].to_plain_array_view::<f32>()?;
        let feat: Vec<f32> = v.iter().copied().collect();
        anyhow::ensure!(feat.len() == FEAT, "mô hình hình ảnh trả về {} số thay vì {FEAT}", feat.len());
        Ok(compress(&feat))
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
