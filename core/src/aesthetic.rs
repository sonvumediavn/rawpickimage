//! How beautiful the whole picture looks to people in general: NIMA (Neural Image Assessment,
//! Talebi & Milanfar, Google 2017) on MobileNet, trained by idealo on the AVA photo-contest set
//! (250 000 photos, each rated by ~200 people), Apache-2.0, converted to ONNX (13 MB).
//!
//! It gives a distribution of votes 1–10; its mean is the "beauty" of the picture as a whole:
//! light, colour, balance, depth, the subject standing out. It knows nothing of the event or of
//! Sơn's taste, so it only adds a little to the score and is mostly one more thing the learned
//! profile weighs.

/// File name of the model inside `models/`.
pub const MODEL: &str = "nima_mobilenet_aesthetic.onnx";
const SIDE: usize = 224;

/// NIMA mean (1–10, ordinary photos 3.5–6) to the app's 0–10 scale.
pub fn to_score(mean: f32) -> f32 {
    ((mean - 3.0) / 3.0 * 10.0).clamp(0.0, 10.0)
}

#[cfg(feature = "faces")]
pub struct AestheticModel {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
}

#[cfg(feature = "faces")]
impl AestheticModel {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use tract_onnx::prelude::*;
        let model = tract_onnx::onnx().model_for_path(path)?.with_input_fact(0, f32::fact([1, SIDE, SIDE, 3]).into())?.into_optimized()?.into_runnable()?;
        Ok(Self { model })
    }

    /// Mean vote (1–10) for the picture, as the whole frame squeezed to 224 × 224 (as trained).
    pub fn mean(&self, img: &image::RgbImage) -> Option<f32> {
        use tract_onnx::prelude::*;
        let small = crate::metrics::resize(img, SIDE as u32, SIDE as u32);
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, SIDE, SIDE, 3), |(_, y, x, c)| small.get_pixel(x as u32, y as u32)[c] as f32 / 127.5 - 1.0).into();
        let out = self.model.run(tvec!(input.into())).ok()?;
        let d: Vec<f32> = out[0].to_plain_array_view::<f32>().ok()?.iter().copied().collect();
        (d.len() == 10).then(|| d.iter().enumerate().map(|(i, p)| (i + 1) as f32 * p).sum())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scale() {
        assert_eq!(super::to_score(3.0), 0.0);
        assert!((super::to_score(4.5) - 5.0).abs() < 1e-4);
        assert_eq!(super::to_score(7.0), 10.0);
    }
}
