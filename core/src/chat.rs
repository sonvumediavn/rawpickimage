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
    pub moments: Option<bool>,
    pub fix_instead_of_reject: Option<bool>,
    pub use_learned: Option<bool>,
    pub learned_weight: Option<f32>,
}

impl Patch {
    pub fn is_empty(&self) -> bool {
        *self == Patch::default()
    }

    /// Apply to `s` with sane limits; returns a Vietnamese line per change. `preset` is
    /// updated when the preset changes or weights are edited by hand ("custom").
    pub fn apply(&self, s: &mut Settings, preset: &mut String) -> Vec<String> {
        let mut out = vec![];
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
}

/// Offline understanding of a request. `w` = current weights (to raise/lower relative to them).
pub fn understand(msg: &str, w: &[f32; 6]) -> Understood {
    let low = msg.to_lowercase();
    let mut p = Patch::default();
    let mut notes = vec![];

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
            ("comp", &["bo cuc", "khung hinh", "1/3", "can giua", "nghieng"]),
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
    Understood { patch: p, notes }
}

pub const HELP: &str = "Tôi chưa hiểu yêu cầu này. Bạn có thể nói kiểu:\n\
- «ưu tiên ảnh có người, biểu cảm đẹp»\n\
- «độ nét quan trọng nhất», «không quan tâm màu sắc»\n\
- «chỉ giữ 300 ảnh», «ngưỡng 7», «chọn nhiều hơn», «khắt khe hơn»\n\
- «tôi thích ảnh tông tối», «giữ ảnh nhắm mắt», «mỗi loạt giữ 2 ảnh»\n\
- «chọn theo phong cách của tôi», «đủ khoảnh khắc»";

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
   "dedupe", "reject_closed_eyes", "moments", "fix_instead_of_reject", "use_learned" (bool), "learned_weight" (0-0.8) }} }}
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
