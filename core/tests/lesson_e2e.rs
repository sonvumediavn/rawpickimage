//! End to end on real JPEG files: analyse a show with the image model and find the picks.
use rawpick_core::analyze::Analyzer;
use rawpick_core::lesson;
use rawpick_core::pipeline::Progress;
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[test]
fn learns_picks_from_a_real_show() {
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("../models");
    if !models.join(rawpick_core::visual::MODEL).exists() {
        eprintln!("bỏ qua: chưa có mô hình hình ảnh");
        return;
    }
    let Ok(src) = std::env::var("RP_SAMPLE_DIR") else {
        eprintln!("bỏ qua: đặt RP_SAMPLE_DIR là thư mục vài ảnh JPEG khác nhau");
        return;
    };
    let d = tempfile::tempdir().unwrap();
    let raw = d.path().join("show").join("goc");
    let picks = d.path().join("show").join("gui khach");
    std::fs::create_dir_all(&raw).unwrap();
    std::fs::create_dir_all(&picks).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&src).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    for (i, f) in files.iter().enumerate() {
        std::fs::copy(f, raw.join(format!("IMG_{i:04}.jpg"))).unwrap();
    }
    // Two picks, renamed.
    std::fs::copy(&files[1], picks.join("khach_a.jpg")).unwrap();
    std::fs::copy(&files[3], picks.join("khach_b.jpg")).unwrap();
    let (an, _) = Analyzer::new(Some(&models.join("face_detection_yunet_2023mar.onnx")), None);
    let cancel = AtomicBool::new(false);
    let p = Progress { on_step: &|_, _, _| {}, cancel: &cancel };
    let (samples, l) = lesson::learn_show(&d.path().join("show"), &an, None, &p).unwrap();
    eprintln!("{l:?}");
    assert!(samples.iter().all(|s| s.e.len() == rawpick_core::visual::FULL));
    assert_eq!(l.kept, 2, "{l:?}");
    assert_eq!(l.dropped, files.len() - 2);
}
