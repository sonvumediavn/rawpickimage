//! From measurements to decisions: weighted score, hard rejects, bursts, moments, limits.

use crate::analyze::{Analysis, CRITERIA};
use crate::learn::{self, Profile};
use crate::metrics;
use crate::options::{Options, StarMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    ClosedEyes,
    Blurry,
    Overexposed,
    Underexposed,
    Tilted,
    Composition,
    Noise,
    Duplicate,
    Broken,
    LowScore,
    /// Not starred on the camera while only starred photos count.
    NotStarred,
}

impl Reason {
    pub const ALL: [Reason; 11] = [
        Reason::ClosedEyes,
        Reason::Blurry,
        Reason::Overexposed,
        Reason::Underexposed,
        Reason::Tilted,
        Reason::Composition,
        Reason::Noise,
        Reason::Duplicate,
        Reason::Broken,
        Reason::LowScore,
        Reason::NotStarred,
    ];

    /// Sub-folder inside `3_Bi-loai` (ASCII so it survives any file system / zip tool).
    pub fn folder(self) -> &'static str {
        match self {
            Reason::ClosedEyes => "Nham-mat",
            Reason::Blurry => "Mo-net",
            Reason::Overexposed => "Chay-sang",
            Reason::Underexposed => "Thieu-sang",
            Reason::Tilted => "Nghieng",
            Reason::Composition => "Bo-cuc",
            Reason::Noise => "Nhieu",
            Reason::Duplicate => "Trung-lap",
            Reason::Broken => "Mat-loi",
            Reason::LowScore => "Diem-thap",
            Reason::NotStarred => "Khong-gan-sao",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Reason::ClosedEyes => "Nhắm mắt",
            Reason::Blurry => "Mờ, rung, trượt nét",
            Reason::Overexposed => "Cháy sáng / thừa sáng",
            Reason::Underexposed => "Thiếu sáng",
            Reason::Tilted => "Nghiêng",
            Reason::Composition => "Sai bố cục",
            Reason::Noise => "Nhiễu hạt nặng",
            Reason::Duplicate => "Trùng lặp",
            Reason::Broken => "File hỏng / không đọc được",
            Reason::LowScore => "Điểm thấp",
            Reason::NotStarred => "Không gắn sao trên máy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason", rename_all = "kebab-case")]
pub enum Decision {
    Selected,
    Review,
    Rejected(Reason),
}

impl Decision {
    pub fn folder(self) -> String {
        match self {
            Decision::Selected => "1_Da-chon".into(),
            Decision::Review => "2_Can-xem-lai".into(),
            Decision::Rejected(r) => format!("3_Bi-loai/{}", r.folder()),
        }
    }

    pub fn label(self) -> String {
        match self {
            Decision::Selected => "Đã chọn".into(),
            Decision::Review => "Cần xem lại".into(),
            Decision::Rejected(r) => format!("Bị loại: {}", r.label()),
        }
    }

    pub fn keep(self) -> bool {
        matches!(self, Decision::Selected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    Balanced,
    Event,
    Landscape,
    Portrait,
}

impl Preset {
    /// Weights in `CRITERIA` order: exposure, sharp, comp, color, noise, people.
    pub fn weights(self) -> [f32; 6] {
        match self {
            Preset::Balanced => [20.0, 25.0, 15.0, 10.0, 10.0, 20.0],
            Preset::Event => [20.0, 25.0, 10.0, 10.0, 5.0, 30.0],
            Preset::Landscape => [25.0, 15.0, 30.0, 20.0, 10.0, 5.0],
            Preset::Portrait => [20.0, 20.0, 10.0, 10.0, 5.0, 35.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub weights: [f32; 6],
    /// Final score needed for "Đã chọn".
    pub threshold: f32,
    /// Scores within this margin below the threshold go to "Cần xem lại".
    pub review_margin: f32,
    /// Similar photos taken within this many seconds count as one series.
    pub dedupe_seconds: f64,
    /// How many photos to keep from each series.
    pub keep_per_series: usize,
    /// A pause longer than this starts a new moment.
    pub moment_gap_seconds: f64,
    pub max_selected: Option<usize>,
    /// Seconds to add to each camera's clock (`Analysis::camera`) when merging cameras.
    pub camera_offsets: HashMap<String, f64>,
    /// Share of the final score taken from the learned profile once it is ready (0–1).
    pub learned_weight: f32,
    pub options: Options,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            weights: Preset::Event.weights(),
            threshold: 7.5,
            review_margin: 1.0,
            dedupe_seconds: 8.0,
            keep_per_series: 1,
            moment_gap_seconds: 90.0,
            max_selected: None,
            camera_offsets: HashMap::new(),
            learned_weight: 0.35,
            options: Options::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub idx: usize,
    /// Final score out of 10 (one decimal).
    pub score: f32,
    /// Score from the rules alone, before the learned profile.
    pub rule_score: f32,
    /// Probability that Sơn keeps it, from the learned profile.
    pub learned: Option<f32>,
    pub decision: Decision,
    /// What the app decided before any manual override.
    pub ai_decision: Decision,
    pub overridden: bool,
    pub notes: Vec<String>,
    /// Suggested fixes (only with "Sửa thay vì loại").
    pub fixes: Vec<String>,
    /// Series of near-identical shots this photo belongs to (index of the series' best photo).
    pub series: Option<usize>,
    pub moment: usize,
    /// Timeline position in seconds (camera offsets applied).
    pub time: Option<f64>,
}

fn r1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// Weighted average of the available criteria plus caps for faults no average should hide.
pub fn rule_score(a: &Analysis, w: &[f32; 6], fix_tilt: bool) -> (f32, Vec<String>) {
    let mut scores = a.scores;
    if fix_tilt && a.comp.horizon_found {
        // Straightening is a one-click fix, so judge the frame as if it were level.
        scores.comp = (scores.comp + (a.comp.tilt_deg.abs() * 0.9).min(4.0)).min(10.0);
    }
    let (mut sum, mut wsum) = (0f32, 0f32);
    for (k, name) in CRITERIA.iter().enumerate() {
        if let Some(v) = scores.get(name) {
            if w[k] > 0.0 {
                sum += v * w[k];
                wsum += w[k];
            }
        }
    }
    let mut s = if wsum > 0.0 { sum / wsum } else { 0.0 };
    let mut notes = vec![];
    let mut cap = |limit: f32, why: &str, s: &mut f32| {
        if *s > limit {
            *s = limit;
            notes.push(format!("Điểm bị giới hạn {limit}: {why}"));
        }
    };
    if a.scores.sharp < 3.0 {
        cap(4.5, "ảnh mờ", &mut s);
    }
    if a.scores.exposure < 2.5 {
        cap(5.0, "phơi sáng hỏng", &mut s);
    }
    if a.scores.noise < 2.5 {
        cap(6.5, "nhiễu nặng", &mut s);
    }
    if a.people.closed_eyes {
        cap(5.5, "có người nhắm mắt", &mut s);
    }
    (r1(s), notes)
}

/// Decide every photo. `items[i]` is `None` when the file could not be read.
pub fn cull(items: &[Option<&Analysis>], s: &Settings, profile: Option<&Profile>) -> Vec<Verdict> {
    let o = &s.options;
    let fix = o.fix_instead_of_reject;
    let profile = profile.filter(|p| o.use_learned && p.ready());
    // Camera stars. A folder where every file is read-only was copied that way (a disc, a
    // shared drive): the camera's protect lock says nothing there.
    let all_locked = {
        let ok: Vec<&Analysis> = items.iter().flatten().copied().collect();
        ok.len() >= 5 && ok.iter().all(|a| a.protected)
    };
    let min_stars = o.min_stars.max(1);
    let starred = |a: &Analysis| a.stars >= min_stars || (a.protected && !all_locked);
    let stars_on = o.stars != StarMode::Off;
    // Comparing every photo with the learned ones is the slow part: all cores at once.
    let learned_all: Vec<Option<f32>> = {
        use rayon::prelude::*;
        items.par_iter().map(|a| a.and_then(|a| profile.and_then(|p| p.predict(&learn::features(a), &a.embed)))).collect()
    };
    let mut out: Vec<Verdict> = items
        .iter()
        .enumerate()
        .map(|(idx, a)| {
            let Some(a) = a else {
                let d = Decision::Rejected(Reason::Broken);
                return Verdict {
                    idx,
                    score: 0.0,
                    rule_score: 0.0,
                    learned: None,
                    decision: d,
                    ai_decision: d,
                    overridden: false,
                    notes: vec!["Không đọc được file, có thể thẻ nhớ lỗi hoặc file chép dở".into()],
                    fixes: vec![],
                    series: None,
                    moment: 0,
                    time: None,
                };
            };
            let (rule, mut notes) = rule_score(a, &s.weights, fix);
            let learned = learned_all[idx];
            // The more Sơn has taught (and the better the model fits him), the more his taste counts.
            let trust = profile.map(|p| p.trust(s.learned_weight)).unwrap_or(0.0);
            let score = match learned {
                Some(p) => r1((1.0 - trust) * rule + trust * 10.0 * p),
                None => rule,
            };
            let mut fixes = vec![];
            let mut decision = first_decision(a, score, s, &mut notes, &mut fixes);
            if let Some(p) = learned {
                decision = learned_override(decision, p, trust, score, s, &mut notes);
            }
            if stars_on && !starred(a) {
                decision = Decision::Rejected(Reason::NotStarred);
            }
            let offset = if o.multi_camera { s.camera_offsets.get(&a.camera).copied().unwrap_or(0.0) } else { 0.0 };
            Verdict {
                idx,
                score,
                rule_score: rule,
                learned,
                decision,
                ai_decision: decision,
                overridden: false,
                notes,
                fixes,
                series: None,
                moment: 0,
                time: a.time.map(|t| t + offset),
            }
        })
        .collect();

    // Timeline order: by time when known, then by name order (input order).
    let mut order: Vec<usize> = (0..out.len()).collect();
    order.sort_by(|&x, &y| match (out[x].time, out[y].time) {
        (Some(a), Some(b)) => a.total_cmp(&b).then(x.cmp(&y)),
        _ => x.cmp(&y),
    });

    if o.dedupe {
        dedupe(items, &mut out, &order, s);
    }
    assign_moments(&mut out, &order, s.moment_gap_seconds);
    if o.moments {
        cover_moments(items, &mut out, s);
    }
    if let Some(n) = s.max_selected {
        limit(&mut out, n);
    }
    for v in &mut out {
        // Sơn's stars are his own choice: they win over anything the AI did afterwards.
        let mine = match (stars_on, items[v.idx]) {
            (true, Some(a)) if !starred(a) => {
                v.decision = Decision::Rejected(Reason::NotStarred);
                true
            }
            (true, Some(a)) => {
                let how = if a.stars > 0 { format!("Bạn gắn {} sao trên máy ảnh", a.stars) } else { "Bạn khóa bảo vệ ảnh này trên máy ảnh".to_string() };
                v.notes.insert(0, how);
                if o.stars == StarMode::Pick {
                    v.decision = Decision::Selected;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        v.ai_decision = v.decision;
        // AI picking off: keep its opinion for display, leave the choice to Sơn.
        if !o.ai_pick && !mine && v.decision != Decision::Rejected(Reason::Broken) {
            v.decision = Decision::Review;
        }
    }
    out
}

/// Sơn's learned taste may overrule the fixed rules, except for things he always wants out
/// (closed eyes of the main person, broken files, real blur) and duplicates.
fn learned_override(d: Decision, p: f32, trust: f32, score: f32, s: &Settings, notes: &mut Vec<String>) -> Decision {
    if trust < 0.3 {
        return d;
    }
    match d {
        Decision::Rejected(r) if !matches!(r, Reason::ClosedEyes | Reason::Broken | Reason::Duplicate) && p >= 0.75 => {
            notes.push(format!("AI đã học: bạn hay giữ kiểu ảnh này ({:.0}%), nên không loại vì «{}»", p * 100.0, r.label()));
            if score >= s.threshold && p >= 0.85 {
                Decision::Selected
            } else {
                Decision::Review
            }
        }
        Decision::Selected if p <= 0.25 => {
            notes.push(format!("AI đã học: bạn thường bỏ kiểu ảnh này (chỉ {:.0}% giữ)", p * 100.0));
            Decision::Review
        }
        _ => d,
    }
}

fn first_decision(a: &Analysis, score: f32, s: &Settings, notes: &mut Vec<String>, fixes: &mut Vec<String>) -> Decision {
    let fix = s.options.fix_instead_of_reject;
    let review_floor = s.threshold - s.review_margin;
    let rescue = |fixes: &mut Vec<String>, what: String| {
        fixes.push(what);
        Decision::Review
    };
    if a.people.closed_eyes && s.options.reject_closed_eyes {
        return Decision::Rejected(Reason::ClosedEyes);
    }
    if a.scores.exposure < 2.5 {
        let bright = a.exposure.mean > 118.0 || a.exposure.clip_hi > 0.08;
        let salvageable = a.scores.exposure >= 1.2 && a.exposure.clip_hi < 0.12 && a.exposure.clip_lo < 0.25;
        if fix && salvageable && score >= review_floor - 2.0 {
            return rescue(fixes, if bright { "Giảm phơi sáng khoảng 1 stop từ file RAW".into() } else { "Tăng phơi sáng, kéo vùng tối từ file RAW".into() });
        }
        return Decision::Rejected(if bright { Reason::Overexposed } else { Reason::Underexposed });
    }
    // After exposure: a very dark or blown frame also measures as soft.
    if a.scores.sharp < 2.5 {
        return Decision::Rejected(Reason::Blurry);
    }
    let tilt = if a.comp.horizon_found { a.comp.tilt_deg.abs() } else { 0.0 };
    if tilt >= 2.0 {
        if fix && tilt <= 10.0 {
            fixes.push(format!("Xoay thẳng {:.1}° rồi crop nhẹ", a.comp.tilt_deg));
        } else if tilt >= 4.0 {
            return Decision::Rejected(Reason::Tilted);
        }
    }
    if a.scores.comp < 3.0 {
        if fix && a.comp.border_pen > 1.0 && score >= review_floor - 1.0 {
            return rescue(fixes, "Chủ thể sát mép: thử crop lại cho thoáng".into());
        }
        return Decision::Rejected(Reason::Composition);
    }
    if a.scores.noise < 2.0 {
        if fix && score >= review_floor - 1.0 {
            return rescue(fixes, "Khử nhiễu khi xuất ảnh".into());
        }
        return Decision::Rejected(Reason::Noise);
    }
    if score >= s.threshold && s.options.review_open_mouth && lead_mouth_open(a) {
        notes.push("Nhân vật chính đang há miệng, khẩu hình chưa đẹp: để bạn xem lại".into());
        return Decision::Review;
    }
    if score >= s.threshold {
        if !fixes.is_empty() {
            notes.push("Đạt điểm chọn nhưng cần sửa nhẹ trước khi giao".into());
            return Decision::Review;
        }
        Decision::Selected
    } else if score >= review_floor {
        Decision::Review
    } else {
        Decision::Rejected(Reason::LowScore)
    }
}

/// Whether one of the two largest main faces has the mouth wide open.
fn lead_mouth_open(a: &Analysis) -> bool {
    let mut main: Vec<_> = a.people.faces.iter().filter(|f| f.main && !f.small).collect();
    main.sort_by(|x, y| (y.bbox[2] * y.bbox[3]).total_cmp(&(x.bbox[2] * x.bbox[3])));
    main.iter().take(2).any(|f| f.mouth_open > 0.3)
}

/// Group consecutive near-identical photos; keep the best `keep_per_series`.
/// Two frames of the same shot: the quick pixel fingerprint matches, or the image model sees the
/// same picture (it is not fooled by a turned head, a small pan or zoom, or a flash that did not
/// fire). Without capture times to back it up, only near-identical frames count.
fn same_shot(a: &Analysis, b: &Analysis, timed: bool) -> bool {
    if metrics::similar(&a.signature, &b.signature) {
        return true;
    }
    if (a.signature.aspect - b.signature.aspect).abs() > 0.05 {
        return false;
    }
    let content = crate::visual::similarity_content(&a.embed, &b.embed);
    if timed {
        content >= SAME_SHOT
    } else {
        content >= SAME_SHOT_UNTIMED && crate::visual::similarity(&a.embed, &b.embed) >= SAME_SHOT_UNTIMED - 0.03
    }
}

/// Image-model similarity for frames a few seconds apart to count as one shot (measured: re-framed,
/// rotated, brightened copies of one photo score 0.84–0.99, different photos below 0.4).
const SAME_SHOT: f32 = 0.86;
const SAME_SHOT_UNTIMED: f32 = 0.94;

fn dedupe(items: &[Option<&Analysis>], out: &mut [Verdict], order: &[usize], s: &Settings) {
    let mut series: Vec<Vec<usize>> = vec![];
    for &i in order {
        let Some(a) = items[i] else { continue };
        let joined = series.last_mut().filter(|g| {
            let &last = g.last().unwrap();
            let timed = match (out[i].time, out[last].time) {
                (Some(t1), Some(t2)) => Some((t1 - t2).abs() <= s.dedupe_seconds),
                _ => None,
            };
            if timed == Some(false) {
                return false;
            }
            // Same shot as any photo of the series so far (people move, the camera pans a little
            // between frames, so comparing with the last frame alone splits real bursts).
            g.iter().rev().take(12).any(|&k| items[k].is_some_and(|b| same_shot(a, b, timed.is_some())))
        });
        match joined {
            Some(g) => g.push(i),
            None => series.push(vec![i]),
        }
    }
    for g in series.into_iter().filter(|g| g.len() > 1) {
        let mut ranked = g.clone();
        ranked.sort_by(|&x, &y| out[y].score.total_cmp(&out[x].score).then(x.cmp(&y)));
        let best = ranked[0];
        let best_name = items[best].map(|a| a.name.clone()).unwrap_or_default();
        for (rank, &i) in ranked.iter().enumerate() {
            out[i].series = Some(best);
            let keepable = matches!(out[i].decision, Decision::Selected | Decision::Review);
            if rank >= s.keep_per_series.max(1) && keepable {
                out[i].decision = Decision::Rejected(Reason::Duplicate);
                out[i].notes.push(format!("Trùng với {best_name} (ảnh đẹp nhất trong loạt {} ảnh)", g.len()));
            }
        }
    }
}

fn assign_moments(out: &mut [Verdict], order: &[usize], gap: f64) {
    let mut m = 0;
    let mut prev: Option<f64> = None;
    for &i in order {
        if let (Some(t), Some(p)) = (out[i].time, prev) {
            if t - p > gap {
                m += 1;
            }
        }
        if out[i].time.is_some() {
            prev = out[i].time;
        }
        out[i].moment = m;
    }
}

/// Each moment keeps at least its best usable photo.
fn cover_moments(items: &[Option<&Analysis>], out: &mut [Verdict], s: &Settings) {
    let n_moments = out.iter().map(|v| v.moment + 1).max().unwrap_or(0);
    for m in 0..n_moments {
        if out.iter().any(|v| v.moment == m && v.decision == Decision::Selected) {
            continue;
        }
        let floor = s.threshold - 2.5 * s.review_margin.max(0.5);
        let best = out
            .iter()
            .filter(|v| v.moment == m && v.score >= floor)
            .filter(|v| matches!(v.decision, Decision::Review | Decision::Rejected(Reason::LowScore) | Decision::Rejected(Reason::Composition)))
            .filter(|v| items[v.idx].is_some_and(|a| !a.people.closed_eyes))
            .max_by(|a, b| a.score.total_cmp(&b.score))
            .map(|v| v.idx);
        if let Some(i) = best {
            out[i].decision = Decision::Selected;
            out[i].notes.push("Giữ lại để không thiếu khoảnh khắc này".into());
        }
    }
}

/// Keep at most `n` selected photos; each moment's best goes in first.
fn limit(out: &mut [Verdict], n: usize) {
    let mut sel: Vec<usize> = out.iter().filter(|v| v.decision == Decision::Selected).map(|v| v.idx).collect();
    if sel.len() <= n {
        return;
    }
    sel.sort_by(|&x, &y| out[y].score.total_cmp(&out[x].score));
    let mut rank_in_moment = HashMap::new();
    let mut key: Vec<(usize, usize)> = vec![];
    for &i in &sel {
        let r = rank_in_moment.entry(out[i].moment).or_insert(0usize);
        key.push((*r, i));
        *r += 1;
    }
    key.sort_by(|a, b| a.0.cmp(&b.0).then(out[b.1].score.total_cmp(&out[a.1].score)));
    for &(_, i) in &key[n..] {
        out[i].decision = Decision::Review;
        out[i].notes.push(format!("Vượt giới hạn {n} ảnh đã chọn"));
    }
}

/// Apply Sơn's manual choice to one photo.
/// Sơn's own choice. It counts as his even when it matches the AI (with AI picking off, the
/// AI's opinion is only advice).
pub fn set_manual(v: &mut Verdict, d: Decision) {
    v.decision = d;
    v.overridden = true;
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub total: usize,
    pub selected: usize,
    pub review: usize,
    pub rejected: usize,
    pub by_reason: Vec<(String, usize)>,
    pub moments: usize,
    pub series: usize,
}

pub fn summarize(v: &[Verdict]) -> Summary {
    let mut by: HashMap<Reason, usize> = HashMap::new();
    let mut s = Summary { total: v.len(), ..Default::default() };
    for x in v {
        match x.decision {
            Decision::Selected => s.selected += 1,
            Decision::Review => s.review += 1,
            Decision::Rejected(r) => {
                s.rejected += 1;
                *by.entry(r).or_default() += 1;
            }
        }
    }
    s.by_reason = Reason::ALL.iter().filter_map(|r| by.get(r).map(|n| (r.label().to_string(), *n))).collect();
    s.moments = v.iter().map(|x| x.moment + 1).max().unwrap_or(0);
    s.series = v.iter().filter_map(|x| x.series).collect::<std::collections::HashSet<_>>().len();
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Signature;

    fn photo(name: &str, t: f64, sc: [f32; 5], bits: u64) -> Analysis {
        let mut a = Analysis::default();
        a.name = name.into();
        a.time = Some(t);
        a.scores.exposure = sc[0];
        a.scores.sharp = sc[1];
        a.scores.comp = sc[2];
        a.scores.color = sc[3];
        a.scores.noise = sc[4];
        a.exposure.mean = 118.0;
        a.signature = Signature { bits, colors: vec![100; 192], aspect: 1.5 };
        a
    }

    #[test]
    fn decisions_follow_scores_and_faults() {
        let good = photo("a", 0.0, [9.0, 9.0, 8.0, 8.0, 9.0], 1);
        let blurry = photo("b", 100.0, [9.0, 1.5, 8.0, 8.0, 9.0], 2);
        let mut eyes = photo("c", 200.0, [9.0, 9.0, 8.0, 8.0, 9.0], 3);
        eyes.people.closed_eyes = true;
        eyes.scores.people = Some(3.0);
        let mid = photo("d", 300.0, [7.0, 7.0, 6.5, 7.0, 7.0], 4);
        let items = vec![Some(&good), Some(&blurry), Some(&eyes), Some(&mid), None];
        let v = cull(&items, &Settings::default(), None);
        assert_eq!(v[0].decision, Decision::Selected);
        assert_eq!(v[1].decision, Decision::Rejected(Reason::Blurry));
        assert!(v[1].score <= 4.5);
        assert_eq!(v[2].decision, Decision::Rejected(Reason::ClosedEyes));
        assert_eq!(v[3].decision, Decision::Review);
        assert_eq!(v[4].decision, Decision::Rejected(Reason::Broken));
    }

    #[test]
    fn keeps_best_of_a_burst() {
        let a = photo("a", 0.0, [8.0, 8.0, 8.0, 8.0, 8.0], 0xFF);
        let b = photo("b", 1.0, [9.0, 9.0, 9.0, 9.0, 9.0], 0xFF);
        let c = photo("c", 2.0, [8.5, 8.5, 8.5, 8.5, 8.5], 0xFE);
        let d = photo("d", 3.0, [9.0, 9.0, 9.0, 9.0, 9.0], !0xFF);
        let items = vec![Some(&a), Some(&b), Some(&c), Some(&d)];
        let v = cull(&items, &Settings::default(), None);
        assert_eq!(v[1].decision, Decision::Selected);
        assert_eq!(v[0].decision, Decision::Rejected(Reason::Duplicate));
        assert_eq!(v[2].decision, Decision::Rejected(Reason::Duplicate));
        assert_eq!(v[3].decision, Decision::Selected, "different picture is not a duplicate");
        assert_eq!(v[0].series, Some(1));
    }

    #[test]
    fn moments_and_limits() {
        let a = photo("a", 0.0, [9.0; 5], 1);
        let b = photo("b", 10.0, [8.0; 5], 0xF0F0);
        let c = photo("c", 1000.0, [6.0; 5], 3);
        let items = vec![Some(&a), Some(&b), Some(&c)];
        let mut s = Settings::default();
        let v = cull(&items, &s, None);
        assert_eq!((v[0].moment, v[2].moment), (0, 1));
        assert_ne!(v[2].decision, Decision::Selected);
        s.options.moments = true;
        s.max_selected = Some(2);
        let v = cull(&items, &s, None);
        assert_eq!(v[2].decision, Decision::Selected, "only photo of its moment is kept");
        assert_eq!(v[0].decision, Decision::Selected);
        assert_eq!(v[1].decision, Decision::Review, "limit drops the second photo of moment 0, not moment 1's only photo");
    }

    #[test]
    fn fix_instead_of_reject_rescues_tilt() {
        let mut a = photo("a", 0.0, [9.0, 9.0, 5.0, 9.0, 9.0], 1);
        a.comp.horizon_found = true;
        a.comp.tilt_deg = 5.0;
        let items = vec![Some(&a)];
        let mut s = Settings::default();
        assert_eq!(cull(&items, &s, None)[0].decision, Decision::Rejected(Reason::Tilted));
        s.options.fix_instead_of_reject = true;
        let v = cull(&items, &s, None);
        assert_eq!(v[0].decision, Decision::Review);
        assert!(v[0].fixes[0].starts_with("Xoay thẳng"));
    }

    #[test]
    fn camera_stars_filter_or_pick() {
        let mut items: Vec<Analysis> = (0..6u64).map(|i| photo(&format!("p{i}"), i as f64 * 600.0, [8.0; 5], i.wrapping_mul(0x9e37_79b9_7f4a_7c15))).collect();
        items[1].stars = 3;
        items[4].protected = true;
        let refs: Vec<Option<&Analysis>> = items.iter().map(Some).collect();
        let mut s = Settings::default();
        s.options.stars = StarMode::Pick;
        let v = cull(&refs, &s, None);
        let picked: Vec<usize> = v.iter().filter(|x| x.decision.keep()).map(|x| x.idx).collect();
        assert_eq!(picked, vec![1, 4]);
        assert!(v.iter().filter(|x| !x.decision.keep()).all(|x| x.decision == Decision::Rejected(Reason::NotStarred)));
        // Filter: the starred ones still go through the AI; the rest are out.
        s.options.stars = StarMode::Filter;
        let v = cull(&refs, &s, None);
        assert!(v.iter().filter(|x| ![1, 4].contains(&x.idx)).all(|x| x.decision == Decision::Rejected(Reason::NotStarred)));
        assert!(v[1].decision != Decision::Rejected(Reason::NotStarred));
        // Everything read-only (copied from a disc): the lock means nothing.
        for a in &mut items {
            a.protected = true;
        }
        let refs: Vec<Option<&Analysis>> = items.iter().map(Some).collect();
        s.options.stars = StarMode::Pick;
        let v = cull(&refs, &s, None);
        assert_eq!(v.iter().filter(|x| x.decision.keep()).map(|x| x.idx).collect::<Vec<_>>(), vec![1]);
    }
}
