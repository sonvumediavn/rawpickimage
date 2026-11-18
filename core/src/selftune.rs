//! The AI training itself. On every show Sơn taught it (all the photos and the ones he kept), it
//! hides his choices, picks by itself with a taste learned WITHOUT that show (as if the show were
//! new), compares with what he really kept, and keeps the settings that match him best: the pick
//! score, how much his learned taste counts, how many photos of each speaker, group and person,
//! how it chooses inside a burst. It also sums up how he chooses, in words.

use crate::analyze::Analysis;
use crate::cull::{self, Decision, Learned, Settings, SubjectKind, Verdict};
use crate::learn::Profile;
use crate::options::StarMode;
use crate::scene::Scene;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One show to check on: every photo, and whether Sơn kept it.
pub struct Exam {
    pub name: String,
    pub items: Vec<Analysis>,
    pub keep: Vec<bool>,
    /// Paths of the photos, to leave them out of the taste it is checked with.
    pub paths: HashSet<String>,
}

/// Fewest photos for a show to count as a check.
const EXAM_MIN: usize = 30;

/// The shows Sơn taught that can still be checked on: lessons (every photo decided by him, from
/// «Học từ show cũ» or two sessions) whose analyses are still in the app's work folders.
pub fn find_exams(profile: &Profile, data_dir: &Path) -> Vec<Exam> {
    // His own decisions, by folder.
    let mut by_dir: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (i, s) in profile.samples.iter().enumerate() {
        if let Some(d) = Path::new(&s.name).parent() {
            by_dir.entry(d.to_path_buf()).or_default().push(i);
        }
    }
    let mut caches: HashMap<PathBuf, HashMap<String, Analysis>> = HashMap::new();
    // Work folder -> samples found there.
    let mut found: HashMap<PathBuf, (PathBuf, Vec<usize>)> = HashMap::new();
    for (dir, idx) in by_dir {
        let manual = idx.iter().filter(|&&i| profile.samples[i].manual).count();
        // A session he only partly decided by hand: the rest are the AI's own picks, no check.
        if idx.len() < EXAM_MIN || (manual as f32) < 0.95 * idx.len() as f32 {
            continue;
        }
        // The lesson was given on this folder or one above it (the show, holding «ảnh gốc»).
        for anc in dir.ancestors().take(3) {
            let work = crate::pipeline::work_dir(data_dir, anc);
            if !work.join("cache.json").is_file() {
                continue;
            }
            let cache = caches.entry(work.clone()).or_insert_with(|| crate::pipeline::cached_analyses(&work));
            let hits: Vec<usize> = idx.iter().copied().filter(|&i| cache.contains_key(&profile.samples[i].name)).collect();
            if hits.len() as f32 >= 0.8 * idx.len() as f32 {
                let e = found.entry(work.clone()).or_insert_with(|| (anc.to_path_buf(), vec![]));
                e.1.extend(hits);
                break;
            }
        }
    }
    let mut exams: Vec<Exam> = found
        .into_iter()
        .filter_map(|(work, (show, idx))| {
            let cache = caches.get(&work)?;
            let mut rows: Vec<(String, Analysis, bool)> = idx.iter().map(|&i| &profile.samples[i]).map(|s| (s.name.clone(), cache[&s.name].clone(), s.keep)).collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows.dedup_by(|a, b| a.0 == b.0);
            let kept = rows.iter().filter(|r| r.2).count();
            if rows.len() < EXAM_MIN || kept < 3 || rows.len() - kept < 3 {
                return None;
            }
            let name = show.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| show.display().to_string());
            let paths = rows.iter().map(|r| r.0.clone()).collect();
            let (items, keep) = rows.into_iter().map(|r| (r.1, r.2)).unzip();
            Some(Exam { name, items, keep, paths })
        })
        .collect();
    exams.sort_by(|a, b| a.name.cmp(&b.name));
    exams
}

/// How well the AI's picks match Sơn's on the shows checked.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Score {
    /// Mean over the shows of the F1 score (0–1): both "picks what he picks" and "picks nothing else".
    pub f1: f32,
    /// Of the AI's picks, the share he also kept.
    pub precision: f32,
    /// Of his keeps, the share the AI picked too.
    pub recall: f32,
    pub picked: usize,
    pub kept: usize,
    pub hit: usize,
}

/// The settings the AI found by checking itself, and what it learned about Sơn.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Tuned {
    pub threshold: f32,
    pub learned_weight: f32,
    pub rank_gain: f32,
    pub keep_per_series: usize,
    pub per_subject: bool,
    pub speaker_max: usize,
    pub subject_max: usize,
    pub subject_fill: usize,
    pub keep_each_shot: bool,
    pub story: bool,
    /// Matching his picks with the settings it started from, and with the ones it found.
    pub before: Score,
    pub after: Score,
    pub shows: Vec<String>,
    pub photos: usize,
    /// What changed, in words.
    pub changes: Vec<String>,
    /// How Sơn chooses, summed up from his shows.
    pub lessons: Vec<String>,
    /// Seconds since 1970.
    pub at: u64,
}

impl Tuned {
    fn from(s: &Settings) -> Self {
        Self {
            threshold: s.threshold,
            learned_weight: s.learned_weight,
            rank_gain: s.rank_gain,
            keep_per_series: s.keep_per_series,
            per_subject: s.options.per_subject,
            speaker_max: s.speaker_max,
            subject_max: s.subject_max,
            subject_fill: s.subject_fill,
            keep_each_shot: s.options.keep_each_shot,
            story: s.options.story,
            ..Default::default()
        }
    }

    /// Put the settings found into `s`; returns whether anything changed.
    pub fn apply(&self, s: &mut Settings) -> bool {
        let before = Tuned::from(s);
        s.threshold = self.threshold;
        s.learned_weight = self.learned_weight;
        s.rank_gain = self.rank_gain;
        s.keep_per_series = self.keep_per_series.max(1);
        s.options.per_subject = self.per_subject;
        s.speaker_max = self.speaker_max.max(1);
        s.subject_max = self.subject_max.max(1);
        s.subject_fill = self.subject_fill;
        s.options.keep_each_shot = self.keep_each_shot;
        s.options.story = self.story;
        before != Tuned::from(s)
    }

    /// Short report for the learning panel.
    pub fn summary(&self) -> Vec<String> {
        let mut out = vec![format!(
            "AI tự kiểm tra trên {} show bạn đã dạy ({} ảnh, mỗi show chấm với gu học từ các show khác): khớp {:.0}% → {:.0}% (chọn trúng {} / {} ảnh bạn giữ, {} ảnh AI chọn)",
            self.shows.len(),
            self.photos,
            self.before.f1 * 100.0,
            self.after.f1 * 100.0,
            self.after.hit,
            self.after.kept,
            self.after.picked
        )];
        if self.changes.is_empty() {
            out.push("Cài đặt đang dùng đã là tốt nhất, không cần chỉnh".into());
        } else {
            out.push(format!("AI tự chỉnh: {}", self.changes.join("; ")));
        }
        out.extend(self.lessons.iter().map(|l| format!("Đúc kết: {l}")));
        out
    }
}

struct Case<'a> {
    refs: Vec<Option<&'a Analysis>>,
    keep: &'a [bool],
    profile: Option<Profile>,
    learned: Learned,
}

fn score(cases: &[Case], s: &Settings) -> (Score, Vec<Vec<Verdict>>) {
    let mut all = Score::default();
    let mut f1 = 0.0;
    let mut verdicts = vec![];
    for c in cases {
        let v = cull::cull_with(&c.refs, s, c.profile.as_ref(), &c.learned);
        let picked = v.iter().filter(|x| x.decision == Decision::Selected).count();
        let hit = v.iter().filter(|x| x.decision == Decision::Selected && c.keep[x.idx]).count();
        let kept = c.keep.iter().filter(|&&k| k).count();
        f1 += if picked + kept == 0 { 1.0 } else { 2.0 * hit as f32 / (picked + kept) as f32 };
        all.picked += picked;
        all.hit += hit;
        all.kept += kept;
        verdicts.push(v);
    }
    all.f1 = f1 / cases.len().max(1) as f32;
    all.precision = if all.picked > 0 { all.hit as f32 / all.picked as f32 } else { 0.0 };
    all.recall = if all.kept > 0 { all.hit as f32 / all.kept as f32 } else { 0.0 };
    (all, verdicts)
}

type Knob = (&'static str, Vec<f32>, fn(&Settings) -> f32, fn(&mut Settings, f32));

fn knobs() -> Vec<Knob> {
    vec![
        ("threshold", (0..=12).map(|k| 6.0 + k as f32 * 0.25).collect(), |s| s.threshold, |s, x| s.threshold = x),
        ("learned_weight", vec![0.2, 0.35, 0.5, 0.65, 0.8], |s| s.learned_weight, |s, x| s.learned_weight = x),
        ("rank_gain", vec![0.0, 0.5, 1.0, 1.5, 2.0, 3.0], |s| s.rank_gain, |s, x| s.rank_gain = x),
        ("keep_per_series", vec![1.0, 2.0], |s| s.keep_per_series as f32, |s, x| s.keep_per_series = x as usize),
        ("per_subject", vec![0.0, 1.0], |s| s.options.per_subject as u8 as f32, |s, x| s.options.per_subject = x > 0.5),
        ("speaker_max", (1..=6).map(|k| k as f32).collect(), |s| s.speaker_max as f32, |s, x| s.speaker_max = x as usize),
        ("subject_max", (1..=5).map(|k| k as f32).collect(), |s| s.subject_max as f32, |s, x| s.subject_max = x as usize),
        ("subject_fill", (0..=4).map(|k| k as f32).collect(), |s| s.subject_fill as f32, |s, x| s.subject_fill = x as usize),
        ("keep_each_shot", vec![0.0, 1.0], |s| s.options.keep_each_shot as u8 as f32, |s, x| s.options.keep_each_shot = x > 0.5),
        ("story", vec![0.0, 1.0], |s| s.options.story as u8 as f32, |s, x| s.options.story = x > 0.5),
    ]
}

/// Rounds over all the knobs, one knob at a time.
const ROUNDS: usize = 2;
/// A change must win at least this much (F1) to be taken: on ties the current setting stays.
const MIN_GAIN: f32 = 0.005;
/// All changes together must win at least this much, or the settings stay as they were.
const MIN_TOTAL: f32 = 0.01;

/// Check on `exams` and find the settings matching Sơn best, starting from `base`. `step(done,
/// total)` reports progress. `None` when there is nothing to check on.
pub fn tune(profile: &Profile, exams: &[Exam], base: &Settings, step: &(dyn Fn(usize, usize) + Sync)) -> Option<Tuned> {
    if exams.is_empty() {
        return None;
    }
    let knobs = knobs();
    let per_round: usize = knobs.iter().map(|k| k.1.len()).sum();
    let total = exams.len() + ROUNDS * per_round + 2;
    let mut done = 0;
    step(done, total);
    // Each show is checked with a taste learned from everything except that show.
    let cases: Vec<Case> = exams
        .iter()
        .map(|e| {
            let mut p = profile.clone();
            p.samples.retain(|s| !e.paths.contains(&s.name));
            p.w.clear();
            p.trained_on = 0;
            p.vis_at = 0;
            p.train();
            let p = (p.ready() || p.rank_pairs > 0).then_some(p);
            let refs: Vec<Option<&Analysis>> = e.items.iter().map(Some).collect();
            let learned = Learned::new(&refs, p.as_ref());
            done += 1;
            step(done, total);
            Case { refs, keep: &e.keep, profile: p, learned }
        })
        .collect();

    let mut s0 = base.clone();
    s0.options.ai_pick = true;
    s0.options.use_learned = true;
    s0.options.stars = StarMode::Off;
    s0.max_selected = None;
    let (before, _) = score(&cases, &s0);
    let mut best = s0.clone();
    let mut best_f1 = before.f1;
    for _ in 0..ROUNDS {
        for (_, values, get, set) in &knobs {
            let now = get(&best);
            for &x in values {
                done += 1;
                step(done, total);
                if (x - now).abs() < 1e-4 {
                    continue;
                }
                let mut s = best.clone();
                set(&mut s, x);
                let (sc, _) = score(&cases, &s);
                if sc.f1 > best_f1 + MIN_GAIN {
                    best_f1 = sc.f1;
                    best = s;
                }
            }
        }
    }
    if best_f1 < before.f1 + MIN_TOTAL {
        best = s0.clone();
    }
    let (after, verdicts) = score(&cases, &best);
    done += 1;
    step(done, total);
    let mut t = Tuned::from(&best);
    t.before = before;
    t.after = after;
    t.shows = exams.iter().map(|e| e.name.clone()).collect();
    t.photos = exams.iter().map(|e| e.items.len()).sum();
    t.changes = changes(&s0, &best);
    t.lessons = lessons(&cases, &verdicts);
    t.at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    step(total, total);
    Some(t)
}

fn onoff(x: bool) -> &'static str {
    if x { "bật" } else { "tắt" }
}

fn changes(a: &Settings, b: &Settings) -> Vec<String> {
    let mut out = vec![];
    let num = |x: f32| format!("{x:.2}").trim_end_matches('0').trim_end_matches('.').replace('.', ",");
    if (a.threshold - b.threshold).abs() > 1e-3 {
        out.push(format!("điểm để chọn {} → {}", num(a.threshold), num(b.threshold)));
    }
    if (a.learned_weight - b.learned_weight).abs() > 1e-3 {
        out.push(format!("mức dựa vào gu đã học {:.0}% → {:.0}%", a.learned_weight * 100.0, b.learned_weight * 100.0));
    }
    if (a.rank_gain - b.rank_gain).abs() > 1e-3 {
        out.push(format!("cách bạn chọn trong một loạt ảnh: hệ số {} → {}", num(a.rank_gain), num(b.rank_gain)));
    }
    if a.keep_per_series != b.keep_per_series {
        out.push(format!("giữ mỗi loạt ảnh trùng {} → {} tấm", a.keep_per_series, b.keep_per_series));
    }
    if a.options.per_subject != b.options.per_subject {
        out.push(format!("giới hạn số ảnh mỗi chủ thể: {}", onoff(b.options.per_subject)));
    }
    if b.options.per_subject {
        if a.speaker_max != b.speaker_max {
            out.push(format!("mỗi người phát biểu tối đa {} → {} tấm", a.speaker_max, b.speaker_max));
        }
        if a.subject_max != b.subject_max {
            out.push(format!("mỗi nhóm / nhân vật tối đa {} → {} tấm", a.subject_max, b.subject_max));
        }
        if a.subject_fill != b.subject_fill {
            out.push(format!("lấy thêm cho đủ {} → {} tấm mỗi chủ thể", a.subject_fill, b.subject_fill));
        }
    }
    if a.options.keep_each_shot != b.options.keep_each_shot {
        out.push(format!("cảnh chụp nhiều tấm giữ ít nhất 1 tấm: {}", onoff(b.options.keep_each_shot)));
    }
    if a.options.story != b.options.story {
        out.push(format!("đủ loại ảnh kể câu chuyện: {}", onoff(b.options.story)));
    }
    out
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

fn count(x: f32) -> String {
    let r = (x * 2.0).round() / 2.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r:.1}").replace('.', ",") }
}

/// How Sơn chooses, in words: share kept, photos per speaker / group / person, per burst, which
/// kinds of photo he keeps most and least, smiles.
fn lessons(cases: &[Case], verdicts: &[Vec<Verdict>]) -> Vec<String> {
    let mut out = vec![];
    let rates: Vec<f32> = cases.iter().map(|c| c.keep.iter().filter(|&&k| k).count() as f32 / c.keep.len().max(1) as f32 * 100.0).collect();
    if let Some(m) = median(rates.clone()) {
        let (lo, hi) = rates.iter().fold((f32::MAX, 0f32), |(l, h), &r| (l.min(r), h.max(r)));
        if rates.len() > 1 {
            out.push(format!("bạn giữ khoảng {m:.0}% số ảnh mỗi show (từ {lo:.0}% đến {hi:.0}%)"));
        } else {
            out.push(format!("bạn giữ khoảng {m:.0}% số ảnh của show"));
        }
    }
    let mut per_kind: HashMap<SubjectKind, Vec<f32>> = HashMap::new();
    let mut per_series: Vec<f32> = vec![];
    let mut scene: HashMap<Scene, (usize, usize)> = HashMap::new();
    let (mut smile, mut plain) = ((0usize, 0usize), (0usize, 0usize));
    for (c, v) in cases.iter().zip(verdicts) {
        let order = cull::timeline(v);
        for g in cull::subjects(&c.refs, v, &order) {
            if g.members.len() >= 3 {
                per_kind.entry(g.kind).or_default().push(g.members.iter().filter(|&&i| c.keep[i]).count() as f32);
            }
        }
        let mut series: HashMap<usize, (usize, usize)> = HashMap::new();
        for x in v {
            if let Some(b) = x.series {
                let e = series.entry(b).or_default();
                e.0 += 1;
                e.1 += c.keep[x.idx] as usize;
            }
            let e = scene.entry(x.scene).or_default();
            e.0 += 1;
            e.1 += c.keep[x.idx] as usize;
            if let Some(a) = c.refs[x.idx] {
                let main: Vec<_> = a.people.faces.iter().filter(|f| f.main && f.happy().is_some()).collect();
                if !main.is_empty() {
                    let happy = main.iter().filter_map(|f| f.happy()).sum::<f32>() / main.len() as f32 >= 0.5;
                    let slot = if happy { &mut smile } else { &mut plain };
                    slot.0 += 1;
                    slot.1 += c.keep[x.idx] as usize;
                }
            }
        }
        per_series.extend(series.values().filter(|(n, k)| *n >= 2 && *k >= 1).map(|&(_, k)| k as f32));
    }
    for (kind, what) in [(SubjectKind::Speaker, "mỗi người phát biểu"), (SubjectKind::Group, "mỗi nhóm chụp tập thể"), (SubjectKind::Person, "mỗi nhân vật chụp riêng")] {
        if let Some(v) = per_kind.get(&kind) {
            if let Some(m) = median(v.clone()) {
                out.push(format!("{what} bạn giữ khoảng {} tấm (xét {} lượt)", count(m), v.len()));
            }
        }
    }
    if per_series.len() >= 5 {
        let m = per_series.iter().sum::<f32>() / per_series.len() as f32;
        out.push(format!("trong một loạt ảnh gần giống nhau bạn thường giữ {} tấm", count(m)));
    }
    let mut rated: Vec<(Scene, f32, usize)> = scene.into_iter().filter(|(_, (n, _))| *n >= 15).map(|(s, (n, k))| (s, k as f32 / n as f32 * 100.0, n)).collect();
    rated.sort_by(|a, b| b.1.total_cmp(&a.1));
    if rated.len() >= 2 {
        let (top, low) = (&rated[0], &rated[rated.len() - 1]);
        if top.1 - low.1 >= 10.0 {
            out.push(format!("bạn hay giữ ảnh «{}» ({:.0}%), ít giữ ảnh «{}» ({:.0}%)", top.0.label(), top.1, low.0.label(), low.1));
        }
    }
    if smile.0 >= 10 && plain.0 >= 10 {
        let (a, b) = (smile.1 as f32 / smile.0 as f32 * 100.0, plain.1 as f32 / plain.0 as f32 * 100.0);
        if (a - b).abs() >= 5.0 {
            out.push(format!("ảnh mọi người cười bạn giữ {a:.0}%, ảnh không cười {b:.0}%"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(name: &str, t: f64, score: f32, k: u32) -> Analysis {
        let mut a = Analysis::default();
        a.name = name.into();
        a.time = Some(t);
        a.scores.exposure = score;
        a.scores.sharp = score;
        a.scores.comp = score;
        a.scores.color = score;
        a.scores.noise = score;
        a.exposure.mean = 118.0;
        a.signature = crate::metrics::Signature { bits: (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15), colors: vec![100; 192], aspect: 1.5 };
        a
    }

    #[test]
    fn finds_the_pick_score_he_uses() {
        // Two shows; he keeps every photo scoring 8.6 or more, nothing under. The default pick
        // score (7.5) takes too many: checking itself, the AI moves it up.
        let mk = |show: &str, n: u32| {
            let items: Vec<Analysis> = (0..n).map(|k| photo(&format!("{show}/p{k}.jpg"), k as f64 * 60.0, 6.0 + (k % 10) as f32 * 0.35, k + n)).collect();
            let keep: Vec<bool> = items.iter().map(|a| a.scores.exposure >= 8.6).collect();
            let paths = items.iter().map(|a| a.name.clone()).collect();
            Exam { name: show.into(), items, keep, paths }
        };
        let exams = vec![mk("/a", 60), mk("/b", 40)];
        let t = tune(&Profile::new("x"), &exams, &Settings::default(), &|_, _| {}).unwrap();
        assert!(t.before.f1 < 0.8, "{:?}", t.before);
        assert!(t.after.f1 > 0.95, "{:?}", t.after);
        assert!(t.threshold > 8.0 && t.threshold <= 8.75, "{}", t.threshold);
        assert!(t.changes.iter().any(|c| c.starts_with("điểm để chọn 7,5 →")), "{:?}", t.changes);
        assert!(t.lessons[0].contains("bạn giữ khoảng"), "{:?}", t.lessons);
        let mut s = Settings::default();
        assert!(t.apply(&mut s));
        assert_eq!(s.threshold, t.threshold);
        assert!(!t.apply(&mut s), "applying twice changes nothing");
        assert!(t.summary()[0].contains("2 show"));
    }

    /// Timing on a show of realistic size: `cargo test --release -- --ignored big_show`.
    #[test]
    #[ignore]
    fn big_show_is_checked_in_seconds() {
        let h = |x: u32| {
            let mut x = x.wrapping_mul(0x9E37_79B9);
            x ^= x >> 15;
            x = x.wrapping_mul(0x846C_A68B);
            x ^ (x >> 16)
        };
        let items: Vec<Analysis> = (0..1000u32)
            .map(|k| {
                let mut a = photo(&format!("/big/p{k}.jpg"), k as f64 * 4.0, 6.0 + (h(k) % 30) as f32 * 0.1, k);
                a.embed = (0..crate::visual::FULL as u32).map(|i| ((h(i ^ (k / 3) << 12) % 200) as i32 - 100) as i8).collect();
                a.people.faces = (0..(k % 4)).map(|j| crate::faces::FaceInfo {
                    bbox: [0.1 + j as f32 * 0.2, 0.3, 0.08, 0.1],
                    main: true,
                    eye_seen: true,
                    id: crate::faceid::quantise(&(0..128u32).map(|i| (h(i + 1000 * (k / 7 + j)) % 200) as f32 - 100.0).collect::<Vec<_>>()).unwrap(),
                    ..Default::default()
                }).collect();
                a
            })
            .collect();
        let keep: Vec<bool> = items.iter().map(|a| a.scores.exposure >= 8.4).collect();
        let paths = items.iter().map(|a| a.name.clone()).collect();
        let exams = vec![Exam { name: "big".into(), items, keep, paths }];
        let t0 = std::time::Instant::now();
        let t = tune(&Profile::new("x"), &exams, &Settings::default(), &|_, _| {}).unwrap();
        eprintln!("{:.1} s, {:?} -> {:?}, {:?}", t0.elapsed().as_secs_f32(), t.before.f1, t.after.f1, t.changes);
    }

    #[test]
    fn finds_shows_he_taught_in_the_work_folders() {
        let dir = std::env::temp_dir().join(format!("rawpick-exam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let show = dir.join("show1");
        let orig = show.join("anh goc");
        let mut p = Profile::new("x");
        let mut entries = serde_json::Map::new();
        for k in 0..40u32 {
            let path = orig.join(format!("p{k}.cr3")).display().to_string();
            let a = photo(&path, k as f64, 7.0 + (k % 4) as f32, k);
            entries.insert(format!("v7|{path}|100|5"), serde_json::to_value(&a).unwrap());
            p.samples.push(crate::learn::Sample::new(path, &a, k % 4 == 3, k % 4 == 3, true));
        }
        // A session he decided only partly by hand: not a check.
        for k in 0..40u32 {
            let path = dir.join("show2").join(format!("q{k}.cr3")).display().to_string();
            p.samples.push(crate::learn::Sample::new(path, &photo("", 0.0, 7.0, k), k < 5, k < 5, k < 3));
        }
        let data = dir.join("data");
        let work = crate::pipeline::work_dir(&data, &show);
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("cache.json"), serde_json::to_vec(&serde_json::json!({ "entries": entries })).unwrap()).unwrap();
        let exams = find_exams(&p, &data);
        assert_eq!(exams.len(), 1);
        assert_eq!(exams[0].name, "show1");
        assert_eq!(exams[0].items.len(), 40);
        assert_eq!(exams[0].keep.iter().filter(|&&k| k).count(), 10);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
