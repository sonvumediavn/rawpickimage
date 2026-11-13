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
pub const N_FEAT: usize = 24;
/// Features from the pictures themselves: how close a photo looks to the ones Sơn kept, and
/// to the ones he dropped (computed from the stored image vectors, not stored).
pub const N_VIS: usize = 2;
/// Inputs of the model.
pub const N_ALL: usize = N_FEAT + N_VIS;
/// Photos with an image vector kept for comparison, shared fairly between shows.
const BANK_MAX: usize = 3000;
/// Samples whose picture-features are worked out during training (spread over all shows; the
/// rest count as neutral).
const VIS_TRAIN_MAX: usize = 2500;
/// Closest photos averaged for a picture-feature.
const TOP_K: usize = 5;
/// Decisions needed (with both kept and dropped photos) before the profile influences scores.
pub const MIN_SAMPLES: usize = 20;
/// Kept/dropped pairs of frames from the same shot before [`Profile::rank`] is used.
pub const MIN_PAIRS: usize = 15;
/// Pairs taken from one series at most (a long burst must not drown the others).
const RANK_PER_SERIES: usize = 12;
/// Each class needs at least this many examples.
const MIN_CLASS: usize = 5;
/// A photo Sơn decided by hand teaches more than one he let the AI decide.
const MANUAL_WEIGHT: f32 = 3.0;
/// Stored decisions per event type. Beyond it, the biggest shows give up their oldest dropped
/// photos first, so ten old shows all keep teaching.
const MAX_SAMPLES: usize = 20000;

/// Indices of `items` to keep so that at most `max` remain, shared fairly between groups: each
/// group keeps up to the same quota, and within a group `rank` decides (lower stays first).
fn fair_pick<T>(items: &[T], max: usize, group: impl Fn(&T) -> u32, rank: impl Fn(usize, &T) -> (u8, std::cmp::Reverse<usize>)) -> Vec<usize> {
    if items.len() <= max {
        return (0..items.len()).collect();
    }
    let mut by: std::collections::HashMap<u32, Vec<usize>> = std::collections::HashMap::new();
    for (i, it) in items.iter().enumerate() {
        by.entry(group(it)).or_default().push(i);
    }
    // Largest quota q with sum(min(size, q)) <= max.
    let sizes: Vec<usize> = by.values().map(Vec::len).collect();
    let (mut lo, mut hi) = (0usize, max);
    while lo < hi {
        let q = (lo + hi).div_ceil(2);
        if sizes.iter().map(|&n| n.min(q)).sum::<usize>() <= max {
            lo = q;
        } else {
            hi = q - 1;
        }
    }
    let mut keep = vec![];
    for (_, mut idx) in by {
        idx.sort_by_key(|&i| rank(i, &items[i]));
        idx.truncate(lo);
        keep.extend(idx);
    }
    keep.sort_unstable();
    keep
}

pub const FEATURE_NAMES: [&str; N_ALL] = [
    "Phơi sáng", "Độ nét", "Bố cục", "Màu sắc", "Nhiễu", "Người", "Có người", "Mắt nhắm", "Nghiêng (độ)", "Cháy sáng (%)", "Số khuôn mặt",
    "Miệng mở (khẩu hình)", "Mặt nghiêng", "Cỡ nhân vật chính", "Chủ thể ở điểm mạnh", "Chủ thể sát mép", "Độ sáng tổng",
    "Cúi đầu", "Người phát biểu cúi đầu", "Tỉ lệ người mở mắt",
    "Mọi người cúi đầu (trung bình)", "Người cúi đầu nhiều nhất", "Mọi người quay mặt đi (trung bình)", "Người quay mặt đi nhiều nhất",
    "Nhìn giống ảnh bạn hay chọn", "Nhìn giống ảnh bạn hay bỏ",
];

/// Values for features a sample saved by an older version doesn't have.
const NEUTRAL: [f32; N_FEAT] = [5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.46, 0.0, 0.0, 9.0, 0.0, 0.0, 0.0, 0.0];

pub fn features(a: &Analysis) -> [f32; N_FEAT] {
    let s = &a.scores;
    // The person a viewer looks at: the biggest face.
    let lead = a.people.faces.iter().max_by(|x, y| (x.bbox[2] * x.bbox[3]).total_cmp(&(y.bbox[2] * y.bbox[3])));
    let has_people = s.people.is_some();
    // Everyone who matters in the frame (a group handed awards, a panel): who bows, who looks away.
    let main: Vec<&crate::faces::FaceInfo> = a.people.faces.iter().filter(|f| f.main && !f.small).collect();
    let mean = |v: &dyn Fn(&crate::faces::FaceInfo) -> f32| if main.is_empty() { 0.0 } else { main.iter().map(|f| v(f)).sum::<f32>() / main.len() as f32 };
    let worst = |v: &dyn Fn(&crate::faces::FaceInfo) -> f32| main.iter().map(|f| v(f)).fold(0.0f32, f32::max);
    let down = |f: &crate::faces::FaceInfo| f.pitch.max(0.0) * 10.0;
    let away = |f: &crate::faces::FaceInfo| f.yaw.abs().min(1.0) * 10.0;
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
        // Bowed head: whether it matters depends on the shot, so also alone in the frame
        // (a speaker at the lectern) versus with others (a candid moment).
        lead.map(|f| f.pitch.max(0.0) * 10.0).unwrap_or(0.0),
        a.people.solo().map(|f| f.pitch.max(0.0) * 10.0).unwrap_or(0.0),
        a.people.open_share() * 10.0,
        mean(&down),
        worst(&down),
        mean(&away),
        worst(&away),
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
    /// Picture-features worked out at the last full training (so a single click does not
    /// compare thousands of photos again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<[f32; N_VIS]>,
    /// Frames of one shot share this tag (a burst, the same group shot again): which one Sơn
    /// kept among them teaches [`Profile::rank`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
}

impl Sample {
    pub fn new(name: String, a: &Analysis, keep: bool, ai_keep: bool, manual: bool) -> Self {
        Self { name, f: features(a).to_vec(), keep, ai_keep, manual, e: a.embed.clone(), origin: None, v: None, series: None }
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
    /// Number of samples when picture-features were last all worked out again.
    #[serde(default)]
    pub vis_at: usize,
    /// Which frame of a series Sơn keeps: weights over the measured features (divided by
    /// `rscale`), learned from kept/dropped frames of the same shot.
    #[serde(default)]
    pub rw: Vec<f32>,
    #[serde(default)]
    pub rscale: Vec<f32>,
    /// Kept/dropped pairs it learned from, and the share it orders right.
    #[serde(default)]
    pub rank_pairs: usize,
    #[serde(default)]
    pub rank_acc: f32,
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
            let x = crc32fast::hash(s.name.as_bytes()) as u64 ^ ((s.keep as u64) << 40) ^ ((!s.e.is_empty() as u64) << 41) ^ ((s.series.is_some() as u64) << 42);
            h = h.wrapping_mul(0x100_0000_01b3).wrapping_add(x);
        }
        h
    }

    fn build_bank(&mut self) {
        let with: Vec<&Sample> = self.samples.iter().filter(|s| visual::usable(&s.e)).collect();
        // Every show keeps its share; inside a show, kept photos first, then the newest.
        let pick = fair_pick(&with, BANK_MAX, |s| s.show(), |i, s| (!s.keep as u8, std::cmp::Reverse(i)));
        self.bank = pick.into_iter().map(|i| with[i]).map(|s| BankEntry { e: s.e.clone(), keep: s.keep, show: s.show() }).collect();
    }

    /// [similarity to kept photos, similarity to dropped photos]; NaN where unknown.
    fn vis(&self, e: &[i8], skip_show: Option<u32>) -> [f32; N_VIS] {
        if !visual::usable(e) {
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
        // What Sơn decided himself (by hand, or in a show he culled) is never replaced by what
        // the AI decided for the same photo later, e.g. when the learned show is opened again.
        let his: std::collections::HashSet<String> = self.samples.iter().filter(|x| x.manual).map(|x| x.name.clone()).collect();
        s.retain(|x| x.manual || !his.contains(&x.name));
        // A photo decided again replaces its earlier sample.
        let names: std::collections::HashSet<_> = s.iter().map(|x| x.name.clone()).collect();
        self.samples.retain(|x| !names.contains(&x.name));
        self.samples.append(&mut s);
        if self.samples.len() > MAX_SAMPLES {
            // Hand choices and kept photos stay longest; the biggest shows shrink first.
            let keep = fair_pick(&self.samples, MAX_SAMPLES, |s| s.show(), |i, s| ((!s.keep as u8) + (!s.manual as u8), std::cmp::Reverse(i)));
            let mut it = keep.into_iter().peekable();
            let mut i = 0usize;
            self.samples.retain(|_| {
                let k = it.peek() == Some(&i);
                if k {
                    it.next();
                }
                i += 1;
                k
            });
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
        self.train_rank();
        let n = self.samples.len();
        let pos = self.samples.iter().filter(|s| s.keep).count();
        if n < MIN_SAMPLES || pos < MIN_CLASS || n - pos < MIN_CLASS {
            return false;
        }
        // Picture-features of each sample, compared only with other shows; with many samples, a
        // fair share of every show.
        let vis_set: std::collections::HashSet<usize> =
            fair_pick(&self.samples, VIS_TRAIN_MAX, |s| s.show(), |i, s| (!visual::usable(&s.e) as u8, std::cmp::Reverse(i))).into_iter().collect();
        // Work everything out again when the photos to compare with changed a lot (a show was
        // learned, another computer's choices arrived); otherwise reuse and only add new ones.
        let full = self.vis_at == 0 || n > self.vis_at + self.vis_at / 10 + 50 || n + n / 10 + 50 < self.vis_at;
        if full {
            self.vis_at = n;
        }
        let vis: Vec<Option<[f32; N_VIS]>> = self
            .samples
            .par_iter()
            .enumerate()
            .map(|(i, s)| {
                if !vis_set.contains(&i) {
                    return None;
                }
                match (full, s.v) {
                    (false, Some(v)) => Some(v),
                    _ => Some(self.vis(&s.e, Some(s.show()))),
                }
            })
            .collect();
        for (s, v) in self.samples.iter_mut().zip(&vis) {
            s.v = *v;
        }
        let raw: Vec<[f32; N_ALL]> = self
            .samples
            .iter()
            .map(|s| {
                let mut x = [0f32; N_ALL];
                x[..N_FEAT].copy_from_slice(&s.f[..N_FEAT]);
                x[N_FEAT..].copy_from_slice(&s.v.unwrap_or([f32::NAN; N_VIS]));
                x
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
        // A single new choice: start from the current weights, a few steps are enough.
        let warm = !full && self.w.len() == N_ALL;
        let (mut w, mut b) = if warm { (self.w.clone(), self.b) } else { (vec![0f32; N_ALL], 0f32) };
        let (lr, l2) = (0.3f32, 0.01f32);
        for _ in 0..if warm { 120 } else { 600 } {
            let (gw, gb) = x
                .par_iter()
                .zip(&y)
                .zip(&sw)
                .with_min_len(512)
                .fold(
                    || ([0f32; N_ALL], 0f32),
                    |(mut gw, mut gb), ((xi, &yi), &si)| {
                        let z = b + xi.iter().zip(&w).map(|(a, c)| a * c).sum::<f32>();
                        let p = 1.0 / (1.0 + (-z).exp());
                        let e = (p - yi) * si * if yi > 0.5 { wp } else { wn };
                        for k in 0..N_ALL {
                            gw[k] += e * xi[k];
                        }
                        gb += e;
                        (gw, gb)
                    },
                )
                .reduce(
                    || ([0f32; N_ALL], 0f32),
                    |(mut a, ab), (c, cb)| {
                        for k in 0..N_ALL {
                            a[k] += c[k];
                        }
                        (a, ab + cb)
                    },
                );
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

    /// Learn which frame of a series Sơn keeps. Every kept frame is compared with the dropped
    /// frames of the same shot (at least one of the two decided by him): what differs between
    /// them (head up or bowed, eyes, looking at the camera, mouth…) is what he chooses on.
    fn train_rank(&mut self) {
        let mut by: HashMap<&str, Vec<&Sample>> = HashMap::new();
        for s in &self.samples {
            if let Some(t) = &s.series {
                by.entry(t.as_str()).or_default().push(s);
            }
        }
        let mut diffs: Vec<[f32; N_FEAT]> = vec![];
        for g in by.values() {
            let mut n = 0;
            for k in g.iter().filter(|s| s.keep) {
                for d in g.iter().filter(|s| !s.keep && (s.manual || k.manual)) {
                    if n >= RANK_PER_SERIES {
                        break;
                    }
                    let mut x = [0f32; N_FEAT];
                    for i in 0..N_FEAT {
                        x[i] = k.f.get(i).copied().unwrap_or(NEUTRAL[i]) - d.f.get(i).copied().unwrap_or(NEUTRAL[i]);
                    }
                    diffs.push(x);
                    n += 1;
                }
            }
        }
        self.rank_pairs = diffs.len();
        if diffs.len() < MIN_PAIRS {
            self.rw.clear();
            self.rscale.clear();
            self.rank_acc = 0.0;
            return;
        }
        let m = diffs.len() as f32;
        let scale: Vec<f32> = (0..N_FEAT).map(|i| (diffs.iter().map(|d| d[i] * d[i]).sum::<f32>() / m).sqrt().max(1e-3)).collect();
        let x: Vec<[f32; N_FEAT]> = diffs.iter().map(|d| std::array::from_fn(|i| d[i] / scale[i])).collect();
        // Logistic on differences without a bias: P(kept frame beats dropped) = σ(w·Δ).
        let mut w = [0f32; N_FEAT];
        let (lr, l2) = (0.5f32, 0.02f32);
        for _ in 0..500 {
            let mut g = [0f32; N_FEAT];
            for xi in &x {
                let z: f32 = xi.iter().zip(&w).map(|(a, b)| a * b).sum();
                let e = 1.0 / (1.0 + (-z).exp()) - 1.0;
                for k in 0..N_FEAT {
                    g[k] += e * xi[k];
                }
            }
            for k in 0..N_FEAT {
                w[k] -= lr * (g[k] / m + l2 * w[k]);
            }
        }
        let right = x.iter().filter(|xi| xi.iter().zip(&w).map(|(a, b)| a * b).sum::<f32>() > 0.0).count();
        self.rank_acc = right as f32 / m;
        self.rw = w.to_vec();
        self.rscale = scale;
    }

    /// How much Sơn would prefer this frame within its series, in log-odds: only differences
    /// between frames of one shot mean anything. None until enough series are learned.
    pub fn rank(&self, f: &[f32; N_FEAT]) -> Option<f32> {
        if self.rw.len() != N_FEAT || self.rscale.len() != N_FEAT {
            return None;
        }
        Some((0..N_FEAT).map(|k| f[k] / self.rscale[k] * self.rw[k]).sum())
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
        self.samples.iter().filter(|s| visual::usable(&s.e)).count()
    }

    /// Plain-language summary of what the profile has learned, strongest first.
    pub fn explain(&self) -> Vec<String> {
        if !self.ready() {
            return vec![format!("Cần ít nhất {MIN_SAMPLES} lựa chọn (ít nhất {MIN_CLASS} giữ và {MIN_CLASS} bỏ) để bắt đầu học, hiện có {}", self.samples.len())];
        }
        let mut idx: Vec<usize> = (0..N_ALL).collect();
        idx.sort_by(|&a, &b| self.w[b].abs().total_cmp(&self.w[a].abs()));
        let mut out: Vec<String> = idx
            .into_iter()
            .take(4)
            .map(|k| format!("{}: {}", FEATURE_NAMES[k], if self.w[k] > 0.0 { "càng cao càng hay được giữ" } else { "càng cao càng hay bị bỏ" }))
            .collect();
        if self.rw.len() == N_FEAT {
            let mut r: Vec<usize> = (0..N_FEAT).collect();
            r.sort_by(|&a, &b| self.rw[b].abs().total_cmp(&self.rw[a].abs()));
            let top: Vec<String> = r
                .into_iter()
                .take(3)
                .map(|k| format!("{} {}", FEATURE_NAMES[k].to_lowercase(), if self.rw[k] > 0.0 { "cao hơn" } else { "thấp hơn" }))
                .collect();
            out.push(format!(
                "Trong một loạt ảnh cùng cảnh ({} cặp giữ/bỏ, đoán đúng {:.0}%), bạn hay chọn tấm có: {}",
                self.rank_pairs,
                self.rank_acc * 100.0,
                top.join(", ")
            ));
        }
        out
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

    /// Save after a single click: the shared file (several MB once old shows are learned)
    /// is rewritten at most every two minutes, so Drive is not uploading on every click.
    /// Returns false when the shared copy was left for later ([`Store::save`] it then).
    pub fn save_quick(&self, p: &Profile) -> std::io::Result<bool> {
        self.save_local(p)?;
        let Some(sh) = &self.shared else { return Ok(true) };
        let fresh = std::fs::metadata(sh.own_file(&p.name))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < std::time::Duration::from_secs(120));
        if fresh {
            return Ok(false);
        }
        let _ = sh.write(p);
        Ok(true)
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
            s.push(Sample { name: format!("a{i}"), f, keep: sharp >= 6.0, ai_keep: true, manual: false, e: vec![], origin: None, v: None, series: None });
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
        p.add(vec![Sample { name: "x".into(), f: vec![5.0; N_FEAT], keep: true, ai_keep: false, manual: false, e: vec![], origin: None, v: None, series: None }]);
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
            s.push(Sample { name: format!("m{i}"), f, keep: !open, ai_keep: true, manual: true, e: vec![], origin: None, v: None, series: None });
        }
        // Plus one sample saved by the old 11-feature version.
        s.push(Sample { name: "old".into(), f: vec![5.0; 11], keep: true, ai_keep: true, manual: false, e: vec![], origin: None, v: None, series: None });
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

    /// A full-length (v4) vector from a whole-frame one, for timing.
    fn full(mut e: Vec<i8>) -> Vec<i8> {
        let extra: Vec<i8> = e.iter().cycle().take(visual::FULL - visual::DIM).copied().collect();
        e.extend(extra);
        e
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
            s.push(Sample { name: format!("/show{}/p{i}.jpg", i % 6), f: NEUTRAL.to_vec(), keep, ai_keep: true, manual: true, e, origin: None, v: None, series: None });
        }
        p.add(s);
        assert!(p.train());
        let a = p.predict(&NEUTRAL, &vec_like(1, 99)).unwrap();
        let b = p.predict(&NEUTRAL, &vec_like(2, 99)).unwrap();
        assert!(a > 0.7 && b < 0.3, "{a} {b}");
        assert!(p.explain().iter().any(|x| x.starts_with("Nhìn giống")));
    }

    #[test]
    fn ten_old_shows_all_keep_teaching() {
        let mut p = Profile::new("nhiều show");
        let mut s = vec![];
        for show in 0..10u32 {
            for i in 0..1200u32 {
                let keep = i % 6 == 0;
                s.push(Sample { name: format!("/show{show}/p{i}.jpg"), f: NEUTRAL.to_vec(), keep, ai_keep: true, manual: true, e: full(vec_like(if keep { 1 } else { 2 }, i)), origin: None, v: None, series: None });
            }
        }
        p.add(s);
        assert!(p.samples.len() <= MAX_SAMPLES);
        let shows: std::collections::HashSet<u32> = p.samples.iter().map(|x| x.show()).collect();
        assert_eq!(shows.len(), 10);
        // Every kept photo survives the trim.
        assert_eq!(p.samples.iter().filter(|x| x.keep).count(), 2000);
        let t = std::time::Instant::now();
        assert!(p.train());
        eprintln!("train {} samples: {:?}", p.samples.len(), t.elapsed());
        // One more click: only the new photo is compared.
        p.add(vec![Sample { name: "/show0/new.jpg".into(), f: NEUTRAL.to_vec(), keep: true, ai_keep: false, manual: true, e: full(vec_like(1, 5)), origin: None, v: None, series: None }]);
        let t = std::time::Instant::now();
        assert!(p.train());
        eprintln!("one more click: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let e = full(vec_like(2, 77));
        for _ in 0..200 {
            std::hint::black_box(p.predict(&NEUTRAL, &e));
        }
        eprintln!("200 predictions: {:?}", t.elapsed());
        assert!(p.samples.iter().filter(|x| x.v.is_some()).count() >= VIS_TRAIN_MAX.min(p.samples.len()) - 1);
        let bank_shows: std::collections::HashSet<u32> = p.bank.iter().map(|b| b.show).collect();
        assert_eq!(bank_shows.len(), 10);
        assert!(p.bank.len() <= BANK_MAX);
    }

    #[test]
    fn computers_share_choices_through_a_folder() {
        let d = tempfile::tempdir().unwrap();
        let drive = d.path().join("drive");
        let sh = |m: &str| Some(Shared { dir: drive.clone(), machine: m.to_string(), label: m.to_string() });
        let win = Store::new(d.path().join("win")).with_shared(sh("win00001"));
        let mac = Store::new(d.path().join("mac")).with_shared(sh("mac00001"));
        let mk = |n: &str, keep: bool| Sample { name: n.into(), f: NEUTRAL.to_vec(), keep, ai_keep: keep, manual: true, e: vec![], origin: None, v: None, series: None };
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
