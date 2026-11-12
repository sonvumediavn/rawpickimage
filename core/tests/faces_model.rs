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
