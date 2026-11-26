//! How people stand and move: body landmarks for each main person, read by a small pose network
//! (MediaPipe Pose landmark "full", Apache-2.0, OpenCV Zoo, 5.5 MB ONNX, ~40 ms per person on CPU).
//!
//! The network wants a square crop around one person. A person detector would cost another
//! 12 MB and 30 ms; the face YuNet already found tells where the body is: below the face, about
//! eight face-heights tall. Parts of that square outside the photo are left black, which is what
//! the network was trained with for people cut by the frame.
//!
//! From the 33 landmarks come the things a photographer looks at:
//! - the frame cutting a person at a joint (knees, ankles, wrists, elbows): the classic crop to avoid;
//! - a hand in front of the face (eyes, nose);
//! - an open, lively gesture (a speaker's hand raised, arms out) versus arms crossed;
//! - shoulders sloping, the body turned away from the camera;
//! - two people's hands meeting (a handshake, an award or a gift handed over): the moment of the shot.

use serde::{Deserialize, Serialize};

/// File name of the pose model inside `models/`.
pub const MODEL: &str = "pose_estimation_mediapipe_2023mar.onnx";
/// People whose pose is read in one photo (largest faces first).
pub const MAX_PEOPLE: usize = 3;
/// Faces narrower than this (pixels) are too far away for a useful pose.
pub const MIN_FACE: f32 = 24.0;
const INPUT: usize = 256;

// MediaPipe BlazePose landmark numbers.
pub const NOSE: usize = 0;
pub const L_SHOULDER: usize = 11;
pub const R_SHOULDER: usize = 12;
pub const L_ELBOW: usize = 13;
pub const R_ELBOW: usize = 14;
pub const L_WRIST: usize = 15;
pub const R_WRIST: usize = 16;
pub const L_HIP: usize = 23;
pub const R_HIP: usize = 24;
pub const L_KNEE: usize = 25;
pub const R_KNEE: usize = 26;
pub const L_ANKLE: usize = 27;
pub const R_ANKLE: usize = 28;
pub const L_FOOT: usize = 31;
pub const R_FOOT: usize = 32;
/// Hand points (pinky, index, thumb) of each side.
const L_HAND: [usize; 4] = [15, 17, 19, 21];
const R_HAND: [usize; 4] = [16, 18, 20, 22];

/// One landmark: position in the photo's pixels, and how sure the network is that the point is
/// inside the picture and not hidden (0–1).
#[derive(Debug, Clone, Copy, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub vis: f32,
    pub pres: f32,
}

/// The body of one person, as landmarks in photo pixels.
#[derive(Debug, Clone)]
pub struct Skeleton {
    pub pts: Vec<Point>,
    /// The network's confidence that a person is there at all.
    pub conf: f32,
}

/// What the pose says about one person (kept with the face, cached).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Body {
    /// Joints the frame edge cuts through ("đầu gối", "cổ chân", "cổ tay", "khuỷu tay").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cut: Vec<String>,
    /// A hand covers part of the face (eyes, nose).
    #[serde(default)]
    pub hand_face: bool,
    /// How lively the arms are, 0–1: a hand raised above the elbow and away from the body (a
    /// speaker gesturing, a wave, a cheer).
    #[serde(default)]
    pub gesture: f32,
    #[serde(default)]
    pub arms_crossed: bool,
    /// Shoulder line slope in degrees (sign: right shoulder lower is positive).
    #[serde(default)]
    pub shoulder_tilt: f32,
    /// Body turned away from the camera, 0 (square to it) – 1 (side on or back).
    #[serde(default)]
    pub turned: f32,
    /// Share of the body (shoulders to ankles) inside the picture, 0–1.
    #[serde(default)]
    pub in_frame: f32,
    /// Wrists seen inside the photo, both coordinates as fractions of the photo's width (for
    /// hands meeting between two people).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hands: Vec<(f32, f32)>,
}

fn sigmoid(v: f32) -> f32 {
    1.0 / (1.0 + (-v).exp())
}

/// Square region (x0, y0, side) in photo pixels where the body of the face `f` = (x, y, w, h) is.
pub fn body_roi(f: [f32; 4]) -> (f32, f32, f32) {
    let (x, y, w, h) = (f[0], f[1], f[2], f[3]);
    let fh = h.max(w);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    // Hips about three face-heights below the face centre; the square reaches from above the
    // head to below the feet of a standing person.
    let hip_y = cy + 3.0 * fh;
    let side = 9.0 * fh;
    (cx - side / 2.0, hip_y - side / 2.0, side)
}

#[cfg(feature = "faces")]
pub struct PoseModel {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
}

#[cfg(feature = "faces")]
impl PoseModel {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use tract_onnx::prelude::*;
        let model = tract_onnx::onnx().model_for_path(path)?.with_input_fact(0, f32::fact([1, INPUT, INPUT, 3]).into())?.into_optimized()?.into_runnable()?;
        Ok(Self { model })
    }

    /// Landmarks of the person whose face box (pixels) is `face`.
    pub fn skeleton(&self, img: &image::RgbImage, face: [f32; 4]) -> Option<Skeleton> {
        use tract_onnx::prelude::*;
        let (x0, y0, side) = body_roi(face);
        let canvas = crop_square(img, x0, y0, side);
        let input: Tensor = tract_ndarray::Array4::<f32>::from_shape_fn((1, INPUT, INPUT, 3), |(_, y, x, c)| canvas.get_pixel(x as u32, y as u32)[c] as f32 / 255.0).into();
        let out = self.model.run(tvec!(input.into())).ok()?;
        let lm: Vec<f32> = out[0].to_plain_array_view::<f32>().ok()?.iter().copied().collect();
        let conf = out[1].to_plain_array_view::<f32>().ok()?.iter().copied().next()?;
        if lm.len() < 33 * 5 {
            return None;
        }
        let k = side / INPUT as f32;
        let pts = (0..33)
            .map(|i| Point { x: x0 + lm[i * 5] * k, y: y0 + lm[i * 5 + 1] * k, vis: sigmoid(lm[i * 5 + 3]), pres: sigmoid(lm[i * 5 + 4]) })
            .collect();
        Some(Skeleton { pts, conf })
    }
}

/// The square (x0, y0, side) of `img` scaled to the network's input; outside the photo is black.
pub fn crop_square(img: &image::RgbImage, x0: f32, y0: f32, side: f32) -> image::RgbImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let mut canvas = image::RgbImage::new(INPUT as u32, INPUT as u32);
    let k = INPUT as f32 / side.max(1.0);
    let (ix0, iy0) = (x0.max(0.0), y0.max(0.0));
    let (ix1, iy1) = ((x0 + side).min(w), (y0 + side).min(h));
    if ix1 - ix0 < 2.0 || iy1 - iy0 < 2.0 {
        return canvas;
    }
    let part = image::imageops::crop_imm(img, ix0 as u32, iy0 as u32, (ix1 - ix0) as u32, (iy1 - iy0) as u32).to_image();
    let (pw, ph) = (((ix1 - ix0) * k).round().max(1.0) as u32, ((iy1 - iy0) * k).round().max(1.0) as u32);
    let small = crate::metrics::resize(&part, pw.min(INPUT as u32), ph.min(INPUT as u32));
    image::imageops::overlay(&mut canvas, &small, ((ix0 - x0) * k).round() as i64, ((iy0 - y0) * k).round() as i64);
    canvas
}

/// Read the pose of one person: `face` = (x, y, w, h) in pixels of a `w` × `h` photo.
pub fn judge(s: &Skeleton, face: [f32; 4], w: f32, h: f32) -> Option<Body> {
    if s.conf < 0.5 {
        return None;
    }
    let p = &s.pts;
    let seen = |i: usize| p[i].pres > 0.5 && p[i].vis > 0.5;
    let fh = face[3].max(face[2]);
    // The body must belong to this face: the nose the network found sits in the face box.
    let n = p[NOSE];
    if !(n.x > face[0] - 0.3 * fh && n.x < face[0] + face[2] + 0.3 * fh && n.y > face[1] - 0.3 * fh && n.y < face[1] + face[3] + 0.3 * fh) {
        return None;
    }
    let mut body = Body::default();

    // Cut at a joint: the photo's edge runs right through it. Mid-thigh or mid-shin is fine.
    let near = 0.3 * fh;
    let edge = |q: &Point| {
        let d = [q.x, q.y, w - q.x, h - q.y].into_iter().map(f32::abs).fold(f32::MAX, f32::min);
        d < near
    };
    let joints: [(&str, [usize; 2]); 4] = [("đầu gối", [L_KNEE, R_KNEE]), ("cổ chân", [L_ANKLE, R_ANKLE]), ("cổ tay", [L_WRIST, R_WRIST]), ("khuỷu tay", [L_ELBOW, R_ELBOW])];
    // The network always places every landmark ("presence" stays high even far outside the
    // photo); only "visibility" says the joint is really seen. Legs must also hang below the hips
    // (a seated or half-length person gets invented legs folded inside the frame).
    let below = |a: usize, b: usize| p[a].y > p[b].y + 0.5 * fh;
    let real = |i: usize| match i {
        L_KNEE => below(L_KNEE, L_HIP),
        R_KNEE => below(R_KNEE, R_HIP),
        L_ANKLE => below(L_ANKLE, L_KNEE) && below(L_KNEE, L_HIP),
        R_ANKLE => below(R_ANKLE, R_KNEE) && below(R_KNEE, R_HIP),
        _ => true,
    };
    for (name, ids) in joints {
        if ids.iter().any(|&i| p[i].vis > 0.5 && real(i) && edge(&p[i])) {
            body.cut.push(name.to_string());
        }
    }
    // Feet cut off while the ankles are inside.
    let inside = |q: &Point| q.x > 0.0 && q.y > 0.0 && q.x < w && q.y < h;
    if body.cut.is_empty() && [(L_ANKLE, L_FOOT), (R_ANKLE, R_FOOT)].iter().any(|&(a, f)| p[a].vis > 0.5 && real(a) && inside(&p[a]) && !inside(&p[f])) {
        body.cut.push("bàn chân".into());
    }

    // A hand in front of the face: hand points inside the face box above the mouth (a hand on a
    // microphone below the mouth is normal for a speaker).
    let upper = face[1] + face[3] * 0.62;
    let on_face = |i: usize| seen(i) && p[i].x > face[0] && p[i].x < face[0] + face[2] && p[i].y > face[1] && p[i].y < upper;
    body.hand_face = L_HAND.iter().chain(R_HAND.iter()).any(|&i| on_face(i));

    let sw = ((p[L_SHOULDER].x - p[R_SHOULDER].x).powi(2) + (p[L_SHOULDER].y - p[R_SHOULDER].y).powi(2)).sqrt();
    if seen(L_SHOULDER) && seen(R_SHOULDER) {
        body.shoulder_tilt = (p[R_SHOULDER].y - p[L_SHOULDER].y).atan2((p[L_SHOULDER].x - p[R_SHOULDER].x).abs().max(1.0)).to_degrees();
        // Square to the camera the shoulders are about 2.2 face widths apart; side on, much less.
        body.turned = (1.0 - sw / (2.0 * face[2].max(1.0))).clamp(0.0, 1.0);
    }

    // Lively arms: a wrist above its elbow and away from the body.
    let mut g: f32 = 0.0;
    for (sh, el, wr) in [(L_SHOULDER, L_ELBOW, L_WRIST), (R_SHOULDER, R_ELBOW, R_WRIST)] {
        if seen(wr) && seen(el) && seen(sh) {
            let up = ((p[el].y - p[wr].y) / fh).clamp(0.0, 1.5) / 1.5;
            let out = ((p[wr].x - p[sh].x).abs() / sw.max(fh)).clamp(0.0, 1.0);
            let raised = ((p[sh].y + fh - p[wr].y) / (2.0 * fh)).clamp(0.0, 1.0);
            g = g.max((0.5 * up + 0.2 * out + 0.3 * raised).clamp(0.0, 1.0));
        }
    }
    body.gesture = g;
    // Arms crossed: each wrist by the other arm's elbow, in front of the chest.
    let d = |a: usize, b: usize| ((p[a].x - p[b].x).powi(2) + (p[a].y - p[b].y).powi(2)).sqrt();
    body.arms_crossed = [L_WRIST, R_WRIST, L_ELBOW, R_ELBOW].iter().all(|&i| seen(i)) && d(L_WRIST, R_ELBOW) < 0.9 * fh && d(R_WRIST, L_ELBOW) < 0.9 * fh;

    // Share of shoulders, hips, knees and ankles inside the picture.
    let chain = [L_SHOULDER, R_SHOULDER, L_HIP, R_HIP, L_KNEE, R_KNEE, L_ANKLE, R_ANKLE];
    body.in_frame = chain.iter().filter(|&&i| inside(&p[i]) && p[i].vis > 0.5 && real(i)).count() as f32 / chain.len() as f32;

    body.hands = [L_WRIST, R_WRIST].iter().filter(|&&i| seen(i) && inside(&p[i])).map(|&i| (p[i].x / w, p[i].y / w)).collect();
    Some(body)
}

/// Two people's hands meeting (a handshake, a certificate or flowers handed over): `bodies` are
/// the people of one photo with their face widths as fractions of the photo's width.
pub fn hands_meet(bodies: &[(&Body, f32)]) -> bool {
    for (i, (a, fa)) in bodies.iter().enumerate() {
        for (b, fb) in bodies.iter().skip(i + 1) {
            let reach = fa.max(*fb) * 1.2;
            for ha in &a.hands {
                for hb in &b.hands {
                    if ((ha.0 - hb.0).powi(2) + (ha.1 - hb.1).powi(2)).sqrt() < reach {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skeleton(f: impl Fn(usize) -> (f32, f32)) -> Skeleton {
        Skeleton { pts: (0..33).map(|i| { let (x, y) = f(i); Point { x, y, vis: 0.9, pres: 0.9 } }).collect(), conf: 0.99 }
    }

    /// A standing person facing the camera, face 40 px tall at (180, 100).
    fn standing(i: usize) -> (f32, f32) {
        match i {
            0..=10 => (200.0, 120.0),
            L_SHOULDER => (245.0, 170.0),
            R_SHOULDER => (155.0, 170.0),
            L_ELBOW => (255.0, 230.0),
            R_ELBOW => (145.0, 230.0),
            15 | 17 | 19 | 21 => (255.0, 285.0),
            16 | 18 | 20 | 22 => (145.0, 285.0),
            L_HIP => (225.0, 300.0),
            R_HIP => (175.0, 300.0),
            L_KNEE => (225.0, 390.0),
            R_KNEE => (175.0, 390.0),
            L_ANKLE | 29 | 31 => (225.0, 470.0),
            _ => (175.0, 470.0),
        }
    }

    const FACE: [f32; 4] = [180.0, 100.0, 40.0, 45.0];

    #[test]
    fn a_whole_body_inside_is_not_cut() {
        let b = judge(&skeleton(standing), FACE, 400.0, 600.0).unwrap();
        assert!(b.cut.is_empty(), "{b:?}");
        assert!(!b.hand_face && !b.arms_crossed && b.gesture < 0.3, "{b:?}");
        assert!(b.in_frame > 0.99 && b.turned < 0.1, "{b:?}");
    }

    #[test]
    fn cutting_at_the_knees_is_seen() {
        // The same person in a photo whose bottom edge runs through the knees.
        let b = judge(&skeleton(standing), FACE, 400.0, 395.0).unwrap();
        assert!(b.cut.iter().any(|c| c == "đầu gối"), "{b:?}");
        // Mid-thigh is fine.
        let b = judge(&skeleton(standing), FACE, 400.0, 345.0).unwrap();
        assert!(b.cut.is_empty(), "{b:?}");
    }

    #[test]
    fn hand_over_face_and_raised_hand() {
        let s = skeleton(|i| if L_HAND.contains(&i) { (195.0, 112.0) } else { standing(i) });
        assert!(judge(&s, FACE, 400.0, 600.0).unwrap().hand_face);
        // Holding a microphone below the mouth is not a hand over the face.
        let s = skeleton(|i| if L_HAND.contains(&i) { (200.0, 140.0) } else { standing(i) });
        assert!(!judge(&s, FACE, 400.0, 600.0).unwrap().hand_face);
        let s = skeleton(|i| if L_HAND.contains(&i) { (300.0, 120.0) } else if i == L_ELBOW { (280.0, 190.0) } else { standing(i) });
        assert!(judge(&s, FACE, 400.0, 600.0).unwrap().gesture > 0.6);
    }

    #[test]
    fn a_skeleton_of_someone_else_is_ignored() {
        let s = skeleton(|i| if i == NOSE { (20.0, 20.0) } else { standing(i) });
        assert!(judge(&s, FACE, 400.0, 600.0).is_none());
    }

    #[test]
    fn hands_meeting_between_two_people() {
        let a = Body { hands: vec![(0.50, 0.5)], ..Default::default() };
        let b = Body { hands: vec![(0.53, 0.51)], ..Default::default() };
        let c = Body { hands: vec![(0.9, 0.5)], ..Default::default() };
        assert!(hands_meet(&[(&a, 0.05), (&b, 0.05)]));
        assert!(!hands_meet(&[(&a, 0.05), (&c, 0.05)]));
    }
}
