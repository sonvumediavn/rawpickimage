//! Chat with the in-app AI to steer the culling ("ưu tiên ảnh có người cười", "chỉ giữ 300 ảnh",
//! "tôi thích ảnh tông tối"…). Requests become a [`Patch`] of the settings.
//!
//! Two brains produce the same patch:
//! - [`understand`]: a small offline parser for common Vietnamese requests (with or without
//!   diacritics). Instant, free, nothing leaves the computer.
//! - Claude, through the Claude Code CLI the user is signed in to ([`claude_prompt`] /
//!   [`parse_claude_answer`]), for free-form requests.

use crate::analyze::CRITERIA;
use crate::cull::{Preset, Settings, Summary};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const LABEL: [&str; 6] = ["Phơi sáng", "Độ nét", "Bố cục", "Màu sắc", "Nhiễu", "Người"];

/// Changes to the culling settings. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Patch {
    /// Criterion weights 0–50, keyed by `exposure`, `sharp`, `comp`, `color`, `noise`, `people`.
    pub weights: BTreeMap<String, f32>,
    pub preset: Option<String>,
    pub threshold: Option<f32>,
    pub review_margin: Option<f32>,
    /// 0 = no limit.
    pub max_selected: Option<usize>,
    pub keep_per_series: Option<usize>,
    pub dedupe_seconds: Option<f64>,
    pub dedupe: Option<bool>,
    pub reject_closed_eyes: Option<bool>,
    pub review_open_mouth: Option<bool>,
    pub speaker_head_up: Option<bool>,
    pub keep_each_shot: Option<bool>,
    pub moments: Option<bool>,
    pub fix_instead_of_reject: Option<bool>,
    pub use_learned: Option<bool>,
    pub learned_weight: Option<f32>,
    /// Back to the default settings (camera offsets and the extras stay as they are).
    pub reset: Option<bool>,
}

impl Patch {
    pub fn is_empty(&self) -> bool {
        *self == Patch::default()
    }

    /// Apply to `s` with sane limits; returns a Vietnamese line per change. `preset` is
    /// updated when the preset changes or weights are edited by hand ("custom").
    pub fn apply(&self, s: &mut Settings, preset: &mut String) -> Vec<String> {
        let mut out = vec![];
        if self.reset == Some(true) {
            let d = Settings::default();
            let keep = std::mem::take(&mut s.camera_offsets);
            let mut o = s.options.clone();
            o.reject_closed_eyes = d.options.reject_closed_eyes;
            o.review_open_mouth = d.options.review_open_mouth;
            o.speaker_head_up = d.options.speaker_head_up;
            o.keep_each_shot = d.options.keep_each_shot;
            o.dedupe = d.options.dedupe;
            o.use_learned = d.options.use_learned;
            o.fix_instead_of_reject = d.options.fix_instead_of_reject;
            o.moments = d.options.moments;
            *s = Settings { camera_offsets: keep, options: o, ..d };
            *preset = "event".into();
            out.push("Đặt lại cách chấm và chọn về mặc định (kiểu «Sự kiện», ngưỡng 7,5)".into());
        }
        if let Some(p) = &self.preset {
            let (pr, name) = match p.as_str() {
                "balanced" => (Preset::Balanced, "Cân bằng"),
                "portrait" => (Preset::Portrait, "Chân dung"),
                "landscape" => (Preset::Landscape, "Phong cảnh"),
                _ => (Preset::Event, "Sự kiện"),
            };
            s.weights = pr.weights();
            *preset = p.clone();
            out.push(format!("Chuyển kiểu chấm điểm sang «{name}»"));
        }
        for (k, v) in &self.weights {
            if let Some(i) = CRITERIA.iter().position(|c| c == k) {
                let v = v.clamp(0.0, 50.0).round();
                if (s.weights[i] - v).abs() > 0.4 {
                    out.push(format!("Trọng số {}: {:.0} → {:.0}", LABEL[i], s.weights[i], v));
                    s.weights[i] = v;
                    *preset = "custom".into();
                }
            }
        }
        if let Some(t) = self.threshold {
            let t = (t.clamp(5.0, 9.5) * 10.0).round() / 10.0;
            out.push(format!("Điểm để chọn: {:.1} → {:.1}", s.threshold, t));
            s.threshold = t;
        }
        if let Some(m) = self.review_margin {
            s.review_margin = m.clamp(0.0, 3.0);
            out.push(format!("Vùng «cần xem lại»: {:.1} điểm dưới ngưỡng", s.review_margin));
        }
        if let Some(n) = self.max_selected {
            s.max_selected = (n > 0).then_some(n);
            out.push(if n > 0 { format!("Giữ tối đa {n} ảnh") } else { "Bỏ giới hạn số ảnh chọn".into() });
        }
        if let Some(k) = self.keep_per_series {
            s.keep_per_series = k.clamp(1, 5);
            out.push(format!("Mỗi loạt chụp giữ {} ảnh", s.keep_per_series));
        }
        if let Some(d) = self.dedupe_seconds {
            s.dedupe_seconds = d.clamp(0.0, 120.0);
            out.push(format!("Ảnh giống nhau trong vòng {:.0} giây tính là trùng", s.dedupe_seconds));
        }
        let mut flag = |cur: &mut bool, v: Option<bool>, on: &str, off: &str| {
            if let Some(v) = v {
                if *cur != v {
                    *cur = v;
                    out.push(if v { on.to_string() } else { off.to_string() });
                }
            }
        };
        let o = &mut s.options;
        flag(&mut o.dedupe, self.dedupe, "Bật lọc ảnh trùng", "Tắt lọc ảnh trùng, giữ mọi ảnh trong loạt");
        flag(&mut o.reject_closed_eyes, self.reject_closed_eyes, "Loại ảnh có người nhắm mắt", "Không loại ảnh nhắm mắt nữa (vẫn bị trừ điểm)");
        flag(
            &mut o.review_open_mouth,
            self.review_open_mouth,
            "Ảnh nhân vật chính đang há miệng đưa vào «Cần xem lại»",
            "Không soi khẩu hình nữa (ảnh há miệng chỉ bị trừ nhẹ)",
        );
        flag(
            &mut o.speaker_head_up,
            self.speaker_head_up,
            "Người phát biểu (một mình trong khung) cúi đầu: đưa vào «Cần xem lại»",
            "Người phát biểu cúi đầu vẫn được chọn",
        );
        flag(&mut o.keep_each_shot, self.keep_each_shot, "Cảnh nào bạn chụp nhiều tấm thì giữ ít nhất 1 tấm đẹp nhất", "Không tự giữ cảnh chụp nhiều tấm nữa");
        flag(&mut o.moments, self.moments, "Bật «Đủ khoảnh khắc»", "Tắt «Đủ khoảnh khắc»");
        flag(&mut o.fix_instead_of_reject, self.fix_instead_of_reject, "Bật «Sửa thay vì loại»", "Tắt «Sửa thay vì loại»");
        flag(&mut o.use_learned, self.use_learned, "Dùng điều AI đã học từ lựa chọn của bạn", "Không dùng điều AI đã học");
        if let Some(w) = self.learned_weight {
            s.learned_weight = w.clamp(0.0, 0.8);
            out.push(format!("Phần điểm theo gu của bạn: {:.0}%", s.learned_weight * 100.0));
        }
        out
    }
}

/// Lower-case and strip Vietnamese diacritics so "uu tien nguoi" == "ưu tiên người".
pub fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'ạ' | 'ả' | 'ã' | 'â' | 'ầ' | 'ấ' | 'ậ' | 'ẩ' | 'ẫ' | 'ă' | 'ằ' | 'ắ' | 'ặ' | 'ẳ' | 'ẵ' => 'a',
            'è' | 'é' | 'ẹ' | 'ẻ' | 'ẽ' | 'ê' | 'ề' | 'ế' | 'ệ' | 'ể' | 'ễ' => 'e',
            'ì' | 'í' | 'ị' | 'ỉ' | 'ĩ' => 'i',
            'ò' | 'ó' | 'ọ' | 'ỏ' | 'õ' | 'ô' | 'ồ' | 'ố' | 'ộ' | 'ổ' | 'ỗ' | 'ơ' | 'ờ' | 'ớ' | 'ợ' | 'ở' | 'ỡ' => 'o',
            'ù' | 'ú' | 'ụ' | 'ủ' | 'ũ' | 'ư' | 'ừ' | 'ứ' | 'ự' | 'ử' | 'ữ' => 'u',
            'ỳ' | 'ý' | 'ỵ' | 'ỷ' | 'ỹ' => 'y',
            'đ' => 'd',
            c => c,
        })
        .collect()
}

fn has(s: &str, words: &[&str]) -> bool {
    words.iter().any(|w| s.contains(w))
}

/// First number after any of `keys` (within a few characters).
fn number_after(s: &str, keys: &[&str]) -> Option<f32> {
    for k in keys {
        let mut from = 0;
        while let Some(i) = s[from..].find(k) {
            let rest = &s[from + i + k.len()..];
            let lead: String = rest.chars().take(14).collect();
            if let Some(start) = lead.find(|c: char| c.is_ascii_digit()) {
                let num: String = lead[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
                if let Ok(v) = num.trim_end_matches(['.', ',']).replace(',', ".").parse() {
                    return Some(v);
                }
            }
            from += i + k.len();
        }
    }
    None
}

pub struct Understood {
    pub patch: Patch,
    /// Extra explanation for the reply (style notes, how learning works…).
    pub notes: Vec<String>,
    /// A question about the open folder or the AI rather than a change.
    pub question: Option<Question>,
}

/// Questions answered from the session's data instead of changing settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// How many photos, selected, review, rejected.
    Summary,
    /// Why photos were rejected, by reason.
    Reasons,
    /// What the AI has learned from Sơn.
    Learned,
    /// The current settings in words.
    Settings,
}

fn question(f: &str) -> Option<Question> {
    if has(f, &["hoc duoc gi", "da hoc duoc", "ai da hoc", "hoc den dau", "hoc toi den dau", "hieu gu", "hieu toi"]) {
        Some(Question::Learned)
    } else if has(f, &["vi sao", "tai sao", "ly do", "sao lai"]) && has(f, &["loai", "bo "]) {
        Some(Question::Reasons)
    } else if has(f, &["tom tat", "thong ke", "bao nhieu anh", "tinh hinh", "ket qua phien"]) {
        Some(Question::Summary)
    } else if has(f, &["thiet lap hien tai", "dang cham", "cham the nao", "cham nhu the nao", "dang dung thiet lap", "cai dat hien tai", "tieu chi hien tai"]) {
        Some(Question::Settings)
    } else {
        None
    }
}

/// Starting points per kind of event: weights in `CRITERIA` order and the switches that suit it.
struct EventStyle {
    name: &'static str,
    weights: [f32; 6],
    closed_eyes: bool,
    open_mouth: bool,
    moments: bool,
    fix: bool,
    keep_per_series: usize,
    dedupe_seconds: f64,
    note: &'static str,
}

fn event_style(f: &str) -> Option<EventStyle> {
    let e = |name, weights, closed_eyes, open_mouth, moments, fix, keep_per_series, dedupe_seconds, note| {
        Some(EventStyle { name, weights, closed_eyes, open_mouth, moments, fix, keep_per_series, dedupe_seconds, note })
    };
    if has(f, &["dam cuoi", "tiec cuoi", "le cuoi", "an hoi", "dam hoi", "wedding", "co dau", "chu re"]) {
        e("Đám cưới", [15.0, 25.0, 12.0, 8.0, 5.0, 35.0], true, true, true, true, 1, 8.0,
          "Ưu tiên cô dâu, chú rể và gia đình: mắt mở, khẩu hình đẹp, đủ các nghi thức.")
    } else if has(f, &["hoi nghi", "hoi thao", "toa dam", "ky ket", "le ky", "dien gia", "conference", "seminar"]) {
        e("Hội nghị – hội thảo", [20.0, 25.0, 15.0, 10.0, 5.0, 25.0], true, true, true, true, 1, 10.0,
          "Diễn giả đang nói dễ há miệng: những ảnh đó đưa vào «Cần xem lại». Màn chiếu cháy sáng phía sau không bị trừ nặng.")
    } else if has(f, &["san khau", "bieu dien", "ca nhac", "concert", "live show", "ca si", "mua ", "nhay"]) {
        e("Sân khấu – biểu diễn", [10.0, 30.0, 15.0, 15.0, 5.0, 25.0], false, false, true, true, 2, 4.0,
          "Đèn sân khấu tối và gắt là bình thường nên trừ nhẹ phơi sáng. Ca sĩ hát há miệng vẫn giữ. Mỗi loạt giữ 2 ảnh vì động tác thay đổi nhanh.")
    } else if has(f, &["tiec", "gala", "sinh nhat", "year end", "tat nien", "party", "liveshow"]) {
        e("Tiệc – gala", [15.0, 25.0, 10.0, 10.0, 10.0, 30.0], true, false, true, true, 1, 8.0,
          "Ưu tiên khoảnh khắc vui, người cười nói tự nhiên (há miệng cười vẫn giữ). Ánh sáng tiệc yếu nên nhiễu được tính thêm một chút.")
    } else if has(f, &["ky yeu", "anh nhom", "chup nhom", "tap the", "team building", "doan the"]) {
        e("Kỷ yếu – ảnh nhóm", [15.0, 25.0, 15.0, 5.0, 0.0, 40.0], true, true, false, true, 1, 15.0,
          "Ảnh nhóm chỉ đạt khi mọi người chính đều mở mắt, nét mặt ổn. Các tấm chụp liên tiếp cùng đội hình chỉ giữ tấm đẹp nhất.")
    } else if has(f, &["the thao", "chay bo", "marathon", "giai dau", "bong da", "tran dau", "sport"]) {
        e("Thể thao", [15.0, 35.0, 20.0, 10.0, 5.0, 15.0], false, false, true, false, 2, 3.0,
          "Độ nét và bố cục đứng đầu, biểu cảm gắng sức (nhắm mắt, há miệng) là chuyện thường nên không loại.")
    } else {
        None
    }
}

/// Offline understanding of a request. `w` = current weights (to raise/lower relative to them).
pub fn understand(msg: &str, w: &[f32; 6]) -> Understood {
    let low = msg.to_lowercase();
    let mut p = Patch::default();
    let mut notes = vec![];
    let whole = fold(&low);
    if let Some(q) = question(&whole) {
        return Understood { patch: p, notes, question: Some(q) };
    }

    for clause in low.split(['.', ',', ';', '\n', '!', '?']).flat_map(|c| c.split(" và ")).filter(|c| !c.trim().is_empty()) {
        let raw = clause.trim();
        let f = fold(raw);

        // Style: dark / bright look chosen on purpose.
        let style = has(&f, &["phong cach", "style", "tong ", "tong", "thich anh", "gu "]);
        let dark_look = has(raw, &["tối", "low key", "lowkey", "trầm", "moody"]) && (style || has(raw, &["tối có chủ ý", "low key", "lowkey", "moody"]));
        let bright_look = has(raw, &["high key", "highkey", "trong trẻo", "sáng sủa"]) || (style && has(raw, &["sáng", "tươi"]) && !has(raw, &["tối"]));
        if dark_look || bright_look {
            p.weights.insert("exposure".into(), (w[0] * 0.4).round());
            p.fix_instead_of_reject = Some(true);
            notes.push(format!(
                "Ảnh {} có chủ ý sẽ không bị trừ nhiều điểm phơi sáng nữa. Ảnh quá {} vẫn được đưa vào «Cần xem lại» kèm gợi ý chỉnh thay vì bị loại.",
                if dark_look { "tông tối" } else { "tông sáng" },
                if dark_look { "tối" } else { "cháy" }
            ));
            continue;
        }

        if has(&f, &["dat lai", "mac dinh", "reset", "lam lai tu dau"]) {
            p.reset = Some(true);
            continue;
        }

        // Kind of event: a whole starting setup at once.
        if let Some(e) = event_style(&f) {
            p.weights = CRITERIA.iter().zip(e.weights).map(|(k, v)| (k.to_string(), v)).collect();
            p.reject_closed_eyes = Some(e.closed_eyes);
            p.review_open_mouth = Some(e.open_mouth);
            p.moments = Some(e.moments);
            p.fix_instead_of_reject = Some(e.fix);
            p.keep_per_series = Some(e.keep_per_series);
            p.dedupe_seconds = Some(e.dedupe_seconds);
            p.dedupe = Some(true);
            notes.push(format!("Chuyển sang cách chọn cho «{}». {}", e.name, e.note));
            continue;
        }

        // Head up / looking down (ngẩng / cúi đầu).
        if has(&f, &["cui dau", "ngang dau", "ngung dau", "cui mat", "nhin xuong", "doc giay"]) {
            let allow = has(&f, &["van giu", "van chon", "khong loai", "dung loai", "cho phep", "chap nhan", "khong sao", "khong can", "bo qua", "duoc"])
                && !has(&f, &["phai ngang", "can ngang", "uu tien ngang"]);
            p.speaker_head_up = Some(!allow);
            notes.push(if allow {
                "Người phát biểu cúi đầu vẫn được chọn. Ảnh khoảnh khắc có nhiều người thì cúi đầu vẫn luôn được giữ.".into()
            } else {
                "Ảnh một người phát biểu đang cúi đầu (đọc giấy, nhìn xuống) sẽ vào «Cần xem lại». Ảnh khoảnh khắc có nhiều người thì cúi đầu vẫn được giữ. Thấy AI nhìn sai thì bấm Ngẩng/Cúi ở ảnh đó, AI nhớ cho lần sau.".into()
            });
            continue;
        }

        // A shot taken many times (award groups, posed groups).
        if has(&f, &["chon thieu", "bi thieu", "thieu nhom", "moi nhom", "nhan giai", "trao giai", "moi canh"]) {
            p.keep_each_shot = Some(true);
            p.dedupe = Some(true);
            notes.push("Cảnh nào bạn chụp nhiều tấm (từng nhóm lên nhận giải, chụp nhóm) thì giữ ít nhất 1 tấm đẹp nhất, kể cả khi điểm hơi thấp hơn ngưỡng chọn.".into());
            continue;
        }

        // Mouth shape (khẩu hình).
        if has(&f, &["ha mieng", "khau hinh", "meo mieng", "mieng dep", "dang noi", "nhep mieng"]) {
            let allow = has(&f, &["giu", "khong loai", "dung loai", "cho phep", "chap nhan", "khong sao", "khong can", "bo qua", "khong quan tam"]);
            p.review_open_mouth = Some(!allow);
            if !allow {
                p.weights.insert("people".into(), (w[5] * 1.3).max(w[5] + 5.0).min(50.0).round());
                notes.push("Ảnh nhân vật chính đang há miệng, đang nói dở sẽ vào «Cần xem lại» để bạn quyết. Ảnh cười tươi vẫn được giữ nếu miệng không mở quá rộng.".into());
            }
            continue;
        }

        // Turned faces are fine: nothing to change, just say so.
        if has(&f, &["mat nghieng", "goc nghieng", "nghieng mat", "quay mat", "mat quay", "goc mat", "khong can thang mat", "khong can nhin may"]) {
            notes.push("Mặt nghiêng đã không bị trừ điểm: chỉ cần mắt nhân vật chính mở và mặt đủ nét. Khi bạn giữ những ảnh góc nghiêng đẹp, AI học theo và chấm kiểu ảnh đó cao hơn.".into());
            continue;
        }

        // Eyes must be open.
        if has(&f, &["mat mo", "mo mat", "mat phai mo", "mat nhan vat"]) && !f.contains("nham mat") {
            p.reject_closed_eyes = Some(true);
            continue;
        }

        // Main subject / close-ups.
        if has(&f, &["nhan vat chinh", "can mat", "can canh", "chu the chinh", "nguoi chinh"]) {
            p.weights.insert("people".into(), (w[5] * 1.4).max(w[5] + 8.0).min(50.0).round());
            p.weights.insert("comp".into(), (w[2] * 1.3).max(w[2] + 5.0).min(50.0).round());
            notes.push("Điểm người và bố cục tính theo khuôn mặt lớn nhất trong khung, người đứng xa phía sau không kéo điểm xuống.".into());
            continue;
        }

        // How much Sơn's own taste counts.
        if has(&f, &["tat gu", "khong dung gu", "chi dung tieu chi", "bo gu", "khong theo gu", "chi cham theo may"]) {
            p.use_learned = Some(false);
            continue;
        }
        if has(&f, &["tin gu", "theo gu nhieu hon", "gu cua toi nhieu hon", "hoc tu toi la chinh", "theo toi la chinh", "gu cua toi la chinh", "uu tien gu"]) {
            p.use_learned = Some(true);
            p.learned_weight = Some(0.7);
            notes.push("Điểm sẽ nghiêng hẳn về gu của bạn (70%) khi AI đã học đủ. Càng sửa tay nhiều, AI càng tự tin làm theo bạn.".into());
            continue;
        }

        // Closed eyes.
        if f.contains("nham mat") {
            p.reject_closed_eyes = Some(!has(&f, &["giu", "khong loai", "dung loai", "cho phep", "chap nhan", "khong can loai"]));
            continue;
        }

        // Count limits.
        if has(&f, &["khong gioi han", "bo gioi han"]) {
            p.max_selected = Some(0);
            continue;
        }
        if let Some(n) = number_after(&f, &["giu toi da", "toi da", "chi giu", "chi chon", "giu lai", "giu khoang", "chon khoang", "chon ra", "lay ra", "lay", "giu", "chon"]) {
            if f.contains("loat") || f.contains("burst") {
                p.keep_per_series = Some(n as usize);
                continue;
            }
            if (has(&f, &["anh", "tam", "file"]) || n >= 20.0) && n >= 1.0 && n.fract() == 0.0 {
                p.max_selected = Some(n as usize);
                continue;
            }
        }
        if let Some(t) = number_after(&f, &["nguong", "diem chon", "tren", "tu "]) {
            if (5.0..=9.5).contains(&t) && has(&f, &["nguong", "diem"]) {
                p.threshold = Some(t);
                continue;
            }
        }

        // Duplicates, moments, fixing, learning.
        if has(&f, &["trung", "giong nhau", "lien tiep", "burst", "loat"]) {
            p.dedupe = Some(!has(&f, &["khong loc", "giu het", "giu tat ca", "dung loc", "tat loc", "khong can loc"]));
            continue;
        }
        if f.contains("khoanh khac") {
            p.moments = Some(!has(&f, &["khong can", "tat", "bo "]));
            continue;
        }
        if has(&f, &["sua thay vi loai", "cuu anh", "cuu lai", "dung loai anh nghieng"]) {
            p.fix_instead_of_reject = Some(true);
            continue;
        }
        if has(&f, &["phong cach cua toi", "style cua toi", "gu cua toi", "theo toi", "hoc theo", "nhu toi chon", "nhu toi"]) {
            p.use_learned = Some(true);
            p.learned_weight = Some(0.5);
            notes.push("AI học gu của bạn mỗi khi bạn xuất hoặc bấm «Huấn luyện AI»: những ảnh bạn giữ, bỏ, sửa tay đều được ghi lại theo loại sự kiện. Càng dùng nhiều, điểm càng sát cách bạn chọn.".into());
            continue;
        }

        // Stricter / looser.
        let loosen = has(&f, &["chon nhieu hon", "nhieu anh hon", "de hon", "noi long", "noi tay", "bot khat khe", "thoang hon", "it khat khe"]);
        let tighten = has(&f, &["it anh hon", "chon it hon", "khat khe", "kho hon", "chat hon", "gat gao", "chat che"]);
        if loosen || tighten {
            p.threshold = Some(p.threshold.unwrap_or(7.5) + if loosen { -0.5 } else { 0.5 });
            notes.push("(Ngưỡng được chỉnh so với 7,5; bạn có thể nói «ngưỡng 7» để đặt số cụ thể.)".into());
            continue;
        }

        // Criteria weights.
        let crit: [(&str, &[&str]); 6] = [
            ("exposure", &["phoi sang", "anh sang", "do sang", "anh du sang", "chay sang", "thieu sang"]),
            ("sharp", &["net", "sac", "mo ", "rung tay", "focus", "lay net"]),
            ("comp", &["bo cuc", "khung hinh", "1/3", "can giua", "nghieng", "goc anh", "goc chup", "goc may"]),
            ("color", &["mau", "tong mau", "white balance", "can bang trang"]),
            ("noise", &["nhieu hat", "hat ", "noise", "iso cao"]),
            ("people", &["nguoi", "khuon mat", "bieu cam", "cuoi", "guong mat", "co dau", "chu re", "nhan vat"]),
        ];
        let down = has(&f, &["khong quan trong", "bo qua", "khong can", "khong quan tam", "it quan tam", "giam", "bot", "dung qua de y", "khong de y"]);
        let off = has(&f, &["bo qua", "khong quan tam", "khong de y"]);
        let mut hit = false;
        for (i, (k, keys)) in crit.iter().enumerate() {
            // "nhiễu" (noise) vs "nhiều" (many) only differ by diacritics: check the original.
            let matched = has(&f, keys) || (*k == "noise" && raw.contains("nhiễu"));
            if !matched {
                continue;
            }
            hit = true;
            let v = if off {
                0.0
            } else if down {
                (w[i] * 0.4).round()
            } else {
                // Mentioned without "less": the user cares about it more.
                (w[i] * 1.6).max(w[i] + 10.0).min(50.0).round()
            };
            p.weights.insert(k.to_string(), v);
        }
        if hit {
            continue;
        }

        // Presets.
        if f.contains("chan dung") {
            p.preset = Some("portrait".into());
        } else if f.contains("phong canh") {
            p.preset = Some("landscape".into());
        } else if has(&f, &["can bang"]) {
            p.preset = Some("balanced".into());
        } else if has(&f, &["su kien", "dam cuoi", "tiec cuoi", "hoi nghi", "le "]) && has(&f, &["kieu", "che do", "cham", "anh"]) {
            p.preset = Some("event".into());
        }
    }
    Understood { patch: p, notes, question: None }
}

pub const HELP: &str = "Tôi chưa hiểu yêu cầu này. Bạn có thể nói kiểu:\n\
- «ưu tiên ảnh có người, biểu cảm đẹp»\n\
- «độ nét quan trọng nhất», «không quan tâm màu sắc»\n\
- «chỉ giữ 300 ảnh», «ngưỡng 7», «chọn nhiều hơn», «khắt khe hơn»\n\
- «tôi thích ảnh tông tối», «giữ ảnh nhắm mắt», «mỗi loạt giữ 2 ảnh»\n\
- «chọn theo phong cách của tôi», «tin gu của tôi nhiều hơn», «đủ khoảnh khắc»\n\
- «đây là đám cưới», «hội nghị», «sân khấu», «tiệc», «ảnh nhóm», «thể thao»\n\
- «loại ảnh há miệng», «mặt nghiêng vẫn giữ», «ưu tiên nhân vật chính»\n\
- hỏi: «tóm tắt phiên này», «vì sao nhiều ảnh bị loại», «AI đã học được gì từ tôi»\n\
- «hoàn tác», «đặt lại mặc định»";

/// Facts about the open folder that help Claude answer.
#[derive(Debug, Clone, Serialize)]
pub struct Context {
    pub summary: Summary,
    /// Mean score per criterion over analysed photos (`CRITERIA` order; people over photos with people).
    pub mean_scores: [f32; 6],
    pub with_people: usize,
    pub cameras: Vec<String>,
    pub hours: f32,
}

pub fn claude_prompt(msg: &str, s: &Settings, preset: &str, ctx: Option<&Context>, learned_ready: bool) -> String {
    let settings = serde_json::json!({
        "preset": preset,
        "weights": CRITERIA.iter().zip(s.weights).map(|(k, v)| (k.to_string(), v)).collect::<BTreeMap<_, _>>(),
        "threshold": s.threshold,
        "review_margin": s.review_margin,
        "max_selected": s.max_selected.unwrap_or(0),
        "keep_per_series": s.keep_per_series,
        "dedupe_seconds": s.dedupe_seconds,
        "dedupe": s.options.dedupe,
        "reject_closed_eyes": s.options.reject_closed_eyes,
        "review_open_mouth": s.options.review_open_mouth,
        "speaker_head_up": s.options.speaker_head_up,
        "keep_each_shot": s.options.keep_each_shot,
        "moments": s.options.moments,
        "fix_instead_of_reject": s.options.fix_instead_of_reject,
        "use_learned": s.options.use_learned,
        "learned_weight": s.learned_weight,
    });
    let ctx = ctx.map(|c| serde_json::to_string(c).unwrap_or_default()).unwrap_or_else(|| "chưa mở thư mục".into());
    format!(
        r#"Bạn là trợ lý chọn ảnh trong ứng dụng SonVu Raw Pick (lọc ảnh sự kiện). Ứng dụng chấm mỗi ảnh 0–10 theo 6 tiêu chí:
exposure (phơi sáng), sharp (độ nét), comp (bố cục), color (màu), noise (nhiễu), people (người: mắt nhắm, mặt mờ, biểu cảm).
Điểm cuối = trung bình có trọng số; ảnh >= threshold vào «Đã chọn», trong khoảng review_margin dưới ngưỡng vào «Cần xem lại».
Lỗi nặng bị loại thẳng (mờ, cháy/thiếu sáng nặng, nghiêng, nhắm mắt nếu reject_closed_eyes). fix_instead_of_reject đưa ảnh lỗi nhẹ vào «Cần xem lại».
Ứng dụng có hồ sơ học gu người dùng (use_learned, learned_weight 0–0.8; đã đủ dữ liệu: {learned_ready}).
Bạn KHÔNG xem được ảnh, chỉ chỉnh các thiết lập dưới đây.

Thiết lập hiện tại: {settings}
Thống kê thư mục đang mở: {ctx}

Yêu cầu của người dùng: «{msg}»

Trả lời DUY NHẤT một đối tượng JSON, không kèm chữ nào khác:
{{"reply": "câu trả lời ngắn bằng tiếng Việt, giải thích bạn chỉnh gì và vì sao",
 "patch": {{ chỉ các trường cần đổi trong: "preset" ("event"|"balanced"|"portrait"|"landscape"), "weights" {{"exposure"..."people": 0-50}},
   "threshold" (5-9.5), "review_margin" (0-3), "max_selected" (0 = không giới hạn), "keep_per_series" (1-5), "dedupe_seconds",
   "dedupe", "reject_closed_eyes", "review_open_mouth" (ảnh nhân vật chính há miệng vào «Cần xem lại»), "speaker_head_up" (người phát biểu một mình cúi đầu vào «Cần xem lại»), "keep_each_shot" (cảnh chụp nhiều tấm giữ 1 tấm đẹp nhất), "moments", "fix_instead_of_reject", "use_learned" (bool), "learned_weight" (0-0.8) }} }}
Nếu yêu cầu không liên quan tới chọn ảnh, trả "patch": {{}} và giải thích ngắn."#
    )
}

#[derive(Debug, Deserialize)]
pub struct ClaudeAnswer {
    pub reply: String,
    #[serde(default)]
    pub patch: Patch,
}

/// Claude Code `--output-format json` prints {"result": "..."}; the result holds our JSON,
/// possibly wrapped in a code fence.
pub fn parse_claude_answer(stdout: &str) -> Result<ClaudeAnswer, String> {
    let text = serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()
        .and_then(|v| {
            if v.get("is_error").and_then(|e| e.as_bool()) == Some(true) {
                return Some(Err(v.get("result").and_then(|r| r.as_str()).unwrap_or("Claude báo lỗi").to_string()));
            }
            v.get("result").and_then(|r| r.as_str()).map(|s| Ok(s.to_string()))
        })
        .unwrap_or_else(|| Ok(stdout.to_string()))?;
    let (a, b) = (text.find('{'), text.rfind('}'));
    match (a, b) {
        (Some(a), Some(b)) if b > a => serde_json::from_str(&text[a..=b]).map_err(|e| format!("Không đọc được câu trả lời của Claude: {e}")),
        _ => Ok(ClaudeAnswer { reply: text.trim().to_string(), patch: Patch::default() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: [f32; 6] = [20.0, 25.0, 10.0, 10.0, 5.0, 30.0];

    #[test]
    fn understands_common_requests() {
        let u = understand("Ưu tiên ảnh có người cười, độ nét quan trọng. Chỉ giữ 300 ảnh", &W);
        assert_eq!(u.patch.weights.get("people"), Some(&48.0));
        assert_eq!(u.patch.weights.get("sharp"), Some(&40.0));
        assert_eq!(u.patch.max_selected, Some(300));

        let u = understand("khong quan tam mau sac, nguong 7", &W);
        assert_eq!(u.patch.weights.get("color"), Some(&0.0));
        assert_eq!(u.patch.threshold, Some(7.0));

        let u = understand("tôi thích ảnh tông tối", &W);
        assert_eq!(u.patch.weights.get("exposure"), Some(&8.0));
        assert_eq!(u.patch.fix_instead_of_reject, Some(true));

        let u = understand("giữ ảnh nhắm mắt và mỗi loạt giữ 2 ảnh", &W);
        assert_eq!(u.patch.reject_closed_eyes, Some(false));
        assert_eq!(u.patch.keep_per_series, Some(2));

        let u = understand("chọn theo phong cách của tôi", &W);
        assert_eq!(u.patch.use_learned, Some(true));

        assert!(understand("hôm nay trời đẹp quá", &W).patch.is_empty());
    }

    #[test]
    fn understands_events_faces_and_questions() {
        let u = understand("Đây là đám cưới", &W);
        assert_eq!(u.patch.review_open_mouth, Some(true));
        assert_eq!(u.patch.weights.get("people"), Some(&35.0));
        let u = understand("sân khấu ca nhạc", &W);
        assert_eq!((u.patch.reject_closed_eyes, u.patch.keep_per_series), (Some(false), Some(2)));

        assert_eq!(understand("loại ảnh há miệng", &W).patch.review_open_mouth, Some(true));
        assert_eq!(understand("há miệng không sao, cứ giữ", &W).patch.review_open_mouth, Some(false));
        assert_eq!(understand("người phát biểu phải ngẩng đầu", &W).patch.speaker_head_up, Some(true));
        assert_eq!(understand("cúi đầu vẫn giữ", &W).patch.speaker_head_up, Some(false));
        assert_eq!(understand("6 nhóm nhận giải mà chọn thiếu", &W).patch.keep_each_shot, Some(true));
        let u = understand("Mỗi nhóm nhận giải giữ 1 ảnh", &W);
        assert_eq!((u.patch.keep_each_shot, u.patch.keep_per_series), (Some(true), None), "{:?}", u.patch);
        let u = understand("mặt nghiêng đẹp vẫn giữ", &W);
        assert!(u.patch.is_empty() && !u.notes.is_empty());
        assert_eq!(understand("mắt nhân vật chính phải mở", &W).patch.reject_closed_eyes, Some(true));

        let u = understand("tin gu của tôi nhiều hơn", &W);
        assert_eq!(u.patch.learned_weight, Some(0.7));
        assert_eq!(understand("chỉ dùng tiêu chí máy, tắt gu", &W).patch.use_learned, Some(false));

        assert_eq!(understand("Vì sao nhiều ảnh bị loại?", &W).question, Some(Question::Reasons));
        assert_eq!(understand("AI đã học được gì từ tôi?", &W).question, Some(Question::Learned));
        assert_eq!(understand("tóm tắt phiên này", &W).question, Some(Question::Summary));

        let mut s = Settings { threshold: 9.0, ..Default::default() };
        let mut preset = "custom".to_string();
        Patch { reset: Some(true), ..Default::default() }.apply(&mut s, &mut preset);
        assert_eq!((s.threshold, preset.as_str()), (7.5, "event"));
    }

    #[test]
    fn noise_is_not_many() {
        assert!(understand("bỏ qua nhiễu", &W).patch.weights.get("noise") == Some(&0.0));
        assert!(understand("chọn nhiều hơn", &W).patch.weights.is_empty());
    }

    #[test]
    fn applies_with_limits() {
        let mut s = Settings::default();
        let mut preset = "event".to_string();
        let p = Patch { threshold: Some(12.0), max_selected: Some(0), weights: [("people".to_string(), 80.0)].into(), ..Default::default() };
        let lines = p.apply(&mut s, &mut preset);
        assert_eq!((s.threshold, s.weights[5], s.max_selected), (9.5, 50.0, None));
        assert_eq!(preset, "custom");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn reads_claude_code_output() {
        let out = r#"{"type":"result","is_error":false,"result":"```json\n{\"reply\":\"Đã tăng trọng số người\",\"patch\":{\"weights\":{\"people\":40}}}\n```"}"#;
        let a = parse_claude_answer(out).unwrap();
        assert_eq!(a.patch.weights.get("people"), Some(&40.0));
        assert!(parse_claude_answer(r#"{"is_error":true,"result":"Not logged in"}"#).is_err());
    }
}
