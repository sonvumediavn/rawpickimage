//! Who is who: a face "fingerprint" so frames of different people on the same stage (student
//! after student receiving a diploma from the same leader, group after group) are never taken
//! for one shot.
//!
//! SFace (OpenCV Zoo, Apache-2.0) turns a face, aligned on its five YuNet landmarks to 112×112,
//! into 128 numbers; two faces of one person point the same way (cosine above ~0.36), two
//! people do not. Runs on the CPU through tract (about 45 ms a face), only for the largest
//! main faces of a photo.

/// File name of the face-recognition model inside `models/`.
pub const MODEL: &str = "face_recognition_sface_2021dec.onnx";
/// Fingerprint length.
pub const DIM: usize = 128;
/// Most faces fingerprinted per photo (the largest ones): enough to tell groups apart, cheap on
/// a crowd.
pub const MAX_FACES: usize = 6;
/// Faces narrower than this (pixels in the 1280 px analysis image) carry too little detail.
pub const MIN_SIDE: f32 = 40.0;

/// Cosine similarity of two fingerprints (stored at unit length × 127).
pub fn similarity(a: &[i8], b: &[i8]) -> Option<f32> {
    if a.len() != DIM || b.len() != DIM {
        return None;
    }
    let dot: i32 = a.iter().zip(b).map(|(&x, &y)| x as i32 * y as i32).sum();
    Some(dot as f32 / (127.0 * 127.0))
}

/// Above this, surely the same person (OpenCV's threshold for SFace is 0.363). Measured: one
/// person in different photos 0.73–0.96, different people 0.06–0.25.
pub const SAME_PERSON: f32 = 0.45;
/// Below this, surely another person; in between the faces are not compared.
pub const OTHER_PERSON: f32 = 0.3;

/// Where SFace wants the five landmarks in its 112×112 input (right eye, left eye, nose tip,
/// right and left mouth corner, as YuNet gives them).
const TEMPLATE: [(f32, f32); 5] = [(38.2946, 51.6963), (73.5318, 51.5014), (56.0252, 71.7366), (41.5493, 92.3655), (70.7299, 92.2041)];

/// Similarity transform (scale, rotation, shift) mapping `src` onto `dst` in the least-squares
/// sense: returns [a, b, tx, ty] for x' = a·x − b·y + tx, y' = b·x + a·y + ty.
fn similarity_fit(src: &[(f32, f32); 5], dst: &[(f32, f32); 5]) -> [f32; 4] {
    let n = 5.0;
    let (mut sx, mut sy, mut dx, mut dy) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..5 {
        sx += src[i].0;
        sy += src[i].1;
        dx += dst[i].0;
        dy += dst[i].1;
    }
    let (sx, sy, dx, dy) = (sx / n, sy / n, dx / n, dy / n);
    let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
    for i in 0..5 {
        let (x, y) = (src[i].0 - sx, src[i].1 - sy);
        let (u, v) = (dst[i].0 - dx, dst[i].1 - dy);
        num_a += x * u + y * v;
        num_b += x * v - y * u;
        den += x * x + y * y;
    }
    let den = den.max(1e-6);
    let (a, b) = (num_a / den, num_b / den);
    [a, b, dx - (a * sx - b * sy), dy - (b * sx + a * sy)]
}

/// The face cut out and straightened to SFace's 112×112 layout (bilinear sampling).
pub fn align(img: &image::RgbImage, lm: &[(f32, f32); 5]) -> image::RgbImage {
    // Fit output -> source directly, so every output pixel looks up where it comes from.
    let m = similarity_fit(&TEMPLATE, lm);
    let (w, h) = (img.width() as f32, img.height() as f32);
    image::RgbImage::from_fn(112, 112, |ox, oy| {
        let (u, v) = (ox as f32, oy as f32);
        let x = (m[0] * u - m[1] * v + m[2]).clamp(0.0, w - 1.001);
        let y = (m[1] * u + m[0] * v + m[3]).clamp(0.0, h - 1.001);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |xx: u32, yy: u32| img.get_pixel(xx.min(img.width() - 1), yy.min(img.height() - 1)).0;
        let (a, b, c, d) = (p(x0, y0), p(x0 + 1, y0), p(x0, y0 + 1), p(x0 + 1, y0 + 1));
        let mut out = [0u8; 3];
        for k in 0..3 {
            let top = a[k] as f32 * (1.0 - fx) + b[k] as f32 * fx;
            let bot = c[k] as f32 * (1.0 - fx) + d[k] as f32 * fx;
            out[k] = (top * (1.0 - fy) + bot * fy).round() as u8;
        }
        image::Rgb(out)
    })
}

/// Unit length × 127, as stored in the analysis cache.
pub fn quantise(v: &[f32]) -> Option<Vec<i8>> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !(n.is_finite() && n > 1e-6) || v.len() != DIM {
        return None;
    }
    Some(v.iter().map(|x| (x / n * 127.0).round().clamp(-127.0, 127.0) as i8).collect())
}

#[cfg(feature = "faces")]
pub struct FaceId {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
}

#[cfg(feature = "faces")]
impl FaceId {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use tract_onnx::prelude::*;
        let model = tract_onnx::onnx().model_for_path(path)?.with_input_fact(0, f32::fact([1, 3, 112, 112]).into())?.into_optimized()?.into_runnable()?;
        Ok(Self { model })
    }

    /// Fingerprint of the face with these landmarks (pixels of `img`).
    pub fn describe(&self, img: &image::RgbImage, lm: &[(f32, f32); 5]) -> Option<Vec<i8>> {
        use tract_onnx::prelude::*;
        let face = align(img, lm);
        // RGB, 0..255, as OpenCV's FaceRecognizerSF feeds it.
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, 3, 112, 112), |(_, c, y, x)| face.get_pixel(x as u32, y as u32)[c] as f32).into();
        let out = self.model.run(tvec!(input.into())).ok()?;
        let v = out[0].to_plain_array_view::<f32>().ok()?;
        let v: Vec<f32> = v.iter().copied().collect();
        quantise(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_the_template_onto_itself_and_a_scaled_copy() {
        let m = similarity_fit(&TEMPLATE, &TEMPLATE);
        assert!((m[0] - 1.0).abs() < 1e-4 && m[1].abs() < 1e-4 && m[2].abs() < 1e-3 && m[3].abs() < 1e-3, "{m:?}");
        let big: Vec<(f32, f32)> = TEMPLATE.iter().map(|&(x, y)| (x * 3.0 + 100.0, y * 3.0 + 50.0)).collect();
        let big: [(f32, f32); 5] = big.try_into().unwrap();
        let m = similarity_fit(&TEMPLATE, &big);
        assert!((m[0] - 3.0).abs() < 1e-3 && m[1].abs() < 1e-3 && (m[2] - 100.0).abs() < 0.01 && (m[3] - 50.0).abs() < 0.01, "{m:?}");
    }

    #[test]
    fn similarity_of_quantised_vectors() {
        let a: Vec<f32> = (0..DIM).map(|i| (i as f32).sin()).collect();
        let b: Vec<f32> = (0..DIM).map(|i| (i as f32 * 1.7).cos()).collect();
        let (qa, qb) = (quantise(&a).unwrap(), quantise(&b).unwrap());
        assert!(similarity(&qa, &qa).unwrap() > 0.99);
        assert!(similarity(&qa, &qb).unwrap().abs() < 0.3);
        assert!(similarity(&qa, &qa[..10]).is_none());
    }
}
