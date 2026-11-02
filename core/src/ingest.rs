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
}
