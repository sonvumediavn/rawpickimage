//! The optional features Sơn can switch on, and the in-app AI's suggestions for a given shoot.
//!
//! The AI only *suggests*. When "để AI tự bật" is on it may switch on features that work purely
//! on this computer; anything that shares, posts or moves files always waits for Sơn.

use crate::analyze::Analysis;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Options {
    // Core culling switches.
    /// Reject photos where a main face has closed eyes.
    pub reject_closed_eyes: bool,
    /// Send photos where the main person's mouth is wide open (mid-word, shouting) to review.
    pub review_open_mouth: bool,
    /// Keep only the best shot of each burst / near-identical series.
    pub dedupe: bool,
    /// Blend in what the AI learned from Sơn's earlier choices.
    pub use_learned: bool,

    // The ten extras.
    /// 1. Scene coverage: every moment of the event keeps at least one photo.
    pub moments: bool,
    /// 2. Key people (bride, groom, speaker…): never drop their only good photo. Planned.
    pub key_people: bool,
    /// 3. Fix instead of reject: slightly tilted or slightly off exposure goes to review with a fix note.
    pub fix_instead_of_reject: bool,
    /// 4. Merge several cameras on one timeline (clock offsets per camera).
    pub multi_camera: bool,
    /// 5. Safe card copy: copy from the memory card with checksums and a backup before culling.
    pub safe_copy: bool,
    /// 6. Live delivery: send picks to Google Drive during the event. Phase 2.
    pub live_delivery: bool,
    /// 7. Guests find their photos with a selfie. Phase 4.
    pub selfie_search: bool,
    /// 8. Safety check before posting (unflattering shots, sensitive content). Phase 3.
    pub safety_check: bool,
    /// 9. Logo / watermark on exported copies. Phase 2.
    pub watermark: bool,
    /// 10. Shoot report: reasons, camera stats, dust-spot warnings.
    pub report: bool,

    /// Let the AI switch on suggested on-device features by itself.
    pub auto_apply: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            reject_closed_eyes: true,
            review_open_mouth: false,
            dedupe: true,
            use_learned: true,
            moments: false,
            key_people: false,
            fix_instead_of_reject: false,
            multi_camera: false,
            safe_copy: false,
            live_delivery: false,
            selfie_search: false,
            safety_check: false,
            watermark: false,
            report: true,
            auto_apply: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureInfo {
    pub key: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    /// Development phase that delivers it (1 = this version).
    pub phase: u8,
    /// Runs only on this computer, so the AI may switch it on by itself.
    pub on_device: bool,
}

pub fn catalog() -> Vec<FeatureInfo> {
    let f = |key, title, desc, phase, on_device| FeatureInfo { key, title, desc, phase, on_device };
    vec![
        f("moments", "Đủ khoảnh khắc", "Chia sự kiện theo thời gian, mỗi khoảnh khắc giữ ít nhất 1 ảnh tốt nhất", 1, true),
        f("key_people", "Nhân vật chính", "Không bao giờ bỏ ảnh tốt duy nhất của cô dâu, chú rể, diễn giả…", 2, true),
        f("fix_instead_of_reject", "Sửa thay vì loại", "Ảnh hơi nghiêng hoặc lệch sáng nhẹ đưa vào Cần xem lại kèm gợi ý sửa", 1, true),
        f("multi_camera", "Gộp nhiều máy", "Ghép ảnh nhiều máy lên một dòng thời gian, chỉnh lệch giờ từng máy", 1, true),
        f("safe_copy", "Chép thẻ an toàn", "Chép từ thẻ nhớ có kiểm tra checksum và bản sao lưu trước khi lọc", 1, true),
        f("live_delivery", "Giao ảnh ngay", "Đẩy ảnh đã chọn lên Google Drive trong lúc sự kiện đang diễn ra", 2, false),
        f("selfie_search", "Tìm ảnh bằng selfie", "Khách gửi ảnh selfie để tìm ảnh có mình", 4, false),
        f("safety_check", "Kiểm tra trước khi đăng", "Chặn ảnh xấu, nhạy cảm trước khi đăng fanpage", 3, false),
        f("watermark", "Logo / watermark", "Đóng logo lên bản xuất để giao khách hoặc đăng mạng", 2, false),
        f("report", "Báo cáo buổi chụp", "Thống kê lý do loại, từng máy, cảnh báo bụi cảm biến", 1, true),
    ]
}

impl Options {
    pub fn get(&self, key: &str) -> bool {
        match key {
            "moments" => self.moments,
            "key_people" => self.key_people,
            "fix_instead_of_reject" => self.fix_instead_of_reject,
            "multi_camera" => self.multi_camera,
            "safe_copy" => self.safe_copy,
            "live_delivery" => self.live_delivery,
            "selfie_search" => self.selfie_search,
            "safety_check" => self.safety_check,
            "watermark" => self.watermark,
            "report" => self.report,
            _ => false,
        }
    }

    pub fn set(&mut self, key: &str, on: bool) {
        match key {
            "moments" => self.moments = on,
            "key_people" => self.key_people = on,
            "fix_instead_of_reject" => self.fix_instead_of_reject = on,
            "multi_camera" => self.multi_camera = on,
            "safe_copy" => self.safe_copy = on,
            "live_delivery" => self.live_delivery = on,
            "selfie_search" => self.selfie_search = on,
            "safety_check" => self.safety_check = on,
            "watermark" => self.watermark = on,
            "report" => self.report = on,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub key: String,
    pub on: bool,
    pub reason: String,
}

/// Heuristics that look at the folder before (and after) analysis.
pub fn suggest(analyses: &[&Analysis], source: Option<&Path>) -> Vec<Suggestion> {
    let mut out = vec![];
    let mut push = |key: &str, on: bool, reason: String| out.push(Suggestion { key: key.into(), on, reason });
    let n = analyses.len();

    if let Some(src) = source {
        if looks_like_card(src) {
            push("safe_copy", true, "Thư mục giống thẻ nhớ (có DCIM): nên chép ra máy có kiểm tra trước khi lọc".into());
        }
    }

    let cams: BTreeSet<&str> = analyses.iter().map(|a| a.camera.as_str()).filter(|c| *c != "Không rõ máy").collect();
    if cams.len() > 1 {
        push("multi_camera", true, format!("Có {} máy ảnh trong thư mục: nên gộp chung dòng thời gian và kiểm tra lệch giờ", cams.len()));
    }

    let mut times: Vec<f64> = analyses.iter().filter_map(|a| a.time).collect();
    times.sort_by(f64::total_cmp);
    if times.len() >= 2 {
        let span_h = (times[times.len() - 1] - times[0]) / 3600.0;
        let gaps = times.windows(2).filter(|w| w[1] - w[0] > 90.0).count() + 1;
        if span_h >= 0.75 && n >= 120 {
            push("moments", true, format!("Buổi chụp kéo dài {:.1} giờ với khoảng {gaps} khoảnh khắc: bật để không sót khoảnh khắc nào", span_h));
        }
    }

    let tilted = analyses
        .iter()
        .filter(|a| a.comp.horizon_found && (2.0..8.0).contains(&a.comp.tilt_deg.abs()) && a.scores.sharp >= 5.0)
        .count();
    let off_exp = analyses.iter().filter(|a| (1.5..4.0).contains(&a.scores.exposure) && a.exposure.clip_hi < 0.1 && a.scores.sharp >= 5.0).count();
    if n > 0 && (tilted + off_exp) as f32 / n as f32 > 0.05 {
        push(
            "fix_instead_of_reject",
            true,
            format!("{tilted} ảnh hơi nghiêng và {off_exp} ảnh lệch sáng nhẹ nhưng vẫn nét: có thể cứu thay vì loại"),
        );
    }

    let with_faces = analyses.iter().filter(|a| !a.people.faces.is_empty()).count();
    if n > 0 && with_faces as f32 / n as f32 > 0.4 {
        push("key_people", true, format!("{with_faces}/{n} ảnh có người: nên đánh dấu nhân vật chính (sắp có)"));
        if n >= 300 {
            push("selfie_search", true, "Sự kiện đông người, nhiều ảnh: khách sẽ thích tự tìm ảnh bằng selfie (giai đoạn 4)".into());
        }
    }

    push("report", true, "Báo cáo chạy rất nhanh và giúp xem lại lý do loại ảnh".into());
    out
}

fn looks_like_card(p: &Path) -> bool {
    p.components().any(|c| c.as_os_str().eq_ignore_ascii_case("DCIM")) || p.join("DCIM").is_dir()
}

/// Apply suggestions the AI may apply by itself (on-device features only).
pub fn auto_apply(opts: &mut Options, sugg: &[Suggestion]) -> Vec<String> {
    let mut applied = vec![];
    if !opts.auto_apply {
        return applied;
    }
    let cat = catalog();
    for s in sugg.iter().filter(|s| s.on) {
        if let Some(f) = cat.iter().find(|f| f.key == s.key) {
            if f.on_device && f.phase == 1 && !opts.get(f.key) {
                opts.set(f.key, true);
                applied.push(f.title.to_string());
            }
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_apply_never_turns_on_sharing() {
        let mut o = Options { auto_apply: true, ..Default::default() };
        let s = vec![
            Suggestion { key: "multi_camera".into(), on: true, reason: String::new() },
            Suggestion { key: "live_delivery".into(), on: true, reason: String::new() },
        ];
        let applied = auto_apply(&mut o, &s);
        assert!(o.multi_camera && !o.live_delivery);
        assert_eq!(applied, vec!["Gộp nhiều máy"]);
    }

    #[test]
    fn suggests_multi_camera() {
        let mut a = Analysis::default();
        a.camera = "Canon EOS R6 #1".into();
        let mut b = Analysis::default();
        b.camera = "Sony ILCE-7M4 #2".into();
        let s = suggest(&[&a, &b], None);
        assert!(s.iter().any(|x| x.key == "multi_camera" && x.on));
    }
}
