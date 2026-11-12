//! Frames of one shot that the pixel fingerprint alone splits apart (small pan, zoom, rotation)
//! are grouped by the image model, so only the best one is kept. Needs models/mobilenetv2-12.onnx
//! (required in CI with RAWPICK_REQUIRE_MODEL=1).
#![cfg(feature = "faces")]

use rawpick_core::analyze::Analyzer;
use rawpick_core::cull::{cull, Decision, Reason, Settings};
use std::path::Path;

#[test]
fn burst_frames_are_grouped_by_picture() {
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("../models");
    if !models.join(rawpick_core::visual::MODEL).exists() {
        assert!(std::env::var("RAWPICK_REQUIRE_MODEL").as_deref() != Ok("1"), "thiếu mô hình hình ảnh");
        eprintln!("bỏ qua: chưa có mô hình hình ảnh");
        return;
    }
    // The image model is looked for next to the face model path (which may be missing).
    let (an, _) = Analyzer::new(Some(&models.join("face_detection_yunet_2023mar.onnx")), None);
    let src = image::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/chan-dung.jpg")).unwrap().to_rgb8();
    let (w, h) = src.dimensions();
    let d = tempfile::tempdir().unwrap();
    let frames = [
        image::imageops::crop_imm(&src, 0, 0, w, h).to_image(),
        image::imageops::resize(&image::imageops::crop_imm(&src, w / 14, 0, w - w / 14, h).to_image(), w, h, image::imageops::FilterType::Triangle),
        image::imageops::resize(&image::imageops::crop_imm(&src, w / 16, h / 16, w - w / 8, h - h / 8).to_image(), w, h, image::imageops::FilterType::Triangle),
        image::imageops::rotate180(&src), // another picture
    ];
    let mut items = vec![];
    for (i, f) in frames.iter().enumerate() {
        let p = d.path().join(format!("IMG_{i}.jpg"));
        f.save(&p).unwrap();
        let mut a = an.analyze(&p, vec![p.clone()], i).unwrap();
        a.time = Some(1000.0 + i as f64);
        items.push(a);
    }
    let sig = |i: usize, j: usize| rawpick_core::metrics::similar(&items[i].signature, &items[j].signature);
    let sim = |i: usize, j: usize| rawpick_core::visual::similarity_content(&items[i].embed, &items[j].embed);
    eprintln!("fingerprint 0-1 {} 0-2 {} · model 0-1 {:.3} 0-2 {:.3} 0-3 {:.3}", sig(0, 1), sig(0, 2), sim(0, 1), sim(0, 2), sim(0, 3));
    let refs: Vec<_> = items.iter().map(Some).collect();
    let v = cull(&refs, &Settings::default(), None);
    let dups = v.iter().filter(|x| x.decision == Decision::Rejected(Reason::Duplicate)).count();
    assert_eq!(dups, 2, "{:?}", v.iter().map(|x| (x.decision, x.series)).collect::<Vec<_>>());
    assert!(v[3].decision != Decision::Rejected(Reason::Duplicate));
}
