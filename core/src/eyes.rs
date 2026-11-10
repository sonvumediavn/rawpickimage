//! Open or closed eyes, decided from three signals and Sơn's own corrections.
//!
//! - A small eye-state network (OpenVINO open-closed-eye-0001, Apache-2.0, 46 KB ONNX) looks at a
//!   32×32 patch around each eye that YuNet found.
//! - The older pixel measure (how tall the dark iris is) backs it up.
//! - Every time Sơn says "this face has its eyes open / closed", the face is fixed at once and
//!   the rule that combines the signals is re-fitted to all his corrections, so the next show
//!   is judged his way. Nothing leaves the computer.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// File name of the eye-state model inside `models/`.
pub const MODEL: &str = "open_closed_eye.onnx";
/// Eye patch side as a share of the face box side (0.20 was best on driver-monitoring data;
/// wider crops that include brow and cheek do worse).
pub const PATCH: f32 = 0.20;

/// What the combiner looks at for one face.
pub const N: usize = 5;

/// Signals for one face: model probability of closed (both eyes averaged), pixel openness
/// (average and lowest eye).
pub fn features(model_p: Option<f32>, pix_avg: f32, pix_min: f32) -> [f32; N] {
    let (has, p) = match model_p {
        Some(p) => (1.0, p - 0.6),
        None => (0.0, 0.0),
    };
    [1.0, has, has * p, pix_avg - 0.2, pix_min - 0.14]
}

/// The starting rule, as weights over [`features`]: with the network, it leads and the pixels
/// only tip the balance; without it, the old pixel thresholds.
const PRIOR: [f32; N] = [0.0, 0.0, 9.0, -14.0, -4.0];

fn sigmoid(z: f32) -> f32 {
    1.0 / (1.0 + (-z).exp())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EyeSample {
    pub x: [f32; N],
    pub closed: bool,
    /// Said by Sơn about this face (true), or guessed from him picking the photo (false).
    pub manual: bool,
    /// Machine that recorded it (None: this one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// Sơn's eye corrections and the combiner fitted to them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EyeBook {
    /// Keyed by `<photo path>#<face number>`.
    pub samples: BTreeMap<String, EyeSample>,
    #[serde(skip)]
    pub w: Option<[f32; N]>,
}

pub fn key(photo: &str, face: usize) -> String {
    format!("{photo}#{face}")
}

impl EyeBook {
    /// Probability that a face with these signals has its eyes closed.
    pub fn p_closed(&self, x: &[f32; N]) -> f32 {
        let w = self.w.unwrap_or(PRIOR);
        sigmoid(w.iter().zip(x).map(|(a, b)| a * b).sum())
    }

    /// Sơn's own word on this face, if he gave one.
    pub fn forced(&self, photo: &str, face: usize) -> Option<bool> {
        self.samples.get(&key(photo, face)).filter(|s| s.manual).map(|s| s.closed)
    }

    pub fn manual_count(&self) -> usize {
        self.samples.values().filter(|s| s.manual).count()
    }

    /// Record a correction (`None` forgets it) and re-fit.
    pub fn set(&mut self, photo: &str, face: usize, x: [f32; N], closed: Option<bool>, manual: bool) {
        let k = key(photo, face);
        match closed {
            Some(c) => {
                // A guess never replaces what he said himself.
                if !manual && self.samples.get(&k).is_some_and(|s| s.manual) {
                    return;
                }
                self.samples.insert(k, EyeSample { x, closed: c, manual, origin: None });
            }
            None => {
                self.samples.remove(&k);
            }
        }
        self.train();
    }

    /// Logistic regression pulled towards the starting rule: a handful of corrections nudge it,
    /// many corrections can reshape it. Hand corrections count double a guess.
    pub fn train(&mut self) {
        let data: Vec<(&[f32; N], f32, f32)> = self.samples.values().map(|s| (&s.x, s.closed as u8 as f32, if s.manual { 1.0 } else { 0.5 })).collect();
        if data.is_empty() {
            self.w = None;
            return;
        }
        let total: f32 = data.iter().map(|d| d.2).sum();
        // Weight of the prior in "samples": strong at first, fading as corrections pile up.
        let lambda = 6.0 / total.max(1.0);
        let mut w = PRIOR;
        for _ in 0..400 {
            let mut g = [0f32; N];
            for (x, y, wt) in &data {
                let p = sigmoid(w.iter().zip(x.iter()).map(|(a, b)| a * b).sum());
                for k in 0..N {
                    g[k] += wt * (p - y) * x[k];
                }
            }
            for k in 0..N {
                g[k] = g[k] / total + lambda * (w[k] - PRIOR[k]) * 0.02;
                w[k] -= 2.0 * g[k];
            }
        }
        self.w = Some(w);
    }

    /// The book on disk, plus what the other computers recorded in the shared folder.
    pub fn load(local: &Path, shared: Option<(&Path, &str)>) -> EyeBook {
        let mut b: EyeBook = std::fs::read(local).ok().and_then(|x| serde_json::from_slice(&x).ok()).unwrap_or_default();
        b.samples.retain(|_, s| s.origin.is_none());
        if let Some((dir, me)) = shared {
            for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                let Some(machine) = name.strip_prefix("eyes-").and_then(|n| n.strip_suffix(".json")) else { continue };
                if machine == me {
                    continue;
                }
                let Some(other) = std::fs::read(f.path()).ok().and_then(|x| serde_json::from_slice::<EyeBook>(&x).ok()) else { continue };
                for (k, mut s) in other.samples {
                    s.origin = Some(machine.to_string());
                    b.samples.entry(k).or_insert(s);
                }
            }
        }
        b.train();
        b
    }

    /// Save this computer's corrections (and share them when a shared folder is set).
    pub fn save(&self, local: &Path, shared: Option<(&Path, &str)>) -> std::io::Result<()> {
        let mine = EyeBook { samples: self.samples.iter().filter(|(_, s)| s.origin.is_none()).map(|(k, s)| (k.clone(), s.clone())).collect(), w: None };
        let bytes = serde_json::to_vec(&mine)?;
        std::fs::write(local, &bytes)?;
        if let Some((dir, me)) = shared {
            let f: PathBuf = dir.join(format!("eyes-{me}.json"));
            if std::fs::read(&f).ok().as_deref() != Some(&bytes[..]) {
                let _ = std::fs::write(f, &bytes);
            }
        }
        Ok(())
    }
}

#[cfg(feature = "faces")]
pub struct EyeModel {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
}

#[cfg(feature = "faces")]
impl EyeModel {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        use tract_onnx::prelude::*;
        let model = tract_onnx::onnx().model_for_path(path)?.with_input_fact(0, f32::fact([1, 3, 32, 32]).into())?.into_optimized()?.into_runnable()?;
        Ok(Self { model })
    }

    /// Probability that the eye centred at (`cx`, `cy`) is closed, from a square patch of side
    /// `side` pixels. None when the patch falls outside the image or is too small to judge.
    pub fn p_closed(&self, img: &image::RgbImage, cx: f32, cy: f32, side: f32) -> Option<f32> {
        use tract_onnx::prelude::*;
        if side < 8.0 {
            return None;
        }
        let (x0, y0) = ((cx - side / 2.0).round(), (cy - side / 2.0).round());
        let s = side.round() as u32;
        if x0 < 0.0 || y0 < 0.0 || x0 as u32 + s > img.width() || y0 as u32 + s > img.height() {
            return None;
        }
        let patch = image::imageops::crop_imm(img, x0 as u32, y0 as u32, s, s).to_image();
        let small = crate::metrics::resize(&patch, 32, 32);
        // BGR, (v − 127) / 255.
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, 3, 32, 32), |(_, c, y, x)| (small.get_pixel(x as u32, y as u32)[2 - c] as f32 - 127.0) / 255.0).into();
        let out = self.model.run(tvec!(input.into())).ok()?;
        let v = out[0].to_plain_array_view::<f32>().ok()?;
        let v: Vec<f32> = v.iter().copied().collect();
        // Already a softmax. The model card says [open, closed] but index 0 is "closed"
        // (found independently by FaceAiSharp and DrowsyGuard).
        v.first().copied().filter(|p| p.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_rule_follows_the_signals() {
        let b = EyeBook::default();
        assert!(b.p_closed(&features(Some(0.95), 0.3, 0.3)) > 0.5);
        assert!(b.p_closed(&features(Some(0.05), 0.12, 0.1)) < 0.5);
        assert!(b.p_closed(&features(None, 0.1, 0.08)) > 0.5);
        assert!(b.p_closed(&features(None, 0.35, 0.3)) < 0.5);
    }

    #[test]
    fn learns_from_corrections() {
        // His camera/lighting: the network says ~0.8 "closed" on open eyes with glasses.
        let mut b = EyeBook::default();
        let x_open = features(Some(0.8), 0.22, 0.2);
        let x_closed = features(Some(0.97), 0.1, 0.08);
        assert!(b.p_closed(&x_open) > 0.5);
        for i in 0..8 {
            b.set(&format!("/a/{i}.jpg"), 0, features(Some(0.78 + i as f32 * 0.005), 0.22, 0.2), Some(false), true);
            b.set(&format!("/b/{i}.jpg"), 0, x_closed, Some(true), true);
        }
        assert!(b.p_closed(&x_open) < 0.5, "{}", b.p_closed(&x_open));
        assert!(b.p_closed(&x_closed) > 0.5);
        assert_eq!(b.forced("/a/1.jpg", 0), Some(false));
        // A guess does not overwrite his word.
        b.set("/a/1.jpg", 0, x_open, Some(true), false);
        assert_eq!(b.forced("/a/1.jpg", 0), Some(false));
    }

    #[test]
    fn shared_between_computers() {
        let d = tempfile::tempdir().unwrap();
        let sync = d.path().join("sync");
        std::fs::create_dir_all(&sync).unwrap();
        let mut a = EyeBook::default();
        a.set("/x.jpg", 0, features(Some(0.9), 0.3, 0.3), Some(false), true);
        a.save(&d.path().join("a.json"), Some((&sync, "win"))).unwrap();
        let b = EyeBook::load(&d.path().join("b.json"), Some((&sync, "mac")));
        assert_eq!(b.forced("/x.jpg", 0), Some(false));
        // Mac saves only its own corrections.
        b.save(&d.path().join("b.json"), Some((&sync, "mac"))).unwrap();
        let mac: EyeBook = serde_json::from_slice(&std::fs::read(sync.join("eyes-mac.json")).unwrap()).unwrap();
        assert!(mac.samples.is_empty());
    }
}
