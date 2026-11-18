//! Crop without losing quality.
//!
//! - JPEG: cut out the chosen part straight from the compressed data (libjpeg-turbo lossless
//!   transform). The pixels inside are bit-for-bit the camera's; nothing is decoded and saved
//!   again. The left/top edge snaps to the JPEG's 8 or 16 px blocks, so the frame may be up to
//!   15 px wider on those sides, never narrower.
//! - RAW: the file is copied untouched, with the crop written into its `.xmp` sidecar for
//!   Lightroom / Camera Raw / Bridge; optionally also a cropped JPEG from the camera's
//!   full-size preview (or its JPEG twin).
//!
//! A crop is stored as `[x, y, w, h]`, fractions of the photo as seen upright.

use anyhow::{bail, Context, Result};

pub type Rect = [f32; 4];

/// Clamp to the picture and refuse empty frames.
pub fn clean(r: Rect) -> Option<Rect> {
    let x = r[0].clamp(0.0, 1.0);
    let y = r[1].clamp(0.0, 1.0);
    let w = r[2].min(1.0 - x);
    let h = r[3].min(1.0 - y);
    (w >= 0.02 && h >= 0.02 && (w < 0.999 || h < 0.999)).then_some([x, y, w, h])
}

/// The same frame in the file's stored orientation (before EXIF orientation `o` turns it upright).
pub fn to_stored(r: Rect, o: u16) -> Rect {
    let [x, y, w, h] = r;
    match o {
        2 => [1.0 - x - w, y, w, h],
        3 => [1.0 - x - w, 1.0 - y - h, w, h],
        4 => [x, 1.0 - y - h, w, h],
        5 => [y, x, h, w],
        6 => [y, 1.0 - x - w, h, w],
        7 => [1.0 - y - h, 1.0 - x - w, h, w],
        8 => [1.0 - y - h, x, h, w],
        _ => r,
    }
}

/// Crop a JPEG losslessly. `r` is the upright frame, `o` the orientation that makes the stored
/// picture upright. With `upright`, the result is also turned upright (losslessly) and carries no
/// metadata, for previews pulled out of RAW files whose orientation lives in the RAW; otherwise
/// the stored orientation and all metadata (EXIF, colour profile) are kept.
pub fn crop_jpeg(bytes: &[u8], r: Rect, o: u16, upright: bool) -> Result<Vec<u8>> {
    use turbojpeg::{Transform, TransformCrop, TransformOp};
    let head = turbojpeg::read_header(bytes).context("không đọc được ảnh JPEG")?;
    let (mut bw, mut bh) = head.subsamp.mcu_size();
    let (mut iw, mut ih) = (head.width, head.height);
    // libjpeg-turbo crops after turning: the frame is then given upright.
    let turned = upright && (2..=8).contains(&o);
    if turned && o >= 5 {
        std::mem::swap(&mut iw, &mut ih);
        std::mem::swap(&mut bw, &mut bh);
    }
    let s = if turned { r } else { to_stored(r, o) };
    let px = |f: f32, n: usize| ((f * n as f32).round() as usize).min(n);
    let (x0, y0) = (px(s[0], iw) / bw * bw, px(s[1], ih) / bh * bh);
    let (x1, y1) = (
        px(s[0] + s[2], iw).max(x0 + 1),
        px(s[1] + s[3], ih).max(y0 + 1),
    );
    if x1 <= x0 || y1 <= y0 {
        bail!("khung crop rỗng");
    }
    let op = if upright {
        match o {
            2 => TransformOp::Hflip,
            3 => TransformOp::Rot180,
            4 => TransformOp::Vflip,
            5 => TransformOp::Transpose,
            6 => TransformOp::Rot90,
            7 => TransformOp::Transverse,
            8 => TransformOp::Rot270,
            _ => TransformOp::None,
        }
    } else {
        TransformOp::None
    };
    let mut t = Transform::op(op);
    t.crop = Some(TransformCrop {
        x: x0,
        y: y0,
        width: Some(x1 - x0),
        height: Some(y1 - y0),
    });
    // Turning a frame whose size is not whole blocks would leave a ragged edge: drop it (a few px).
    t.trim = op != TransformOp::None;
    t.copy_none = upright;
    let out = turbojpeg::transform(&t, bytes).context("không cắt được ảnh JPEG")?;
    Ok(out.to_vec())
}

const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";

/// An `.xmp` sidecar that tells Lightroom / Camera Raw to crop to `r` (upright frame; Adobe
/// stores crops in the sensor's orientation `o`). `existing` is the photo's own sidecar, if any:
/// its other settings are kept, only the crop is replaced.
pub fn xmp_with_crop(existing: Option<&str>, r: Rect, o: u16) -> String {
    let s = to_stored(r, o);
    let attrs = format!(
        " crs:HasCrop=\"True\" crs:CropLeft=\"{:.6}\" crs:CropTop=\"{:.6}\" crs:CropRight=\"{:.6}\" crs:CropBottom=\"{:.6}\" crs:CropAngle=\"0\" crs:CropConstrainToWarp=\"0\"",
        s[0],
        s[1],
        s[0] + s[2],
        s[1] + s[3]
    );
    if let Some(x) = existing.filter(|x| x.contains("<rdf:Description")) {
        let mut x = strip_crop(x);
        let at = x.find("<rdf:Description").unwrap() + "<rdf:Description".len();
        let mut add = String::new();
        if !x.contains("xmlns:crs=") {
            add.push_str(&format!(" xmlns:crs=\"{CRS}\""));
        }
        add.push_str(&attrs);
        x.insert_str(at, &add);
        return x;
    }
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"SonVu Raw Pick\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\" xmlns:crs=\"{CRS}\"{attrs}/>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>\n"
    )
}

/// Remove earlier crop settings (attribute form) from a sidecar.
fn strip_crop(x: &str) -> String {
    let mut out = x.to_string();
    for name in [
        "HasCrop",
        "CropLeft",
        "CropTop",
        "CropRight",
        "CropBottom",
        "CropAngle",
        "CropConstrainToWarp",
    ] {
        let key = format!("crs:{name}=\"");
        while let Some(i) = out.find(&key) {
            let start = out[..i].trim_end().len();
            let Some(end) = out[i + key.len()..].find('"') else {
                break;
            };
            out.replace_range(start..i + key.len() + end + 1, "");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([
                (x * 255 / w) as u8,
                (y * 255 / h) as u8,
                ((x / 16 + y / 16) % 2 * 200) as u8,
            ])
        });
        crate::metrics::encode_jpeg(&img, 90).unwrap()
    }

    #[test]
    fn orientation_round_trip() {
        let r = [0.1, 0.2, 0.3, 0.4];
        // A centred frame stays centred, sizes swap with 90° turns.
        for o in 1..=8u16 {
            let s = to_stored(r, o);
            assert!((s[2] * s[3] - 0.12).abs() < 1e-6, "{o}");
        }
        let near = |a: Rect, b: Rect| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(near(to_stored(r, 6), [0.2, 0.6, 0.4, 0.3]));
        assert!(near(to_stored(r, 8), [0.4, 0.1, 0.4, 0.3]));
        assert!(clean([0.0, 0.0, 1.0, 1.0]).is_none());
        assert!(clean([0.5, 0.5, 0.9, 0.9]).is_some_and(|c| c[2] <= 0.5));
    }

    #[test]
    fn jpeg_crop_keeps_the_pixels() {
        let src = jpeg(640, 480);
        let full = image::load_from_memory(&src).unwrap().to_rgb8();
        let out = crop_jpeg(&src, [0.25, 0.25, 0.5, 0.5], 1, false).unwrap();
        let c = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(c.dimensions(), (320, 240));
        // Lossless: identical pixels to the same region of the decoded original (away from edges).
        let (mut diff, mut n) = (0u64, 0u64);
        for y in 16..224 {
            for x in 16..304 {
                let a = c.get_pixel(x, y);
                let b = full.get_pixel(x + 160, y + 120);
                diff += (0..3)
                    .map(|k| (a[k] as i32 - b[k] as i32).unsigned_abs() as u64)
                    .sum::<u64>();
                n += 3;
            }
        }
        assert!(
            (diff as f64 / n as f64) < 0.5,
            "mean diff {}",
            diff as f64 / n as f64
        );
        // Upright: a portrait photo stored sideways (orientation 6) comes out standing.
        let up = crop_jpeg(&src, [0.0, 0.0, 1.0, 0.5], 6, true).unwrap();
        let u = image::load_from_memory(&up).unwrap().to_rgb8();
        assert_eq!(u.dimensions(), (480, 320));
        let rot = image::imageops::rotate90(&full);
        let d: f64 = (16..300)
            .flat_map(|y| (16..460).map(move |x| (x, y)))
            .map(|(x, y)| {
                let (a, b) = (u.get_pixel(x, y), rot.get_pixel(x, y));
                (0..3)
                    .map(|k| (a[k] as i32 - b[k] as i32).abs() as f64)
                    .sum::<f64>()
            })
            .sum::<f64>()
            / (284.0 * 444.0 * 3.0);
        assert!(d < 0.5, "upright differs: {d}");
    }

    #[test]
    fn sidecar_gets_the_crop_and_keeps_the_rest() {
        let x = xmp_with_crop(None, [0.1, 0.1, 0.5, 0.5], 1);
        assert!(x.contains("crs:HasCrop=\"True\"") && x.contains("crs:CropRight=\"0.600000\""));
        let lr = r#"<x:xmpmeta><rdf:RDF><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="3" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+0.50" crs:HasCrop="False" crs:CropTop="0"/></rdf:RDF></x:xmpmeta>"#;
        let y = xmp_with_crop(Some(lr), [0.2, 0.0, 0.6, 1.0], 1);
        assert!(y.contains("xmp:Rating=\"3\"") && y.contains("crs:Exposure2012=\"+0.50\""));
        assert_eq!(y.matches("crs:HasCrop=").count(), 1);
        assert_eq!(y.matches("crs:CropTop=").count(), 1);
        assert_eq!(y.matches("xmlns:crs=").count(), 1);
    }
}
