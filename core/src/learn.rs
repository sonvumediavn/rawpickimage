//! On-device learning from Sơn's own choices.
//!
//! Each event type ("cưới", "hội nghị", "sân khấu"…) has its own profile: a small logistic
//! regression over the measured scores. It trains in milliseconds on CPU and never leaves the
//! machine. Until a profile has seen enough decisions it stays silent and the rule-based score
//! is used unchanged.

use crate::analyze::Analysis;
use crate::visual;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Measured features stored with each sample.
pub const N_FEAT: usize = 17;
/// Features from the pictures themselves: how close a photo looks to the ones Sơn kept, and
/// to the ones he dropped (computed from the stored image vectors, not stored).
pub const N_VIS: usize = 2;
/// Inputs of the model.
pub const N_ALL: usize = N_FEAT + N_VIS;
/// Most recent photos with an image vector used for comparison.
const BANK_MAX: usize = 1500;
/// Samples whose picture-features are worked out during training (the rest count as neutral).
const VIS_TRAIN_MAX: usize = 3000;
/// Closest photos averaged for a picture-feature.
const TOP_K: usize = 5;
/// Decisions needed (with both kept and dropped photos) before the profile influences scores.
pub const MIN_SAMPLES: usize = 20;
/// Each class needs at least this many examples.
const MIN_CLASS: usize = 5;
/// A photo Sơn decided by hand teaches more than one he let the AI decide.
const MANUAL_WEIGHT: f32 = 3.0;
const MAX_SAMPLES: usize = 8000;

pub const FEATURE_NAMES: [&str; N_ALL] = [
    "Phơi sáng", "Độ nét", "Bố cục", "Màu sắc", "Nhiễu", "Người", "Có người", "Mắt nhắm", "Nghiêng (độ)", "Cháy sáng (%)", "Số khuôn mặt",
    "Miệng mở (khẩu hình)", "Mặt nghiêng", "Cỡ nhân vật chính", "Chủ thể ở điểm mạnh", "Chủ thể sát mép", "Độ sáng tổng",
    "Nhìn giống ảnh bạn hay chọn", "Nhìn giống ảnh bạn hay bỏ",
];

/// Values for features a sample saved by an older version doesn't have.
const NEUTRAL: [f32; N_FEAT] = [5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.46];

pub fn features(a: &Analysis) -> [f32; N_FEAT] {
    let s = &a.scores;
    // The person a viewer looks at: the biggest face.
    let lead = a.people.faces.iter().max_by(|x, y| (x.bbox[2] * x.bbox[3]).total_cmp(&(y.bbox[2] * y.bbox[3])));
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
        lead.map(|f| f.mouth_open.min(0.6) * 10.0).unwrap_or(0.0),
        lead.map(|f| f.yaw.abs().min(1.0) * 10.0).unwrap_or(0.0),
        lead.map(|f| (f.bbox[2] * f.bbox[3]).sqrt() * 10.0).unwrap_or(0.0),
        a.comp.pos * 10.0,
        a.comp.border_pen.min(5.0),
        a.exposure.mean / 255.0,
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub name: String,
    pub f: Vec<f32>,
    pub keep: bool,
    /// What the app had decided before Sơn's choice (for the agreement rate).
    pub ai_keep: bool,
    /// Decided by hand (Chọn / Loại) rather than left to the AI.
    #[serde(default)]
    pub manual: bool,
    /// How the photo looks ([`visual`]); empty for samples from before image learning.
    #[serde(default, with = "crate::visual::hex", skip_serializing_if = "Vec::is_empty")]
    pub e: Vec<i8>,
    /// Computer that made this choice (another machine's id when shared through Google Drive);
    /// `None` = this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl Sample {
    pub fn new(name: String, a: &Analysis, keep: bool, ai_keep: bool, manual: bool) -> Self {
        Self { name, f: features(a).to_vec(), keep, ai_keep, manual, e: a.embed.clone(), origin: None }
    }

    /// The show (folder) the photo came from: similar frames of the same show must not vouch
    /// for each other while training.
    fn show(&self) -> u32 {
        let p = Path::new(&self.name);
        crc32fast::hash(p.parent().unwrap_or(p).to_string_lossy().as_bytes())
    }
}

/// A photo to compare with.
#[derive(Debug, Clone)]
struct BankEntry {
    e: Vec<i8>,
    keep: bool,
    show: u32,
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
    /// Which samples the weights were trained on (to notice choices merged from another computer).
    #[serde(default)]
    pub fp: u64,
    #[serde(skip)]
    bank: Vec<BankEntry>,
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), ..Default::default() }
    }

    /// Bring samples saved by an older version up to the current feature set and retrain.
    pub fn upgrade(&mut self) {
        let mut changed = false;
        for s in &mut self.samples {
            if s.f.len() < N_FEAT {
                let k = s.f.len();
                s.f.extend_from_slice(&NEUTRAL[k..]);
                changed = true;
            }
            s.f.truncate(N_FEAT);
        }
        if changed || (self.w.len() != N_ALL && !self.samples.is_empty()) || (self.fp != self.fingerprint() && !self.samples.is_empty()) {
            self.w.clear();
            self.trained_on = 0;
            self.train();
        } else {
            self.build_bank();
        }
    }

    /// Cheap summary of the sample set.
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = self.samples.len() as u64;
        for s in &self.samples {
            let x = crc32fast::hash(s.name.as_bytes()) as u64 ^ ((s.keep as u64) << 40) ^ ((!s.e.is_empty() as u64) << 41);
            h = h.wrapping_mul(0x100_0000_01b3).wrapping_add(x);
        }
        h
    }

    fn build_bank(&mut self) {
        let with: Vec<&Sample> = self.samples.iter().filter(|s| s.e.len() == visual::DIM).collect();
        let from = with.len().saturating_sub(BANK_MAX);
        self.bank = with[from..].iter().map(|s| BankEntry { e: s.e.clone(), keep: s.keep, show: s.show() }).collect();
    }

    /// [similarity to kept photos, similarity to dropped photos]; NaN where unknown.
    fn vis(&self, e: &[i8], skip_show: Option<u32>) -> [f32; N_VIS] {
        if e.len() != visual::DIM {
            return [f32::NAN; N_VIS];
        }
        let mut kept = vec![];
        let mut dropped = vec![];
        for b in &self.bank {
            if Some(b.show) == skip_show {
                continue;
            }
            let v = visual::similarity(e, &b.e);
            if b.keep {
                kept.push(v);
            } else {
                dropped.push(v);
            }
        }
        let top = |mut v: Vec<f32>| {
            if v.is_empty() {
                return f32::NAN;
            }
            v.sort_by(|a, b| b.total_cmp(a));
            let k = v.len().min(TOP_K);
            v[..k].iter().sum::<f32>() / k as f32
        };
        [top(kept), top(dropped)]
    }

    /// Model inputs for a photo: measured features plus picture-features.
    fn inputs(&self, f: &[f32], e: &[i8], skip_show: Option<u32>) -> [f32; N_ALL] {
        let mut x = [0f32; N_ALL];
        x[..N_FEAT].copy_from_slice(&f[..N_FEAT]);
        let v = self.vis(e, skip_show);
        x[N_FEAT..].copy_from_slice(&v);
        x
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
        self.trained_on >= MIN_SAMPLES && self.w.len() == N_ALL
    }

    /// Full-batch gradient descent with L2; tiny data, so this is instant.
    pub fn train(&mut self) -> bool {
        self.build_bank();
        let n = self.samples.len();
        let pos = self.samples.iter().filter(|s| s.keep).count();
        if n < MIN_SAMPLES || pos < MIN_CLASS || n - pos < MIN_CLASS {
            return false;
        }
        // Picture-features of each sample, compared only with other shows.
        let vis_from = n.saturating_sub(VIS_TRAIN_MAX);
        let raw: Vec<[f32; N_ALL]> = self
            .samples
            .par_iter()
            .enumerate()
            .map(|(i, s)| {
                if i >= vis_from {
                    self.inputs(&s.f, &s.e, Some(s.show()))
                } else {
                    self.inputs(&s.f, &[], None)
                }
            })
            .collect();
        // Unknown values (old samples, nothing to compare with) count as average.
        let mut mean = vec![0f32; N_ALL];
        let mut cnt = [0usize; N_ALL];
        for r in &raw {
            for k in 0..N_ALL {
                if r[k].is_finite() {
                    mean[k] += r[k];
                    cnt[k] += 1;
                }
            }
        }
        for k in 0..N_ALL {
            mean[k] = if cnt[k] > 0 { mean[k] / cnt[k] as f32 } else { 0.0 };
        }
        let raw: Vec<[f32; N_ALL]> = raw.into_iter().map(|mut r| {
            for k in 0..N_ALL {
                if !r[k].is_finite() {
                    r[k] = mean[k];
                }
            }
            r
        }).collect();
        let mut std = vec![0f32; N_ALL];
        for r in &raw {
            for k in 0..N_ALL {
                std[k] += (r[k] - mean[k]).powi(2) / n as f32;
            }
        }
        for v in &mut std {
            *v = v.sqrt().max(1e-3);
        }
        let x: Vec<Vec<f32>> = raw.iter().map(|r| (0..N_ALL).map(|k| (r[k] - mean[k]) / std[k]).collect()).collect();
        let y: Vec<f32> = self.samples.iter().map(|s| s.keep as u8 as f32).collect();
        // Balance classes so a shoot with 90 % rejects does not teach "reject everything".
        let sw: Vec<f32> = self.samples.iter().map(|s| if s.manual { MANUAL_WEIGHT } else { 1.0 }).collect();
        let tot: f32 = sw.iter().sum();
        let tp: f32 = self.samples.iter().zip(&sw).filter(|(s, _)| s.keep).map(|(_, w)| w).sum();
        let wp = tot / (2.0 * tp);
        let wn = tot / (2.0 * (tot - tp));
        let (mut w, mut b) = (vec![0f32; N_ALL], 0f32);
        let (lr, l2) = (0.3f32, 0.01f32);
        for _ in 0..600 {
            let mut gw = [0f32; N_ALL];
            let mut gb = 0f32;
            for ((xi, &yi), &si) in x.iter().zip(&y).zip(&sw) {
                let z = b + xi.iter().zip(&w).map(|(a, c)| a * c).sum::<f32>();
                let p = 1.0 / (1.0 + (-z).exp());
                let e = (p - yi) * si * if yi > 0.5 { wp } else { wn };
                for k in 0..N_ALL {
                    gw[k] += e * xi[k];
                }
                gb += e;
            }
            for k in 0..N_ALL {
                w[k] -= lr * (gw[k] / tot + l2 * w[k]);
            }
            b -= lr * gb / tot;
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
        self.fp = self.fingerprint();
        true
    }

    /// Choices Sơn made by hand.
    pub fn manual_count(&self) -> usize {
        self.samples.iter().filter(|s| s.manual).count()
    }

    /// How much the learned taste weighs against the fixed rules. Sơn's own taste is meant to
    /// lead (his words, 2026-11-03): it starts at half the score once 20 choices are in and grows
    /// with his hand-made choices and with how well the model reproduces them, up to 95 %.
    pub fn trust(&self, base: f32) -> f32 {
        if !self.ready() {
            return 0.0;
        }
        let grow = (self.manual_count() as f32 / 80.0).min(1.0) * 0.45;
        ((base.max(0.5) + grow) * (0.6 + 0.4 * self.accuracy)).clamp(0.0, 0.95)
    }

    /// Probability that Sơn keeps this photo, once the profile is trained. `e` is the photo's
    /// image vector (empty if unknown).
    pub fn predict(&self, f: &[f32; N_FEAT], e: &[i8]) -> Option<f32> {
        if !self.ready() {
            return None;
        }
        let x = self.inputs(f, e, None);
        let z = self.b + (0..N_ALL).map(|k| if x[k].is_finite() { (x[k] - self.mean[k]) / self.std[k] * self.w[k] } else { 0.0 }).sum::<f32>();
        Some(1.0 / (1.0 + (-z).exp()))
    }

    /// Photos with an image vector, i.e. what the AI can compare by look.
    pub fn seen_count(&self) -> usize {
        self.samples.iter().filter(|s| s.e.len() == visual::DIM).count()
    }

    /// Plain-language summary of what the profile has learned, strongest first.
    pub fn explain(&self) -> Vec<String> {
        if !self.ready() {
            return vec![format!("Cần ít nhất {MIN_SAMPLES} lựa chọn (ít nhất {MIN_CLASS} giữ và {MIN_CLASS} bỏ) để bắt đầu học, hiện có {}", self.samples.len())];
        }
        let mut idx: Vec<usize> = (0..N_ALL).collect();
        idx.sort_by(|&a, &b| self.w[b].abs().total_cmp(&self.w[a].abs()));
        idx.into_iter()
            .take(4)
            .map(|k| format!("{}: {}", FEATURE_NAMES[k], if self.w[k] > 0.0 { "càng cao càng hay được giữ" } else { "càng cao càng hay bị bỏ" }))
            .collect()
    }
}

/// One computer's own choices for one profile, in the shared Google Drive folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SharedFile {
    pub profile: String,
    pub machine: String,
    /// Computer name, for people.
    pub label: String,
    pub samples: Vec<Sample>,
}

/// A folder (on Google Drive) where several computers share what they learned. Each computer
/// writes only its own file, so the Drive app never has two edits of one file to reconcile.
#[derive(Debug, Clone)]
pub struct Shared {
    pub dir: PathBuf,
    pub machine: String,
    pub label: String,
}

/// Shared files already read, by path: (modified time, content).
static SHARED_CACHE: Mutex<Option<HashMap<PathBuf, (std::time::SystemTime, SharedFile)>>> = Mutex::new(None);

impl Shared {
    fn folder(&self, profile: &str) -> PathBuf {
        self.dir.join(slug(profile))
    }

    fn own_file(&self, profile: &str) -> PathBuf {
        let label: String = self.label.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).take(24).collect();
        self.folder(profile).join(format!("{label}-{}.json", &self.machine[..self.machine.len().min(8)]))
    }

    /// Other computers' files for a profile.
    fn others(&self, profile: &str) -> Vec<SharedFile> {
        let Ok(rd) = std::fs::read_dir(self.folder(profile)) else { return vec![] };
        let mut cache = SHARED_CACHE.lock().unwrap();
        let cache = cache.get_or_insert_with(HashMap::new);
        let mut out = vec![];
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Ok(t) = e.metadata().and_then(|m| m.modified()) else { continue };
            let f = match cache.get(&p) {
                Some((ct, f)) if *ct == t => f.clone(),
                _ => {
                    let Some(f) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<SharedFile>(&b).ok()) else { continue };
                    cache.insert(p.clone(), (t, f.clone()));
                    f
                }
            };
            if f.machine != self.machine && !f.machine.is_empty() {
                out.push(f);
            }
        }
        out
    }

    fn profiles(&self) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return vec![] };
        rd.flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|d| {
                let f = std::fs::read_dir(d.path()).ok()?.flatten().find(|e| e.path().extension().is_some_and(|x| x == "json"))?;
                let sf: SharedFile = serde_json::from_slice(&std::fs::read(f.path()).ok()?).ok()?;
                Some(sf.profile)
            })
            .collect()
    }

    fn write(&self, p: &Profile) -> std::io::Result<()> {
        let mine: Vec<Sample> = p.samples.iter().filter(|s| s.origin.as_deref().is_none_or(|o| o == self.machine)).cloned().collect();
        let f = SharedFile { profile: p.name.clone(), machine: self.machine.clone(), label: self.label.clone(), samples: mine };
        let path = self.own_file(&p.name);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let body = serde_json::to_vec(&f)?;
        // Unchanged: don't touch the file, so Drive has nothing to upload.
        if std::fs::read(&path).ok().as_deref() == Some(&body[..]) {
            return Ok(());
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(tmp, path)
    }
}

fn slug(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect()
}

/// Profiles live as JSON files in the app's data folder, optionally shared with other
/// computers through [`Shared`].
pub struct Store {
    pub dir: PathBuf,
    pub shared: Option<Shared>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), shared: None }
    }

    pub fn with_shared(mut self, shared: Option<Shared>) -> Self {
        self.shared = shared;
        self
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(format!("profile-{}.json", slug(name)))
    }

    /// This computer's profile with the other computers' choices merged in, trained.
    pub fn load(&self, name: &str) -> Profile {
        let mut p: Profile = std::fs::read(self.file(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| Profile::new(name));
        if let Some(sh) = &self.shared {
            for f in sh.others(name) {
                p.samples.retain(|s| s.origin.as_deref() != Some(f.machine.as_str()));
                let mut theirs = f.samples;
                for s in &mut theirs {
                    s.origin = Some(f.machine.clone());
                }
                // The same photo decided on both computers: the shared copy wins.
                let names: std::collections::HashSet<_> = theirs.iter().map(|x| x.name.clone()).collect();
                p.samples.retain(|s| !names.contains(&s.name) || s.origin.is_some());
                p.samples.extend(theirs);
            }
        }
        let fp = p.fp;
        p.upgrade();
        if p.fp != fp {
            let _ = self.save_local(&p);
        }
        p
    }

    fn save_local(&self, p: &Profile) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.file(&p.name).with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(p)?)?;
        std::fs::rename(tmp, self.file(&p.name))
    }

    pub fn save(&self, p: &Profile) -> std::io::Result<()> {
        self.save_local(p)?;
        if let Some(sh) = &self.shared {
            // Drive may be offline or not mounted: the local copy is what counts.
            let _ = sh.write(p);
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(|e| {
                    let p = e.ok()?.path();
                    let b = std::fs::read(&p).ok()?;
                    let prof: Profile = serde_json::from_slice(&b).ok()?;
                    Some(prof.name)
                })
                .collect()
            })
            .unwrap_or_default();
        if let Some(sh) = &self.shared {
            for n in sh.profiles() {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
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
            s.push(Sample { name: format!("a{i}"), f, keep: sharp >= 6.0, ai_keep: true, manual: false, e: vec![], origin: None });
        }
        p.add(s);
        assert!(p.train());
        assert!(p.accuracy > 0.9, "{}", p.accuracy);
        let mut hi = [6.0; N_FEAT];
        hi[1] = 9.0;
        let mut lo = hi;
        lo[1] = 2.0;
        assert!(p.predict(&hi, &[]).unwrap() > 0.8 && p.predict(&lo, &[]).unwrap() < 0.2);
        assert!(p.explain()[0].starts_with("Độ nét"));
    }

    #[test]
    fn stays_silent_without_enough_data() {
        let mut p = Profile::new("ít");
        p.add(vec![Sample { name: "x".into(), f: vec![5.0; N_FEAT], keep: true, ai_keep: false, manual: false, e: vec![], origin: None }]);
        assert!(!p.train());
        assert!(p.predict(&[5.0; N_FEAT], &[]).is_none());
        assert_eq!(p.agreement(), Some(0.0));
    }

    #[test]
    fn learns_from_twenty_hand_choices_and_upgrades_old_samples() {
        // Sơn keeps speakers with a closed mouth, drops the ones caught mid-word.
        let mut p = Profile::new("hội nghị");
        let mut s = vec![];
        for i in 0..24 {
            let mut f = NEUTRAL.to_vec();
            let open = i % 2 == 0;
            f[11] = if open { 4.0 } else { 0.5 };
            s.push(Sample { name: format!("m{i}"), f, keep: !open, ai_keep: true, manual: true, e: vec![], origin: None });
        }
        // Plus one sample saved by the old 11-feature version.
        s.push(Sample { name: "old".into(), f: vec![5.0; 11], keep: true, ai_keep: true, manual: false, e: vec![], origin: None });
        p.add(s);
        p.upgrade();
        assert!(p.ready() && p.samples.iter().all(|x| x.f.len() == N_FEAT));
        let mut talk = NEUTRAL;
        talk[11] = 4.0;
        let mut calm = NEUTRAL;
        calm[11] = 0.5;
        assert!(p.predict(&talk, &[]).unwrap() < 0.3 && p.predict(&calm, &[]).unwrap() > 0.7);
        assert!(p.trust(0.35) > 0.5 && p.trust(0.35) <= 0.95);
    }

    fn vec_like(seed: u32, noise: u32) -> Vec<i8> {
        let base: Vec<f32> = (0..1280u32).map(|i| ((i.wrapping_mul(2654435761).wrapping_add(seed * 977)) % 1000) as f32 / 100.0).collect();
        let f: Vec<f32> = base.iter().enumerate().map(|(i, v)| v + ((i as u32 * 31 + noise * 7) % 13) as f32 / 40.0).collect();
        visual::compress(&f)
    }

    #[test]
    fn learns_the_look_of_kept_photos() {
        // Two looks, same measurements: Sơn keeps look A, drops look B, over several shows.
        let mut p = Profile::new("nhìn");
        let mut s = vec![];
        for i in 0..60u32 {
            let keep = i % 2 == 0;
            let e = vec_like(if keep { 1 } else { 2 }, i);
            s.push(Sample { name: format!("/show{}/p{i}.jpg", i % 6), f: NEUTRAL.to_vec(), keep, ai_keep: true, manual: true, e, origin: None });
        }
        p.add(s);
        assert!(p.train());
        let a = p.predict(&NEUTRAL, &vec_like(1, 99)).unwrap();
        let b = p.predict(&NEUTRAL, &vec_like(2, 99)).unwrap();
        assert!(a > 0.7 && b < 0.3, "{a} {b}");
        assert!(p.explain().iter().any(|x| x.starts_with("Nhìn giống")));
    }

    #[test]
    fn computers_share_choices_through_a_folder() {
        let d = tempfile::tempdir().unwrap();
        let drive = d.path().join("drive");
        let sh = |m: &str| Some(Shared { dir: drive.clone(), machine: m.to_string(), label: m.to_string() });
        let win = Store::new(d.path().join("win")).with_shared(sh("win00001"));
        let mac = Store::new(d.path().join("mac")).with_shared(sh("mac00001"));
        let mk = |n: &str, keep: bool| Sample { name: n.into(), f: NEUTRAL.to_vec(), keep, ai_keep: keep, manual: true, e: vec![], origin: None };
        let mut p = win.load("cưới");
        p.add((0..12).map(|i| mk(&format!("w{i}"), i % 2 == 0)).collect());
        win.save(&p).unwrap();
        let mut q = mac.load("cưới");
        assert_eq!(q.samples.len(), 12);
        q.add((0..12).map(|i| mk(&format!("m{i}"), i % 3 == 0)).collect());
        mac.save(&q).unwrap();
        // The Mac's file holds only its own 12 choices; Windows now sees all 24.
        let p2 = win.load("cưới");
        assert_eq!(p2.samples.len(), 24);
        assert!(p2.ready());
        assert!(mac.list().contains(&"cưới".to_string()));
        let mac_file: SharedFile = serde_json::from_slice(&std::fs::read(mac.shared.as_ref().unwrap().own_file("cưới")).unwrap()).unwrap();
        assert_eq!(mac_file.samples.len(), 12);
    }
}
