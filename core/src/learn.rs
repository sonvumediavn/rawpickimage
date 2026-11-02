//! On-device learning from Sơn's own choices.
//!
//! Each event type ("cưới", "hội nghị", "sân khấu"…) has its own profile: a small logistic
//! regression over the measured scores. It trains in milliseconds on CPU and never leaves the
//! machine. Until a profile has seen enough decisions it stays silent and the rule-based score
//! is used unchanged.

use crate::analyze::Analysis;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const N_FEAT: usize = 11;
/// Decisions needed (with both kept and dropped photos) before the profile influences scores.
pub const MIN_SAMPLES: usize = 40;
const MAX_SAMPLES: usize = 8000;

pub const FEATURE_NAMES: [&str; N_FEAT] = [
    "Phơi sáng", "Độ nét", "Bố cục", "Màu sắc", "Nhiễu", "Người", "Có người", "Mắt nhắm", "Nghiêng (độ)", "Cháy sáng (%)", "Số khuôn mặt",
];

pub fn features(a: &Analysis) -> [f32; N_FEAT] {
    let s = &a.scores;
    let has_people = s.people.is_some();
    [
        s.exposure,
        s.sharp,
        s.comp,
        s.color,
        s.noise,
        s.people.unwrap_or(5.0),
        has_people as u8 as f32,
        a.people.closed_eyes as u8 as f32,
        if a.comp.horizon_found { a.comp.tilt_deg.abs().min(15.0) } else { 0.0 },
        (a.exposure.clip_hi * 100.0).min(30.0),
        (a.people.faces.len() as f32).min(12.0),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub name: String,
    pub f: Vec<f32>,
    pub keep: bool,
    /// What the app had decided before Sơn's choice (for the agreement rate).
    pub ai_keep: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub samples: Vec<Sample>,
    pub w: Vec<f32>,
    pub b: f32,
    pub mean: Vec<f32>,
    pub std: Vec<f32>,
    pub trained_on: usize,
    /// Share of training decisions the model reproduces.
    pub accuracy: f32,
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), ..Default::default() }
    }

    pub fn add(&mut self, mut s: Vec<Sample>) {
        // A photo decided again replaces its earlier sample.
        let names: std::collections::HashSet<_> = s.iter().map(|x| x.name.clone()).collect();
        self.samples.retain(|x| !names.contains(&x.name));
        self.samples.append(&mut s);
        if self.samples.len() > MAX_SAMPLES {
            let cut = self.samples.len() - MAX_SAMPLES;
            self.samples.drain(..cut);
        }
    }

    /// How often the app's own decision matched Sơn's, over the stored samples.
    pub fn agreement(&self) -> Option<f32> {
        if self.samples.is_empty() {
            return None;
        }
        Some(self.samples.iter().filter(|s| s.keep == s.ai_keep).count() as f32 / self.samples.len() as f32)
    }

    pub fn ready(&self) -> bool {
        self.trained_on >= MIN_SAMPLES && self.w.len() == N_FEAT
    }

    /// Full-batch gradient descent with L2; tiny data, so this is instant.
    pub fn train(&mut self) -> bool {
        let n = self.samples.len();
        let pos = self.samples.iter().filter(|s| s.keep).count();
        if n < MIN_SAMPLES || pos == 0 || pos == n {
            return false;
        }
        let mut mean = vec![0f32; N_FEAT];
        let mut std = vec![0f32; N_FEAT];
        for s in &self.samples {
            for k in 0..N_FEAT {
                mean[k] += s.f[k] / n as f32;
            }
        }
        for s in &self.samples {
            for k in 0..N_FEAT {
                std[k] += (s.f[k] - mean[k]).powi(2) / n as f32;
            }
        }
        for v in &mut std {
            *v = v.sqrt().max(1e-3);
        }
        let x: Vec<Vec<f32>> = self.samples.iter().map(|s| (0..N_FEAT).map(|k| (s.f[k] - mean[k]) / std[k]).collect()).collect();
        let y: Vec<f32> = self.samples.iter().map(|s| s.keep as u8 as f32).collect();
        // Balance classes so a shoot with 90 % rejects does not teach "reject everything".
        let wp = n as f32 / (2.0 * pos as f32);
        let wn = n as f32 / (2.0 * (n - pos) as f32);
        let (mut w, mut b) = (vec![0f32; N_FEAT], 0f32);
        let (lr, l2) = (0.3f32, 0.01f32);
        for _ in 0..600 {
            let mut gw = vec![0f32; N_FEAT];
            let mut gb = 0f32;
            for (xi, &yi) in x.iter().zip(&y) {
                let z = b + xi.iter().zip(&w).map(|(a, c)| a * c).sum::<f32>();
                let p = 1.0 / (1.0 + (-z).exp());
                let e = (p - yi) * if yi > 0.5 { wp } else { wn };
                for k in 0..N_FEAT {
                    gw[k] += e * xi[k];
                }
                gb += e;
            }
            for k in 0..N_FEAT {
                w[k] -= lr * (gw[k] / n as f32 + l2 * w[k]);
            }
            b -= lr * gb / n as f32;
        }
        let correct = x
            .iter()
            .zip(&y)
            .filter(|(xi, &yi)| {
                let z = b + xi.iter().zip(&w).map(|(a, c)| a * c).sum::<f32>();
                (z > 0.0) == (yi > 0.5)
            })
            .count();
        self.w = w;
        self.b = b;
        self.mean = mean;
        self.std = std;
        self.trained_on = n;
        self.accuracy = correct as f32 / n as f32;
        true
    }

    /// Probability that Sơn keeps this photo, once the profile is trained.
    pub fn predict(&self, f: &[f32; N_FEAT]) -> Option<f32> {
        if !self.ready() {
            return None;
        }
        let z = self.b + (0..N_FEAT).map(|k| (f[k] - self.mean[k]) / self.std[k] * self.w[k]).sum::<f32>();
        Some(1.0 / (1.0 + (-z).exp()))
    }

    /// Plain-language summary of what the profile has learned, strongest first.
    pub fn explain(&self) -> Vec<String> {
        if !self.ready() {
            return vec![format!("Cần ít nhất {MIN_SAMPLES} lựa chọn (có cả giữ và bỏ) để bắt đầu học, hiện có {}", self.samples.len())];
        }
        let mut idx: Vec<usize> = (0..N_FEAT).collect();
        idx.sort_by(|&a, &b| self.w[b].abs().total_cmp(&self.w[a].abs()));
        idx.into_iter()
            .take(4)
            .map(|k| format!("{}: {}", FEATURE_NAMES[k], if self.w[k] > 0.0 { "càng cao càng hay được giữ" } else { "càng cao càng hay bị bỏ" }))
            .collect()
    }
}

/// Profiles live as JSON files in the app's data folder.
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, name: &str) -> PathBuf {
        let slug: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
        self.dir.join(format!("profile-{slug}.json"))
    }

    pub fn load(&self, name: &str) -> Profile {
        std::fs::read(self.file(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| Profile::new(name))
    }

    pub fn save(&self, p: &Profile) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.file(&p.name).with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(p)?)?;
        std::fs::rename(tmp, self.file(&p.name))
    }

    pub fn list(&self) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return vec![] };
        rd.filter_map(|e| {
            let p = e.ok()?.path();
            let b = std::fs::read(&p).ok()?;
            let prof: Profile = serde_json::from_slice(&b).ok()?;
            Some(prof.name)
        })
        .collect()
    }
}

pub fn default_dir(base: &Path) -> PathBuf {
    base.join("ai-profiles")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_a_preference_for_sharp_photos() {
        let mut p = Profile::new("thử");
        let mut s = vec![];
        for i in 0..120 {
            let sharp = (i % 10) as f32;
            let mut f = vec![6.0; N_FEAT];
            f[1] = sharp;
            f[3] = ((i * 7) % 10) as f32; // irrelevant
            s.push(Sample { name: format!("a{i}"), f, keep: sharp >= 6.0, ai_keep: true });
        }
        p.add(s);
        assert!(p.train());
        assert!(p.accuracy > 0.9, "{}", p.accuracy);
        let mut hi = [6.0; N_FEAT];
        hi[1] = 9.0;
        let mut lo = hi;
        lo[1] = 2.0;
        assert!(p.predict(&hi).unwrap() > 0.8 && p.predict(&lo).unwrap() < 0.2);
        assert!(p.explain()[0].starts_with("Độ nét"));
    }

    #[test]
    fn stays_silent_without_enough_data() {
        let mut p = Profile::new("ít");
        p.add(vec![Sample { name: "x".into(), f: vec![5.0; N_FEAT], keep: true, ai_keep: false }]);
        assert!(!p.train());
        assert!(p.predict(&[5.0; N_FEAT]).is_none());
        assert_eq!(p.agreement(), Some(0.0));
    }
}
