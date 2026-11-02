//! Classic image-quality measures. Each returns a 0–10 score plus plain-language notes (Vietnamese,
//! shown as-is in the app). These are ports of the browser draft, tuned on its sample set.

use image::{imageops, RgbImage};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Gray {
    pub fn from_rgb(img: &RgbImage) -> Gray {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let px = img.pixels().map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).collect();
        Gray { w, h, px }
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.px[y * self.w + x]
    }
}

pub fn resize_long(img: &RgbImage, long: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let l = w.max(h);
    if l <= long {
        return img.clone();
    }
    let f = long as f32 / l as f32;
    imageops::resize(img, ((w as f32 * f).round() as u32).max(1), ((h as f32 * f).round() as u32).max(1), imageops::FilterType::Triangle)
}

fn clamp10(v: f32) -> f32 {
    v.clamp(0.0, 10.0)
}
fn pct(v: f32) -> String {
    if v < 0.1 {
        format!("{:.1}%", v * 100.0)
    } else {
        format!("{:.0}%", v * 100.0)
    }
}

// ---------------------------------------------------------------- exposure

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Exposure {
    pub score: f32,
    pub mean: f32,
    pub clip_hi: f32,
    pub clip_lo: f32,
    pub range: f32,
    pub hist: Vec<u32>,
    pub notes: Vec<String>,
}

pub fn exposure(g: &Gray) -> Exposure {
    let n = g.px.len() as f32;
    let mut hist = vec![0u32; 256];
    let mut sum = 0.0;
    for &v in &g.px {
        hist[(v as usize).min(255)] += 1;
        sum += v;
    }
    let mean = sum / n;
    let hi: u32 = hist[250..].iter().sum();
    let lo: u32 = hist[..=5].iter().sum();
    let (clip_hi, clip_lo) = (hi as f32 / n, lo as f32 / n);
    let (mut acc, mut p2, mut p98) = (0u32, 0usize, 255usize);
    for (i, &c) in hist.iter().enumerate() {
        acc += c;
        if (acc as f32) < n * 0.02 {
            p2 = i;
        }
        if (acc as f32) < n * 0.98 {
            p98 = i;
        }
    }
    let range = p98 as f32 - p2 as f32;
    let dev = (mean - 118.0) / 118.0;
    let hi_pen = (clip_hi - 0.01).max(0.0) * 55.0;
    let lo_pen = (clip_lo - 0.02).max(0.0) * 35.0;
    let m_pen = (dev.abs() - 0.22).max(0.0) * 9.0;
    let r_pen = if range < 140.0 { (140.0 - range) / 22.0 } else { 0.0 };
    let mut notes = vec![];
    if clip_hi > 0.02 {
        notes.push(format!("Cháy sáng {} diện tích, mất chi tiết vùng sáng", pct(clip_hi)));
    }
    if clip_lo > 0.04 {
        notes.push(format!("Vùng tối bị đen kịt {} diện tích", pct(clip_lo)));
    }
    if dev > 0.3 {
        notes.push(format!("Tổng thể quá sáng (độ sáng TB {:.0}/255)", mean));
    } else if dev < -0.35 {
        notes.push(format!("Tổng thể quá tối (độ sáng TB {:.0}/255)", mean));
    }
    if range < 110.0 {
        notes.push("Tương phản thấp, ảnh bị bệt".into());
    }
    if notes.is_empty() {
        notes.push("Phơi sáng cân đối, giữ được chi tiết cả vùng sáng lẫn vùng tối".into());
    }
    Exposure { score: clamp10(10.0 - hi_pen - lo_pen - m_pen - r_pen), mean, clip_hi, clip_lo, range, hist, notes }
}

// ---------------------------------------------------------------- noise

/// Immerkær fast noise estimate on low-gradient, unclipped pixels (sigma in 8-bit units).
pub fn noise_sigma(g: &Gray) -> f32 {
    let (w, h) = (g.w, g.h);
    if w < 8 || h < 8 {
        return 0.0;
    }
    let n = (w - 2) * (h - 2);
    let mut conv = Vec::with_capacity(n);
    let mut grad = Vec::with_capacity(n);
    let mut ok = Vec::with_capacity(n);
    let mut gh = vec![0u32; 512];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let p = &g.px;
            let (a, b, c) = (p[i - w - 1], p[i - w], p[i - w + 1]);
            let (d, e, f) = (p[i - 1], p[i], p[i + 1]);
            let (gg, hh, ii) = (p[i + w - 1], p[i + w], p[i + w + 1]);
            conv.push((a - 2.0 * b + c - 2.0 * d + 4.0 * e - 2.0 * f + gg - 2.0 * hh + ii).abs());
            let gx = (c + 2.0 * f + ii) - (a + 2.0 * d + gg);
            let gy = (gg + 2.0 * hh + ii) - (a + 2.0 * b + c);
            let m = gx.abs() + gy.abs();
            grad.push(m);
            let o = e > 8.0 && e < 247.0;
            ok.push(o);
            if o {
                gh[((m as usize) >> 1).min(511)] += 1;
            }
        }
    }
    let tot: u32 = gh.iter().sum();
    if tot < 100 {
        return 0.0;
    }
    let (mut acc, mut thr) = (0u32, 0.0f32);
    for (i, &c) in gh.iter().enumerate() {
        acc += c;
        if acc as f32 >= tot as f32 * 0.5 {
            thr = (i * 2 + 2) as f32;
            break;
        }
    }
    let (mut s, mut cnt) = (0.0f64, 0usize);
    for i in 0..n {
        if ok[i] && grad[i] <= thr {
            s += conv[i] as f64;
            cnt += 1;
        }
    }
    if cnt == 0 {
        0.0
    } else {
        ((std::f64::consts::PI / 2.0).sqrt() * s / (6.0 * cnt as f64)) as f32
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Noise {
    pub score: f32,
    pub sigma: f32,
    pub notes: Vec<String>,
}

/// `sigma_native` is the noise at the preview's native resolution.
pub fn noise(sigma_native: f32, iso: Option<u32>) -> Noise {
    let score = clamp10(10.0 - (sigma_native - 1.2).max(0.0) * 1.5);
    let mut note = if sigma_native < 2.0 {
        "Sạch nhiễu".to_string()
    } else if sigma_native < 4.0 {
        "Có nhiễu nhẹ, khử nhiễu dễ".to_string()
    } else {
        "Nhiễu hạt rõ, sẽ lộ khi in hoặc crop".to_string()
    };
    note.push_str(&format!(" (σ ≈ {:.1}{})", sigma_native, iso.map(|i| format!(", ISO {i}")).unwrap_or_default()));
    Noise { score, sigma: sigma_native, notes: vec![note] }
}

// ---------------------------------------------------------------- sharpness

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sharpness {
    pub score: f32,
    pub top: f32,
    /// 8x8 grid of local sharpness (Laplacian sd minus noise), row-major.
    pub tiles: Vec<f32>,
    pub best: usize,
    pub notes: Vec<String>,
}

pub const SHARP_GRID: usize = 8;

pub fn sharpness(g: &Gray, sigma: f32) -> Sharpness {
    let t = SHARP_GRID;
    let (w, h) = (g.w, g.h);
    let mut sum = vec![0f64; t * t];
    let mut sq = vec![0f64; t * t];
    let mut cnt = vec![0u32; t * t];
    for y in 1..h.saturating_sub(1) {
        let ty = (y * t / h).min(t - 1);
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let p = &g.px;
            let l = (p[i - w] + p[i + w] + p[i - 1] + p[i + 1] - 4.0 * p[i]) as f64;
            let k = ty * t + (x * t / w).min(t - 1);
            sum[k] += l;
            sq[k] += l * l;
            cnt[k] += 1;
        }
    }
    let tiles: Vec<f32> = (0..t * t)
        .map(|k| {
            let n = cnt[k].max(1) as f64;
            let m = sum[k] / n;
            ((sq[k] / n - m * m - 20.0 * (sigma as f64).powi(2)).max(0.0)).sqrt() as f32
        })
        .collect();
    let mut sorted = tiles.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let top = (sorted[0] + sorted[1] + sorted[2]) / 3.0;
    let best = (0..t * t).max_by(|&a, &b| tiles[a].partial_cmp(&tiles[b]).unwrap()).unwrap_or(0);
    let score = clamp10(10.0 / (1.0 + (-(top.max(0.01).ln() - 11f32.ln()) * 2.4).exp()));
    let mut notes = vec![];
    if score < 4.0 {
        notes.push("Ảnh mờ hoặc lệch nét: ngay cả vùng nét nhất vẫn mềm".into());
    } else if score < 6.5 {
        notes.push("Độ nét trung bình, chi tiết chưa sắc".into());
    } else {
        notes.push("Vùng nét nhất có chi tiết sắc rõ".into());
    }
    if sigma > 2.5 {
        notes.push("Đã trừ phần nhiễu để không tính nhầm nhiễu là chi tiết".into());
    }
    Sharpness { score, top, tiles, best, notes }
}

// ---------------------------------------------------------------- colour

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Color {
    pub score: f32,
    pub colorfulness: f32,
    pub tint: f32,
    pub temp: f32,
    pub notes: Vec<String>,
}

pub fn color(img: &RgbImage, g: &Gray) -> Color {
    let n = (img.width() * img.height()) as f64;
    let (mut mrg, mut myb, mut srg, mut syb) = (0f64, 0f64, 0f64, 0f64);
    let mut yh = vec![0u32; 256];
    let near_neutral = |r: f32, gg: f32, b: f32| {
        let mx = r.max(gg).max(b);
        mx < 250.0 && mx - r.min(gg).min(b) < 0.4 * mx
    };
    for (i, p) in img.pixels().enumerate() {
        let (r, gg, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let rg = (r - gg) as f64;
        let yb = (0.5 * (r + gg) - b) as f64;
        mrg += rg;
        myb += yb;
        srg += rg * rg;
        syb += yb * yb;
        if g.px[i] > 40.0 && near_neutral(r, gg, b) {
            yh[(g.px[i] as usize).min(255)] += 1;
        }
    }
    mrg /= n;
    myb /= n;
    let sd = ((srg / n - mrg * mrg).max(0.0) + (syb / n - myb * myb).max(0.0)).sqrt();
    let c = (sd + 0.3 * (mrg * mrg + myb * myb).sqrt()) as f32;

    // White patch: the brightest near-neutral pixels should be grey; their tint is the cast.
    let tot: u32 = yh.iter().sum();
    let (mut acc, mut cut) = (0u32, 255usize);
    for k in (0..256).rev() {
        acc += yh[k];
        if acc as f32 >= tot as f32 * 0.05 {
            cut = k;
            break;
        }
    }
    let (mut rr, mut gsum, mut bb, mut cnt) = (0f64, 0f64, 0f64, 0u32);
    for (i, p) in img.pixels().enumerate() {
        let (r, gg, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        if g.px[i] >= cut as f32 && g.px[i] > 40.0 && near_neutral(r, gg, b) {
            rr += r as f64;
            gsum += gg as f64;
            bb += b as f64;
            cnt += 1;
        }
    }
    let cast_ok = tot as f64 > n * 0.01 && cnt > 0;
    let (mut tint, mut temp) = (0f32, 0f32);
    if cast_ok {
        let (r, gg, b) = (rr / cnt as f64, gsum / cnt as f64, bb / cnt as f64);
        let m = ((r + gg + b) / 3.0).max(1.0);
        tint = ((gg - (r + b) / 2.0) / m) as f32;
        temp = ((r - b) / m) as f32;
    }
    let mut notes = vec![];
    let cf = if c < 4.0 {
        notes.push("Ảnh gần như đen trắng, không chấm độ rực màu".into());
        8.0
    } else if c < 15.0 {
        notes.push("Màu nhạt, thiếu sức sống".into());
        4.0 + 4.0 * (c - 4.0) / 11.0
    } else if c < 30.0 {
        notes.push("Màu hơi trầm nhưng ổn".into());
        8.0 + 2.0 * (c - 15.0) / 15.0
    } else if c > 95.0 {
        notes.push("Màu quá rực, có thể bị bão hoà".into());
        10.0 - (c - 95.0) / 8.0
    } else {
        notes.push("Độ rực màu dễ chịu".into());
        10.0
    };
    let tint_pen = ((tint.abs() - 0.05).max(0.0) * 40.0).min(5.0);
    let temp_pen = (if temp > 0.0 { (temp - 0.45).max(0.0) * 6.0 } else { (-temp - 0.2).max(0.0) * 8.0 }).min(3.0);
    if tint.abs() > 0.07 {
        notes.push(if tint > 0.0 { "Ám xanh lá, nên chỉnh tint về phía magenta" } else { "Ám tím / magenta, nên chỉnh tint về phía xanh lá" }.into());
    }
    if temp < -0.25 {
        notes.push("Ám lạnh (xanh dương), cân bằng trắng có thể sai".into());
    } else if temp > 0.55 {
        notes.push("Tông rất ấm; ổn nếu là ánh nắng chiều, sai nếu chụp trong nhà".into());
    }
    if !cast_ok {
        notes.push("Ít vùng trung tính (trắng, xám) nên chưa kiểm tra được cân bằng trắng".into());
    } else if tint_pen + temp_pen > 0.5 {
        notes.push("Với file RAW, cân bằng trắng chỉnh lại được khi hậu kỳ".into());
    }
    Color { score: clamp10(cf - tint_pen - temp_pen), colorfulness: c, tint, temp, notes }
}

// ---------------------------------------------------------------- composition

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Composition {
    pub score: f32,
    /// How well the subject sits on a third / centre (0–1).
    pub pos: f32,
    /// Subject position, 0–1 of width / height.
    pub cx: f32,
    pub cy: f32,
    pub tilt_deg: f32,
    pub horizon_y: f32,
    pub horizon_found: bool,
    pub border_pen: f32,
    pub notes: Vec<String>,
}

/// Multi-scale centre-surround contrast in an opponent colour space (simplified Itti saliency).
fn subject_map(img: &RgbImage, gw: usize, gh: usize) -> Vec<f32> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let nc = gw * gh;
    let mut l = vec![vec![0f32; nc]; 3];
    let mut cn = vec![0f32; nc];
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let p = img.get_pixel(x as u32, y as u32);
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            let c = (y * gh / h).min(gh - 1) * gw + (x * gw / w).min(gw - 1);
            l[0][c] += 0.3 * r + 0.59 * g + 0.11 * b;
            l[1][c] += r - g;
            l[2][c] += 0.5 * (r + g) - b;
            cn[c] += 1.0;
        }
    }
    for ch in l.iter_mut() {
        for i in 0..nc {
            ch[i] /= cn[i].max(1.0);
        }
    }
    let blur = |src: &[f32], rad: usize| -> Vec<f32> {
        let mut o = vec![0f32; nc];
        for y in 0..gh {
            for x in 0..gw {
                let (mut s, mut n) = (0f32, 0f32);
                for yy in y.saturating_sub(rad)..=(y + rad).min(gh - 1) {
                    for xx in x.saturating_sub(rad)..=(x + rad).min(gw - 1) {
                        s += src[yy * gw + xx];
                        n += 1.0;
                    }
                }
                o[y * gw + x] = s / n;
            }
        }
        o
    };
    let mut out = vec![0f32; nc];
    for (c, sr) in [(1usize, 6usize), (1, 10), (2, 14)] {
        let cc: Vec<Vec<f32>> = l.iter().map(|a| blur(a, c)).collect();
        let ss: Vec<Vec<f32>> = l.iter().map(|a| blur(a, sr)).collect();
        for i in 0..nc {
            let d0 = cc[0][i] - ss[0][i];
            let d1 = 1.4 * (cc[1][i] - ss[1][i]);
            let d2 = 1.4 * (cc[2][i] - ss[2][i]);
            out[i] += (d0 * d0 + d1 * d1 + d2 * d2).sqrt();
        }
    }
    let mx = out.iter().cloned().fold(0f32, f32::max);
    out.iter().map(|v| if mx > 0.0 { (v / mx).powi(3) } else { 0.0 }).collect()
}

/// Rule-of-thirds / centred placement quality for a subject at (cx, cy).
pub fn placement(cx: f32, cy: f32) -> (f32, f32, f32) {
    let pts = [(1. / 3., 1. / 3.), (2. / 3., 1. / 3.), (1. / 3., 2. / 3.), (2. / 3., 2. / 3.)];
    let d3 = pts.iter().map(|(x, y)| ((cx - x).powi(2) + (cy - y).powi(2)).sqrt()).fold(f32::MAX, f32::min);
    let dc = ((cx - 0.5).powi(2) + (cy - 0.5).powi(2)).sqrt();
    let p3 = (1.0 - d3 / 0.2).clamp(0.0, 1.0);
    let pc = (1.0 - dc / 0.1).clamp(0.0, 1.0) * 0.85;
    (p3, pc, p3.max(pc).max(0.15))
}

pub fn composition(img: &RgbImage, g: &Gray) -> Composition {
    let (w, h) = (g.w, g.h);
    let gw = 48usize;
    let gh = ((48.0 * h as f32 / w as f32).round() as usize).max(8);
    let sal = subject_map(img, gw, gh);
    let (mut wsum, mut cx, mut cy, mut ring) = (0f32, 0f32, 0f32, 0f32);
    for y in 0..gh {
        for x in 0..gw {
            let v = sal[y * gw + x];
            wsum += v;
            cx += v * (x as f32 + 0.5) / gw as f32;
            cy += v * (y as f32 + 0.5) / gh as f32;
            if x < 2 || y < 2 || x >= gw - 2 || y >= gh - 2 {
                ring += v;
            }
        }
    }
    if wsum > 0.0 {
        cx /= wsum;
        cy /= wsum;
    } else {
        cx = 0.5;
        cy = 0.5;
    }
    let mut srt = sal.clone();
    srt.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let top_n = ((gw * gh) as f32 * 0.15).round() as usize;
    let conc = if wsum > 0.0 { srt[..top_n].iter().sum::<f32>() / wsum } else { 0.0 };
    let ring_area = 1.0 - ((gw - 4) * (gh - 4)) as f32 / (gw * gh) as f32;
    let ring_ratio = if wsum > 0.0 { (ring / wsum) / ring_area } else { 0.0 };

    let mut notes = vec![];
    let pos = if conc < 0.32 {
        notes.push("Không có chủ thể nổi bật rõ ràng".to_string());
        0.55
    } else {
        let (p3, pc, pos) = placement(cx, cy);
        notes.push(
            if p3 >= pc && p3 > 0.55 {
                "Chủ thể nằm gần điểm mạnh 1/3"
            } else if pc > p3 && pc > 0.5 {
                "Chủ thể đặt giữa khung, hợp với ảnh đối xứng / chân dung"
            } else {
                "Chủ thể lơ lửng, không theo 1/3 cũng không cân giữa"
            }
            .to_string(),
        );
        pos
    };
    let border_pen = if ring_ratio > 1.4 { ((ring_ratio - 1.4) * 3.5).min(5.0) } else { 0.0 };
    if border_pen > 0.4 {
        notes.push("Chủ thể sát mép hoặc bị cắt khung".into());
    }

    let (tilt, hy, found) = horizon(g);
    let mut tilt_pen = 0.0;
    if found && (0.8..=12.0).contains(&tilt.abs()) {
        tilt_pen = (tilt.abs() * 0.9).min(4.0);
        notes.push(format!(
            "Đường chân trời nghiêng khoảng {:.1}° ({})",
            tilt.abs(),
            if tilt > 0.0 { "bên phải cao hơn" } else { "bên trái cao hơn" }
        ));
    } else if found {
        notes.push("Đường chân trời thẳng".into());
    }
    Composition {
        score: clamp10(3.0 + 7.0 * pos - border_pen - tilt_pen),
        pos,
        cx,
        cy,
        tilt_deg: tilt,
        horizon_y: hy,
        horizon_found: found,
        border_pen,
        notes,
    }
}

/// Strongest horizontal edges per vertical strip, then a RANSAC line through them.
/// Returns (tilt in degrees, positive = right side higher; y of the line at centre, 0–1; found).
pub fn horizon(g: &Gray) -> (f32, f32, bool) {
    let (w, h) = (g.w, g.h);
    let sn = 24usize;
    struct P {
        k: usize,
        x: f32,
        y: f32,
    }
    let mut pts: Vec<P> = vec![];
    for k in 0..sn {
        let x0 = (k * w / sn).max(1);
        let x1 = ((k + 1) * w / sn).min(w - 1);
        if x1 <= x0 {
            continue;
        }
        let mut prof = vec![0f32; h];
        for y in 1..h - 1 {
            let mut s = 0f32;
            for x in x0..x1 {
                let i = y * w + x;
                let p = &g.px;
                s += ((p[i + w - 1] + 2.0 * p[i + w] + p[i + w + 1]) - (p[i - w - 1] + 2.0 * p[i - w] + p[i - w + 1])).abs();
            }
            prof[y] = s / (x1 - x0) as f32;
        }
        let sp: Vec<f32> = (0..h)
            .map(|y| {
                let a = if y > 0 { prof[y - 1] } else { prof[y] };
                let b = if y + 1 < h { prof[y + 1] } else { prof[y] };
                a + 2.0 * prof[y] + b
            })
            .collect();
        let mut used: Vec<usize> = vec![];
        for _ in 0..3 {
            let mut by: Option<usize> = None;
            let y_lo = (h as f32 * 0.04).round() as usize;
            let y_hi = (h as f32 * 0.96) as usize;
            for y in y_lo..y_hi {
                if used.iter().any(|&u| (u as f32 - y as f32).abs() < h as f32 * 0.03) {
                    continue;
                }
                if by.map_or(true, |b| sp[y] > sp[b]) {
                    by = Some(y);
                }
            }
            let Some(by) = by else { break };
            used.push(by);
            pts.push(P { k, x: (x0 + x1) as f32 / 2.0, y: by as f32 });
        }
    }
    let tol = h as f32 * 0.012;
    let mut best: Option<(usize, f32, usize)> = None; // (inliers, slope, anchor index)
    for a in 0..pts.len() {
        for b in a + 1..pts.len() {
            let (p, q) = (&pts[a], &pts[b]);
            if p.k == q.k || (p.k as i32 - q.k as i32).abs() < 4 {
                continue;
            }
            let sl = (q.y - p.y) / (q.x - p.x);
            if sl.atan().abs() > 0.18 {
                continue;
            }
            let mut ks = vec![false; sn];
            let mut sc = 0;
            for r in &pts {
                if ks[r.k] {
                    continue;
                }
                if (p.y + sl * (r.x - p.x) - r.y).abs() <= tol {
                    ks[r.k] = true;
                    sc += 1;
                }
            }
            if best.map_or(true, |(s, _, _)| sc > s) {
                best = Some((sc, sl, a));
            }
        }
    }
    let Some((sc, sl0, ai)) = best else { return (0.0, 0.5, false) };
    if (sc as f32) < sn as f32 * 0.72 {
        return (0.0, 0.5, false);
    }
    let anchor = &pts[ai];
    let mut ks = vec![false; sn];
    let inl: Vec<&P> = pts
        .iter()
        .filter(|r| {
            if ks[r.k] {
                return false;
            }
            let ok = (anchor.y + sl0 * (r.x - anchor.x) - r.y).abs() <= tol;
            if ok {
                ks[r.k] = true;
            }
            ok
        })
        .collect();
    let n = inl.len() as f32;
    let mx = inl.iter().map(|r| r.x).sum::<f32>() / n;
    let my = inl.iter().map(|r| r.y).sum::<f32>() / n;
    let (mut num, mut den) = (0f32, 0f32);
    for r in &inl {
        num += (r.x - mx) * (r.y - my);
        den += (r.x - mx).powi(2);
    }
    let sl = if den > 0.0 { num / den } else { 0.0 };
    let tilt = -sl.atan().to_degrees();
    let hy = (my + sl * (w as f32 / 2.0 - mx)) / h as f32;
    (tilt, hy, true)
}

// ---------------------------------------------------------------- near-duplicate signature

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Signature {
    pub bits: u64,
    pub colors: Vec<u8>,
    pub aspect: f32,
}

pub fn signature(img: &RgbImage, g: &Gray) -> Signature {
    let (w, h) = (g.w as f32, g.h as f32);
    let mut grid = [[[0f32; 4]; 9]; 8];
    for (gy, row) in grid.iter_mut().enumerate() {
        for (gx, cell) in row.iter_mut().enumerate() {
            let (x0, x1) = ((gx as f32 * w / 9.0) as usize, (((gx + 1) as f32 * w / 9.0) as usize).max((gx as f32 * w / 9.0) as usize + 1));
            let (y0, y1) = ((gy as f32 * h / 8.0) as usize, (((gy + 1) as f32 * h / 8.0) as usize).max((gy as f32 * h / 8.0) as usize + 1));
            let mut s = [0f32; 4];
            let mut n = 0f32;
            for y in y0..y1.min(g.h) {
                for x in x0..x1.min(g.w) {
                    let p = img.get_pixel(x as u32, y as u32);
                    s[0] += g.at(x, y);
                    s[1] += p[0] as f32;
                    s[2] += p[1] as f32;
                    s[3] += p[2] as f32;
                    n += 1.0;
                }
            }
            for k in 0..4 {
                cell[k] = s[k] / n.max(1.0);
            }
        }
    }
    let mut bits = 0u64;
    let mut colors = Vec::with_capacity(8 * 8 * 3);
    for y in 0..8 {
        for x in 0..8 {
            if grid[y][x][0] > grid[y][x + 1][0] {
                bits |= 1 << (y * 8 + x);
            }
            colors.extend(grid[y][x][1..].iter().map(|v| v.round().clamp(0.0, 255.0) as u8));
        }
    }
    Signature { bits, colors, aspect: w / h }
}

pub fn similar(a: &Signature, b: &Signature) -> bool {
    if (a.aspect - b.aspect).abs() > 0.05 {
        return false;
    }
    let ham = (a.bits ^ b.bits).count_ones();
    let cd: f32 = a.colors.iter().zip(&b.colors).map(|(x, y)| (*x as f32 - *y as f32).abs()).sum::<f32>() / a.colors.len() as f32;
    ham <= 12 && cd < 14.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32, cell: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let v = if ((x / cell) + (y / cell)) % 2 == 0 { 40 } else { 210 };
            image::Rgb([v, v, v])
        })
    }

    #[test]
    fn blur_lowers_sharpness() {
        let sharp = checker(400, 300, 3);
        let soft = imageops::blur(&sharp, 4.0);
        let s1 = sharpness(&Gray::from_rgb(&sharp), 0.0).score;
        let s2 = sharpness(&Gray::from_rgb(&soft), 0.0).score;
        assert!(s1 > 8.0 && s2 < 4.0, "{s1} {s2}");
    }

    #[test]
    fn overexposure_is_penalised() {
        let mid = RgbImage::from_fn(200, 150, |x, _| image::Rgb([(x + 20) as u8; 3]));
        let blown = RgbImage::from_pixel(200, 150, image::Rgb([255, 255, 255]));
        assert!(exposure(&Gray::from_rgb(&mid)).score > exposure(&Gray::from_rgb(&blown)).score + 5.0);
    }

    #[test]
    fn noise_estimate_tracks_added_noise() {
        let mut seed = 7u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 1000) as f32 / 1000.0
        };
        let clean = RgbImage::from_pixel(300, 200, image::Rgb([120, 120, 120]));
        let noisy = RgbImage::from_fn(300, 200, |_, _| {
            let n = (rnd() + rnd() + rnd() + rnd() - 2.0) * 30.0;
            let v = (120.0 + n).clamp(0.0, 255.0) as u8;
            image::Rgb([v, v, v])
        });
        assert!(noise_sigma(&Gray::from_rgb(&clean)) < 0.5);
        assert!(noise_sigma(&Gray::from_rgb(&noisy)) > 8.0);
    }

    #[test]
    fn detects_tilted_horizon() {
        let (w, h) = (640u32, 427u32);
        let tilt = 5f32.to_radians();
        let img = RgbImage::from_fn(w, h, |x, y| {
            let line = h as f32 * 0.5 - (x as f32 - w as f32 / 2.0) * tilt.tan();
            if (y as f32) < line { image::Rgb([180, 200, 230]) } else { image::Rgb([60, 70, 50]) }
        });
        let (t, _, found) = horizon(&Gray::from_rgb(&img));
        assert!(found);
        assert!((t - 5.0).abs() < 0.6, "tilt {t}");
    }

    #[test]
    fn duplicate_signature() {
        let a = checker(300, 200, 25);
        let b = imageops::blur(&a, 1.0);
        let c = RgbImage::from_fn(300, 200, |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, 30]));
        let (sa, sb, sc) = (signature(&a, &Gray::from_rgb(&a)), signature(&b, &Gray::from_rgb(&b)), signature(&c, &Gray::from_rgb(&c)));
        assert!(similar(&sa, &sb));
        assert!(!similar(&sa, &sc));
    }
}
