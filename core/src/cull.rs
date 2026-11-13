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
    /// Kind of event photo (overview, speaker, detail…), a suggestion.
    #[serde(default)]
    pub scene: crate::scene::Scene,
    /// Key people Sơn marked who are in this photo.
    #[serde(default)]
    pub key_people: Vec<u32>,
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
                    scene: Default::default(),
                    key_people: vec![],
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
            let mut key_people: Vec<u32> = a.key_faces.iter().map(|&(_, p)| p).collect();
            key_people.sort_unstable();
            key_people.dedup();
            if o.key_people && !key_people.is_empty() {
                if let Decision::Rejected(r @ (Reason::LowScore | Reason::Composition | Reason::Noise | Reason::Tilted | Reason::Underexposed | Reason::Overexposed)) = decision {
                    decision = Decision::Review;
                    notes.push(format!("Có nhân vật quan trọng: không loại vì {}, để bạn xem", r.label().to_lowercase()));
                }
            }
            if a.width > 0 && a.width.max(a.height) < LOW_RES {
                notes.push(format!("Độ phân giải thấp ({}×{}): đủ đăng web, chưa đủ in lớn", a.width, a.height));
            }
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
                scene: crate::scene::classify(a),
                key_people,
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
        // Which frame of a series Sơn keeps, once learned.
        let gain = profile.map(|p| 2.5 * (2.0 * p.rank_acc - 1.0).max(0.0)).unwrap_or(0.0);
        let rank: Vec<Option<f32>> = items.iter().map(|a| a.filter(|_| gain > 0.1).and_then(|a| profile.and_then(|p| p.rank(&learn::features(a))))).collect();
        dedupe(items, &mut out, &order, s, &rank, gain);
    }
    assign_moments(&mut out, &order, s.moment_gap_seconds);
    if o.moments {
        cover_moments(items, &mut out, s);
    }
    if o.key_people {
        cover_key_people(items, &mut out, s);
    }
    if o.story {
        cover_story(items, &mut out, s);
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
    if score >= s.threshold && s.options.speaker_head_up && speaker_head_down(a) {
        notes.push("Người phát biểu đang cúi đầu: để bạn xem lại (ảnh có người khác cùng khung thì cúi đầu vẫn giữ)".into());
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
    main.iter().take(2).any(|f| f.mouth_bad)
}

/// Group consecutive near-identical photos; keep the best `keep_per_series`.
/// Two frames of the same shot: the quick pixel fingerprint matches, or the image model sees the
/// same picture (it is not fooled by a turned head, a small pan or zoom, or a flash that did not
/// fire) with the same people in the same places. `gap` = seconds between them, when known.
fn same_shot(a: &Analysis, b: &Analysis, gap: Option<f64>) -> bool {
    // Someone at the same spot is another person (the next student shaking the same leader's
    // hand, the next group): never one shot, however alike the pictures are.
    if other_people(a, b) {
        return false;
    }
    let content = crate::visual::similarity_content(&a.embed, &b.embed);
    // Same stage, same backdrop, other people (award groups called up one after another):
    // the picture looks alike, even to the pixel fingerprint, the people do not.
    // Only frames of one press of the shutter may differ in faces found (a head turns away
    // and the detector loses it).
    if !same_people_layout(a, b) {
        let burst = gap.is_some_and(|g| g <= BURST_GAP);
        return burst && (content >= SAME_PICTURE || (a.embed.is_empty() && metrics::similar(&a.signature, &b.signature)));
    }
    if metrics::similar(&a.signature, &b.signature) {
        return true;
    }
    if (a.signature.aspect - b.signature.aspect).abs() > 0.05 {
        return false;
    }
    match gap {
        // Pressed again within a second or two: the same shot even if people moved a lot.
        Some(g) if g <= BURST_GAP => content >= SAME_SHOT_BURST,
        // The same group a few seconds later (people shifting, smiling, the photographer
        // re-framing): the faces say it is them.
        Some(_) if same_group(a, b) => content >= SAME_SHOT_BURST,
        Some(_) => content >= SAME_SHOT,
        None => content >= SAME_SHOT_UNTIMED && crate::visual::similarity(&a.embed, &b.embed) >= SAME_SHOT_UNTIMED - 0.03,
    }
}

/// Whether two frames show about the same number of people in about the same places.
fn same_people_layout(a: &Analysis, b: &Analysis) -> bool {
    let centres = |x: &Analysis| -> Vec<(f32, f32)> {
        x.people.faces.iter().filter(|f| f.main && !f.small).map(|f| (f.bbox[0] + f.bbox[2] / 2.0, f.bbox[1] + f.bbox[3] / 2.0)).collect()
    };
    let (ca, cb) = (centres(a), centres(b));
    let (lo, hi) = (ca.len().min(cb.len()), ca.len().max(cb.len()));
    if hi <= 1 {
        return true;
    }
    if lo == 0 || hi - lo >= 2 {
        return false;
    }
    let (small, big) = if ca.len() <= cb.len() { (&ca, &cb) } else { (&cb, &ca) };
    let matched = small.iter().filter(|p| big.iter().any(|q| (p.0 - q.0).abs() < 0.1 && (p.1 - q.1).abs() < 0.12)).count();
    matched as f32 >= 0.6 * hi as f32
}

/// Whether a face in one frame is, by its fingerprint, surely someone who is not in the other
/// frame, while a face stands at the same spot there. Only faces seen well enough count: large,
/// facing the camera.
fn other_people(a: &Analysis, b: &Analysis) -> bool {
    use crate::faceid::{similarity, OTHER_PERSON, SAME_PERSON};
    let (fa, fb) = (known_faces(a), known_faces(b));
    if fa.is_empty() || fb.is_empty() {
        return false;
    }
    let centre = |f: &crate::faces::FaceInfo| (f.bbox[0] + f.bbox[2] / 2.0, f.bbox[1] + f.bbox[3] / 2.0);
    let frontal = |f: &crate::faces::FaceInfo| f.yaw.abs() < 0.3 && !f.turned;
    let stranger = |f: &crate::faces::FaceInfo, others: &[&crate::faces::FaceInfo]| {
        if !frontal(f) || others.iter().any(|g| similarity(&f.id, &g.id).unwrap_or(0.0) >= SAME_PERSON) {
            return false;
        }
        let (cx, cy) = centre(f);
        let near: Vec<_> = others.iter().filter(|g| frontal(g) && (centre(g).0 - cx).abs() < 0.1 && (centre(g).1 - cy).abs() < 0.12).collect();
        !near.is_empty() && near.iter().all(|g| similarity(&f.id, &g.id).unwrap_or(1.0) < OTHER_PERSON)
    };
    fa.iter().any(|f| stranger(f, &fb)) || fb.iter().any(|f| stranger(f, &fa))
}

/// Two people or more, and by their fingerprints the same people in both frames.
fn same_group(a: &Analysis, b: &Analysis) -> bool {
    use crate::faceid::{similarity, SAME_PERSON};
    let (fa, fb) = (known_faces(a), known_faces(b));
    let (lo, hi) = (fa.len().min(fb.len()), fa.len().max(fb.len()));
    if lo < 2 || hi - lo > 1 {
        return false;
    }
    let (small, big) = if fa.len() <= fb.len() { (&fa, &fb) } else { (&fb, &fa) };
    let found = small.iter().filter(|f| big.iter().any(|g| similarity(&f.id, &g.id).unwrap_or(0.0) >= SAME_PERSON)).count();
    found as f32 >= 0.75 * lo as f32
}

fn known_faces(x: &Analysis) -> Vec<&crate::faces::FaceInfo> {
    x.people.faces.iter().filter(|f| f.main && !f.small && !f.id.is_empty()).collect()
}

/// Image-model similarity for frames a few seconds apart to count as one shot (measured: re-framed,
/// rotated, brightened copies of one photo score 0.84–0.99, different photos below 0.4).
const SAME_SHOT: f32 = 0.86;
const SAME_SHOT_UNTIMED: f32 = 0.94;
/// Frames this close (seconds) are one press of the shutter, people moving or not.
const BURST_GAP: f64 = 2.0;
const SAME_SHOT_BURST: f32 = 0.78;
/// Within one press of the shutter, a frame where the detector found other faces still counts
/// when the picture is practically the same.
const SAME_PICTURE: f32 = 0.9;
/// A series longer than this (seconds) is a long speech or performance, not one shot: it starts
/// a new series so it keeps a few photos.
const SERIES_SPAN: f64 = 45.0;
/// A shot taken this many times was meant: its best frame is kept when usable.
const MEANT_SHOT: usize = 3;
/// The same for a frame with two people or more.
const MEANT_GROUP: usize = 2;

/// The rules want this photo looked at by Sơn rather than picked (mouth wide open, a speaker
/// looking down).
fn flagged(a: &Analysis, s: &Settings) -> bool {
    (s.options.review_open_mouth && lead_mouth_open(a)) || (s.options.speaker_head_up && speaker_head_down(a))
}

fn speaker_head_down(a: &Analysis) -> bool {
    a.people.solo().is_some_and(|f| f.down)
}

/// Best frame first: a frame the rules would pick, then the score, then (with several people)
/// the frame where everyone has their eyes open and heads up.
fn frame_rank(a: Option<&Analysis>, v: &Verdict, s: &Settings, learned: Option<f32>, gain: f32) -> f32 {
    let mut k = v.score;
    if let Some(r) = learned {
        k += gain * r;
    }
    if let Some(a) = a {
        if a.people.faces.iter().filter(|f| f.main).count() >= 2 {
            k += 2.0 * (a.people.open_share() - 1.0) + 0.6 * (a.people.up_share() - 1.0);
        }
        if flagged(a, s) {
            k -= 1.5;
        }
    }
    // Usable frames first. With Sơn's learned way of choosing inside a series, a frame the
    // rules left for review can still be the one he would keep.
    let usable = if learned.is_some() { matches!(v.decision, Decision::Selected | Decision::Review) } else { v.decision == Decision::Selected };
    if usable {
        k += 100.0;
    }
    k
}

/// Frames of the same shot, in timeline `order`: each group is one burst or one group photo
/// taken again. `times` are capture times (with camera offsets).
pub fn series(items: &[Option<&Analysis>], times: &[Option<f64>], order: &[usize], dedupe_seconds: f64) -> Vec<Vec<usize>> {
    let mut series: Vec<Vec<usize>> = vec![];
    for &i in order {
        let Some(a) = items[i] else { continue };
        let ti = times[i];
        let joined = series.last_mut().filter(|g| {
            let (&first, &last) = (g.first().unwrap(), g.last().unwrap());
            if let (Some(t), Some(l)) = (ti, times[last]) {
                if (t - l).abs() > dedupe_seconds {
                    return false;
                }
            }
            if let (Some(t), Some(f)) = (ti, times[first]) {
                if t - f > SERIES_SPAN.max(dedupe_seconds * 3.0) {
                    return false;
                }
            }
            // Same shot as a photo of the series taken just before (people move, the camera pans
            // a little, so comparing with the last frame alone splits real bursts; comparing with
            // frames long before would chain different groups together).
            g.iter().rev().take(12).any(|&k| {
                let gap = match (ti, times[k]) {
                    (Some(t), Some(u)) => Some((t - u).abs()),
                    _ => None,
                };
                gap.is_none_or(|g| g <= dedupe_seconds) && items[k].is_some_and(|b| same_shot(a, b, gap))
            })
        });
        match joined {
            Some(g) => g.push(i),
            None => series.push(vec![i]),
        }
    }
    series
}

fn dedupe(items: &[Option<&Analysis>], out: &mut [Verdict], order: &[usize], s: &Settings, rank: &[Option<f32>], rank_gain: f32) {
    let times: Vec<Option<f64>> = out.iter().map(|v| v.time).collect();
    let series = series(items, &times, order, s.dedupe_seconds);
    for g in series.into_iter().filter(|g| g.len() > 1) {
        let mut ranked = g.clone();
        let key: HashMap<usize, f32> = g.iter().map(|&i| (i, frame_rank(items[i], &out[i], s, rank[i], rank_gain))).collect();
        ranked.sort_by(|&x, &y| key[&y].total_cmp(&key[&x]).then(x.cmp(&y)));
        let best = ranked[0];
        // The rules picked another frame of this shot, but Sơn's way of choosing prefers this one.
        if rank[best].is_some() && out[best].decision == Decision::Review && g.iter().any(|&i| out[i].decision == Decision::Selected) {
            out[best].decision = Decision::Selected;
            out[best].notes.push("AI đã học: trong một loạt ảnh, bạn hay chọn tấm như tấm này".into());
        }
        let best_name = items[best].map(|a| a.name.clone()).unwrap_or_default();
        for (rank, &i) in ranked.iter().enumerate() {
            out[i].series = Some(best);
            let keepable = matches!(out[i].decision, Decision::Selected | Decision::Review);
            if rank >= s.keep_per_series.max(1) && keepable {
                out[i].decision = Decision::Rejected(Reason::Duplicate);
                out[i].notes.push(format!("Trùng với {best_name} (ảnh đẹp nhất trong loạt {} ảnh)", g.len()));
            }
        }
        // Several frames of one shot: Sơn meant it. Keep the best one if it is usable.
        // People called up in turn (awards, gifts, group photos) are often shot only twice each.
        let group = items[best].is_some_and(|a| a.people.faces.iter().filter(|f| f.main && !f.small).count() >= 2);
        let meant = if group { MEANT_GROUP } else { MEANT_SHOT };
        if s.options.keep_each_shot && g.len() >= meant && !g.iter().any(|&i| out[i].decision == Decision::Selected) {
            let usable = items[best].is_some_and(|a| !a.people.closed_eyes && !a.people.blurred_face && !flagged(a, s));
            let near = out[best].score >= s.threshold - s.review_margin;
            if usable && near && matches!(out[best].decision, Decision::Review | Decision::Rejected(Reason::LowScore)) {
                out[best].decision = Decision::Selected;
                out[best].notes.push(format!("Bạn chụp {} tấm cảnh này: giữ tấm đẹp nhất", g.len()));
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

/// A photo that may be picked to fill a gap: not ruined, nobody important blinking.
fn fills_gap(v: &Verdict, a: Option<&Analysis>, floor: f32, s: &Settings) -> bool {
    v.score >= floor
        && v.fixes.is_empty()
        && matches!(v.decision, Decision::Review | Decision::Rejected(Reason::LowScore) | Decision::Rejected(Reason::Composition))
        && a.is_some_and(|a| !a.people.closed_eyes && !a.people.blurred_face && !flagged(a, s))
}

/// Every key person keeps their best usable photo of each moment they are in.
fn cover_key_people(items: &[Option<&Analysis>], out: &mut [Verdict], s: &Settings) {
    let mut seen: Vec<(u32, usize)> = out.iter().flat_map(|v| v.key_people.iter().map(move |&p| (p, v.moment))).collect();
    seen.sort_unstable();
    seen.dedup();
    let floor = s.threshold - 2.0 * s.review_margin.max(0.5);
    for (p, m) in seen {
        if out.iter().any(|v| v.moment == m && v.decision == Decision::Selected && v.key_people.contains(&p)) {
            continue;
        }
        let best = out
            .iter()
            .filter(|v| v.moment == m && v.key_people.contains(&p) && fills_gap(v, items[v.idx], floor, s))
            .max_by(|a, b| a.score.total_cmp(&b.score))
            .map(|v| v.idx);
        if let Some(i) = best {
            out[i].decision = Decision::Selected;
            out[i].notes.push("Ảnh đẹp nhất của nhân vật quan trọng trong khoảnh khắc này".into());
        }
    }
}

/// The event told whole: each kind of photo it has keeps its best usable one.
fn cover_story(items: &[Option<&Analysis>], out: &mut [Verdict], s: &Settings) {
    let floor = s.threshold - s.review_margin.max(0.5);
    for sc in crate::scene::Scene::ALL {
        if out.iter().any(|v| v.scene == sc && v.decision == Decision::Selected) {
            continue;
        }
        let best = out.iter().filter(|v| v.scene == sc && fills_gap(v, items[v.idx], floor, s)).max_by(|a, b| a.score.total_cmp(&b.score)).map(|v| v.idx);
        if let Some(i) = best {
            out[i].decision = Decision::Selected;
            out[i].notes.push(format!("Đủ loại ảnh kể câu chuyện: tấm «{}» đẹp nhất", sc.label()));
        }
    }
}

/// Long side (pixels) under which a photo is fine for the web but not for large prints.
const LOW_RES: u32 = 2000;

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

    /// A look vector at cosine ~`c` from look `base` (seeded noise mixed in).
    fn look(base: u32, c: f32, seed: u32) -> Vec<i8> {
        let v = |k: u32, i: usize| (((i as u32).wrapping_mul(2654435761).wrapping_add(k.wrapping_mul(40503)) >> 7) % 200) as f32 - 100.0;
        let mix = (1.0 / (c * c) - 1.0).max(0.0).sqrt();
        let raw: Vec<f32> = (0..crate::visual::FULL).map(|i| v(base, i) + mix * v(seed + 1000, i)).collect();
        // Each part is stored at unit length (scaled to 127), like the real vectors.
        let (d, r) = (crate::visual::DIM, crate::visual::REG);
        let mut out = vec![];
        for part in [&raw[..d], &raw[d..d + r], &raw[d + r..]] {
            let n = part.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
            out.extend(part.iter().map(|x| (x / n * 127.0).round().clamp(-127.0, 127.0) as i8));
        }
        out
    }

    fn face(x: f32) -> crate::faces::FaceInfo {
        crate::faces::FaceInfo { bbox: [x, 0.3, 0.06, 0.09], main: true, eye_seen: true, ..Default::default() }
    }

    fn shot(name: &str, t: f64, score: f32, base: u32, c: f32, seed: u32, people: usize) -> Analysis {
        let mut a = photo(name, t, [score; 5], seed as u64 * 7919);
        a.embed = look(base, c, seed);
        a.people.faces = (0..people).map(|k| face(0.1 + k as f32 * 0.13)).collect();
        a
    }

    #[test]
    fn bursts_group_by_picture_and_people() {
        // One press of the shutter, the speaker moving: frames 0.5 s apart, pictures alike.
        let burst: Vec<Analysis> = (0..4).map(|k| shot(&format!("b{k}"), 100.0 + k as f64 * 0.5, 8.0 + k as f32 * 0.2, 1, 0.94, k, 1)).collect();
        let c = crate::visual::similarity_content(&burst[0].embed, &burst[1].embed);
        assert!(c > 0.78 && c < 0.86, "test frames should be in the burst-only band: {c}");
        let refs: Vec<_> = burst.iter().map(Some).collect();
        let v = cull(&refs, &Settings::default(), None);
        assert_eq!(v.iter().filter(|x| x.decision == Decision::Selected).count(), 1, "{:?}", v.iter().map(|x| x.decision).collect::<Vec<_>>());
        assert_eq!(v[3].decision, Decision::Selected);

        // Award groups on one stage: same backdrop, 5, 3, 6 people, photographed a few seconds
        // apart. Each group is its own shot.
        let mut g = vec![];
        for (n, people) in [5usize, 3, 6].iter().enumerate() {
            for k in 0..3 {
                g.push(shot(&format!("g{n}-{k}"), 200.0 + n as f64 * 6.0 + k as f64 * 0.7, 8.2 + k as f32 * 0.1, 2, 0.97, (n * 10 + k) as u32, *people));
            }
        }
        let refs: Vec<_> = g.iter().map(Some).collect();
        let v = cull(&refs, &Settings::default(), None);
        let picked: Vec<&str> = v.iter().filter(|x| x.decision == Decision::Selected).map(|x| g[x.idx].name.as_str()).collect();
        assert_eq!(picked, vec!["g0-2", "g1-2", "g2-2"], "{:?}", v.iter().map(|x| (g[x.idx].name.clone(), x.decision, x.series, x.score)).collect::<Vec<_>>());
    }

    fn person(k: u32) -> Vec<i8> {
        let v: Vec<f32> = (0..crate::faceid::DIM).map(|i| ((i as u32).wrapping_mul(2654435761).wrapping_add(k.wrapping_mul(40503)) as f32 * 1e-3).sin()).collect();
        crate::faceid::quantise(&v).unwrap()
    }

    #[test]
    fn diplomas_handed_one_after_another_keep_one_photo_each() {
        // The same leader on the right hands a diploma to student after student at the same
        // spot, each shot twice, a few seconds apart: the pictures are practically identical,
        // only the student's face tells them apart.
        let ids: Vec<Vec<i8>> = (0..6).map(person).collect();
        assert!(crate::faceid::similarity(&ids[0], &ids[1]).unwrap() < crate::faceid::OTHER_PERSON);
        let mut g = vec![];
        for st in 1..6u32 {
            for k in 0..2 {
                let mut a = photo(&format!("sv{st}-{k}"), 300.0 + st as f64 * 4.0 + k as f64 * 0.8, [8.2 + k as f32 * 0.1; 5], 4242);
                a.embed = look(3, 0.99, st * 10 + k);
                let mut leader = face(0.6);
                leader.id = ids[0].clone();
                let mut student = face(0.35);
                student.id = ids[st as usize].clone();
                a.people.faces = vec![student, leader];
                g.push(a);
            }
        }
        let refs: Vec<_> = g.iter().map(Some).collect();
        let v = cull(&refs, &Settings::default(), None);
        let picked: Vec<&str> = v.iter().filter(|x| x.decision == Decision::Selected).map(|x| g[x.idx].name.as_str()).collect();
        assert_eq!(picked, vec!["sv1-1", "sv2-1", "sv3-1", "sv4-1", "sv5-1"], "{:?}", v.iter().map(|x| (g[x.idx].name.clone(), x.decision, x.series)).collect::<Vec<_>>());

        // Without fingerprints (older analysis) they would all be one shot.
        for a in g.iter_mut() {
            a.people.faces.iter_mut().for_each(|f| f.id.clear());
        }
        let refs: Vec<_> = g.iter().map(Some).collect();
        assert_eq!(cull(&refs, &Settings::default(), None).iter().filter(|x| x.decision == Decision::Selected).count(), 1);
    }

    #[test]
    fn one_group_shot_over_several_seconds_is_one_shot() {
        // A group photo: 4 people, 5 frames 3 s apart, people shifting so the pictures differ
        // more than a burst's. Then the next group, 3 s later, same backdrop.
        let ids: Vec<Vec<i8>> = (10..18).map(person).collect();
        let mut g = vec![];
        for (n, who) in [&ids[0..4], &ids[4..8]].iter().enumerate() {
            for k in 0..5 {
                let mut a = shot(&format!("nhom{n}-{k}"), 100.0 + (n * 5 + k) as f64 * 3.0, 8.0 + k as f32 * 0.1, 5, 0.94, (n * 10 + k) as u32, 4);
                // People moved: the pixel fingerprints differ too.
                a.signature.bits = ((n * 5 + k) as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                for (f, id) in a.people.faces.iter_mut().zip(who.iter()) {
                    f.id = id.clone();
                }
                g.push(a);
            }
        }
        let c = crate::visual::similarity_content(&g[0].embed, &g[1].embed);
        assert!(c > 0.78 && c < 0.86, "frames of one group should only match as one group: {c}");
        let refs: Vec<_> = g.iter().map(Some).collect();
        let v = cull(&refs, &Settings::default(), None);
        let picked: Vec<&str> = v.iter().filter(|x| x.decision == Decision::Selected).map(|x| g[x.idx].name.as_str()).collect();
        assert_eq!(picked, vec!["nhom0-4", "nhom1-4"], "{:?}", v.iter().map(|x| (g[x.idx].name.clone(), x.decision, x.series)).collect::<Vec<_>>());
        // Without fingerprints the pictures alone are too different 3 s apart.
        for a in g.iter_mut() {
            a.people.faces.iter_mut().for_each(|f| f.id.clear());
        }
        let refs: Vec<_> = g.iter().map(Some).collect();
        let v = cull(&refs, &Settings::default(), None);
        assert!(v.iter().filter(|x| x.decision == Decision::Selected).count() > 2, "{:?}", v.iter().map(|x| (g[x.idx].name.clone(), x.decision, x.series, x.score)).collect::<Vec<_>>());
    }

    #[test]
    fn the_story_keeps_one_of_each_kind_and_key_people_are_not_dropped() {
        // Ten good photos of people, and one detail shot (no faces) a little under the pick score.
        let mut g: Vec<Analysis> = (0..10).map(|k| shot(&format!("a{k}"), k as f64 * 100.0, 8.5, 10 + k, 1.0, k, 2)).collect();
        let mut d = photo("logo", 2000.0, [7.0; 5], 99);
        d.embed = look(50, 1.0, 1);
        g.push(d);
        // A key person in a weak photo of its own moment.
        let mut kp = shot("thay", 5000.0, 5.6, 60, 1.0, 3, 1);
        kp.key_faces = vec![(0, 7)];
        g.push(kp);
        let refs: Vec<_> = g.iter().map(Some).collect();
        let mut on = Settings::default();
        on.options.key_people = true;
        let v = cull(&refs, &on, None);
        assert_eq!(v[10].scene, crate::scene::Scene::Detail);
        assert_eq!(v[10].decision, Decision::Selected, "{:?}", v[10].notes);
        assert_eq!(v[11].key_people, vec![7]);
        assert_eq!(v[11].decision, Decision::Selected, "{:?}", v[11].notes);
        let mut s = Settings::default();
        s.options.story = false;
        s.options.key_people = false;
        let v = cull(&refs, &s, None);
        assert_eq!(v[10].decision, Decision::Review);
        assert_eq!(v[11].decision, Decision::Rejected(Reason::LowScore));
        s.options.key_people = true;
        // Too weak to be picked for him alone, but never rejected.
        g[11].scores = crate::analyze::Scores { exposure: 4.5, sharp: 4.5, comp: 4.5, color: 4.5, noise: 4.5, people: None };
        let refs: Vec<_> = g.iter().map(Some).collect();
        assert_eq!(cull(&refs, &s, None)[11].decision, Decision::Review);
    }

    #[test]
    fn a_shot_taken_many_times_keeps_its_best() {
        // Just under the pick score: one is kept anyway (he shot it three times), and in a
        // group the frame where nobody blinks wins.
        let mut g: Vec<Analysis> = (0..3).map(|k| shot(&format!("s{k}"), 50.0 + k as f64, 7.0, 3, 0.95, k, 4)).collect();
        g[0].scores.exposure = 7.3;
        g[0].people.faces[2].closed = true;
        let refs: Vec<_> = g.iter().map(Some).collect();
        let v = cull(&refs, &Settings::default(), None);
        let picked: Vec<usize> = v.iter().filter(|x| x.decision == Decision::Selected).map(|x| x.idx).collect();
        assert_eq!(picked.len(), 1, "{:?}", v.iter().map(|x| (x.decision, x.score)).collect::<Vec<_>>());
        assert_ne!(picked[0], 0, "the frame with someone blinking loses");
        let mut s = Settings::default();
        s.options.keep_each_shot = false;
        s.options.story = false;
        assert!(cull(&refs, &s, None).iter().all(|x| x.decision != Decision::Selected));
    }

    #[test]
    fn a_speaker_looking_down_is_for_review() {
        let mut a = shot("talk", 0.0, 9.0, 4, 1.0, 1, 1);
        a.people.faces[0].down = true;
        let mut b = shot("moment", 500.0, 9.0, 5, 1.0, 2, 3);
        b.people.faces[0].down = true;
        let refs = vec![Some(&a), Some(&b)];
        let v = cull(&refs, &Settings::default(), None);
        assert_eq!(v[0].decision, Decision::Review);
        assert_eq!(v[1].decision, Decision::Selected, "looking down in a moment with others is fine");
        let mut s = Settings::default();
        s.options.speaker_head_up = false;
        assert_eq!(cull(&refs, &s, None)[0].decision, Decision::Selected);
    }

    #[test]
    fn learns_which_frame_of_a_burst_he_keeps() {
        // Old show: in each award group shot three times he kept the frame where people look up
        // most and dropped those where they look down at the flowers, even though those measured
        // a little better. How much heads are bowed differs a lot from one scene to the next,
        // and whole scenes he skipped are just as upright, so "bowed" alone tells little;
        // comparing frames of the same shot shows how he chooses.
        let bow = |a: &mut Analysis, pitch: f32| {
            for f in a.people.faces.iter_mut().take(2) {
                f.pitch = pitch;
            }
        };
        let mut p = Profile::new("x");
        let mut samples = vec![];
        for n in 0..24u32 {
            let base = 0.2 + ((n * 7) % 7) as f32 * 0.05;
            let skipped = n % 2 == 1;
            for k in 0..3u32 {
                let mut a = shot(&format!("old{n}-{k}"), 1000.0 * n as f64 + k as f64, if k == 0 { 8.0 } else { 8.4 }, 7, 0.97, n * 10 + k, 4);
                // Scenes he skipped: everyone upright, all frames dropped.
                bow(&mut a, if skipped { 0.0 } else if k == 0 { base } else { base + 0.15 });
                let keep = !skipped && k == 0;
                let mut x = learn::Sample::new(format!("/old/{n}-{k}.jpg"), &a, keep, !keep, true);
                x.series = Some(format!("/old/{n}-0.jpg"));
                samples.push(x);
            }
        }
        p.add(samples);
        p.train();
        assert!(p.rank_pairs >= 15 && p.rank_acc > 0.9, "{} {}", p.rank_pairs, p.rank_acc);

        // A new show: the bowed frames again measure better.
        let mut g: Vec<Analysis> = (0..3).map(|k| shot(&format!("new{k}"), 50.0 + k as f64, if k == 1 { 8.0 } else { 8.4 }, 3, 0.97, 500 + k, 4)).collect();
        bow(&mut g[0], 0.25);
        bow(&mut g[1], 0.1);
        bow(&mut g[2], 0.25);
        let refs: Vec<_> = g.iter().map(Some).collect();
        let mut s = Settings::default();
        s.options.story = false;
        let pick = |v: Vec<Verdict>| v.iter().filter(|x| x.decision == Decision::Selected).map(|x| x.idx).collect::<Vec<usize>>();
        assert_ne!(pick(cull(&refs, &s, None)), vec![1], "the rules alone prefer a bowed frame");
        let r: Vec<f32> = g.iter().map(|a| p.rank(&learn::features(a)).unwrap()).collect();
        assert!(r[1] > r[0] + 1.0 && r[1] > r[2] + 1.0, "{r:?}");
        let v = cull(&refs, &s, Some(&p));
        assert_eq!(pick(v.clone()), vec![1], "{:?}", v.iter().map(|x| (x.decision, x.score, x.notes.clone())).collect::<Vec<_>>());
    }

}
