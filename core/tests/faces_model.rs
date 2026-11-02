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

    let closed = an.analyze(&data.join("chan-dung-nham-mat.jpg"), vec![], 1).unwrap();
    assert!(closed.people.closed_eyes, "{:?}", closed.people.faces);
    assert!(closed.scores.people.unwrap() < open.scores.people.unwrap());
}
