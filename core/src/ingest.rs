//! Safe card copy: copy a memory card to the computer (and optionally a backup drive) with a
//! checksum check of every file, before any culling happens. Re-running resumes.

use crate::export::copy_verified;
use crate::preview;
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default, Serialize)]
pub struct CopyReport {
    pub files: usize,
    pub copied: usize,
    pub skipped_same: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
    pub dest: PathBuf,
    pub backup: Option<PathBuf>,
}

/// All photo files (and sidecars) under `src`, with paths relative to it.
pub fn list_media(src: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = vec![];
    let mut stack = vec![src.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let e = e?;
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if e.file_type()?.is_dir() {
                stack.push(p);
            } else if preview::is_supported(&p) || preview::ext_of(&p) == "xmp" {
                out.push(p.strip_prefix(src).unwrap_or(&p).to_path_buf());
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn safe_copy(
    src: &Path,
    dest: &Path,
    backup: Option<&Path>,
    progress: &(dyn Fn(usize, usize) + Sync),
    cancel: &AtomicBool,
) -> Result<CopyReport> {
    let files = list_media(src)?;
    let mut rep = CopyReport { files: files.len(), dest: dest.to_path_buf(), backup: backup.map(Path::to_path_buf), ..Default::default() };
    for (i, rel) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            rep.errors.push("Đã dừng theo yêu cầu".into());
            break;
        }
        progress(i, files.len());
        let from = src.join(rel);
        for root in std::iter::once(dest).chain(backup) {
            match copy_verified(&from, &root.join(rel)) {
                Ok(Some(b)) => {
                    rep.copied += 1;
                    rep.bytes += b;
                }
                Ok(None) => rep.skipped_same += 1,
                Err(e) => rep.errors.push(format!("{e:#}")),
            }
        }
    }
    progress(files.len(), files.len());
    Ok(rep)
}

/// Suffix of the folder that quick copies of starred photos go to.
pub const STAR_SUFFIX: &str = "_anh sao";

#[derive(Debug, Clone, Default, Serialize)]
pub struct StarCopy {
    pub out_dir: PathBuf,
    /// Photos looked at, and how many of them were starred.
    pub photos: usize,
    pub starred: usize,
    pub files: usize,
    pub skipped_same: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
}

/// Copy the photos starred on the camera (at least `min_stars`, or locked with "protect") from
/// `source` (a folder or straight from the memory card) into `<dest>/<source name>_anh sao`,
/// RAW and JPEG together, each file checked. No AI analysis: only the stars are read, so it is
/// quick when photos are needed at once. Originals are never touched.
pub fn copy_starred(source: &Path, dest: &Path, min_stars: u8, progress: &(dyn Fn(usize, usize) + Sync), cancel: &AtomicBool) -> Result<StarCopy> {
    let groups = crate::analyze::collect_with(source, true, true)?;
    let name = source.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Anh".into());
    let out = dest.join(format!("{name}{STAR_SUFFIX}"));
    let mut r = StarCopy { out_dir: out.clone(), photos: groups.len(), ..Default::default() };
    let ratings: Vec<crate::rating::Rating> = groups.iter().map(|(_, files)| crate::rating::read(files)).collect();
    // A folder where every file is read-only was copied that way: the lock says nothing there.
    let all_locked = ratings.len() >= 5 && ratings.iter().all(|x| x.protected);
    let min = min_stars.max(1);
    let wanted: Vec<usize> = (0..groups.len()).filter(|&i| ratings[i].stars >= min || (ratings[i].protected && !all_locked)).collect();
    r.starred = wanted.len();
    let mut used = std::collections::HashSet::new();
    for (n, &i) in wanted.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            r.errors.push("Đã dừng theo yêu cầu".into());
            break;
        }
        progress(n, wanted.len());
        for f in &groups[i].1 {
            let stem = f.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let ext = f.extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default();
            let mut dst = out.join(format!("{stem}{ext}"));
            let mut k = 2;
            while !used.insert(dst.clone()) {
                dst = out.join(format!("{stem}_{k}{ext}"));
                k += 1;
            }
            match copy_verified(f, &dst) {
                Ok(Some(b)) => {
                    r.files += 1;
                    r.bytes += b;
                }
                Ok(None) => r.skipped_same += 1,
                Err(e) => r.errors.push(format!("{e:#}")),
            }
        }
    }
    progress(wanted.len(), wanted.len());
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_card_to_two_places() {
        let t = tempfile::tempdir().unwrap();
        let card = t.path().join("card/DCIM/100CANON");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("IMG_1.CR3"), vec![7u8; 5000]).unwrap();
        std::fs::write(card.join("note.txt"), b"skip me").unwrap();
        let (d, b) = (t.path().join("pc"), t.path().join("backup"));
        let r = safe_copy(&t.path().join("card"), &d, Some(&b), &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((r.files, r.copied), (1, 2));
        assert!(d.join("DCIM/100CANON/IMG_1.CR3").exists() && b.join("DCIM/100CANON/IMG_1.CR3").exists());
        let r = safe_copy(&t.path().join("card"), &d, Some(&b), &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(r.skipped_same, 2);
    }

    #[test]
    fn copies_only_starred_photos() {
        let t = tempfile::tempdir().unwrap();
        let card = t.path().join("card/DCIM/100CANON");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("IMG_1.CR3"), b"ftypcrx <x:xmpmeta> xmp:Rating=\"3\" </x:xmpmeta>").unwrap();
        std::fs::write(card.join("IMG_1.JPG"), b"\xFF\xD8 jpeg").unwrap();
        std::fs::write(card.join("IMG_2.CR3"), b"ftypcrx no stars").unwrap();
        std::fs::write(card.join("IMG_3.CR3"), b"ftypcrx xmp:Rating=\"1\"").unwrap();
        let dest = t.path().join("out");
        let r = copy_starred(&t.path().join("card"), &dest, 2, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((r.photos, r.starred, r.files), (3, 1, 2), "{r:?}");
        assert!(r.out_dir.join("IMG_1.CR3").exists() && r.out_dir.join("IMG_1.JPG").exists());
        assert!(!r.out_dir.join("IMG_2.CR3").exists() && !r.out_dir.join("IMG_3.CR3").exists());
        assert!(card.join("IMG_1.CR3").exists(), "originals stay");
        let again = copy_starred(&t.path().join("card"), &dest, 1, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((again.starred, again.files, again.skipped_same), (2, 1, 2));
    }
}
