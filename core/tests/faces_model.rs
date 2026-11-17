//! Runs the real YuNet model on two real photos (public-domain NASA portrait; the second has
//! the eyes painted closed). Skipped when the model is missing, unless RAWPICK_REQUIRE_MODEL=1
//! (set in CI after scripts/fetch-models.sh).
#![cfg(feature = "faces")]

use rawpick_core::analyze::Analyzer;
use std::path::{Path, PathBuf};

fn model() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../models/face_detection_yunet_2023mar.onnx");
    if p.exists() {
        Some(p)
    } else if std::env::var("RAWPICK_REQUIRE_MODEL").as_deref() == Ok("1") {
        panic!("thiếu {}", p.display());
    } else {
        eprintln!("bỏ qua: chưa có mô hình YuNet (chạy scripts/fetch-models.sh)");
        None
    }
}

#[test]
fn finds_the_face_and_tells_open_from_closed_eyes() {
    let Some(m) = model() else { return };
    let (an, warn) = Analyzer::new(Some(&m), None);
    assert!(warn.is_none(), "{warn:?}");
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let open = an.analyze(&data.join("chan-dung.jpg"), vec![], 0).unwrap();
    assert!(open.faces_checked);
    assert_eq!(open.people.faces.len(), 1, "{:?}", open.people);
    assert!(!open.people.closed_eyes, "{:?}", open.people.faces[0]);
    let f = &open.people.faces[0];
    // bbox is normalised; the face is centred around x ≈ 0.44.
    let cx = f.bbox[0] + f.bbox[2] / 2.0;
    assert!((0.3..0.6).contains(&cx), "{:?}", f.bbox);

    eprintln!("mở: mạng mắt {:?}, điểm ảnh {:.2}, khả năng nhắm {:.2}", f.eye_p, f.eye_open, f.p_closed);
    // A portrait facing the camera: not looking down, not turned.
    eprintln!("đầu {:.3}, nghiêng {:.3}, miệng {:.3}", f.pitch, f.yaw, f.mouth_open);
    assert!(!f.down && !f.turned, "{:?}", f);
    assert!(f.pitch.abs() < 0.2, "pitch {}", f.pitch);
    if std::env::var("RAWPICK_REQUIRE_MODEL").as_deref() == Ok("1") {
        assert!(f.eye_p.is_some(), "thiếu mô hình mắt {}", rawpick_core::eyes::MODEL);
    }

    // The image model also says what is in the picture (sorting photos by kind).
    if an.visual.is_some() {
        eprintln!("lớp ImageNet: {:?}, loại ảnh {:?}", open.objects, rawpick_core::scene::classify(&open));
        assert!(!open.objects.is_empty());
        assert_eq!(rawpick_core::scene::classify(&open), rawpick_core::scene::Scene::Portrait);
    }
    let closed = an.analyze(&data.join("chan-dung-nham-mat.jpg"), vec![], 1).unwrap();
    let g = &closed.people.faces[0];
    eprintln!("nhắm: mạng mắt {:?}, điểm ảnh {:.2}, khả năng nhắm {:.2}", g.eye_p, g.eye_open, g.p_closed);
    assert!(closed.people.closed_eyes, "{:?}", closed.people.faces);
    // Same person with the eyes closed: the face fingerprint still says it is her.
    if !f.id.is_empty() {
        let same = rawpick_core::faceid::similarity(&f.id, &g.id).unwrap();
        eprintln!("cùng người: {same:.3}");
        assert!(same >= rawpick_core::faceid::SAME_PERSON, "{same}");
    } else if std::env::var("RAWPICK_REQUIRE_MODEL").as_deref() == Ok("1") {
        panic!("thiếu mô hình nhận diện người {}", rawpick_core::faceid::MODEL);
    }
    assert!(closed.scores.people.unwrap() < open.scores.people.unwrap());
}

#[test]
fn finds_a_far_speaker_in_a_wide_shot_and_knows_who_it_is() {
    // A hall seen from the back: the speaker is a small face far away (about 22 px in the
    // analysis image). The app looks again at full resolution, judges the eyes, and the face
    // fingerprint still matches the same person seen close up.
    let Some(m) = model() else { return };
    let (an, _) = Analyzer::new(Some(&m), None);
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let face = image::open(data.join("chan-dung.jpg")).unwrap().to_rgb8();
    let side: u32 = std::env::var("SIDE").ok().and_then(|v| v.parse().ok()).unwrap_or(260);
    let small = image::imageops::resize(&face, side, side, image::imageops::FilterType::Lanczos3);
    // Rows of seats: plenty of sharp detail, as in a real hall.
    let mut hall = image::RgbImage::from_fn(6000, 4000, |x, y| {
        let seat = ((x / 7) % 2 == 0) ^ ((y / 9) % 2 == 0);
        if seat { image::Rgb([150, 40, 40]) } else { image::Rgb([30, 30, 35]) }
    });
    image::imageops::overlay(&mut hall, &small, 4300, 1300);
    // Someone in the front row, nearer but still small: the first look finds only them.
    let near = image::imageops::resize(&face, 800, 800, image::imageops::FilterType::Lanczos3);
    image::imageops::overlay(&mut hall, &near, 800, 2900);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("toan-canh.jpg");
    std::fs::write(&p, rawpick_core::metrics::encode_jpeg(&hall, 92).unwrap()).unwrap();
    let _ = an.analyze(&data.join("chan-dung.jpg"), vec![], 9).unwrap(); // warm up
    let empty = dir.path().join("trong.jpg");
    std::fs::write(&empty, rawpick_core::metrics::encode_jpeg(&image::RgbImage::from_fn(6000, 4000, |x, y| if ((x / 7) % 2 == 0) ^ ((y / 9) % 2 == 0) { image::Rgb([150, 40, 40]) } else { image::Rgb([30, 30, 35]) }), 92).unwrap()).unwrap();
    let t = std::time::Instant::now();
    let e = an.analyze(&empty, vec![], 2).unwrap();
    eprintln!("không người: {} mặt trong {:?}", e.people.faces.len(), t.elapsed());
    let t = std::time::Instant::now();
    let a = an.analyze(&p, vec![], 0).unwrap();
    eprintln!("toàn cảnh: {:?} trong {:?}", a.people.faces.iter().map(|f| (f.bbox, f.small, f.eye_seen, f.eye_p, f.id.len())).collect::<Vec<_>>(), t.elapsed());
    assert_eq!(a.people.faces.len(), 2, "{:?}", a.people.faces);
    // The speaker is the one on the right (faces are sorted left to right).
    let f = &a.people.faces[1];
    assert!(!f.small && f.eye_seen, "the far face is measured at full resolution: {f:?}");
    assert!(!a.people.closed_eyes);
    let close = an.analyze(&data.join("chan-dung.jpg"), vec![], 1).unwrap();
    if !close.people.faces[0].id.is_empty() {
        let same = rawpick_core::faceid::similarity(&f.id, &close.people.faces[0].id).unwrap();
        eprintln!("cùng người (xa/gần): {same:.3}");
        assert!(same >= rawpick_core::faceid::SAME_PERSON, "{same}");
    }
}
