//! A culling session: scan a folder, analyse every shot in parallel (with a cache so reopening a
//! folder is instant), decide, learn and export. Shared by the CLI and the desktop app.

use crate::analyze::{self, Analysis, Analyzer};
use crate::cull::{self, Decision, Settings, Verdict};
use crate::export::{self, Entry, ExportOptions, ExportReport};
use crate::learn::{self, Profile, Sample};
use crate::options::{self, Suggestion};
use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub path: PathBuf,
    pub files: Vec<PathBuf>,
    pub analysis: Option<Analysis>,
    pub error: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    /// key: path|size|mtime
    entries: HashMap<String, Analysis>,
}

/// Bump when the analysis measures something new, so cached results are redone once.
const ANALYSIS_VERSION: u32 = 2;

fn cache_key(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(format!("v{ANALYSIS_VERSION}|{}|{}|{}", p.display(), m.len(), t))
}

/// Working folder for one source folder inside the app's data folder.
pub fn work_dir(data_dir: &Path, source: &Path) -> PathBuf {
    let h = crc32fast::hash(source.to_string_lossy().as_bytes());
    data_dir.join("sessions").join(format!("{h:08x}"))
}

/// Threads to use: leave one core for the UI on small machines.
pub fn threads() -> usize {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    if n <= 2 {
        n
    } else {
        n - 1
    }
}

pub struct Progress<'a> {
    pub on_step: &'a (dyn Fn(usize, usize, &str) + Sync),
    pub cancel: &'a AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Timing {
    pub photos: usize,
    pub seconds: f32,
    pub per_photo_ms: f32,
    pub from_cache: usize,
}

pub fn analyze_all(source: &Path, recursive: bool, analyzer: &Analyzer, work: Option<&Path>, p: &Progress) -> Result<(Vec<Item>, Timing)> {
    let t0 = Instant::now();
    let groups = analyze::collect(source, recursive)?;
    let cache_file = work.map(|w| w.join("cache.json"));
    let cache: Cache = cache_file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let done = AtomicUsize::new(0);
    let hits = AtomicUsize::new(0);
    let total = groups.len();
    (p.on_step)(0, total, "Đang phân tích ảnh");
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads()).build()?;
    let items: Vec<Item> = pool.install(|| {
        groups
            .into_par_iter()
            .enumerate()
            .map(|(i, (path, files))| {
                if p.cancel.load(Ordering::Relaxed) {
                    return Item { path, files, analysis: None, error: Some("Đã dừng".into()) };
                }
                let key = cache_key(&path);
                let cached = key.as_ref().and_then(|k| cache.entries.get(k)).filter(|a| a.thumb.as_ref().is_none_or(|t| t.exists()));
                let res = match cached {
                    Some(a) => {
                        hits.fetch_add(1, Ordering::Relaxed);
                        let mut a = a.clone();
                        a.files = files.clone();
                        Ok(a)
                    }
                    None => analyzer.analyze(&path, files.clone(), i),
                };
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                (p.on_step)(n, total, "Đang phân tích ảnh");
                match res {
                    Ok(a) => Item { path, files, analysis: Some(a), error: None },
                    Err(e) => Item { path, files, analysis: None, error: Some(format!("{e:#}")) },
                }
            })
            .collect()
    });
    if let Some(f) = cache_file {
        let mut c = cache;
        c.entries.retain(|k, _| k.starts_with(&format!("v{ANALYSIS_VERSION}|")));
        for it in &items {
            if let (Some(a), Some(k)) = (&it.analysis, cache_key(&it.path)) {
                c.entries.insert(k, a.clone());
            }
        }
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&f, serde_json::to_vec(&c)?);
    }
    let secs = t0.elapsed().as_secs_f32();
    let timing = Timing { photos: total, seconds: secs, per_photo_ms: if total > 0 { secs * 1000.0 / total as f32 } else { 0.0 }, from_cache: hits.into_inner() };
    Ok((items, timing))
}

pub fn decide(items: &[Item], settings: &Settings, profile: Option<&Profile>) -> Vec<Verdict> {
    let refs: Vec<Option<&Analysis>> = items.iter().map(|i| i.analysis.as_ref()).collect();
    cull::cull(&refs, settings, profile)
}

pub fn suggestions(items: &[Item], source: &Path) -> Vec<Suggestion> {
    let refs: Vec<&Analysis> = items.iter().filter_map(|i| i.analysis.as_ref()).collect();
    options::suggest(&refs, Some(source))
}

/// Keep manual choices (keyed by file path) across re-culls with new settings.
pub fn apply_overrides(verdicts: &mut [Verdict], items: &[Item], overrides: &HashMap<String, Decision>) {
    for (v, it) in verdicts.iter_mut().zip(items) {
        if let Some(d) = overrides.get(&it.path.display().to_string()) {
            cull::set_manual(v, *d);
        }
    }
}

/// Every analysed photo becomes a training sample: kept = Sơn's final "Đã chọn".
pub fn training_samples(items: &[Item], verdicts: &[Verdict]) -> Vec<Sample> {
    items
        .iter()
        .zip(verdicts)
        .filter_map(|(it, v)| {
            let a = it.analysis.as_ref()?;
            Some(Sample { name: it.path.display().to_string(), f: learn::features(a).to_vec(), keep: v.decision.keep(), ai_keep: v.ai_decision.keep(), manual: v.overridden })
        })
        .collect()
}

pub fn export_all(source: &Path, items: &[Item], verdicts: &[Verdict], out_dir: &Path, opts: &ExportOptions, p: &Progress) -> Result<ExportReport> {
    let entries: Vec<Entry> = items
        .iter()
        .zip(verdicts)
        .map(|(it, v)| Entry {
            files: &it.files,
            analysis: it.analysis.as_ref(),
            verdict: v,
            rel_dir: it.path.parent().and_then(|d| d.strip_prefix(source).ok()).map(Path::to_path_buf).unwrap_or_default(),
        })
        .collect();
    export::export(&entries, out_dir, opts, &|n, t| (p.on_step)(n, t, "Đang xuất ảnh"), p.cancel)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GatherReport {
    pub dir: PathBuf,
    pub photos: usize,
    /// Files copied now.
    pub files: usize,
    /// Files already there, identical (an earlier run).
    pub skipped: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
}

/// Marker left in a copy folder so later scans of the source folder skip it.
pub const COPY_MARKER: &str = ".sonvu-anh-chon";

/// Copy the selected photos (RAW + JPEG twin + XMP, byte for byte, checksum-verified) into a
/// sub-folder of the source folder. The originals are not touched.
pub fn gather_selected(source: &Path, items: &[Item], verdicts: &[Verdict], sub: &str, p: &Progress) -> Result<GatherReport> {
    let sub = sub.trim().trim_matches(['/', '\\']);
    anyhow::ensure!(!sub.is_empty() && !sub.contains("..") && !sub.contains(['/', '\\', ':']), "Tên thư mục không hợp lệ");
    let dir = source.join(sub);
    std::fs::create_dir_all(&dir)?;
    let _ = std::fs::write(dir.join(COPY_MARKER), "Thư mục ảnh chọn do SonVu Raw Pick chép ra, không phân tích lại.\n");
    let mut rep = GatherReport { dir: dir.clone(), ..Default::default() };
    let chosen: Vec<&Item> = items.iter().zip(verdicts).filter(|(it, v)| v.decision == Decision::Selected && !it.path.starts_with(&dir)).map(|(it, _)| it).collect();
    let total = chosen.len();
    for (n, it) in chosen.into_iter().enumerate() {
        if p.cancel.load(Ordering::Relaxed) {
            rep.errors.push("Đã dừng theo yêu cầu".into());
            break;
        }
        (p.on_step)(n, total, "Đang chép ảnh đã chọn vào thư mục con");
        let rel = it.path.parent().and_then(|d| d.strip_prefix(source).ok()).map(Path::to_path_buf).unwrap_or_default();
        let target = dir.join(rel);
        let mut copied_any = false;
        for f in &it.files {
            let name = f.file_name().unwrap_or_default();
            let mut dst = target.join(name);
            // Same name but a different file already there: keep both.
            let mut k = 2;
            while dst.exists() && !same_file_content(f, &dst) {
                let stem = f.file_stem().unwrap_or_default().to_string_lossy();
                let ext = f.extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default();
                dst = target.join(format!("{stem}_{k}{ext}"));
                k += 1;
            }
            match export::copy_verified(f, &dst) {
                Ok(Some(b)) => {
                    rep.files += 1;
                    rep.bytes += b;
                    copied_any = true;
                }
                Ok(None) => {
                    rep.skipped += 1;
                    copied_any = true;
                }
                Err(e) => rep.errors.push(format!("{e:#}")),
            }
        }
        rep.photos += copied_any as usize;
    }
    (p.on_step)(total, total, "Đang chép ảnh đã chọn vào thư mục con");
    Ok(rep)
}

fn same_file_content(a: &Path, b: &Path) -> bool {
    matches!((export::crc_file(a), export::crc_file(b)), (Ok(x), Ok(y)) if x == y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_selected_shots_with_their_twins_and_keeps_originals() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("Cuoi");
        std::fs::create_dir_all(src.join("Cam2")).unwrap();
        let raw = src.join("Cam2/DSC_1.NEF");
        let jpg = src.join("Cam2/DSC_1.JPG");
        let other = src.join("DSC_9.JPG");
        for (p, b) in [(&raw, &b"raw"[..]), (&jpg, b"jpg"), (&other, b"x")] {
            std::fs::write(p, b).unwrap();
        }
        let items = vec![
            Item { path: jpg.clone(), files: vec![jpg.clone(), raw.clone()], analysis: None, error: None },
            Item { path: other.clone(), files: vec![other.clone()], analysis: None, error: None },
        ];
        let refs: Vec<Option<&Analysis>> = vec![None, None];
        let mut v = cull::cull(&refs, &Settings::default(), None);
        cull::set_manual(&mut v[0], Decision::Selected);
        let cancel = AtomicBool::new(false);
        let p = Progress { on_step: &|_, _, _| {}, cancel: &cancel };
        let r = gather_selected(&src, &items, &v, "Anh-da-chon", &p).unwrap();
        assert_eq!((r.photos, r.files), (1, 2), "{:?}", r.errors);
        assert_eq!(std::fs::read(src.join("Anh-da-chon/Cam2/DSC_1.NEF")).unwrap(), b"raw");
        assert!(raw.exists() && jpg.exists() && other.exists(), "originals stay");
        // Again: nothing new to copy, no duplicates.
        let r = gather_selected(&src, &items, &v, "Anh-da-chon", &p).unwrap();
        assert_eq!((r.files, r.skipped), (0, 2));
        // The copy folder is not analysed again.
        let groups = crate::analyze::collect(&src, true).unwrap();
        assert!(groups.iter().all(|(p, _)| !p.starts_with(src.join("Anh-da-chon"))));
        assert!(gather_selected(&src, &items, &v, "../x", &p).is_err());
    }

    #[test]
    fn moves_read_only_files_and_keeps_the_mark() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a.JPG");
        std::fs::write(&a, b"protected shot").unwrap();
        let mut p = std::fs::metadata(&a).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&a, p).unwrap();
        std::fs::create_dir(t.path().join("sub")).unwrap();
        let b = t.path().join("sub/a.JPG");
        export::move_verified(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&b).unwrap(), b"protected shot");
        assert!(std::fs::metadata(&b).unwrap().permissions().readonly());
    }
}
