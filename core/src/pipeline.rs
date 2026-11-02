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

fn cache_key(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(format!("{}|{}|{}", p.display(), m.len(), t))
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
            Some(Sample { name: it.path.display().to_string(), f: learn::features(a).to_vec(), keep: v.decision.keep(), ai_keep: v.ai_decision.keep() })
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
