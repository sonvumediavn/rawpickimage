//! «Học từ show cũ»: Sơn drops in the folder of a show he already culled, in whatever layout
//! he keeps it. The app finds which photos he picked by looking at the pictures, not at file
//! or folder names, and turns the whole show into lessons for the AI.
//!
//! How the picks are found: a folder of picks is a folder whose photos are nearly all copies of
//! photos elsewhere in the show (same picture, maybe re-exported or colour-graded, maybe
//! renamed). A copy is recognised by the same capture time (EXIF survives exports) and a
//! similar look, or by a near-identical look alone. Originals with a copy among the picks were
//! kept, the other originals were dropped. With no originals around, every photo counts as a
//! pick.

use crate::analyze::{self, Analyzer};
use crate::learn::Sample;
use crate::pipeline::{self, Item, Progress};
use crate::visual;
use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Same picture: capture times this close (seconds) and looks at least this similar…
const SAME_TIME: f64 = 0.6;
const SIM_WITH_TIME: f32 = 0.75;
/// …or, without usable times, looks this similar.
const SIM_ALONE: f32 = 0.93;
/// Share of a folder's photos that must be copies for it to count as a folder of picks.
const PICK_SHARE: f32 = 0.6;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Lesson {
    /// Photos looked at.
    pub photos: usize,
    /// Originals Sơn kept / dropped.
    pub kept: usize,
    pub dropped: usize,
    /// Folders recognised as his picks.
    pub pick_folders: Vec<PathBuf>,
    /// No originals found: everything was taken as a pick.
    pub only_picks: bool,
    /// Photos that could not be read.
    pub unreadable: usize,
}

/// Look at a whole show and return what it teaches.
pub fn learn_show(dir: &Path, analyzer: &Analyzer, work: Option<&Path>, p: &Progress) -> Result<(Vec<Sample>, Lesson)> {
    let groups = analyze::collect_with(dir, true, false)?;
    let (items, _) = pipeline::analyze_groups(groups, analyzer, work, p, "Đang xem ảnh show cũ")?;
    Ok(lessons(&items))
}

/// The decision part of [`learn_show`], on analysed photos.
pub fn lessons(items: &[Item]) -> (Vec<Sample>, Lesson) {
    let ok: Vec<&Item> = items.iter().filter(|i| i.analysis.is_some()).collect();
    let mut lesson = Lesson { photos: ok.len(), unreadable: items.len() - ok.len(), ..Default::default() };
    let folder = |i: &Item| i.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let folders: Vec<PathBuf> = ok.iter().map(|i| folder(i)).collect();

    // Best copy of each photo in another folder.
    let twin: Vec<Option<usize>> = (0..ok.len())
        .into_par_iter()
        .map(|i| {
            let a = ok[i].analysis.as_ref().unwrap();
            let mut best: Option<(usize, f32)> = None;
            for (j, other) in ok.iter().enumerate() {
                if folders[j] == folders[i] {
                    continue;
                }
                let b = other.analysis.as_ref().unwrap();
                let sim = visual::similarity(&a.embed, &b.embed);
                let same_time = matches!((a.time, b.time), (Some(x), Some(y)) if (x - y).abs() <= SAME_TIME);
                let same_name = a.name.rsplit_once('.').map(|x| x.0.to_lowercase()) == b.name.rsplit_once('.').map(|x| x.0.to_lowercase());
                let hit = (same_time && (sim >= SIM_WITH_TIME || a.embed.is_empty() || b.embed.is_empty() || same_name)) || sim >= SIM_ALONE;
                if hit && best.is_none_or(|(_, s)| sim > s) {
                    best = Some((j, sim));
                }
            }
            best.map(|(j, _)| j)
        })
        .collect();

    // Folders of picks: mostly copies, and not bigger than what they copy from.
    let mut per: BTreeMap<&PathBuf, (usize, usize)> = BTreeMap::new();
    for (i, f) in folders.iter().enumerate() {
        let e = per.entry(f).or_default();
        e.0 += 1;
        if twin[i].is_some() {
            e.1 += 1;
        }
    }
    let size = |f: &PathBuf| per.get(f).map(|x| x.0).unwrap_or(0);
    let picks: Vec<PathBuf> = per
        .iter()
        .filter(|(f, (n, copies))| {
            let share = *copies as f32 / *n as f32;
            // Where its copies come from must be bigger (a folder of JPEG twins of every RAW is no pick).
            let src = (0..ok.len()).filter(|&i| &folders[i] == **f).filter_map(|i| twin[i]).map(|j| size(&folders[j])).max().unwrap_or(0);
            share >= PICK_SHARE && src > *n
        })
        .map(|(f, _)| (*f).clone())
        .collect();

    let mut samples = vec![];
    if picks.is_empty() {
        lesson.only_picks = true;
        for it in &ok {
            samples.push(Sample::new(it.path.display().to_string(), it.analysis.as_ref().unwrap(), true, true, true));
        }
        lesson.kept = samples.len();
        return (samples, lesson);
    }
    let mut kept = vec![false; ok.len()];
    for i in 0..ok.len() {
        if picks.contains(&folders[i]) {
            if let Some(j) = twin[i] {
                kept[j] = true;
            }
        }
    }
    for (i, it) in ok.iter().enumerate() {
        if picks.contains(&folders[i]) {
            continue;
        }
        let k = kept[i];
        if k {
            lesson.kept += 1;
        } else {
            lesson.dropped += 1;
        }
        // His own decision, so it teaches like a hand-made choice.
        samples.push(Sample::new(it.path.display().to_string(), it.analysis.as_ref().unwrap(), k, k, true));
    }
    lesson.pick_folders = picks;
    (samples, lesson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::Analysis;

    fn look(seed: u32) -> Vec<i8> {
        let f: Vec<f32> = (0..1280u32).map(|i| ((i.wrapping_mul(2654435761).wrapping_add(seed.wrapping_mul(40503))) % 1000) as f32 / 100.0).collect();
        visual::compress(&f)
    }

    fn item(path: &str, t: Option<f64>, seed: u32) -> Item {
        let a = Analysis { name: Path::new(path).file_name().unwrap().to_string_lossy().into(), time: t, embed: look(seed), ..Default::default() };
        Item { path: path.into(), files: vec![path.into()], analysis: Some(a), error: None }
    }

    #[test]
    fn finds_the_picks_by_picture_whatever_the_names() {
        let mut items = vec![];
        for i in 0..10u32 {
            items.push(item(&format!("/show/raw/IMG_{i}.CR3"), Some(1000.0 + i as f64 * 5.0), i));
        }
        // Three picks, renamed and re-exported (same capture time, same look).
        for (n, i) in [2u32, 5, 7].iter().enumerate() {
            items.push(item(&format!("/show/gui khach/khach_{n}.jpg"), Some(1000.0 + *i as f64 * 5.0), *i));
        }
        let (samples, l) = lessons(&items);
        assert_eq!((l.kept, l.dropped, l.only_picks), (3, 7, false));
        assert_eq!(l.pick_folders, vec![PathBuf::from("/show/gui khach")]);
        assert_eq!(samples.len(), 10);
        assert!(samples.iter().filter(|s| s.keep).all(|s| ["IMG_2", "IMG_5", "IMG_7"].iter().any(|n| s.name.contains(n))));
    }

    #[test]
    fn only_picks_are_all_kept() {
        let items: Vec<Item> = (0..5u32).map(|i| item(&format!("/da chon/{i}.jpg"), None, i)).collect();
        let (samples, l) = lessons(&items);
        assert!(l.only_picks && samples.iter().all(|s| s.keep) && l.kept == 5);
    }
}
